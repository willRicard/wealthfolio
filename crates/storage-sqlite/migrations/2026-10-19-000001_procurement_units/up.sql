ALTER TABLE procurement_items ADD COLUMN unit TEXT NOT NULL DEFAULT '個';
ALTER TABLE procurement_items ADD COLUMN tax_type TEXT NOT NULL DEFAULT 'INCLUDED';

CREATE TABLE procurement_purchases_new (
    item_id TEXT NOT NULL REFERENCES procurement_items(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    quantity TEXT NOT NULL CHECK (CAST(quantity AS REAL) > 0),
    subtotal TEXT NOT NULL CHECK (CAST(subtotal AS REAL) >= 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (activity_id, item_id)
);

INSERT INTO procurement_purchases_new (item_id, activity_id, quantity, subtotal, created_at, updated_at)
SELECT
    item_id,
    activity_id,
    quantity,
    CAST(ROUND(CAST(unit_price AS REAL) * CAST(quantity AS REAL), 10) AS TEXT),
    created_at,
    updated_at
FROM procurement_purchases;

DROP TABLE procurement_purchases;
ALTER TABLE procurement_purchases_new RENAME TO procurement_purchases;

CREATE INDEX ix_procurement_purchases_item ON procurement_purchases(item_id);
