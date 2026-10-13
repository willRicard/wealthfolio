//! The storage backend seam.

use crate::error::Result;
use crate::key::BlobKey;
use async_trait::async_trait;

#[async_trait]
pub trait BlobStore: Send + Sync {
    fn backend_id(&self) -> &'static str;

    async fn put(&self, key: &BlobKey, bytes: Vec<u8>, content_type: &str) -> Result<()>;

    async fn get(&self, key: &BlobKey) -> Result<Vec<u8>>;

    /// Idempotent: deleting an object that is not there succeeds.
    async fn delete(&self, key: &BlobKey) -> Result<()>;
}
