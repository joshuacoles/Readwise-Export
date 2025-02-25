-- Convert datetime columns in books table
ALTER TABLE books ADD COLUMN last_highlight_at_new DATETIME;
UPDATE books SET last_highlight_at_new = datetime(last_highlight_at);
ALTER TABLE books DROP COLUMN last_highlight_at;
ALTER TABLE books RENAME COLUMN last_highlight_at_new TO last_highlight_at;

ALTER TABLE books ADD COLUMN updated_new DATETIME;
UPDATE books SET updated_new = datetime(updated);
ALTER TABLE books DROP COLUMN updated;
ALTER TABLE books RENAME COLUMN updated_new TO updated;

-- Convert datetime columns in highlights table
ALTER TABLE highlights ADD COLUMN highlighted_at_new DATETIME;
UPDATE highlights SET highlighted_at_new = datetime(highlighted_at);
ALTER TABLE highlights DROP COLUMN highlighted_at;
ALTER TABLE highlights RENAME COLUMN highlighted_at_new TO highlighted_at;

ALTER TABLE highlights ADD COLUMN updated_new DATETIME;
UPDATE highlights SET updated_new = datetime(updated);
ALTER TABLE highlights DROP COLUMN updated;
ALTER TABLE highlights RENAME COLUMN updated_new TO updated;

-- Convert datetime columns in documents table
ALTER TABLE documents ADD COLUMN created_at_new DATETIME;
UPDATE documents SET created_at_new = datetime(created_at);
ALTER TABLE documents DROP COLUMN created_at;
ALTER TABLE documents RENAME COLUMN created_at_new TO created_at;

ALTER TABLE documents ADD COLUMN updated_at_new DATETIME;
UPDATE documents SET updated_at_new = datetime(updated_at);
ALTER TABLE documents DROP COLUMN updated_at;
ALTER TABLE documents RENAME COLUMN updated_at_new TO updated_at;

ALTER TABLE documents ADD COLUMN published_date_new DATETIME;
UPDATE documents SET published_date_new = datetime(published_date);
ALTER TABLE documents DROP COLUMN published_date;
ALTER TABLE documents RENAME COLUMN published_date_new TO published_date;

ALTER TABLE documents ADD COLUMN first_opened_at_new DATETIME;
UPDATE documents SET first_opened_at_new = datetime(first_opened_at);
ALTER TABLE documents DROP COLUMN first_opened_at;
ALTER TABLE documents RENAME COLUMN first_opened_at_new TO first_opened_at;

ALTER TABLE documents ADD COLUMN last_opened_at_new DATETIME;
UPDATE documents SET last_opened_at_new = datetime(last_opened_at);
ALTER TABLE documents DROP COLUMN last_opened_at;
ALTER TABLE documents RENAME COLUMN last_opened_at_new TO last_opened_at;

ALTER TABLE documents ADD COLUMN saved_at_new DATETIME;
UPDATE documents SET saved_at_new = datetime(saved_at);
ALTER TABLE documents DROP COLUMN saved_at;
ALTER TABLE documents RENAME COLUMN saved_at_new TO saved_at;

ALTER TABLE documents ADD COLUMN last_moved_at_new DATETIME;
UPDATE documents SET last_moved_at_new = datetime(last_moved_at);
ALTER TABLE documents DROP COLUMN last_moved_at;
ALTER TABLE documents RENAME COLUMN last_moved_at_new TO last_moved_at;

-- Convert datetime column in sync_state table
ALTER TABLE sync_state ADD COLUMN last_updated_new DATETIME;
UPDATE sync_state SET last_updated_new = datetime(last_updated);
ALTER TABLE sync_state DROP COLUMN last_updated;
ALTER TABLE sync_state RENAME COLUMN last_updated_new TO last_updated;