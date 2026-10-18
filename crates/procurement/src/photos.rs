use crate::{
    items::ItemPhoto,
    photo_drafts::{PhotoDraft, PhotoDraftsRepositoryTrait},
    ProcurementError,
};
use anyhow::Result;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;
use wealthfolio_files::{
    stored_filename, BlobKey, StoreRequest, StoredFileRef, StoredFileService, LOCAL_BACKEND_ID,
};

pub const MAX_PHOTO_BYTES: usize = 20 * 1024 * 1024;

pub struct PhotoService {
    files: StoredFileService,
    drafts: Arc<dyn PhotoDraftsRepositoryTrait>,
}
impl PhotoService {
    pub fn new(root: PathBuf, drafts: Arc<dyn PhotoDraftsRepositoryTrait>) -> Result<Self> {
        Ok(Self {
            files: StoredFileService::from_env(root, 2)?,
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
        let mut draft = PhotoDraft {
            id: id.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            photo: ItemPhoto {
                backend: LOCAL_BACKEND_ID.into(),
                key: key.as_str().into(),
                preview_key: preview_key.as_str().into(),
                filename: filename.into(),
                content_type: content_type.into(),
                size_bytes: bytes.len() as i64,
            },
        };
        let mut saved = draft.clone();
        let stored = self
            .files
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
                    saved.photo.filename =
                        stored_filename(&saved.photo.filename, &stored.content_type);
                    saved.photo.content_type = stored.content_type;
                    saved.photo.size_bytes = stored.size_bytes;
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
        draft.photo.filename = stored_filename(filename, &stored.content_type);
        draft.photo.content_type = stored.content_type;
        draft.photo.size_bytes = stored.size_bytes;
        draft.photo.backend = stored.backend;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Drafts(Mutex<Option<PhotoDraft>>);

    #[async_trait::async_trait]
    impl PhotoDraftsRepositoryTrait for Drafts {
        async fn insert(&self, draft: PhotoDraft) -> Result<()> {
            *self.0.lock().unwrap() = Some(draft);
            Ok(())
        }
        async fn take(&self, _id: &str) -> Result<Option<PhotoDraft>> {
            Ok(self.0.lock().unwrap().take())
        }
        fn older_than(&self, _cutoff: &str) -> Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn uploaded_photo_metadata_matches_the_saved_webp_and_persisted_draft() {
        let dir = tempfile::tempdir().unwrap();
        let drafts = Arc::new(Drafts::default());
        let service = PhotoService {
            files: StoredFileService::new(
                Arc::new(
                    wealthfolio_files::LocalBlobStore::new(dir.path().join("photos")).unwrap(),
                ),
                2,
            ),
            drafts: drafts.clone(),
        };
        let png = include_bytes!("../../../e2e/fixtures/transaction-attachments/receipt.png");
        let draft = service
            .upload(
                "photo.png",
                "image/png",
                png.to_vec(),
                service.upload_slot().await.unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(draft.photo.filename, "photo.webp");
        assert_eq!(draft.photo.content_type, "image/webp");
        let bytes = service.read(&draft.photo, false).await.unwrap();
        assert_eq!(draft.photo.size_bytes, bytes.len() as i64);
        assert!(bytes.starts_with(b"RIFF"));
        assert_eq!(&bytes[8..12], b"WEBP");
        assert_eq!(
            serde_json::to_value(&draft).unwrap(),
            serde_json::to_value(drafts.0.lock().unwrap().as_ref().unwrap()).unwrap()
        );
        service.discard(&draft.id).await.unwrap();
        assert!(service.read(&draft.photo, false).await.is_err());
        assert!(service.read(&draft.photo, true).await.is_err());
    }
}
