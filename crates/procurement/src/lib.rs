//! Local procurement registry and purchase prices. Activity data stays owned by core;
//! this leaf crate neither participates in valuation nor registers device-sync entities.
pub mod error;
pub mod items;
pub mod purchases;

pub const CATEGORIES_TAXONOMY_ID: &str = "procurement_categories";
pub use error::ProcurementError;

pub mod photo_drafts;
#[cfg(feature = "photos")]
pub mod photos;
