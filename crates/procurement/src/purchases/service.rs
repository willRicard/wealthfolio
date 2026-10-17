use super::{stats::fold_prices, ItemPrices, Purchase, PurchasesRepositoryTrait, SavePurchase};
use crate::{items::ItemsRepositoryTrait, ProcurementError};
use anyhow::Result;
use std::sync::Arc;
use wealthfolio_core::{
    activities::ActivityRepositoryTrait, fx::FxServiceTrait, settings::SettingsServiceTrait,
};

pub struct PurchasesService {
    repository: Arc<dyn PurchasesRepositoryTrait>,
    items: Arc<dyn ItemsRepositoryTrait>,
    activities: Arc<dyn ActivityRepositoryTrait>,
    fx: Arc<dyn FxServiceTrait>,
    settings: Arc<dyn SettingsServiceTrait>,
}

impl PurchasesService {
    pub fn new(
        repository: Arc<dyn PurchasesRepositoryTrait>,
        items: Arc<dyn ItemsRepositoryTrait>,
        activities: Arc<dyn ActivityRepositoryTrait>,
        fx: Arc<dyn FxServiceTrait>,
        settings: Arc<dyn SettingsServiceTrait>,
    ) -> Self {
        Self {
            repository,
            items,
            activities,
            fx,
            settings,
        }
    }
    pub fn list_for_item(&self, item_id: &str) -> Result<ItemPrices> {
        self.require_item(item_id)?;
        let base = self.base_currency()?;
        let (prices, stats) = fold_prices(
            self.repository.list(Some(item_id))?,
            &base,
            |amount, currency, date| {
                self.fx
                    .convert_currency_for_date(amount, currency, &base, date)
                    .ok()
            },
        )?;
        Ok(ItemPrices { prices, stats })
    }
    pub fn list_for_activity(&self, activity_id: &str) -> Result<Vec<Purchase>> {
        self.repository.list_for_activity(activity_id)
    }
    pub async fn set_for_activity(
        &self,
        activity_id: &str,
        purchases: Vec<SavePurchase>,
    ) -> Result<Vec<Purchase>> {
        let mut seen = std::collections::HashSet::new();
        for input in &purchases {
            if input.activity_id != activity_id {
                return Err(ProcurementError::InvalidInput(
                    "Purchase identifiers must match the activity".into(),
                )
                .into());
            }
            if !seen.insert(input.item_id.as_str()) {
                return Err(ProcurementError::InvalidInput(
                    "An item can only be purchased once per activity".into(),
                )
                .into());
            }
            validate_amounts(input)?;
            self.require_item(&input.item_id)?;
        }
        if !purchases.is_empty() {
            let activity = self.activities.get_activity(activity_id)?;
            self.fx
                .ensure_fx_pairs(vec![(activity.currency, self.base_currency()?)])
                .await?;
        }
        self.repository
            .set_for_activity(activity_id, purchases)
            .await
    }
    fn require_item(&self, id: &str) -> Result<()> {
        if self.items.get(id)?.is_none() {
            return Err(ProcurementError::NotFound {
                entity: "Item",
                id: id.into(),
            }
            .into());
        }
        Ok(())
    }
    fn base_currency(&self) -> Result<String> {
        self.settings.get_base_currency()?.ok_or_else(|| {
            ProcurementError::InvalidInput("Base currency is not configured".into()).into()
        })
    }
}

pub fn validate_amounts(input: &SavePurchase) -> Result<()> {
    if input.quantity <= rust_decimal::Decimal::ZERO
        || input.unit_price < rust_decimal::Decimal::ZERO
    {
        return Err(ProcurementError::InvalidInput(
            "Quantity must be positive and unit price nonnegative".into(),
        )
        .into());
    }
    Ok(())
}
