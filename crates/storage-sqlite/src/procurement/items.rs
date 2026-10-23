use crate::{
    db::{get_connection, DbPool, WriteHandle},
    errors::StorageError,
    schema::{
        procurement_item_aliases as aliases, procurement_items as items,
        taxonomy_categories as categories,
    },
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use diesel::prelude::*;
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;
use wealthfolio_procurement::{
    items::{validate, Item, ItemPhoto, ItemsRepositoryTrait, SaveItem},
    CATEGORIES_TAXONOMY_ID,
};

#[derive(Queryable, Selectable, Insertable, AsChangeset, Clone)]
#[diesel(table_name = items)]
#[diesel(treat_none_as_null = true)]
struct ItemDB {
    id: String,
    name: String,
    brand: Option<String>,
    memo: Option<String>,
    category_id: String,
    subcategory_id: Option<String>,
    rating: Option<f64>,
    photo_backend: Option<String>,
    photo_key: Option<String>,
    photo_preview_key: Option<String>,
    photo_filename: Option<String>,
    photo_content_type: Option<String>,
    photo_size_bytes: Option<i64>,
    created_at: String,
    updated_at: String,
    unit: String,
    tax_type: String,
}
impl ItemDB {
    fn into_model(self, aliases: Vec<String>) -> Result<Item> {
        let photo = if let Some(backend) = self.photo_backend {
            Some(ItemPhoto {
                backend,
                key: self.photo_key.context("Missing photo key")?,
                preview_key: self.photo_preview_key.context("Missing preview key")?,
                filename: self.photo_filename.context("Missing photo filename")?,
                content_type: self
                    .photo_content_type
                    .context("Missing photo content type")?,
                size_bytes: self.photo_size_bytes.context("Missing photo size")?,
            })
        } else {
            None
        };
        Ok(Item {
            id: self.id,
            name: self.name,
            brand: self.brand,
            memo: self.memo,
            category_id: self.category_id,
            subcategory_id: self.subcategory_id,
            rating: self.rating,
            unit: self.unit,
            tax_type: self.tax_type,
            aliases,
            photo,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

pub struct ItemsRepository {
    pool: Arc<DbPool>,
    writer: WriteHandle,
}
impl ItemsRepository {
    pub fn new(pool: Arc<DbPool>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
    async fn save(&self, id: Option<&str>, input: SaveItem) -> Result<(Item, Option<ItemPhoto>)> {
        let input = validate(input)?;
        let updating = id.is_some();
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let saved = self
            .writer
            .exec_tx(move |tx| {
                let parent = categories::table
                    .filter(categories::taxonomy_id.eq(CATEGORIES_TAXONOMY_ID))
                    .filter(categories::id.eq(&input.category_id))
                    .select(categories::parent_id)
                    .first::<Option<String>>(tx.conn())
                    .map_err(StorageError::from)?;
                if parent.is_some() {
                    return Err(wealthfolio_core::Error::Validation(
                        wealthfolio_core::errors::ValidationError::InvalidInput(
                            "Category must be a root category".into(),
                        ),
                    ));
                }
                if let Some(subcategory) = &input.subcategory_id {
                    let parent = categories::table
                        .filter(categories::taxonomy_id.eq(CATEGORIES_TAXONOMY_ID))
                        .filter(categories::id.eq(subcategory))
                        .select(categories::parent_id)
                        .first::<Option<String>>(tx.conn())
                        .map_err(StorageError::from)?;
                    if parent.as_deref() != Some(input.category_id.as_str()) {
                        return Err(wealthfolio_core::Error::Validation(
                            wealthfolio_core::errors::ValidationError::InvalidInput(
                                "Subcategory must belong to the selected category".into(),
                            ),
                        ));
                    }
                }
                let now = chrono::Utc::now().to_rfc3339();
                let mut row = if updating {
                    items::table
                        .find(&id)
                        .select(ItemDB::as_select())
                        .first::<ItemDB>(tx.conn())
                        .map_err(StorageError::from)?
                } else {
                    ItemDB {
                        id: id.clone(),
                        name: String::new(),
                        brand: None,
                        memo: None,
                        category_id: String::new(),
                        subcategory_id: None,
                        rating: None,
                        photo_backend: None,
                        photo_key: None,
                        photo_preview_key: None,
                        photo_filename: None,
                        photo_content_type: None,
                        photo_size_bytes: None,
                        created_at: now.clone(),
                        updated_at: now.clone(),
                        unit: String::new(),
                        tax_type: String::new(),
                    }
                };
                let previous_photo = if input.draft_id.is_some() {
                    row.clone()
                        .into_model(Vec::new())
                        .map_err(|_| {
                            wealthfolio_core::Error::Validation(
                                wealthfolio_core::errors::ValidationError::InvalidInput(
                                    "Invalid stored photo metadata".into(),
                                ),
                            )
                        })?
                        .photo
                } else {
                    None
                };
                if let Some(draft_id) = &input.draft_id {
                    use crate::schema::procurement_photo_drafts as drafts;
                    let draft = drafts::table
                        .find(draft_id)
                        .select(super::photo_drafts::DraftDB::as_select())
                        .first::<super::photo_drafts::DraftDB>(tx.conn())
                        .map_err(StorageError::from)?;
                    row.photo_backend = Some(draft.storage_backend);
                    row.photo_key = Some(draft.storage_key);
                    row.photo_preview_key = Some(draft.preview_key);
                    row.photo_filename = Some(draft.filename);
                    row.photo_content_type = Some(draft.content_type);
                    row.photo_size_bytes = Some(draft.size_bytes);
                    diesel::delete(drafts::table.find(draft_id))
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                }
                row.name = input.name;
                row.brand = input.brand;
                row.memo = input.memo;
                row.category_id = input.category_id;
                row.subcategory_id = input.subcategory_id;
                row.rating = input.rating;
                row.unit = input.unit;
                row.tax_type = input.tax_type;
                row.updated_at = now;
                if updating {
                    diesel::update(items::table.find(&id))
                        .set(&row)
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                } else {
                    diesel::insert_into(items::table)
                        .values(&row)
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                }
                diesel::delete(aliases::table.filter(aliases::item_id.eq(&id)))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                for (index, alias) in input.aliases.iter().enumerate() {
                    diesel::insert_into(aliases::table)
                        .values((
                            aliases::id.eq(Uuid::new_v4().to_string()),
                            aliases::item_id.eq(&id),
                            aliases::alias.eq(alias),
                            aliases::sort_order.eq(index as i32),
                        ))
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                }
                Ok((row, input.aliases, previous_photo))
            })
            .await?;
        Ok((saved.0.into_model(saved.1)?, saved.2))
    }
}

#[async_trait]
impl ItemsRepositoryTrait for ItemsRepository {
    fn list(&self) -> Result<Vec<Item>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = items::table
            .order((items::name.asc(), items::id.asc()))
            .select(ItemDB::as_select())
            .load::<ItemDB>(&mut conn)?;
        let mut grouped = HashMap::<String, Vec<String>>::new();
        for (item_id, alias) in aliases::table
            .order((aliases::sort_order.asc(), aliases::id.asc()))
            .select((aliases::item_id, aliases::alias))
            .load::<(String, String)>(&mut conn)?
        {
            grouped.entry(item_id).or_default().push(alias);
        }
        rows.into_iter()
            .map(|row| {
                let aliases = grouped.remove(&row.id).unwrap_or_default();
                row.into_model(aliases)
            })
            .collect()
    }
    fn get(&self, id: &str) -> Result<Option<Item>> {
        let mut conn = get_connection(&self.pool)?;
        let row = items::table
            .find(id)
            .select(ItemDB::as_select())
            .first::<ItemDB>(&mut conn)
            .optional()?;
        row.map(|row| {
            let aliases = aliases::table
                .filter(aliases::item_id.eq(id))
                .order((aliases::sort_order.asc(), aliases::id.asc()))
                .select(aliases::alias)
                .load::<String>(&mut conn)?;
            row.into_model(aliases)
        })
        .transpose()
    }
    async fn create(&self, input: SaveItem) -> Result<Item> {
        self.save(None, input).await.map(|(item, _)| item)
    }
    async fn update(&self, id: &str, input: SaveItem) -> Result<(Item, Option<ItemPhoto>)> {
        self.save(Some(id), input).await
    }
    async fn delete(&self, id: &str) -> Result<Option<ItemPhoto>> {
        let target = id.to_string();
        let row = self
            .writer
            .exec_tx(move |tx| {
                let row = items::table
                    .find(&target)
                    .select(ItemDB::as_select())
                    .first::<ItemDB>(tx.conn())
                    .map_err(StorageError::from)?;
                diesel::delete(items::table.find(target))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(row)
            })
            .await?;
        Ok(row.into_model(Vec::new())?.photo)
    }
}
