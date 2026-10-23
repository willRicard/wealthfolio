use super::{Item, ItemWithStats, ItemsRepositoryTrait, SaveItem};
use crate::{
    purchases::{stats::fold_prices, PurchasesRepositoryTrait},
    ProcurementError,
};
use anyhow::Result;
use std::{collections::HashMap, sync::Arc};
use wealthfolio_core::{fx::FxServiceTrait, settings::SettingsServiceTrait};

pub struct ItemsService {
    repository: Arc<dyn ItemsRepositoryTrait>,
    purchases: Arc<dyn PurchasesRepositoryTrait>,
    fx: Arc<dyn FxServiceTrait>,
    settings: Arc<dyn SettingsServiceTrait>,
    #[cfg(feature = "photos")]
    photos: Option<Arc<crate::photos::PhotoService>>,
}

impl ItemsService {
    pub fn new(
        repository: Arc<dyn ItemsRepositoryTrait>,
        purchases: Arc<dyn PurchasesRepositoryTrait>,
        fx: Arc<dyn FxServiceTrait>,
        settings: Arc<dyn SettingsServiceTrait>,
    ) -> Self {
        Self {
            repository,
            purchases,
            fx,
            settings,
            #[cfg(feature = "photos")]
            photos: None,
        }
    }

    #[cfg(feature = "photos")]
    pub fn with_photos(mut self, photos: Arc<crate::photos::PhotoService>) -> Self {
        self.photos = Some(photos);
        self
    }

    async fn remove_photo(&self, photo: Option<super::ItemPhoto>) {
        #[cfg(feature = "photos")]
        if let (Some(files), Some(photo)) = (&self.photos, photo) {
            if files.remove(&photo).await.is_err() {
                log::warn!("Failed to remove a replaced procurement photo");
            }
        }
        #[cfg(not(feature = "photos"))]
        let _ = photo;
    }

    pub fn list_with_stats(&self) -> Result<Vec<ItemWithStats>> {
        let base = self.settings.get_base_currency()?.ok_or_else(|| {
            ProcurementError::InvalidInput("Base currency is not configured".into())
        })?;
        let mut groups = HashMap::<String, Vec<_>>::new();
        for purchase in self.purchases.list(None)? {
            groups
                .entry(purchase.item_id.clone())
                .or_default()
                .push(purchase);
        }
        self.repository
            .list()?
            .into_iter()
            .map(|item| {
                let prices = groups.remove(&item.id).unwrap_or_default();
                let (_, stats) = fold_prices(prices, &base, |amount, currency, date| {
                    self.fx
                        .convert_currency_for_date(amount, currency, &base, date)
                        .ok()
                })?;
                Ok(ItemWithStats { item, stats })
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<Item> {
        self.repository.get(id)?.ok_or_else(|| {
            ProcurementError::NotFound {
                entity: "Item",
                id: id.into(),
            }
            .into()
        })
    }

    pub async fn create(&self, input: SaveItem) -> Result<Item> {
        self.repository.create(validate(input)?).await
    }

    pub async fn update(&self, id: &str, input: SaveItem) -> Result<Item> {
        let (item, previous_photo) = self.repository.update(id, validate(input)?).await?;
        self.remove_photo(previous_photo).await;
        Ok(item)
    }

    pub async fn clone_item(&self, id: &str) -> Result<Item> {
        let item = self.get(id)?;
        self.create(SaveItem {
            name: item.name,
            brand: item.brand,
            memo: item.memo,
            category_id: item.category_id,
            subcategory_id: item.subcategory_id,
            rating: item.rating,
            unit: item.unit,
            tax_type: item.tax_type,
            aliases: item.aliases,
            draft_id: None,
        })
        .await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let photo = self.repository.delete(id).await?;
        self.remove_photo(photo).await;
        Ok(())
    }
}

pub fn validate(mut input: SaveItem) -> Result<SaveItem> {
    input.name = input.name.trim().to_string();
    if input.name.is_empty() {
        return Err(ProcurementError::InvalidInput("Name is required".into()).into());
    }
    if input
        .rating
        .is_some_and(|r| !r.is_finite() || !(0.0..=5.0).contains(&r) || r != r.round())
    {
        return Err(ProcurementError::InvalidInput(
            "Rating must be between 0 and 5 in whole-star steps".into(),
        )
        .into());
    }
    input.unit = input.unit.trim().to_string();
    if !super::is_known_unit(&input.unit) {
        return Err(ProcurementError::InvalidInput(format!(
            "Unit must be one of {}",
            super::UNITS.join(", ")
        ))
        .into());
    }
    input.tax_type = input.tax_type.trim().to_string();
    if !super::is_known_tax_type(&input.tax_type) {
        return Err(ProcurementError::InvalidInput(format!(
            "Tax type must be one of {}",
            super::TAX_TYPES.join(", ")
        ))
        .into());
    }
    input.brand = input
        .brand
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    input.memo = input
        .memo
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut aliases = Vec::new();
    for alias in input.aliases {
        let alias = alias.trim().to_string();
        if !alias.is_empty() && !aliases.contains(&alias) {
            aliases.push(alias);
        }
    }
    input.aliases = aliases;
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(rating: Option<f64>) -> SaveItem {
        SaveItem {
            name: " Item ".into(),
            brand: Some(" ".into()),
            memo: Some(" notes ".into()),
            category_id: "cat_groceries".into(),
            subcategory_id: None,
            rating,
            unit: " kg ".into(),
            tax_type: " INCLUDED ".into(),
            aliases: vec![" alias ".into(), "alias".into(), " ".into()],
            draft_id: None,
        }
    }
    #[test]
    fn validates_whole_stars_and_normalizes_aliases() {
        let saved = validate(input(Some(3.0))).unwrap();
        assert_eq!(saved.name, "Item");
        assert_eq!(saved.aliases, vec!["alias"]);
        assert_eq!(saved.brand, None);
        assert_eq!(saved.memo.as_deref(), Some("notes"));
        for rating in [f64::NAN, f64::INFINITY, -0.5, 5.5, 3.25, 3.5] {
            assert!(validate(input(Some(rating))).is_err());
        }
    }
    #[test]
    fn trims_the_tax_type_and_rejects_one_off_the_list() {
        assert_eq!(validate(input(None)).unwrap().tax_type, "INCLUDED");
        for tax_type in ["", " ", "included", "EXCLUDED_5"] {
            let mut value = input(None);
            value.tax_type = tax_type.into();
            assert!(validate(value).is_err());
        }
        for tax_type in crate::items::TAX_TYPES {
            let mut value = input(None);
            value.tax_type = (*tax_type).into();
            assert_eq!(validate(value).unwrap().tax_type, *tax_type);
        }
    }

    #[test]
    fn rejects_blank_name() {
        let mut value = input(None);
        value.name = " ".into();
        assert!(validate(value).is_err());
    }
    #[test]
    fn trims_the_unit_and_rejects_one_off_the_list() {
        assert_eq!(validate(input(None)).unwrap().unit, "kg");
        for unit in ["oz", "KG", "", " ", "kilogram"] {
            let mut value = input(None);
            value.unit = unit.into();
            assert!(validate(value).is_err(), "{unit} must be rejected");
        }
        for unit in crate::items::UNITS {
            let mut value = input(None);
            value.unit = (*unit).into();
            assert_eq!(validate(value).unwrap().unit, *unit);
        }
    }
}
