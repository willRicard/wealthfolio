use crate::{
    db::{get_connection, DbPool, WriteHandle},
    errors::StorageError,
    schema::{activities, procurement_purchases as purchases},
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::{str::FromStr, sync::Arc};
use wealthfolio_procurement::{
    purchases::{
        validate_amounts, Purchase, PurchasePrice, PurchasesRepositoryTrait, SavePurchase,
    },
    ProcurementError,
};

#[derive(Queryable, Selectable, Insertable, AsChangeset, Clone)]
#[diesel(table_name = purchases)]
struct PurchaseDB {
    item_id: String,
    activity_id: String,
    quantity: String,
    unit_price: String,
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
                unit_price: Decimal::from_str(&self.unit_price)?,
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
    async fn save(&self, updating: bool, input: SavePurchase) -> Result<Purchase> {
        validate_amounts(&input)?;
        let now = chrono::Utc::now().to_rfc3339();
        let row = PurchaseDB {
            item_id: input.item_id,
            activity_id: input.activity_id,
            quantity: input.quantity.normalize().to_string(),
            unit_price: input.unit_price.normalize().to_string(),
            created_at: now.clone(),
            updated_at: now,
        };
        let saved = self
            .writer
            .exec_tx(move |tx| {
                let mut row = row;
                if updating {
                    let existing = purchases::table
                        .find((&row.activity_id, &row.item_id))
                        .select(PurchaseDB::as_select())
                        .first::<PurchaseDB>(tx.conn())
                        .map_err(StorageError::from)?;
                    row.created_at = existing.created_at;
                    diesel::update(purchases::table.find((&row.activity_id, &row.item_id)))
                        .set(&row)
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                } else {
                    diesel::insert_into(purchases::table)
                        .values(&row)
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                }
                Ok(row)
            })
            .await?;
        saved.into_model()
    }
}
#[async_trait]
impl PurchasesRepositoryTrait for PurchasesRepository {
    fn list(&self, item_id: Option<&str>) -> Result<Vec<PurchasePrice>> {
        let mut conn = get_connection(&self.pool)?;
        let mut query = purchases::table
            .inner_join(activities::table)
            .select((
                PurchaseDB::as_select(),
                activities::activity_date,
                activities::notes,
                activities::currency,
            ))
            .into_boxed();
        if let Some(id) = item_id {
            query = query.filter(purchases::item_id.eq(id));
        }
        let rows = query.load::<(PurchaseDB, String, Option<String>, String)>(&mut conn)?;
        rows.into_iter()
            .map(|(row, date, shop, currency)| {
                let purchase = row.into_model()?;
                Ok(PurchasePrice {
                    item_id: purchase.input.item_id,
                    activity_id: purchase.input.activity_id,
                    date: crate::activities::parse_db_datetime(&date)
                        .context("Invalid procurement activity date")?,
                    shop,
                    quantity: purchase.input.quantity,
                    unit_price: purchase.input.unit_price,
                    currency,
                    unit_price_base: None,
                    base_currency: String::new(),
                })
            })
            .collect()
    }
    async fn create(&self, input: SavePurchase) -> Result<Purchase> {
        self.save(false, input).await
    }
    async fn update(&self, input: SavePurchase) -> Result<Purchase> {
        self.save(true, input).await
    }
    async fn delete(&self, activity_id: &str, item_id: &str) -> Result<()> {
        let target = (activity_id.to_string(), item_id.to_string());
        let deleted = self
            .writer
            .exec_tx(move |tx| {
                diesel::delete(purchases::table.find(target))
                    .execute(tx.conn())
                    .map_err(StorageError::from)
                    .map_err(Into::into)
            })
            .await?;
        if deleted == 0 {
            return Err(ProcurementError::NotFound {
                entity: "Purchase",
                id: format!("{activity_id}/{item_id}"),
            }
            .into());
        }
        Ok(())
    }
}
