DROP TABLE procurement_photo_drafts;
DROP TABLE procurement_purchases;
DROP TABLE procurement_item_aliases;
DROP TABLE procurement_items;

DELETE FROM taxonomy_categories WHERE taxonomy_id = 'procurement_categories';
DELETE FROM taxonomies WHERE id = 'procurement_categories';
