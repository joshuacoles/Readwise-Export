CREATE TABLE books (
    id BIGINT PRIMARY KEY,
    title TEXT NOT NULL,
    author TEXT,
    category TEXT NOT NULL,
    num_highlights BIGINT NOT NULL,
    last_highlight_at TIMESTAMP,
    updated TIMESTAMP,
    cover_image_url TEXT,
    highlights_url TEXT,
    source_url TEXT,
    asin TEXT
);

CREATE TABLE highlights (
    id BIGINT PRIMARY KEY,
    text TEXT NOT NULL,
    note TEXT NOT NULL,
    location BIGINT NOT NULL,
    location_type TEXT NOT NULL,
    highlighted_at TIMESTAMP,
    url TEXT,
    color TEXT NOT NULL,
    updated TIMESTAMP NOT NULL,
    book_id BIGINT NOT NULL,
    FOREIGN KEY (book_id) REFERENCES books(id)
);

CREATE TABLE tags (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE book_tags (
    book_id BIGINT NOT NULL,
    tag_id BIGINT NOT NULL,
    PRIMARY KEY (book_id, tag_id),
    FOREIGN KEY (book_id) REFERENCES books(id),
    FOREIGN KEY (tag_id) REFERENCES tags(id)
);

CREATE TABLE highlight_tags (
    highlight_id BIGINT NOT NULL,
    tag_id BIGINT NOT NULL,
    PRIMARY KEY (highlight_id, tag_id),
    FOREIGN KEY (highlight_id) REFERENCES highlights(id),
    FOREIGN KEY (tag_id) REFERENCES tags(id)
);

CREATE TABLE documents (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    title TEXT,
    author TEXT,
    source TEXT,
    category TEXT,
    location TEXT,
    site_name TEXT,
    word_count BIGINT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    published_date TIMESTAMP,
    summary TEXT,
    image_url TEXT,
    content TEXT,
    source_url TEXT,
    notes TEXT,
    parent_id TEXT,
    reading_progress REAL NOT NULL,
    first_opened_at TIMESTAMP,
    last_opened_at TIMESTAMP,
    saved_at TIMESTAMP NOT NULL,
    last_moved_at TIMESTAMP NOT NULL
);

CREATE TABLE sync_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    last_books_sync TIMESTAMP,
    last_highlights_sync TIMESTAMP,
    last_documents_sync TIMESTAMP
);

-- Create indexes for common queries
CREATE INDEX idx_highlights_book_id ON highlights(book_id);
CREATE INDEX idx_book_tags_book_id ON book_tags(book_id);
CREATE INDEX idx_highlight_tags_highlight_id ON highlight_tags(highlight_id);