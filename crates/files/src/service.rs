//! Storing a file together with its generated preview.

use crate::error::{invalid, FileStoreError, Result};
use crate::key::BlobKey;
use crate::store::{BlobStore, LOCAL_BACKEND_ID, S3_BACKEND_ID};
use crate::thumbnail::thumbnail;
use std::future::Future;
use std::path::PathBuf;
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

pub fn stored_filename(filename: &str, content_type: &str) -> String {
    if content_type == "image/webp" {
        let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
        let mut end = stem.len().min(175);
        while !stem.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}.webp", &stem[..end])
    } else {
        filename.to_owned()
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
    read_stores: Vec<Arc<dyn BlobStore>>,
    render_slots: Arc<Semaphore>,
}

impl StoredFileService {
    pub fn new(store: Arc<dyn BlobStore>, render_concurrency: usize) -> Self {
        Self {
            store,
            read_stores: Vec::new(),
            render_slots: Arc::new(Semaphore::new(render_concurrency)),
        }
    }

    pub fn from_env(root: PathBuf, render_concurrency: usize) -> Result<Self> {
        let local: Arc<dyn BlobStore> = Arc::new(crate::LocalBlobStore::new(root)?);
        let configured = ["WF_S3_URL", "WF_S3_BUCKET", "WF_S3_ACCESS", "WF_S3_SECRET"]
            .iter()
            .any(|name| std::env::var(name).is_ok_and(|value| !value.is_empty()));
        let backend = std::env::var("WF_FILE_STORAGE_BACKEND")
            .ok()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                if configured {
                    S3_BACKEND_ID
                } else {
                    LOCAL_BACKEND_ID
                }
                .into()
            });
        if !matches!(backend.as_str(), S3_BACKEND_ID | LOCAL_BACKEND_ID) {
            return Err(invalid("WF_FILE_STORAGE_BACKEND must be s3 or local"));
        }
        let s3: Option<Arc<dyn BlobStore>> = if configured || backend == S3_BACKEND_ID {
            Some(Arc::new(crate::S3BlobStore::new(
                crate::S3Config::from_env()?,
            )?))
        } else {
            None
        };
        let mut service = Self::new(local.clone(), render_concurrency);
        if let Some(s3) = s3 {
            if backend == S3_BACKEND_ID {
                service.store = s3;
                service.read_stores.push(local);
            } else {
                service.read_stores.push(s3);
            }
        }
        Ok(service)
    }

    fn backend(&self, id: &str) -> Result<&dyn BlobStore> {
        if self.store.backend_id() == id {
            return Ok(self.store.as_ref());
        }
        self.read_stores
            .iter()
            .find(|store| store.backend_id() == id)
            .map(|store| store.as_ref())
            .ok_or_else(|| invalid("Stored file backend is not configured"))
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
        let render_type = content_type.clone();

        let rendered = tokio::task::spawn_blocking(
            move || -> Result<(Vec<u8>, Vec<u8>, OwnedSemaphorePermit)> {
                let preview = thumbnail(&bytes, &render_type)?;
                let bytes = if render_type.starts_with("image/") {
                    crate::thumbnail::optimize_image(&bytes)?
                } else {
                    bytes
                };
                Ok((bytes, preview, slot))
            },
        )
        .await
        .map_err(FileStoreError::from)
        .map_err(E::from)?;
        let (bytes, preview, slot) = rendered.map_err(E::from)?;
        let content_type = if content_type.starts_with("image/") {
            "image/webp".to_owned()
        } else {
            content_type
        };

        let stored = StoredFile {
            backend: self.store.backend_id().to_owned(),
            key: key.clone(),
            preview_key: Some(preview_key.clone()),
            content_type: content_type.clone(),
            size_bytes: bytes.len() as i64,
        };

        self.store
            .put(&key, bytes, &content_type)
            .await
            .map_err(E::from)?;
        let written = self.store.put(&preview_key, preview, "image/webp").await;
        drop(slot);

        if let Err(error) = written {
            self.store.delete(&key).await.ok();
            return Err(E::from(error));
        }

        let result = commit(stored.clone()).await;
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
        self.backend(&file.backend)?.get(key).await
    }

    pub async fn remove(&self, files: &[StoredFileRef]) -> Result<()> {
        let mut first_error = None;
        for file in files {
            let store = match self.backend(&file.backend) {
                Ok(store) => store,
                Err(error) => {
                    first_error.get_or_insert(error);
                    continue;
                }
            };
            for key in std::iter::once(&file.key).chain(file.preview_key.as_ref()) {
                if let Err(error) = store.delete(key).await {
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

    #[test]
    fn converted_filenames_match_the_content_and_stay_bounded() {
        assert_eq!(stored_filename("receipt.png", "image/webp"), "receipt.webp");
        assert_eq!(stored_filename("receipt", "image/webp"), "receipt.webp");
        assert_eq!(
            stored_filename("receipt.pdf", "application/pdf"),
            "receipt.pdf"
        );
        let filename = stored_filename(&format!("{}.jpg", "領".repeat(58)), "image/webp");
        validate_filename(&filename).unwrap();
    }

    #[test]
    fn backend_selection_is_explicit_and_configuration_is_validated_without_requests() {
        for (backend, configured, expected) in [
            ("", false, "local"),
            ("", true, "s3"),
            ("local", true, "local"),
            ("s3", false, "error"),
            ("invalid", false, "error"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "service::tests::backend_selection_worker"])
                .env_remove("WF_S3_URL")
                .env_remove("WF_S3_BUCKET")
                .env_remove("WF_S3_ACCESS")
                .env_remove("WF_S3_SECRET")
                .env_remove("WF_S3_REGION")
                .env("WF_FILE_STORAGE_BACKEND", backend)
                .env("WF_STORAGE_EXPECTED", expected)
                .env("WF_STORAGE_TEST_ROOT", dir.path());
            if configured {
                command
                    .env("WF_S3_URL", "https://s3.eu-west-par.io.cloud.ovh.net")
                    .env("WF_S3_BUCKET", "test-bucket")
                    .env("WF_S3_ACCESS", "test-access")
                    .env("WF_S3_SECRET", "test-secret");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }

    #[test]
    fn backend_selection_worker() {
        let Ok(expected) = std::env::var("WF_STORAGE_EXPECTED") else {
            return;
        };
        let root = PathBuf::from(std::env::var_os("WF_STORAGE_TEST_ROOT").unwrap());
        let result = StoredFileService::from_env(root, 2);
        if expected == "error" {
            assert!(result.is_err());
        } else {
            let service = result.unwrap();
            assert_eq!(service.store.backend_id(), expected);
            assert_eq!(service.backend("local").unwrap().backend_id(), "local");
            assert!(service.backend("unknown").is_err());
        }
    }

    #[tokio::test]
    async fn recorded_backend_routes_reads_and_deletes_without_touching_other_stores() {
        struct OtherStore(LocalBlobStore);
        #[async_trait::async_trait]
        impl BlobStore for OtherStore {
            fn backend_id(&self) -> &'static str {
                "other"
            }
            async fn put(&self, key: &BlobKey, bytes: Vec<u8>, content_type: &str) -> Result<()> {
                self.0.put(key, bytes, content_type).await
            }
            async fn get(&self, key: &BlobKey) -> Result<Vec<u8>> {
                self.0.get(key).await
            }
            async fn delete(&self, key: &BlobKey) -> Result<()> {
                self.0.delete(key).await
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let local = Arc::new(LocalBlobStore::new(dir.path().join("local")).unwrap());
        let other = Arc::new(OtherStore(
            LocalBlobStore::new(dir.path().join("other")).unwrap(),
        ));
        let key = BlobKey::parse("ab/test.original").unwrap();
        local
            .put(&key, b"local".to_vec(), "text/plain")
            .await
            .unwrap();
        other
            .put(&key, b"other".to_vec(), "text/plain")
            .await
            .unwrap();
        let mut service = StoredFileService::new(other.clone(), 2);
        service.read_stores.push(local.clone());
        let file = StoredFileRef {
            backend: "local".into(),
            key: key.clone(),
            preview_key: None,
        };
        assert_eq!(service.read(&file, false).await.unwrap(), b"local");
        service.remove(&[file]).await.unwrap();
        assert!(local.get(&key).await.is_err());
        assert_eq!(other.get(&key).await.unwrap(), b"other");
        let unknown = StoredFileRef {
            backend: "unknown".into(),
            key: key.clone(),
            preview_key: None,
        };
        assert!(service.read(&unknown, false).await.is_err());
        assert!(service.remove(&[unknown]).await.is_err());
        assert_eq!(other.get(&key).await.unwrap(), b"other");
    }

    #[tokio::test]
    async fn rejected_put_does_not_delete_existing_objects() {
        let (service, _dir) = service();
        let (key, preview_key, bytes) = request(png());
        service
            .store
            .put(&key, b"existing".to_vec(), "text/plain")
            .await
            .unwrap();
        let result = service
            .store_with_preview(
                StoreRequest {
                    key: key.clone(),
                    preview_key,
                    filename: "receipt.png",
                    content_type: "image/png",
                    bytes,
                },
                service.render_slot().await.unwrap(),
                |_| async { Ok::<(), FileStoreError>(()) },
            )
            .await;
        assert!(matches!(result, Err(FileStoreError::AlreadyExists(_))));
        assert_eq!(service.store.get(&key).await.unwrap(), b"existing");
    }

    #[tokio::test]
    async fn rejected_preview_cleans_up_only_the_new_original() {
        let (service, _dir) = service();
        let (key, preview_key, bytes) = request(png());
        service
            .store
            .put(&preview_key, b"existing preview".to_vec(), "image/webp")
            .await
            .unwrap();
        let result = service
            .store_with_preview(
                StoreRequest {
                    key: key.clone(),
                    preview_key: preview_key.clone(),
                    filename: "receipt.png",
                    content_type: "image/png",
                    bytes,
                },
                service.render_slot().await.unwrap(),
                |_| async { Ok::<(), FileStoreError>(()) },
            )
            .await;
        assert!(matches!(result, Err(FileStoreError::AlreadyExists(_))));
        assert!(service.store.get(&key).await.is_err());
        assert_eq!(
            service.store.get(&preview_key).await.unwrap(),
            b"existing preview"
        );
    }

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

        assert_eq!(stored.content_type, "image/webp");
        assert_eq!(stored.backend, "local");
        let reference = StoredFileRef::from(&stored);
        let saved = service.read(&reference, false).await.expect("saved image");
        assert_eq!(stored.size_bytes, saved.len() as i64);
        assert_eq!(
            image::guess_format(&saved).expect("format"),
            ImageFormat::WebP
        );
        assert_eq!(image::load_from_memory(&saved).expect("decode").width(), 40);
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
