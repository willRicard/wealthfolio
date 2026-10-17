use crate::{
    db::{get_connection, DbPool, WriteHandle},
    errors::StorageError,
    schema::procurement_photo_drafts as drafts,
};
use anyhow::Result;
use async_trait::async_trait;
use diesel::prelude::*;
use std::sync::Arc;
use wealthfolio_procurement::{
    items::ItemPhoto,
    photo_drafts::{PhotoDraft, PhotoDraftsRepositoryTrait},
};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = drafts)]
pub(super) struct DraftDB {
    pub id: String,
    pub storage_backend: String,
    pub storage_key: String,
    pub preview_key: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: String,
}
impl DraftDB {
    pub fn into_model(self) -> PhotoDraft {
        PhotoDraft {
            id: self.id,
            photo: ItemPhoto {
                backend: self.storage_backend,
                key: self.storage_key,
                preview_key: self.preview_key,
                filename: self.filename,
                content_type: self.content_type,
                size_bytes: self.size_bytes,
            },
            created_at: self.created_at,
        }
    }
}
pub struct PhotoDraftsRepository {
    pool: Arc<DbPool>,
    writer: WriteHandle,
}
impl PhotoDraftsRepository {
    pub fn new(pool: Arc<DbPool>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}
#[async_trait]
impl PhotoDraftsRepositoryTrait for PhotoDraftsRepository {
    async fn insert(&self, draft: PhotoDraft) -> Result<()> {
        let row = DraftDB {
            id: draft.id,
            storage_backend: draft.photo.backend,
            storage_key: draft.photo.key,
            preview_key: draft.photo.preview_key,
            filename: draft.photo.filename,
            content_type: draft.photo.content_type,
            size_bytes: draft.photo.size_bytes,
            created_at: draft.created_at,
        };
        self.writer
            .exec_tx(move |tx| {
                diesel::insert_into(drafts::table)
                    .values(row)
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await?;
        Ok(())
    }
    async fn take(&self, id: &str) -> Result<Option<PhotoDraft>> {
        let id = id.to_owned();
        let row = self
            .writer
            .exec_tx(move |tx| {
                let row = drafts::table
                    .find(&id)
                    .select(DraftDB::as_select())
                    .first::<DraftDB>(tx.conn())
                    .optional()
                    .map_err(StorageError::from)?;
                diesel::delete(drafts::table.find(&id))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(row)
            })
            .await?;
        Ok(row.map(DraftDB::into_model))
    }
    fn older_than(&self, cutoff: &str) -> Result<Vec<String>> {
        let mut conn = get_connection(&self.pool)?;
        Ok(drafts::table
            .filter(drafts::created_at.lt(cutoff))
            .select(drafts::id)
            .load(&mut conn)?)
    }
}
