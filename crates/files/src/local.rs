//! Local-filesystem backend.
//!
//! Objects are private to the account running the process: the root and every
//! directory below it are `0700`, every file is `0600`, writes refuse to
//! clobber, and nothing is read through a symlink.

use crate::error::{invalid, FileStoreError, Result};
use crate::key::BlobKey;
use crate::store::{BlobStore, LOCAL_BACKEND_ID};
use async_trait::async_trait;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct LocalBlobStore {
    root: PathBuf,
}

fn private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn private_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    if let Err(error) = file.write_all(bytes) {
        drop(file);
        std::fs::remove_file(path).ok();
        return Err(error);
    }
    Ok(())
}

impl LocalBlobStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        private_dir(&root)?;
        if std::fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err(FileStoreError::Backend(anyhow::anyhow!(
                "Storage root must not be a symlink"
            )));
        }
        Ok(Self { root })
    }

    fn path(&self, key: &BlobKey) -> PathBuf {
        // `BlobKey` has already rejected absolute paths, backslashes, and every
        // `.`/`..` segment, so this join cannot escape the root.
        self.root.join(key.as_str())
    }
}

#[async_trait]
impl BlobStore for LocalBlobStore {
    fn backend_id(&self) -> &'static str {
        LOCAL_BACKEND_ID
    }

    async fn put(&self, key: &BlobKey, bytes: Vec<u8>, _content_type: &str) -> Result<()> {
        let path = self.path(key);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let parent = path
                .parent()
                .ok_or_else(|| invalid("Invalid storage key"))?;
            private_dir(parent)?;
            if std::fs::symlink_metadata(parent)?.file_type().is_symlink() {
                return Err(invalid("Invalid storage directory"));
            }
            match private_write(&path, &bytes) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(
                    FileStoreError::AlreadyExists("Storage key already in use".into()),
                ),
                Err(e) => Err(e.into()),
            }
        })
        .await?
    }

    async fn get(&self, key: &BlobKey) -> Result<Vec<u8>> {
        let path = self.path(key);
        if !tokio::fs::symlink_metadata(&path)
            .await?
            .file_type()
            .is_file()
        {
            return Err(invalid("Invalid stored file"));
        }
        Ok(tokio::fs::read(path).await?)
    }

    async fn delete(&self, key: &BlobKey) -> Result<()> {
        let path = self.path(key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        // `remove_dir` only succeeds on an empty directory, so this prunes a
        // per-owner directory once its last file is gone.
        if let Some(parent) = path.parent() {
            if parent != self.root {
                tokio::fs::remove_dir(parent).await.ok();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (LocalBlobStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = LocalBlobStore::new(dir.path().join("objects")).expect("store");
        (store, dir)
    }

    #[tokio::test]
    async fn round_trips_and_refuses_to_clobber() {
        let (store, _dir) = store();
        let key = BlobKey::parse("ab/receipt.original").expect("key");

        store
            .put(&key, b"one".to_vec(), "image/png")
            .await
            .expect("put");
        assert_eq!(store.get(&key).await.expect("get"), b"one");

        let second = store.put(&key, b"two".to_vec(), "image/png").await;
        assert!(matches!(second, Err(FileStoreError::AlreadyExists(_))));
        assert_eq!(store.get(&key).await.expect("get"), b"one");
    }

    #[tokio::test]
    async fn delete_is_idempotent() {
        let (store, _dir) = store();
        let key = BlobKey::parse("ab/gone.original").expect("key");
        store
            .delete(&key)
            .await
            .expect("missing object deletes cleanly");
        store
            .put(&key, b"x".to_vec(), "image/png")
            .await
            .expect("put");
        store.delete(&key).await.expect("delete");
        assert!(store.get(&key).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn objects_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (store, _dir) = store();
        let key = BlobKey::parse("ab/private.original").expect("key");
        store
            .put(&key, b"x".to_vec(), "image/png")
            .await
            .expect("put");

        let file = std::fs::metadata(store.path(&key)).expect("file metadata");
        assert_eq!(file.permissions().mode() & 0o777, 0o600);
        let dir =
            std::fs::metadata(store.path(&key).parent().expect("parent")).expect("dir metadata");
        assert_eq!(dir.permissions().mode() & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinked_objects_are_not_read() {
        let (store, dir) = store();
        let secret = dir.path().join("secret");
        std::fs::write(&secret, b"secret").expect("write secret");

        let key = BlobKey::parse("ab/link.original").expect("key");
        let path = store.path(&key);
        private_dir(path.parent().expect("parent")).expect("dir");
        std::os::unix::fs::symlink(&secret, &path).expect("symlink");

        assert!(store.get(&key).await.is_err());
    }
}
