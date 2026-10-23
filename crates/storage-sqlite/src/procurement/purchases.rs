use crate::{
    db::{get_connection, DbPool, WriteHandle},
    errors::StorageError,
    schema::{activities, procurement_items as items, procurement_purchases as purchases},
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::{str::FromStr, sync::Arc};
use wealthfolio_procurement::purchases::{
    validate_amounts, Purchase, PurchasePrice, PurchasesRepositoryTrait, SavePurchase,
};

#[derive(Queryable, Selectable, Insertable, AsChangeset, Clone)]
#[diesel(table_name = purchases)]
struct PurchaseDB {
    item_id: String,
    activity_id: String,
    quantity: String,
    subtotal: String,
    created_at: String,
    updated_at: String,
}
impl PurchaseDB {
    fn into_model(self) -> Result<Purchase> {
        Ok(Purchase {
            input: SavePurchase {
                item_id: self.item_id,
                activity_id: self.activity_id,
                quantity: Decimal::from_str(&self.quantity)?,
                subtotal: Decimal::from_str(&self.subtotal)?,
            },
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

pub struct PurchasesRepository {
    pool: Arc<DbPool>,
    writer: WriteHandle,
}
impl PurchasesRepository {
    pub fn new(pool: Arc<DbPool>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}
#[async_trait]
impl PurchasesRepositoryTrait for PurchasesRepository {
    fn list(&self, item_id: Option<&str>) -> Result<Vec<PurchasePrice>> {
        let mut conn = get_connection(&self.pool)?;
        let mut query = purchases::table
            .inner_join(activities::table)
            .inner_join(items::table)
            .select((
                PurchaseDB::as_select(),
                activities::activity_date,
                activities::notes,
                activities::currency,
                items::tax_type,
            ))
            .into_boxed();
        if let Some(id) = item_id {
            query = query.filter(purchases::item_id.eq(id));
        }
        let rows = query.load::<(PurchaseDB, String, Option<String>, String, String)>(&mut conn)?;
        rows.into_iter()
            .map(|(row, date, shop, currency, tax_type)| {
                let purchase = row.into_model()?;
                let gross = purchase
                    .input
                    .subtotal
                    .checked_mul(wealthfolio_procurement::items::tax_multiplier(&tax_type))
                    .context("Procurement purchase subtotal overflow")?;
                let unit_price = gross
                    .checked_div(purchase.input.quantity)
                    .context("Procurement purchase has a zero quantity")?;
                Ok(PurchasePrice {
                    item_id: purchase.input.item_id,
                    activity_id: purchase.input.activity_id,
                    date: crate::activities::parse_db_datetime(&date)
                        .context("Invalid procurement activity date")?,
                    shop,
                    quantity: purchase.input.quantity,
                    unit_price,
                    currency,
                    unit_price_base: None,
                    base_currency: String::new(),
                })
            })
            .collect()
    }
    fn list_for_activity(&self, activity_id: &str) -> Result<Vec<Purchase>> {
        let mut conn = get_connection(&self.pool)?;
        purchases::table
            .filter(purchases::activity_id.eq(activity_id))
            .order((purchases::created_at.asc(), purchases::item_id.asc()))
            .select(PurchaseDB::as_select())
            .load::<PurchaseDB>(&mut conn)?
            .into_iter()
            .map(PurchaseDB::into_model)
            .collect()
    }
    async fn set_for_activity(
        &self,
        activity_id: &str,
        purchases: Vec<SavePurchase>,
    ) -> Result<Vec<Purchase>> {
        for input in &purchases {
            validate_amounts(input)?;
        }
        let activity_id = activity_id.to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let rows: Vec<PurchaseDB> = purchases
            .into_iter()
            .map(|input| PurchaseDB {
                item_id: input.item_id,
                activity_id: activity_id.clone(),
                quantity: input.quantity.normalize().to_string(),
                subtotal: input.subtotal.normalize().to_string(),
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .collect();
        let saved = self
            .writer
            .exec_tx(move |tx| {
                let kept: Vec<&str> = rows.iter().map(|row| row.item_id.as_str()).collect();
                diesel::delete(
                    purchases::table
                        .filter(purchases::activity_id.eq(&activity_id))
                        .filter(purchases::item_id.ne_all(kept)),
                )
                .execute(tx.conn())
                .map_err(StorageError::from)?;
                let mut rows = rows;
                for row in &mut rows {
                    // Keeps the link's own creation time, which also keeps the
                    // read order stable across edits.
                    let existing = purchases::table
                        .find((&row.activity_id, &row.item_id))
                        .select(purchases::created_at)
                        .first::<String>(tx.conn())
                        .optional()
                        .map_err(StorageError::from)?;
                    match existing {
                        Some(created_at) => {
                            row.created_at = created_at;
                            diesel::update(purchases::table.find((&row.activity_id, &row.item_id)))
                                .set(&*row)
                                .execute(tx.conn())
                                .map_err(StorageError::from)?;
                        }
                        None => {
                            diesel::insert_into(purchases::table)
                                .values(&*row)
                                .execute(tx.conn())
                                .map_err(StorageError::from)?;
                        }
                    }
                }
                Ok(rows)
            })
            .await?;
        saved.into_iter().map(PurchaseDB::into_model).collect()
    }
}
