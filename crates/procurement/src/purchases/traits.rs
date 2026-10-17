use super::{Purchase, PurchasePrice, SavePurchase};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait PurchasesRepositoryTrait: Send + Sync {
    /// All joined rows in one query when item_id is None.
    fn list(&self, item_id: Option<&str>) -> Result<Vec<PurchasePrice>>;
    fn list_for_activity(&self, activity_id: &str) -> Result<Vec<Purchase>>;
    async fn set_for_activity(
        &self,
        activity_id: &str,
        purchases: Vec<SavePurchase>,
    ) -> Result<Vec<Purchase>>;
}
