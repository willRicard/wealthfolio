use super::{Item, ItemPhoto, SaveItem};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait ItemsRepositoryTrait: Send + Sync {
    fn list(&self) -> Result<Vec<Item>>;
    fn get(&self, id: &str) -> Result<Option<Item>>;
    async fn create(&self, input: SaveItem) -> Result<Item>;
    async fn update(&self, id: &str, input: SaveItem) -> Result<(Item, Option<ItemPhoto>)>;
    async fn delete(&self, id: &str) -> Result<Option<ItemPhoto>>;
}
