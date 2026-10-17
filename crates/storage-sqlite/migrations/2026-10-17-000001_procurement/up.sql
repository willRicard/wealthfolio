CREATE TABLE procurement_items (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    brand TEXT,
    memo TEXT,
    category_id TEXT NOT NULL,
    subcategory_id TEXT,
    rating REAL CHECK (rating >= 0 AND rating <= 5 AND rating * 2 = CAST(rating * 2 AS INTEGER)),
    photo_backend TEXT,
    photo_key TEXT,
    photo_preview_key TEXT,
    photo_filename TEXT,
    photo_content_type TEXT,
    photo_size_bytes BIGINT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE procurement_item_aliases (
    id TEXT NOT NULL PRIMARY KEY,
    item_id TEXT NOT NULL REFERENCES procurement_items(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    UNIQUE (item_id, alias)
);

CREATE TABLE procurement_purchases (
    item_id TEXT NOT NULL REFERENCES procurement_items(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    quantity TEXT NOT NULL CHECK (CAST(quantity AS REAL) > 0),
    unit_price TEXT NOT NULL CHECK (CAST(unit_price AS REAL) >= 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (activity_id, item_id)
);

CREATE INDEX ix_procurement_purchases_item ON procurement_purchases(item_id);

CREATE TABLE procurement_photo_drafts (
    id TEXT NOT NULL PRIMARY KEY,
    storage_backend TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    preview_key TEXT NOT NULL,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

INSERT INTO taxonomies (id, name, is_system, is_single_select, scope)
VALUES ('procurement_categories', 'Procurement Categories', 1, 1, 'procurement');

INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order)
SELECT id, 'procurement_categories', parent_id, name, key, color, icon, sort_order
FROM taxonomy_categories
WHERE taxonomy_id = 'spending_categories'
  AND id IN (
      'cat_groceries',
      'cat_housing', 'cat_housing_maintenance', 'cat_housing_furnishing',
      'cat_food', 'cat_food_restaurants', 'cat_food_coffee',
      'cat_food_delivery', 'cat_food_alcohol',
      'cat_transport', 'cat_transport_gas', 'cat_transport_public',
      'cat_transport_rideshare', 'cat_transport_maintenance',
      'cat_shopping', 'cat_shopping_clothing', 'cat_shopping_electronics',
      'cat_shopping_home', 'cat_shopping_online',
      'cat_entertainment', 'cat_entertainment_streaming',
      'cat_entertainment_games', 'cat_entertainment_hobbies',
      'cat_entertainment_sports',
      'cat_health', 'cat_health_medical', 'cat_health_pharmacy',
      'cat_health_dental', 'cat_health_vision', 'cat_health_fitness',
      'cat_bills', 'cat_bills_phone', 'cat_bills_internet',
      'cat_bills_subscriptions', 'cat_bills_software',
      'cat_personal', 'cat_education', 'cat_travel'
  )
ORDER BY (parent_id IS NOT NULL), sort_order;
