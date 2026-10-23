CREATE TABLE procurement_purchases_old (
    item_id TEXT NOT NULL REFERENCES procurement_items(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    quantity TEXT NOT NULL CHECK (CAST(quantity AS REAL) > 0),
    unit_price TEXT NOT NULL CHECK (CAST(unit_price AS REAL) >= 0),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (activity_id, item_id)
);

INSERT INTO procurement_purchases_old (item_id, activity_id, quantity, unit_price, created_at, updated_at)
SELECT
    item_id,
    activity_id,
    quantity,
    CAST(ROUND(CAST(subtotal AS REAL) / CAST(quantity AS REAL), 10) AS TEXT),
    created_at,
    updated_at
FROM procurement_purchases;

DROP TABLE procurement_purchases;
ALTER TABLE procurement_purchases_old RENAME TO procurement_purchases;

CREATE INDEX ix_procurement_purchases_item ON procurement_purchases(item_id);

ALTER TABLE procurement_items DROP COLUMN tax_type;
ALTER TABLE procurement_items DROP COLUMN unit;
