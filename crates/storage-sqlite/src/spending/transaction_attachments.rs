use crate::{
    db::{get_connection, DbPool, WriteHandle},
    errors::StorageError,
    schema::spending_transaction_attachments as attachments,
};
use anyhow::Result;
use async_trait::async_trait;
use diesel::prelude::*;
use std::sync::Arc;
use wealthfolio_spending::transaction_attachments::{
    TransactionAttachment, TransactionAttachmentsRepository,
};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = attachments)]
struct AttachmentRow {
    id: String,
    activity_id: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    created_at: String,
}
impl From<AttachmentRow> for TransactionAttachment {
    fn from(row: AttachmentRow) -> Self {
        Self {
            id: row.id,
            activity_id: row.activity_id,
            filename: row.filename,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            created_at: row.created_at,
        }
    }
}

pub struct SqliteTransactionAttachmentsRepository {
    pool: Arc<DbPool>,
    writer: WriteHandle,
}
impl SqliteTransactionAttachmentsRepository {
    pub fn new(pool: Arc<DbPool>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}
#[async_trait]
impl TransactionAttachmentsRepository for SqliteTransactionAttachmentsRepository {
    async fn activities_with_attachments(&self, account_id: &str) -> Result<Vec<String>> {
        #[derive(QueryableByName)]
        struct ActivityId {
            #[diesel(sql_type = diesel::sql_types::Text)]
            id: String,
        }
        let mut conn = get_connection(&self.pool)?;
        Ok(diesel::sql_query("SELECT DISTINCT a.id FROM activities a INNER JOIN spending_transaction_attachments d ON d.activity_id = a.id WHERE a.account_id = ?")
            .bind::<diesel::sql_types::Text, _>(account_id).load::<ActivityId>(&mut conn)?.into_iter().map(|row| row.id).collect())
    }
    async fn list(&self, activity_id: &str) -> Result<Vec<TransactionAttachment>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(attachments::table
            .filter(attachments::activity_id.eq(activity_id))
            .order((attachments::created_at.asc(), attachments::id.asc()))
            .select(AttachmentRow::as_select())
            .load::<AttachmentRow>(&mut conn)?
            .into_iter()
            .map(Into::into)
            .collect())
    }
    async fn insert_attachment(&self, value: TransactionAttachment) -> Result<()> {
        self.writer
            .exec_tx(move |tx| {
                diesel::insert_into(attachments::table)
                    .values(AttachmentRow {
                        id: value.id,
                        activity_id: value.activity_id,
                        filename: value.filename,
                        content_type: value.content_type,
                        size_bytes: value.size_bytes,
                        created_at: value.created_at,
                    })
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
            .map_err(Into::into)
    }
    async fn delete_attachment(&self, activity_id: &str, id: &str) -> Result<()> {
        let activity_id = activity_id.to_owned();
        let id = id.to_owned();
        self.writer
            .exec_tx(move |tx| {
                diesel::delete(
                    attachments::table
                        .filter(attachments::activity_id.eq(activity_id))
                        .filter(attachments::id.eq(id)),
                )
                .execute(tx.conn())
                .map_err(StorageError::from)?;
                Ok(())
            })
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_pool, init, run_migrations, write_actor::spawn_writer};
    #[tokio::test]
    async fn transaction_attachments_sqlite_constraints_and_cascades() {
        std::env::set_var("CONNECT_API_URL", "http://test.local");
        let directory = tempfile::tempdir().expect("temporary database");
        let path =
            init(directory.path().to_str().expect("temporary path")).expect("initialize database");
        run_migrations(&path).expect("migrations");
        let pool = create_pool(&path).expect("pool");
        let writer = spawn_writer(pool.as_ref().clone()).expect("writer");
        let repo = SqliteTransactionAttachmentsRepository::new(pool.clone(), writer);
        let mut conn = get_connection(&pool).expect("connection");
        diesel::sql_query("INSERT INTO accounts (id,name,account_type,currency,is_default,is_active,is_archived,tracking_mode,created_at,updated_at) VALUES ('account','Test','CASH','USD',0,1,0,'TRANSACTIONS',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&mut conn).expect("account");
        diesel::sql_query("INSERT INTO activities (id,account_id,activity_type,status,activity_date,amount,currency,is_user_modified,needs_review,created_at,updated_at) VALUES ('transaction','account','WITHDRAWAL','POSTED','2026-10-09T00:00:00Z','42','USD',0,0,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").execute(&mut conn).expect("activity");
        let attachment = |id: &str| TransactionAttachment {
            id: id.into(),
            activity_id: "transaction".into(),
            filename: "receipt.png".into(),
            content_type: "image/png".into(),
            size_bytes: 42,
            created_at: "2026-10-09T00:00:00Z".into(),
        };
        let mut invalid = attachment("oversized");
        invalid.size_bytes = 20 * 1024 * 1024 + 1;
        assert!(repo.insert_attachment(invalid).await.is_err());
        let mut invalid = attachment("unsupported");
        invalid.content_type = "image/svg+xml".into();
        assert!(repo.insert_attachment(invalid).await.is_err());
        for i in 0..10 {
            repo.insert_attachment(attachment(&format!("file-{i}")))
                .await
                .expect("attachment");
        }
        // Exercise the database trigger independently of the domain pre-check.
        assert!(repo
            .insert_attachment(attachment("eleventh"))
            .await
            .is_err());
        assert_eq!(
            repo.list("transaction").await.expect("attachments").len(),
            10
        );
        assert_eq!(
            repo.activities_with_attachments("account")
                .await
                .expect("account files"),
            vec!["transaction"]
        );
        diesel::sql_query("DELETE FROM activities WHERE id = 'transaction'")
            .execute(&mut conn)
            .expect("delete transaction");
        let attachments = repo
            .list("transaction")
            .await
            .expect("cascaded attachments");
        assert!(attachments.is_empty());
    }
}
