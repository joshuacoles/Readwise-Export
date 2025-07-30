-- Alter sync_state to have separate last_updated timestamps for each data kind
ALTER TABLE sync_state RENAME TO sync_state_old;

CREATE TABLE sync_state (
                            id INTEGER PRIMARY KEY NOT NULL DEFAULT 1 CHECK (id = 1), -- Ensures only one row
                            last_books_sync TIMESTAMP,
                            last_highlights_sync TIMESTAMP,
                            last_documents_sync TIMESTAMP
);

INSERT INTO sync_state (id, last_books_sync, last_highlights_sync, last_documents_sync)
SELECT 1, last_updated, last_updated, last_updated FROM sync_state_old WHERE id = 1;

-- If there was no old state, insert a default new one (though typically the app creates it)
INSERT OR IGNORE INTO sync_state (id) VALUES (1);

DROP TABLE sync_state_old;