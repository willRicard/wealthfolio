use crate::purchases::PriceStats;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPhoto {
    pub backend: String,
    pub key: String,
    pub preview_key: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub brand: Option<String>,
    pub memo: Option<String>,
    pub category_id: String,
    pub subcategory_id: Option<String>,
    pub rating: Option<f64>,
    pub unit: String,
    pub tax_type: String,
    pub aliases: Vec<String>,
    pub photo: Option<ItemPhoto>,
    pub created_at: String,
    pub updated_at: String,
}

/// Full editable form; an omitted draft preserves an existing photo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveItem {
    pub name: String,
    pub brand: Option<String>,
    pub memo: Option<String>,
    pub category_id: String,
    pub subcategory_id: Option<String>,
    pub rating: Option<f64>,
    pub unit: String,
    pub tax_type: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub draft_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemWithStats {
    #[serde(flatten)]
    pub item: Item,
    #[serde(flatten)]
    pub stats: PriceStats,
}
