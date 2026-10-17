use crate::{
    items::ItemPhoto,
    photo_drafts::{PhotoDraft, PhotoDraftsRepositoryTrait},
    ProcurementError,
};
use anyhow::Result;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;
use wealthfolio_files::{BlobKey, LocalBlobStore, StoreRequest, StoredFileRef, StoredFileService};

pub const MAX_PHOTO_BYTES: usize = 20 * 1024 * 1024;

pub struct PhotoService {
    files: StoredFileService,
    drafts: Arc<dyn PhotoDraftsRepositoryTrait>,
}
impl PhotoService {
    pub fn new(root: PathBuf, drafts: Arc<dyn PhotoDraftsRepositoryTrait>) -> Result<Self> {
        Ok(Self {
            files: StoredFileService::new(Arc::new(LocalBlobStore::new(root)?), 2),
            drafts,
        })
    }
    pub async fn upload_slot(&self) -> Result<OwnedSemaphorePermit> {
        Ok(self.files.render_slot().await?)
    }
    pub async fn upload(
        &self,
        filename: &str,
        content_type: &str,
        bytes: Vec<u8>,
        slot: OwnedSemaphorePermit,
    ) -> Result<PhotoDraft> {
        if bytes.is_empty()
            || bytes.len() > MAX_PHOTO_BYTES
            || !matches!(content_type, "image/jpeg" | "image/png" | "image/webp")
        {
            return Err(ProcurementError::InvalidInput(
                "Photo must be JPEG, PNG or WebP, between 1 byte and 20 MiB".into(),
            )
            .into());
        }
        let id = Uuid::new_v4();
        let key = BlobKey::mint(id, "original")?;
        let preview_key = BlobKey::mint(id, "webp")?;
        let draft = PhotoDraft {
            id: id.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            photo: ItemPhoto {
                backend: "local".into(),
                key: key.as_str().into(),
                preview_key: preview_key.as_str().into(),
                filename: filename.into(),
                content_type: content_type.into(),
                size_bytes: bytes.len() as i64,
            },
        };
        let mut saved = draft.clone();
        self.files
            .store_with_preview(
                StoreRequest {
                    key,
                    preview_key,
                    filename,
                    content_type,
                    bytes,
                },
                slot,
                |stored| async move {
                    saved.photo.backend = stored.backend;
                    self.drafts.insert(saved).await
                },
            )
            .await
            .map_err(|error: anyhow::Error| {
                if let Some(wealthfolio_files::FileStoreError::InvalidInput(message)) =
                    error.downcast_ref::<wealthfolio_files::FileStoreError>()
                {
                    ProcurementError::InvalidInput(message.clone()).into()
                } else {
                    error
                }
            })?;
        Ok(draft)
    }
    pub async fn discard(&self, id: &str) -> Result<()> {
        if let Some(draft) = self.drafts.take(id).await? {
            self.remove(&draft.photo).await?;
        }
        Ok(())
    }
    pub async fn remove(&self, photo: &ItemPhoto) -> Result<()> {
        self.files.remove(&[file_ref(photo)?]).await?;
        Ok(())
    }
    pub async fn read(&self, photo: &ItemPhoto, preview: bool) -> Result<Vec<u8>> {
        Ok(self.files.read(&file_ref(photo)?, preview).await?)
    }
    pub async fn sweep_drafts(&self) -> Result<()> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        for id in self.drafts.older_than(&cutoff)? {
            self.discard(&id).await?;
        }
        Ok(())
    }
}
fn file_ref(photo: &ItemPhoto) -> Result<StoredFileRef> {
    Ok(StoredFileRef {
        backend: photo.backend.clone(),
        key: BlobKey::parse(&photo.key)?,
        preview_key: Some(BlobKey::parse(&photo.preview_key)?),
    })
}
