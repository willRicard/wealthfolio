//! Storing a file together with its generated preview.

use crate::error::{invalid, FileStoreError, Result};
use crate::key::BlobKey;
use crate::store::BlobStore;
use crate::thumbnail::thumbnail;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub struct StoreRequest<'a> {
    pub key: BlobKey,
    pub preview_key: BlobKey,
    pub filename: &'a str,
    pub content_type: &'a str,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredFile {
    pub backend: String,
    pub key: BlobKey,
    pub preview_key: Option<BlobKey>,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredFileRef {
    pub backend: String,
    pub key: BlobKey,
    pub preview_key: Option<BlobKey>,
}

impl From<&StoredFile> for StoredFileRef {
    fn from(file: &StoredFile) -> Self {
        Self {
            backend: file.backend.clone(),
            key: file.key.clone(),
            preview_key: file.preview_key.clone(),
        }
    }
}

/// Rejects names that are unsafe to echo back or to write to a filesystem.
pub fn validate_filename(filename: &str) -> Result<()> {
    if filename.is_empty()
        || filename.len() > 180
        || filename
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
        || filename == "."
        || filename == ".."
    {
        return Err(invalid("Invalid attachment filename (maximum 180 bytes)"));
    }
    Ok(())
}

pub struct StoredFileService {
    store: Arc<dyn BlobStore>,
    render_slots: Arc<Semaphore>,
}

impl StoredFileService {
    pub fn new(store: Arc<dyn BlobStore>, render_concurrency: usize) -> Self {
        Self {
            store,
            render_slots: Arc::new(Semaphore::new(render_concurrency)),
        }
    }

    pub async fn render_slot(&self) -> Result<OwnedSemaphorePermit> {
        self.render_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| {
                FileStoreError::Backend(anyhow::Error::new(e).context("File uploads unavailable"))
            })
    }

    pub async fn store_with_preview<F, Fut, E>(
        &self,
        request: StoreRequest<'_>,
        slot: OwnedSemaphorePermit,
        commit: F,
    ) -> std::result::Result<StoredFile, E>
    where
        E: From<FileStoreError>,
        F: FnOnce(StoredFile) -> Fut + Send,
        Fut: Future<Output = std::result::Result<(), E>> + Send,
    {
        validate_filename(request.filename).map_err(E::from)?;

        let StoreRequest {
            key,
            preview_key,
            content_type,
            bytes,
            ..
        } = request;
        let content_type = content_type.to_owned();
        let size_bytes = bytes.len() as i64;
        let render_type = content_type.clone();

        let rendered = tokio::task::spawn_blocking(
            move || -> Result<(Vec<u8>, Vec<u8>, OwnedSemaphorePermit)> {
                let preview = thumbnail(&bytes, &render_type)?;
                Ok((bytes, preview, slot))
            },
        )
        .await
        .map_err(FileStoreError::from)
        .map_err(E::from)?;
        let (bytes, preview, slot) = rendered.map_err(E::from)?;

        let stored = StoredFile {
            backend: self.store.backend_id().to_owned(),
            key: key.clone(),
            preview_key: Some(preview_key.clone()),
            content_type: content_type.clone(),
            size_bytes,
        };

        let written = async {
            self.store.put(&key, bytes, &content_type).await?;
            self.store.put(&preview_key, preview, "image/webp").await
        }
        .await;
        drop(slot);

        let result = match written {
            Ok(()) => commit(stored.clone()).await,
            Err(e) => Err(E::from(e)),
        };
        if let Err(error) = result {
            self.remove(std::slice::from_ref(&StoredFileRef::from(&stored)))
                .await
                .ok();
            return Err(error);
        }
        Ok(stored)
    }

    pub async fn read(&self, file: &StoredFileRef, preview: bool) -> Result<Vec<u8>> {
        let key = if preview {
            file.preview_key
                .as_ref()
                .ok_or_else(|| invalid("No preview available"))?
        } else {
            &file.key
        };
        self.store.get(key).await
    }

    pub async fn remove(&self, files: &[StoredFileRef]) -> Result<()> {
        let mut first_error = None;
        for file in files {
            for key in std::iter::once(&file.key).chain(file.preview_key.as_ref()) {
                if let Err(error) = self.store.delete(key).await {
                    first_error.get_or_insert(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalBlobStore;
    use image::ImageFormat;
    use std::io::Cursor;

    fn png() -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(40, 20)
            .write_to(&mut bytes, ImageFormat::Png)
            .expect("encode png");
        bytes.into_inner()
    }

    fn service() -> (StoredFileService, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = LocalBlobStore::new(dir.path().join("objects")).expect("store");
        (StoredFileService::new(Arc::new(store), 2), dir)
    }

    fn request(bytes: Vec<u8>) -> (BlobKey, BlobKey, Vec<u8>) {
        let id = uuid::Uuid::new_v4();
        (
            BlobKey::mint(id, "original").expect("key"),
            BlobKey::mint(id, "webp").expect("preview key"),
            bytes,
        )
    }

    #[tokio::test]
    async fn stores_original_and_preview() {
        let (service, _dir) = service();
        let (key, preview_key, bytes) = request(png());
        let slot = service.render_slot().await.expect("slot");

        let stored = service
            .store_with_preview(
                StoreRequest {
                    key: key.clone(),
                    preview_key: preview_key.clone(),
                    filename: "receipt.png",
                    content_type: "image/png",
                    bytes: bytes.clone(),
                },
                slot,
                |_| async { Ok::<(), FileStoreError>(()) },
            )
            .await
            .expect("store");

        assert_eq!(stored.size_bytes, bytes.len() as i64);
        assert_eq!(stored.backend, "local");
        let reference = StoredFileRef::from(&stored);
        assert_eq!(
            service.read(&reference, false).await.expect("original"),
            bytes
        );
        let preview = service.read(&reference, true).await.expect("preview");
        assert_eq!(
            image::guess_format(&preview).expect("format"),
            ImageFormat::WebP
        );
    }

    #[tokio::test]
    async fn a_failed_commit_removes_both_objects() {
        let (service, _dir) = service();
        let (key, preview_key, bytes) = request(png());
        let slot = service.render_slot().await.expect("slot");

        let result = service
            .store_with_preview(
                StoreRequest {
                    key: key.clone(),
                    preview_key: preview_key.clone(),
                    filename: "receipt.png",
                    content_type: "image/png",
                    bytes,
                },
                slot,
                |_| async { Err::<(), FileStoreError>(invalid("row rejected")) },
            )
            .await;

        assert!(result.is_err());
        let orphan = StoredFileRef {
            backend: "local".into(),
            key,
            preview_key: Some(preview_key),
        };
        assert!(service.read(&orphan, false).await.is_err());
        assert!(service.read(&orphan, true).await.is_err());
    }

    #[tokio::test]
    async fn a_failed_render_stores_nothing() {
        let (service, _dir) = service();
        let (key, preview_key, _) = request(Vec::new());
        let slot = service.render_slot().await.expect("slot");

        let result = service
            .store_with_preview(
                StoreRequest {
                    key: key.clone(),
                    preview_key: preview_key.clone(),
                    filename: "receipt.png",
                    content_type: "image/png",
                    bytes: b"not a png".to_vec(),
                },
                slot,
                |_| async { Ok::<(), FileStoreError>(()) },
            )
            .await;

        assert!(matches!(result, Err(FileStoreError::InvalidInput(_))));
        let orphan = StoredFileRef {
            backend: "local".into(),
            key,
            preview_key: Some(preview_key),
        };
        assert!(service.read(&orphan, false).await.is_err());
    }

    #[tokio::test]
    async fn an_unsafe_filename_is_refused() {
        let (service, _dir) = service();
        let (key, preview_key, bytes) = request(png());
        let slot = service.render_slot().await.expect("slot");

        let result = service
            .store_with_preview(
                StoreRequest {
                    key,
                    preview_key,
                    filename: "../escape.png",
                    content_type: "image/png",
                    bytes,
                },
                slot,
                |_| async { Ok::<(), FileStoreError>(()) },
            )
            .await;

        assert!(matches!(result, Err(FileStoreError::InvalidInput(_))));
    }

    #[test]
    fn filenames_are_checked() {
        for name in [
            "../receipt.png",
            "a/b.png",
            "a\\b.png",
            "\r\nreceipt.png",
            "",
            "..",
        ] {
            assert!(validate_filename(name).is_err());
        }
        assert!(validate_filename("領収書.png").is_ok());
    }
}
