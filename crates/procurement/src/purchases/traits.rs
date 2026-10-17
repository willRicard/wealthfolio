use super::{Purchase, PurchasePrice, SavePurchase};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait PurchasesRepositoryTrait: Send + Sync {
    /// All joined rows in one query when item_id is None.
    fn list(&self, item_id: Option<&str>) -> Result<Vec<PurchasePrice>>;
    async fn create(&self, input: SavePurchase) -> Result<Purchase>;
    async fn update(&self, input: SavePurchase) -> Result<Purchase>;
    async fn delete(&self, activity_id: &str, item_id: &str) -> Result<()>;
}
