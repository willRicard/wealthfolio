use crate::{
    db::{get_connection, DbPool, WriteHandle},
    errors::StorageError,
    schema::spending_attachment_drafts as drafts,
    schema::spending_transaction_attachments as attachments,
};
use anyhow::Result;
use async_trait::async_trait;
use diesel::prelude::*;
use std::sync::Arc;
use wealthfolio_spending::transaction_attachments::{
    AttachmentDraft, AttachmentStorage, StoredAttachment, StoredDraft, TransactionAttachment,
    TransactionAttachmentsRepository,
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
    position: i32,
    storage_backend: String,
    storage_key: String,
    preview_key: String,
}
impl From<AttachmentRow> for StoredAttachment {
    fn from(row: AttachmentRow) -> Self {
        Self {
            storage: AttachmentStorage {
                backend: row.storage_backend,
                key: row.storage_key,
                preview_key: row.preview_key,
            },
            attachment: TransactionAttachment {
                id: row.id,
                activity_id: row.activity_id,
                filename: row.filename,
                content_type: row.content_type,
                size_bytes: row.size_bytes,
                created_at: row.created_at,
            },
        }
    }
}
impl From<StoredAttachment> for AttachmentRow {
    fn from(value: StoredAttachment) -> Self {
        Self {
            id: value.attachment.id,
            activity_id: value.attachment.activity_id,
            filename: value.attachment.filename,
            content_type: value.attachment.content_type,
            size_bytes: value.attachment.size_bytes,
            created_at: value.attachment.created_at,
            position: 0,
            storage_backend: value.storage.backend,
            storage_key: value.storage.key,
            preview_key: value.storage.preview_key,
        }
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = attachments)]
struct StorageRow {
    activity_id: String,
    storage_backend: String,
    storage_key: String,
    preview_key: String,
}
impl From<StorageRow> for (String, AttachmentStorage) {
    fn from(row: StorageRow) -> Self {
        (
            row.activity_id,
            AttachmentStorage {
                backend: row.storage_backend,
                key: row.storage_key,
                preview_key: row.preview_key,
            },
        )
    }
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = drafts)]
struct DraftRow {
    id: String,
    token: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    created_at: String,
    position: i32,
    storage_backend: String,
    storage_key: String,
    preview_key: String,
}
impl From<DraftRow> for StoredDraft {
    fn from(row: DraftRow) -> Self {
        Self {
            storage: AttachmentStorage {
                backend: row.storage_backend,
                key: row.storage_key,
                preview_key: row.preview_key,
            },
            draft: AttachmentDraft {
                id: row.id,
                token: row.token,
                filename: row.filename,
                content_type: row.content_type,
                size_bytes: row.size_bytes,
                created_at: row.created_at,
            },
        }
    }
}
impl From<StoredDraft> for DraftRow {
    fn from(value: StoredDraft) -> Self {
        Self {
            id: value.draft.id,
            token: value.draft.token,
            filename: value.draft.filename,
            content_type: value.draft.content_type,
            size_bytes: value.draft.size_bytes,
            created_at: value.draft.created_at,
            position: 0,
            storage_backend: value.storage.backend,
            storage_key: value.storage.key,
            preview_key: value.storage.preview_key,
        }
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = drafts)]
struct DraftStorageRow {
    storage_backend: String,
    storage_key: String,
    preview_key: String,
}
impl From<DraftStorageRow> for AttachmentStorage {
    fn from(row: DraftStorageRow) -> Self {
        Self {
            backend: row.storage_backend,
            key: row.storage_key,
            preview_key: row.preview_key,
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
    async fn storage_for_account(&self, account_id: &str) -> Result<Vec<AttachmentStorage>> {
        // Raw SQL because `activities` and `spending_transaction_attachments`
        // are not declared joinable in `schema.rs`.
        #[derive(QueryableByName)]
        struct Row {
            #[diesel(sql_type = diesel::sql_types::Text)]
            storage_backend: String,
            #[diesel(sql_type = diesel::sql_types::Text)]
            storage_key: String,
            #[diesel(sql_type = diesel::sql_types::Text)]
            preview_key: String,
        }
        let mut conn = get_connection(&self.pool)?;
        Ok(diesel::sql_query("SELECT d.storage_backend, d.storage_key, d.preview_key FROM activities a INNER JOIN spending_transaction_attachments d ON d.activity_id = a.id WHERE a.account_id = ?")
            .bind::<diesel::sql_types::Text, _>(account_id)
            .load::<Row>(&mut conn)?
            .into_iter()
            .map(|row| AttachmentStorage {
                backend: row.storage_backend,
                key: row.storage_key,
                preview_key: row.preview_key,
            })
            .collect())
    }
    async fn storage_for_activities(
        &self,
        activity_ids: &[String],
    ) -> Result<Vec<(String, AttachmentStorage)>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(attachments::table
            .filter(attachments::activity_id.eq_any(activity_ids))
            .select(StorageRow::as_select())
            .load::<StorageRow>(&mut conn)?
            .into_iter()
            .map(Into::into)
            .collect())
    }
    async fn list(&self, activity_id: &str) -> Result<Vec<TransactionAttachment>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(attachments::table
            .filter(attachments::activity_id.eq(activity_id))
            .order((
                attachments::position.asc(),
                attachments::created_at.asc(),
                attachments::id.asc(),
            ))
            .select(AttachmentRow::as_select())
            .load::<AttachmentRow>(&mut conn)?
            .into_iter()
            .map(|row| StoredAttachment::from(row).attachment)
            .collect())
    }
    async fn find_attachment(
        &self,
        activity_id: &str,
        id: &str,
    ) -> Result<Option<StoredAttachment>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(attachments::table
            .filter(attachments::activity_id.eq(activity_id))
            .filter(attachments::id.eq(id))
            .select(AttachmentRow::as_select())
            .first::<AttachmentRow>(&mut conn)
            .optional()?
            .map(Into::into))
    }
    async fn insert_attachment(&self, value: StoredAttachment) -> Result<()> {
        self.writer
            .exec_tx(move |tx| {
                let mut row = AttachmentRow::from(value);
                let next = attachments::table
                    .filter(attachments::activity_id.eq(&row.activity_id))
                    .select(diesel::dsl::max(attachments::position))
                    .first::<Option<i32>>(tx.conn())
                    .map_err(StorageError::from)?
                    .map_or(0, |highest| highest + 1);
                row.position = next;
                diesel::insert_into(attachments::table)
                    .values(row)
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

    async fn list_drafts(&self, token: &str) -> Result<Vec<AttachmentDraft>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(drafts::table
            .filter(drafts::token.eq(token))
            .order((
                drafts::position.asc(),
                drafts::created_at.asc(),
                drafts::id.asc(),
            ))
            .select(DraftRow::as_select())
            .load::<DraftRow>(&mut conn)?
            .into_iter()
            .map(|row| StoredDraft::from(row).draft)
            .collect())
    }
    async fn find_draft(&self, token: &str, id: &str) -> Result<Option<StoredDraft>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(drafts::table
            .filter(drafts::token.eq(token))
            .filter(drafts::id.eq(id))
            .select(DraftRow::as_select())
            .first::<DraftRow>(&mut conn)
            .optional()?
            .map(Into::into))
    }
    async fn insert_draft(&self, value: StoredDraft) -> Result<()> {
        self.writer
            .exec_tx(move |tx| {
                let mut row = DraftRow::from(value);
                let next = drafts::table
                    .filter(drafts::token.eq(&row.token))
                    .select(diesel::dsl::max(drafts::position))
                    .first::<Option<i32>>(tx.conn())
                    .map_err(StorageError::from)?
                    .map_or(0, |highest| highest + 1);
                row.position = next;
                diesel::insert_into(drafts::table)
                    .values(row)
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
            .map_err(Into::into)
    }
    async fn delete_draft(&self, token: &str, id: &str) -> Result<()> {
        let token = token.to_owned();
        let id = id.to_owned();
        self.writer
            .exec_tx(move |tx| {
                diesel::delete(
                    drafts::table
                        .filter(drafts::token.eq(token))
                        .filter(drafts::id.eq(id)),
                )
                .execute(tx.conn())
                .map_err(StorageError::from)?;
                Ok(())
            })
            .await
            .map_err(Into::into)
    }
    async fn reorder(&self, activity_id: &str, ids: &[String]) -> Result<()> {
        let activity_id = activity_id.to_owned();
        let ids = ids.to_vec();
        self.writer
            .exec_tx(move |tx| {
                for (index, id) in ids.iter().enumerate() {
                    diesel::update(
                        attachments::table
                            .filter(attachments::activity_id.eq(&activity_id))
                            .filter(attachments::id.eq(id)),
                    )
                    .set(attachments::position.eq(index as i32))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                }
                Ok(())
            })
            .await
            .map_err(Into::into)
    }
    async fn claim_drafts(&self, token: &str, activity_id: &str) -> Result<usize> {
        let token = token.to_owned();
        let activity_id = activity_id.to_owned();
        self.writer
            .exec_tx(move |tx| {
                let offset = attachments::table
                    .filter(attachments::activity_id.eq(&activity_id))
                    .select(diesel::dsl::max(attachments::position))
                    .first::<Option<i32>>(tx.conn())
                    .map_err(StorageError::from)?
                    .map_or(0, |highest| highest + 1);
                let claimed = diesel::sql_query(
                    "INSERT INTO spending_transaction_attachments \
                     (id, activity_id, filename, content_type, size_bytes, created_at, position, storage_backend, storage_key, preview_key) \
                     SELECT id, ?, filename, content_type, size_bytes, created_at, position + ?, storage_backend, storage_key, preview_key \
                     FROM spending_attachment_drafts WHERE token = ?",
                )
                .bind::<diesel::sql_types::Text, _>(&activity_id)
                .bind::<diesel::sql_types::Integer, _>(offset)
                .bind::<diesel::sql_types::Text, _>(&token)
                .execute(tx.conn())
                .map_err(StorageError::from)?;
                diesel::delete(drafts::table.filter(drafts::token.eq(&token)))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(claimed)
            })
            .await
            .map_err(Into::into)
    }
    async fn discard_drafts(&self, token: &str) -> Result<Vec<AttachmentStorage>> {
        let token = token.to_owned();
        self.writer
            .exec_tx(move |tx| {
                let storage = drafts::table
                    .filter(drafts::token.eq(&token))
                    .select(DraftStorageRow::as_select())
                    .load::<DraftStorageRow>(tx.conn())
                    .map_err(StorageError::from)?;
                diesel::delete(drafts::table.filter(drafts::token.eq(&token)))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(storage.into_iter().map(Into::into).collect())
            })
            .await
            .map_err(Into::into)
    }
    async fn sweep_drafts(&self, cutoff: &str) -> Result<Vec<AttachmentStorage>> {
        let cutoff = cutoff.to_owned();
        self.writer
            .exec_tx(move |tx| {
                let storage = drafts::table
                    .filter(drafts::created_at.lt(&cutoff))
                    .select(DraftStorageRow::as_select())
                    .load::<DraftStorageRow>(tx.conn())
                    .map_err(StorageError::from)?;
                diesel::delete(drafts::table.filter(drafts::created_at.lt(&cutoff)))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(storage.into_iter().map(Into::into).collect())
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
        let attachment = |id: &str| StoredAttachment {
            attachment: TransactionAttachment {
                id: id.into(),
                activity_id: "transaction".into(),
                filename: "receipt.png".into(),
                content_type: "image/png".into(),
                size_bytes: 42,
                created_at: "2026-10-09T00:00:00Z".into(),
            },
            storage: AttachmentStorage {
                backend: "local".into(),
                key: format!("ab/{id}.original"),
                preview_key: format!("ab/{id}.webp"),
            },
        };
        let mut invalid = attachment("oversized");
        invalid.attachment.size_bytes = 20 * 1024 * 1024 + 1;
        assert!(repo.insert_attachment(invalid).await.is_err());
        let mut invalid = attachment("unsupported");
        invalid.attachment.content_type = "image/svg+xml".into();
        assert!(repo.insert_attachment(invalid).await.is_err());
        for i in 0..10 {
            repo.insert_attachment(attachment(&format!("file-{i}")))
                .await
                .expect("attachment");
        }
        assert!(repo
            .insert_attachment(attachment("eleventh"))
            .await
            .is_err());
        assert_eq!(
            repo.list("transaction").await.expect("attachments").len(),
            10
        );
        assert_eq!(
            repo.storage_for_account("account")
                .await
                .expect("account files")
                .len(),
            10
        );
        let pending = repo
            .storage_for_activities(&["transaction".to_string()])
            .await
            .expect("activity files");
        assert_eq!(pending.len(), 10);
        assert!(pending.iter().all(|(id, _)| id == "transaction"));

        diesel::sql_query("DELETE FROM activities WHERE id = 'transaction'")
            .execute(&mut conn)
            .expect("delete transaction");
        let attachments = repo
            .list("transaction")
            .await
            .expect("cascaded attachments");
        assert!(attachments.is_empty());
        assert!(repo
            .storage_for_activities(&["transaction".to_string()])
            .await
            .expect("cascaded storage")
            .is_empty());
    }

    #[tokio::test]
    async fn attachment_drafts_claim_without_moving_files_and_sweep_by_age() {
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

        let draft = |id: &str, token: &str, created_at: &str| StoredDraft {
            draft: AttachmentDraft {
                id: id.into(),
                token: token.into(),
                filename: "receipt.png".into(),
                content_type: "image/png".into(),
                size_bytes: 42,
                created_at: created_at.into(),
            },
            storage: AttachmentStorage {
                backend: "local".into(),
                key: format!("ab/{id}.original"),
                preview_key: format!("ab/{id}.webp"),
            },
        };
        let now = "2026-10-13T00:00:00+00:00";

        for i in 0..10 {
            repo.insert_draft(draft(&format!("d{i}"), "token-a", now))
                .await
                .expect("draft");
        }
        assert!(repo
            .insert_draft(draft("eleventh", "token-a", now))
            .await
            .is_err());
        assert_eq!(repo.list_drafts("token-a").await.expect("drafts").len(), 10);

        for i in 0..3 {
            repo.insert_attachment(StoredAttachment {
                attachment: TransactionAttachment {
                    id: format!("existing-{i}"),
                    activity_id: "transaction".into(),
                    filename: "receipt.png".into(),
                    content_type: "image/png".into(),
                    size_bytes: 42,
                    created_at: now.into(),
                },
                storage: AttachmentStorage {
                    backend: "local".into(),
                    key: format!("cd/existing-{i}.original"),
                    preview_key: format!("cd/existing-{i}.webp"),
                },
            })
            .await
            .expect("attachment");
        }
        assert!(repo.claim_drafts("token-a", "transaction").await.is_err());
        assert_eq!(repo.list_drafts("token-a").await.expect("drafts").len(), 10);
        assert_eq!(
            repo.list("transaction").await.expect("attachments").len(),
            3
        );

        repo.insert_draft(draft("fits", "token-b", now))
            .await
            .expect("draft");
        let before = repo
            .find_draft("token-b", "fits")
            .await
            .expect("draft")
            .expect("present")
            .storage;
        assert_eq!(
            repo.claim_drafts("token-b", "transaction").await.unwrap(),
            1
        );
        assert!(repo
            .list_drafts("token-b")
            .await
            .expect("drafts")
            .is_empty());
        let after = repo
            .find_attachment("transaction", "fits")
            .await
            .expect("attachment")
            .expect("present");
        assert_eq!(after.storage, before);
        assert_eq!(after.attachment.filename, "receipt.png");

        repo.insert_draft(draft("kept", "token-c", now))
            .await
            .expect("draft");
        let discarded = repo.discard_drafts("token-a").await.expect("discard");
        assert_eq!(discarded.len(), 10);
        assert!(repo
            .list_drafts("token-a")
            .await
            .expect("drafts")
            .is_empty());
        assert_eq!(repo.list_drafts("token-c").await.expect("drafts").len(), 1);

        repo.insert_draft(draft("stale", "token-d", "2026-10-01T00:00:00+00:00"))
            .await
            .expect("draft");
        let swept = repo
            .sweep_drafts("2026-10-10T00:00:00+00:00")
            .await
            .expect("sweep");
        assert_eq!(swept.len(), 1);
        assert_eq!(swept[0].key, "ab/stale.original");
        assert_eq!(repo.list_drafts("token-c").await.expect("drafts").len(), 1);
    }

    #[tokio::test]
    async fn attachment_positions_append_reorder_and_survive_a_claim() {
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
        for activity in ["first", "second"] {
            diesel::sql_query(format!("INSERT INTO activities (id,account_id,activity_type,status,activity_date,amount,currency,is_user_modified,needs_review,created_at,updated_at) VALUES ('{activity}','account','WITHDRAWAL','POSTED','2026-10-09T00:00:00Z','42','USD',0,0,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")).execute(&mut conn).expect("activity");
        }
        let now = "2026-10-13T00:00:00+00:00";
        let attachment = |id: &str, activity_id: &str| StoredAttachment {
            attachment: TransactionAttachment {
                id: id.into(),
                activity_id: activity_id.into(),
                filename: "receipt.png".into(),
                content_type: "image/png".into(),
                size_bytes: 42,
                created_at: now.into(),
            },
            storage: AttachmentStorage {
                backend: "local".into(),
                key: format!("ab/{id}.original"),
                preview_key: format!("ab/{id}.webp"),
            },
        };
        let ids = |attachments: Vec<TransactionAttachment>| {
            attachments
                .into_iter()
                .map(|attachment| attachment.id)
                .collect::<Vec<_>>()
        };

        for id in ["c", "a", "b"] {
            repo.insert_attachment(attachment(id, "first"))
                .await
                .expect("attachment");
        }
        for id in ["z", "y"] {
            repo.insert_attachment(attachment(id, "second"))
                .await
                .expect("attachment");
        }
        assert_eq!(
            ids(repo.list("first").await.expect("attachments")),
            ["c", "a", "b"]
        );
        assert_eq!(
            ids(repo.list("second").await.expect("attachments")),
            ["z", "y"]
        );

        repo.reorder("first", &["b".into(), "c".into(), "a".into()])
            .await
            .expect("reorder");
        assert_eq!(
            ids(repo.list("first").await.expect("attachments")),
            ["b", "c", "a"]
        );
        repo.insert_attachment(attachment("d", "first"))
            .await
            .expect("attachment");
        assert_eq!(
            ids(repo.list("first").await.expect("attachments")),
            ["b", "c", "a", "d"]
        );
        assert_eq!(
            ids(repo.list("second").await.expect("attachments")),
            ["z", "y"]
        );

        let draft = |id: &str, token: &str| StoredDraft {
            draft: AttachmentDraft {
                id: id.into(),
                token: token.into(),
                filename: "receipt.png".into(),
                content_type: "image/png".into(),
                size_bytes: 42,
                created_at: now.into(),
            },
            storage: AttachmentStorage {
                backend: "local".into(),
                key: format!("cd/{id}.original"),
                preview_key: format!("cd/{id}.webp"),
            },
        };
        for id in ["s", "r", "t"] {
            repo.insert_draft(draft(id, "token")).await.expect("draft");
        }
        assert_eq!(
            repo.list_drafts("token")
                .await
                .expect("drafts")
                .into_iter()
                .map(|draft| draft.id)
                .collect::<Vec<_>>(),
            ["s", "r", "t"]
        );
        assert_eq!(
            repo.claim_drafts("token", "second").await.expect("claim"),
            3
        );
        assert_eq!(
            ids(repo.list("second").await.expect("attachments")),
            ["z", "y", "s", "r", "t"]
        );
    }
}
