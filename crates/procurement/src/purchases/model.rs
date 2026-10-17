use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePurchase {
    pub item_id: String,
    pub activity_id: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unit_price: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Purchase {
    #[serde(flatten)]
    pub input: SavePurchase,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchasePrice {
    pub item_id: String,
    pub activity_id: String,
    pub date: DateTime<Utc>,
    pub shop: Option<String>,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unit_price: Decimal,
    pub currency: String,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub unit_price_base: Option<Decimal>,
    pub base_currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceStats {
    pub base_currency: String,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub min_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub avg_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub latest_price: Option<Decimal>,
    pub latest_date: Option<DateTime<Utc>>,
    pub purchase_count: usize,
    pub unconverted_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemPrices {
    pub prices: Vec<PurchasePrice>,
    pub stats: PriceStats,
}
