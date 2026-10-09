ALTER TABLE activities ADD COLUMN detailed_notes TEXT CHECK(detailed_notes IS NULL OR length(detailed_notes) <= 20000);

CREATE TABLE spending_transaction_attachments (
    id TEXT PRIMARY KEY NOT NULL,
    activity_id TEXT NOT NULL REFERENCES activities(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL CHECK(content_type IN ('image/jpeg', 'image/png', 'image/webp', 'application/pdf')),
    size_bytes BIGINT NOT NULL CHECK(size_bytes > 0 AND size_bytes <= 20971520),
    created_at TEXT NOT NULL
);
CREATE INDEX spending_transaction_attachments_activity ON spending_transaction_attachments(activity_id);
CREATE TRIGGER spending_transaction_attachments_limit BEFORE INSERT ON spending_transaction_attachments
WHEN (SELECT COUNT(*) FROM spending_transaction_attachments WHERE activity_id = NEW.activity_id) >= 10
BEGIN SELECT RAISE(ABORT, 'Maximum 10 attachments per transaction'); END;
