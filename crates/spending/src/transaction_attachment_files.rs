//! Private, bounded server files. Originals never have a public/static URL.

use crate::{
    transaction_attachments::{
        AttachmentDraft, AttachmentStorage, StoredAttachment, StoredDraft, TransactionAttachment,
        TransactionAttachmentsService, MAX_ATTACHMENT_BYTES,
    },
    SpendingError,
};
use anyhow::Result;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;
use wealthfolio_files::{
    validate_filename, BlobKey, FileStoreError, LocalBlobStore, StoreRequest, StoredFile,
    StoredFileRef, StoredFileService,
};

pub struct TransactionAttachmentFiles {
    files: Arc<StoredFileService>,
    attachments: Arc<TransactionAttachmentsService>,
}

fn invalid(message: &str) -> anyhow::Error {
    SpendingError::InvalidInput {
        message: message.into(),
    }
    .into()
}

fn domain(error: FileStoreError) -> anyhow::Error {
    match error {
        FileStoreError::InvalidInput(message) | FileStoreError::AlreadyExists(message) => {
            SpendingError::InvalidInput { message }.into()
        }
        FileStoreError::NotFound(id) => SpendingError::NotFound {
            entity: "Attachment",
            id,
        }
        .into(),
        FileStoreError::Backend(error) => error,
    }
}

struct DomainError(anyhow::Error);

impl From<FileStoreError> for DomainError {
    fn from(error: FileStoreError) -> Self {
        Self(domain(error))
    }
}

fn storage_of(stored: StoredFile) -> AttachmentStorage {
    AttachmentStorage {
        backend: stored.backend,
        key: stored.key.as_str().to_owned(),
        preview_key: stored
            .preview_key
            .as_ref()
            .map(|key| key.as_str().to_owned())
            .unwrap_or_default(),
    }
}

fn prepare(filename: &str, bytes: &[u8]) -> Result<(Uuid, BlobKey, BlobKey)> {
    validate_filename(filename).map_err(domain)?;
    if bytes.is_empty() || bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(invalid("Attachment must be between 1 byte and 20 MiB"));
    }
    let id = Uuid::new_v4();
    Ok((
        id,
        BlobKey::mint(id, "original").map_err(domain)?,
        BlobKey::mint(id, "webp").map_err(domain)?,
    ))
}

fn file_ref(storage: &AttachmentStorage) -> Result<StoredFileRef> {
    Ok(StoredFileRef {
        backend: storage.backend.clone(),
        key: BlobKey::parse(&storage.key).map_err(domain)?,
        preview_key: Some(BlobKey::parse(&storage.preview_key).map_err(domain)?),
    })
}

impl TransactionAttachmentFiles {
    pub fn new(root: PathBuf, attachments: Arc<TransactionAttachmentsService>) -> Result<Self> {
        let store = LocalBlobStore::new(root).map_err(domain)?;
        Ok(Self {
            files: Arc::new(StoredFileService::new(Arc::new(store), 2)),
            attachments,
        })
    }

    pub async fn upload_slot(&self) -> Result<OwnedSemaphorePermit> {
        self.files.render_slot().await.map_err(domain)
    }

    pub async fn upload(
        &self,
        activity_id: &str,
        filename: &str,
        content_type: &str,
        bytes: Vec<u8>,
        slot: OwnedSemaphorePermit,
    ) -> Result<TransactionAttachment> {
        let attachments = self.attachments.list(activity_id).await?;
        if attachments.len() >= crate::transaction_attachments::MAX_ATTACHMENTS {
            return Err(invalid("Maximum 10 attachments per transaction"));
        }
        let (id, key, preview_key) = prepare(filename, &bytes)?;
        let attachment = TransactionAttachment {
            id: id.to_string(),
            activity_id: activity_id.into(),
            filename: filename.into(),
            content_type: content_type.into(),
            size_bytes: bytes.len() as i64,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let row = attachment.clone();
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
                move |stored| async move {
                    self.attachments
                        .add_attachment(StoredAttachment {
                            attachment: row,
                            storage: storage_of(stored),
                        })
                        .await
                        .map_err(DomainError)
                },
            )
            .await
            .map_err(|DomainError(error)| error)?;
        Ok(attachment)
    }

    pub async fn upload_draft(
        &self,
        token: &str,
        filename: &str,
        content_type: &str,
        bytes: Vec<u8>,
        slot: OwnedSemaphorePermit,
    ) -> Result<AttachmentDraft> {
        let (id, key, preview_key) = prepare(filename, &bytes)?;
        let draft = AttachmentDraft {
            id: id.to_string(),
            token: token.into(),
            filename: filename.into(),
            content_type: content_type.into(),
            size_bytes: bytes.len() as i64,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let row = draft.clone();
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
                move |stored| async move {
                    self.attachments
                        .add_draft(StoredDraft {
                            draft: row,
                            storage: storage_of(stored),
                        })
                        .await
                        .map_err(DomainError)
                },
            )
            .await
            .map_err(|DomainError(error)| error)?;
        Ok(draft)
    }

    pub async fn read_draft(
        &self,
        token: &str,
        id: &str,
        preview: bool,
    ) -> Result<(AttachmentDraft, Vec<u8>)> {
        let stored = self.attachments.stored_draft(token, id).await?;
        let bytes = self
            .files
            .read(&file_ref(&stored.storage)?, preview)
            .await
            .map_err(domain)?;
        Ok((stored.draft, bytes))
    }

    pub async fn delete_draft(&self, token: &str, id: &str) -> Result<()> {
        // Read the location before the row that holds it goes away.
        let stored = self.attachments.stored_draft(token, id).await?;
        self.attachments.delete_draft(token, id).await?;
        self.remove_all(std::slice::from_ref(&stored.storage)).await
    }

    pub async fn discard_drafts(&self, token: &str) -> Result<()> {
        let storage = self.attachments.discard_drafts(token).await?;
        self.remove_all(&storage).await
    }

    pub async fn sweep_drafts(&self) -> Result<usize> {
        let storage = self.attachments.sweep_drafts().await?;
        let swept = storage.len();
        self.remove_all(&storage).await?;
        Ok(swept)
    }

    pub async fn claim_drafts(&self, token: &str, activity_id: &str) -> Result<usize> {
        self.attachments.claim_drafts(token, activity_id).await
    }

    pub async fn reorder(&self, activity_id: &str, ids: &[String]) -> Result<()> {
        self.attachments.reorder(activity_id, ids).await
    }

    pub async fn read(
        &self,
        activity_id: &str,
        id: &str,
        preview: bool,
    ) -> Result<(TransactionAttachment, Vec<u8>)> {
        let stored = self.attachments.stored(activity_id, id).await?;
        let bytes = self
            .files
            .read(&file_ref(&stored.storage)?, preview)
            .await
            .map_err(domain)?;
        Ok((stored.attachment, bytes))
    }

    pub async fn delete(&self, activity_id: &str, id: &str) -> Result<()> {
        // Read the location before the row that holds it goes away.
        let stored = self.attachments.stored(activity_id, id).await?;
        self.attachments.delete_attachment(activity_id, id).await?;
        self.remove_all(std::slice::from_ref(&stored.storage)).await
    }

    pub async fn remove_all(&self, storage: &[AttachmentStorage]) -> Result<()> {
        let files = storage.iter().map(file_ref).collect::<Result<Vec<_>>>()?;
        self.files.remove(&files).await.map_err(domain)
    }
}
