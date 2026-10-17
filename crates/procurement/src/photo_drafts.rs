use crate::items::ItemPhoto;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoDraft {
    pub id: String,
    pub photo: ItemPhoto,
    pub created_at: String,
}

#[async_trait]
pub trait PhotoDraftsRepositoryTrait: Send + Sync {
    async fn insert(&self, draft: PhotoDraft) -> Result<()>;
    async fn take(&self, id: &str) -> Result<Option<PhotoDraft>>;
    fn older_than(&self, cutoff: &str) -> Result<Vec<String>>;
}
