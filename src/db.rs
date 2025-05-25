use crate::Library;
use crate::readwise::Tag;
use crate::ReadwiseObjectKind;
use anyhow::Context;
use chrono::{DateTime, Utc};
use sqlx::{SqlitePool, Row};
use sqlx::sqlite::{SqliteConnectOptions};

mod types {
    use chrono::{DateTime, NaiveDateTime, Utc};
    use crate::library;

    #[derive(Debug, Clone)]
    pub struct Document {
        pub id: String,
        pub url: String,
        pub title: Option<String>,
        pub author: Option<String>,
        pub source: Option<String>,
        pub category: Option<String>,
        pub location: Option<String>,
        pub site_name: Option<String>,
        pub word_count: Option<i64>,
        pub created_at: String,
        pub updated_at: String,
        pub published_date: Option<DateTime<Utc>>,
        pub summary: Option<String>,
        pub image_url: Option<String>,
        pub content: Option<String>,
        pub source_url: Option<String>,
        pub notes: Option<String>,
        pub parent_id: Option<String>,
        pub reading_progress: f64,
        pub first_opened_at: Option<String>,
        pub last_opened_at: Option<String>,
        pub saved_at: String,
        pub last_moved_at: String,
    }

    impl Into<library::Document> for Document {
        fn into(self) -> library::Document {
            library::Document {
                id: self.id,
                url: self.url,
                title: self.title,
                author: self.author,
                source: self.source,
                category: self.category,
                location: self.location,
                site_name: self.site_name,
                word_count: self.word_count,
                created_at: self.created_at.parse().unwrap(),
                updated_at: self.updated_at.parse().unwrap(),
                published_date: self.published_date,
                summary: self.summary,
                image_url: self.image_url,
                content: self.content,
                source_url: self.source_url,
                notes: self.notes,
                parent_id: self.parent_id,
                reading_progress: self.reading_progress,
                first_opened_at: self.first_opened_at.map(|dt| dt.parse().unwrap()),
                last_opened_at: self.last_opened_at.map(|dt| dt.parse().unwrap()),
                saved_at: self.saved_at.parse().unwrap(),
                last_moved_at: self.last_moved_at.parse().unwrap(),
            }
        }
    }

    #[derive(Clone, Debug)]
    pub struct Book {
        pub id: i64,
        pub title: String,
        pub author: Option<String>,
        pub category: String,
        pub num_highlights: i64,
        pub last_highlight_at: Option<NaiveDateTime>,
        pub updated: Option<NaiveDateTime>,
        pub cover_image_url: Option<String>,
        pub highlights_url: Option<String>,
        pub source_url: Option<String>,
        pub asin: Option<String>,
    }

    impl Into<library::Book> for Book {
        fn into(self) -> library::Book {
            library::Book {
                id: self.id,
                title: self.title,
                author: self.author,
                category: self.category,
                num_highlights: self.num_highlights,
                last_highlight_at: self.last_highlight_at.map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)),
                updated: self.updated.map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)),
                cover_image_url: self.cover_image_url,
                highlights_url: self.highlights_url,
                source_url: self.source_url,
                asin: self.asin,
            }
        }
    }

    #[derive(Debug)]
    pub struct Highlight {
        pub id: i64,
        pub text: String,
        pub note: String,
        pub location: i64,
        pub location_type: String,
        pub highlighted_at: Option<NaiveDateTime>,
        pub url: Option<String>,
        pub color: String,
        pub updated: Option<DateTime<Utc>>,
        pub book_id: i64,
    }

    impl Into<library::Highlight> for Highlight {
        fn into(self) -> library::Highlight {
            library::Highlight {
                id: self.id,
                text: self.text,
                note: self.note,
                location: self.location,
                location_type: self.location_type,
                highlighted_at: self.highlighted_at.map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)),
                url: self.url,
                color: self.color,
                updated: self.updated.unwrap_or_else(|| Utc::now()),
                book_id: self.book_id,
            }
        }
    }
}

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(database_path: &str) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("Failed to run migrations")?;

        Ok(Self { pool })
    }

    pub async fn insert_book(&self, book: &crate::readwise::Book) -> anyhow::Result<()> {
        self.insert_books(&[book]).await
    }

    pub async fn insert_books(&self, books: &[&crate::readwise::Book]) -> anyhow::Result<()> {
        if books.is_empty() {
            return Ok(());
        }

        // Collect all unique tags first
        let mut all_tags = std::collections::HashMap::new();
        for book in books {
            for tag in &book.tags {
                all_tags.insert(tag.id, tag);
            }
        }

        // Batch insert tags if any exist
        if !all_tags.is_empty() {
            let tags_to_insert: Vec<&Tag> = all_tags.values().cloned().collect();
            self.insert_tags(&tags_to_insert).await?;
        }

        let mut tx = self.pool.begin().await?;

        // Batch insert books using multiple value tuples
        let mut book_ids = Vec::new();
        let mut book_titles = Vec::new();
        let mut book_authors = Vec::new();
        let mut book_categories = Vec::new();
        let mut book_num_highlights = Vec::new();
        let mut book_last_highlight_ats = Vec::new();
        let mut book_updateds = Vec::new();
        let mut book_cover_image_urls = Vec::new();
        let mut book_highlights_urls = Vec::new();
        let mut book_source_urls = Vec::new();
        let mut book_asins = Vec::new();

        for book in books {
            book_ids.push(book.id);
            book_titles.push(&book.title);
            book_authors.push(book.author.as_deref());
            book_categories.push(&book.category);
            book_num_highlights.push(book.num_highlights);
            book_last_highlight_ats.push(book.last_highlight_at.as_deref());
            book_updateds.push(book.updated.as_deref());
            book_cover_image_urls.push(book.cover_image_url.as_deref());
            book_highlights_urls.push(book.highlights_url.as_deref());
            book_source_urls.push(book.source_url.as_deref());
            book_asins.push(book.asin.as_deref());
        }

        // Build a query with multiple VALUES clauses
        let placeholders: Vec<String> = (0..books.len())
            .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string())
            .collect();
        
        let book_query = format!(
            "INSERT INTO books (
                id, title, author, category, num_highlights,
                last_highlight_at, updated, cover_image_url,
                highlights_url, source_url, asin
            )
            VALUES {}
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                author = excluded.author,
                category = excluded.category,
                num_highlights = excluded.num_highlights,
                last_highlight_at = excluded.last_highlight_at,
                updated = excluded.updated,
                cover_image_url = excluded.cover_image_url,
                highlights_url = excluded.highlights_url,
                source_url = excluded.source_url,
                asin = excluded.asin",
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&book_query);
        for i in 0..books.len() {
            query = query
                .bind(book_ids[i])
                .bind(book_titles[i])
                .bind(book_authors[i])
                .bind(book_categories[i])
                .bind(book_num_highlights[i])
                .bind(book_last_highlight_ats[i])
                .bind(book_updateds[i])
                .bind(book_cover_image_urls[i])
                .bind(book_highlights_urls[i])
                .bind(book_source_urls[i])
                .bind(book_asins[i]);
        }
        query.execute(&mut *tx).await?;

        // Batch insert book_tags relationships
        let mut book_tag_pairs = Vec::new();
        for book in books {
            for tag in &book.tags {
                book_tag_pairs.push((book.id, tag.id));
            }
        }

        if !book_tag_pairs.is_empty() {
            let placeholders: Vec<String> = (0..book_tag_pairs.len())
                .map(|_| "(?, ?)".to_string())
                .collect();
                
            let book_tag_query = format!(
                "INSERT INTO book_tags (book_id, tag_id) VALUES {} ON CONFLICT DO NOTHING",
                placeholders.join(", ")
            );
            
            let mut query = sqlx::query(&book_tag_query);
            for (book_id, tag_id) in &book_tag_pairs {
                query = query.bind(*book_id).bind(*tag_id);
            }
            query.execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn insert_highlight(&self, highlight: &crate::readwise::Highlight) -> anyhow::Result<()> {
        self.insert_highlights(&[highlight]).await
    }

    pub async fn insert_highlights(&self, highlights: &[&crate::readwise::Highlight]) -> anyhow::Result<()> {
        if highlights.is_empty() {
            return Ok(());
        }

        // Collect all unique tags first
        let mut all_tags = std::collections::HashMap::new();
        for highlight in highlights {
            for tag in &highlight.tags {
                all_tags.insert(tag.id, tag);
            }
        }

        // Batch insert tags if any exist
        if !all_tags.is_empty() {
            let tags_to_insert: Vec<&Tag> = all_tags.values().cloned().collect();
            self.insert_tags(&tags_to_insert).await?;
        }

        let mut tx = self.pool.begin().await?;

        // Batch insert highlights using multiple value tuples
        let mut highlight_ids = Vec::new();
        let mut highlight_texts = Vec::new();
        let mut highlight_notes = Vec::new();
        let mut highlight_locations = Vec::new();
        let mut highlight_location_types = Vec::new();
        let mut highlight_highlighted_ats = Vec::new();
        let mut highlight_urls = Vec::new();
        let mut highlight_colors = Vec::new();
        let mut highlight_updateds = Vec::new();
        let mut highlight_book_ids = Vec::new();

        for highlight in highlights {
            highlight_ids.push(highlight.id);
            highlight_texts.push(&highlight.text);
            highlight_notes.push(&highlight.note);
            highlight_locations.push(highlight.location);
            highlight_location_types.push(&highlight.location_type);
            highlight_highlighted_ats.push(highlight.highlighted_at.as_deref());
            highlight_urls.push(highlight.url.as_deref());
            highlight_colors.push(&highlight.color);
            highlight_updateds.push(&highlight.updated);
            highlight_book_ids.push(highlight.book_id);
        }

        // Build a query with multiple VALUES clauses
        let placeholders: Vec<String> = (0..highlights.len())
            .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string())
            .collect();
        
        let highlight_query = format!(
            "INSERT INTO highlights (
                id, text, note, location, location_type,
                highlighted_at, url, color, updated, book_id
            )
            VALUES {}
            ON CONFLICT(id) DO UPDATE SET
                text = excluded.text,
                note = excluded.note,
                location = excluded.location,
                location_type = excluded.location_type,
                highlighted_at = excluded.highlighted_at,
                url = excluded.url,
                color = excluded.color,
                updated = excluded.updated,
                book_id = excluded.book_id",
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&highlight_query);
        for i in 0..highlights.len() {
            query = query
                .bind(highlight_ids[i])
                .bind(highlight_texts[i])
                .bind(highlight_notes[i])
                .bind(highlight_locations[i])
                .bind(highlight_location_types[i])
                .bind(highlight_highlighted_ats[i])
                .bind(highlight_urls[i])
                .bind(highlight_colors[i])
                .bind(highlight_updateds[i])
                .bind(highlight_book_ids[i]);
        }
        query.execute(&mut *tx).await?;

        // Batch insert highlight_tags relationships
        let mut highlight_tag_pairs = Vec::new();
        for highlight in highlights {
            for tag in &highlight.tags {
                highlight_tag_pairs.push((highlight.id, tag.id));
            }
        }

        if !highlight_tag_pairs.is_empty() {
            let placeholders: Vec<String> = (0..highlight_tag_pairs.len())
                .map(|_| "(?, ?)".to_string())
                .collect();
                
            let highlight_tag_query = format!(
                "INSERT INTO highlight_tags (highlight_id, tag_id) VALUES {} ON CONFLICT DO NOTHING",
                placeholders.join(", ")
            );
            
            let mut query = sqlx::query(&highlight_tag_query);
            for (highlight_id, tag_id) in &highlight_tag_pairs {
                query = query.bind(*highlight_id).bind(*tag_id);
            }
            query.execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn insert_document(&self, document: &crate::readwise::Document) -> anyhow::Result<()> {
        self.insert_documents(&[document]).await
    }

    pub async fn insert_documents(&self, documents: &[&crate::readwise::Document]) -> anyhow::Result<()> {
        if documents.is_empty() {
            return Ok(());
        }

        // Batch insert documents using multiple value tuples
        let mut document_ids = Vec::new();
        let mut document_urls = Vec::new();
        let mut document_titles = Vec::new();
        let mut document_authors = Vec::new();
        let mut document_sources = Vec::new();
        let mut document_categories = Vec::new();
        let mut document_locations = Vec::new();
        let mut document_site_names = Vec::new();
        let mut document_word_counts = Vec::new();
        let mut document_created_ats = Vec::new();
        let mut document_updated_ats = Vec::new();
        let mut document_published_dates = Vec::new();
        let mut document_summaries = Vec::new();
        let mut document_image_urls = Vec::new();
        let mut document_contents = Vec::new();
        let mut document_source_urls = Vec::new();
        let mut document_notes = Vec::new();
        let mut document_parent_ids = Vec::new();
        let mut document_reading_progresses = Vec::new();
        let mut document_first_opened_ats = Vec::new();
        let mut document_last_opened_ats = Vec::new();
        let mut document_saved_ats = Vec::new();
        let mut document_last_moved_ats = Vec::new();

        for document in documents {
            let published_date = match &document.published_date {
                Some(published_date) => Some(published_date.as_date_time()),
                None => None,
            };

            document_ids.push(&document.id);
            document_urls.push(&document.url);
            document_titles.push(document.title.as_deref());
            document_authors.push(document.author.as_deref());
            document_sources.push(document.source.as_deref());
            document_categories.push(document.category.as_deref());
            document_locations.push(document.location.as_deref());
            document_site_names.push(document.site_name.as_deref());
            document_word_counts.push(document.word_count);
            document_created_ats.push(&document.created_at);
            document_updated_ats.push(&document.updated_at);
            document_published_dates.push(published_date);
            document_summaries.push(document.summary.as_deref());
            document_image_urls.push(document.image_url.as_deref());
            document_contents.push(document.content.as_deref());
            document_source_urls.push(document.source_url.as_deref());
            document_notes.push(document.notes.as_deref());
            document_parent_ids.push(document.parent_id.as_deref());
            document_reading_progresses.push(document.reading_progress);
            document_first_opened_ats.push(document.first_opened_at.as_deref());
            document_last_opened_ats.push(document.last_opened_at.as_deref());
            document_saved_ats.push(&document.saved_at);
            document_last_moved_ats.push(&document.last_moved_at);
        }

        // Build a query with multiple VALUES clauses
        let placeholders: Vec<String> = (0..documents.len())
            .map(|_| "(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)".to_string())
            .collect();
        
        let document_query = format!(
            "INSERT INTO documents (
                id, url, title, author, source, category,
                location, site_name, word_count, created_at,
                updated_at, published_date, summary, image_url,
                content, source_url, notes, parent_id,
                reading_progress, first_opened_at, last_opened_at,
                saved_at, last_moved_at
            )
            VALUES {}
            ON CONFLICT(id) DO UPDATE SET
                url = excluded.url,
                title = excluded.title,
                author = excluded.author,
                source = excluded.source,
                category = excluded.category,
                location = excluded.location,
                site_name = excluded.site_name,
                word_count = excluded.word_count,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                published_date = excluded.published_date,
                summary = excluded.summary,
                image_url = excluded.image_url,
                content = excluded.content,
                source_url = excluded.source_url,
                notes = excluded.notes,
                parent_id = excluded.parent_id,
                reading_progress = excluded.reading_progress,
                first_opened_at = excluded.first_opened_at,
                last_opened_at = excluded.last_opened_at,
                saved_at = excluded.saved_at,
                last_moved_at = excluded.last_moved_at",
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&document_query);
        for i in 0..documents.len() {
            query = query
                .bind(document_ids[i])
                .bind(document_urls[i])
                .bind(document_titles[i])
                .bind(document_authors[i])
                .bind(document_sources[i])
                .bind(document_categories[i])
                .bind(document_locations[i])
                .bind(document_site_names[i])
                .bind(document_word_counts[i])
                .bind(document_created_ats[i])
                .bind(document_updated_ats[i])
                .bind(document_published_dates[i])
                .bind(document_summaries[i])
                .bind(document_image_urls[i])
                .bind(document_contents[i])
                .bind(document_source_urls[i])
                .bind(document_notes[i])
                .bind(document_parent_ids[i])
                .bind(document_reading_progresses[i])
                .bind(document_first_opened_ats[i])
                .bind(document_last_opened_ats[i])
                .bind(document_saved_ats[i])
                .bind(document_last_moved_ats[i]);
        }
        query.execute(&self.pool).await?;

        Ok(())
    }

    pub async fn insert_tags(&self, tags: &[&crate::readwise::Tag]) -> anyhow::Result<()> {
        if tags.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        let placeholders: Vec<String> = (0..tags.len())
            .map(|_| "(?, ?)".to_string())
            .collect();

        let query_str = format!(
            "INSERT INTO tags (id, name)
            VALUES {}
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name",
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&query_str);
        for tag in tags {
            query = query.bind(tag.id).bind(&tag.name);
        }

        query.execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn insert_tag<'a>(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        tag: &Tag,
    ) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO tags (id, name)
            VALUES (?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name
            "#,
            tag.id,
            tag.name,
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn update_sync_state(&self, kind: ReadwiseObjectKind, updated_at: DateTime<Utc>) -> anyhow::Result<()> {
        match kind {
            ReadwiseObjectKind::Book => {
                sqlx::query!(
                    r#"
                    INSERT INTO sync_state (id, last_books_sync)
                    VALUES (1, ?)
                    ON CONFLICT(id) DO UPDATE SET
                        last_books_sync = excluded.last_books_sync
                    "#,
                    updated_at,
                )
                .execute(&self.pool)
                .await?;
            }
            ReadwiseObjectKind::Highlight => {
                sqlx::query!(
                    r#"
                    INSERT INTO sync_state (id, last_highlights_sync)
                    VALUES (1, ?)
                    ON CONFLICT(id) DO UPDATE SET
                        last_highlights_sync = excluded.last_highlights_sync
                    "#,
                    updated_at,
                )
                .execute(&self.pool)
                .await?;
            }
            ReadwiseObjectKind::ReaderDocument => {
                sqlx::query!(
                    r#"
                    INSERT INTO sync_state (id, last_documents_sync)
                    VALUES (1, ?)
                    ON CONFLICT(id) DO UPDATE SET
                        last_documents_sync = excluded.last_documents_sync
                    "#,
                    updated_at,
                )
                .execute(&self.pool)
                .await?;
            }
        }

        Ok(())
    }

    pub async fn get_last_sync(&self, kind: ReadwiseObjectKind) -> anyhow::Result<Option<DateTime<Utc>>> {
        let row = sqlx::query!(
            r#"
            SELECT last_books_sync, last_highlights_sync, last_documents_sync
            FROM sync_state
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|record| match kind {
            ReadwiseObjectKind::Book => record.last_books_sync.map(|dt| dt.and_utc()),
            ReadwiseObjectKind::Highlight => record.last_highlights_sync.map(|dt| dt.and_utc()),
            ReadwiseObjectKind::ReaderDocument => record.last_documents_sync.map(|dt| dt.and_utc()),
        }))
    }

    pub async fn export_to_library(&self) -> anyhow::Result<Library> {
        // Use raw queries to avoid type conversion issues
        let rows = sqlx::query("SELECT * FROM books")
            .fetch_all(&self.pool)
            .await?;
            
        let mut books = Vec::new();
        for row in rows {
            let book = types::Book {
                id: row.get("id"),
                title: row.get("title"),
                author: row.get("author"),
                category: row.get("category"),
                num_highlights: row.get("num_highlights"),
                last_highlight_at: row.get("last_highlight_at"),
                updated: row.get("updated"),
                cover_image_url: row.get("cover_image_url"),
                highlights_url: row.get("highlights_url"),
                source_url: row.get("source_url"),
                asin: row.get("asin"),
            };
            books.push(book.into());
        }

        let rows = sqlx::query("SELECT * FROM highlights")
            .fetch_all(&self.pool)
            .await?;
            
        let mut highlights = Vec::new();
        for row in rows {
            let highlight = types::Highlight {
                id: row.get("id"),
                text: row.get("text"),
                note: row.get("note"),
                location: row.get("location"),
                location_type: row.get("location_type"),
                highlighted_at: row.get("highlighted_at"),
                url: row.get("url"),
                color: row.get("color"),
                updated: row.get("updated"),
                book_id: row.get("book_id"),
            };
            highlights.push(highlight.into());
        }

        let rows = sqlx::query("SELECT * FROM documents")
            .fetch_all(&self.pool)
            .await?;
            
        let mut documents = Vec::new();
        for row in rows {
            let document = types::Document {
                id: row.get("id"),
                url: row.get("url"),
                title: row.get("title"),
                author: row.get("author"),
                source: row.get("source"),
                category: row.get("category"),
                location: row.get("location"),
                site_name: row.get("site_name"),
                word_count: row.get("word_count"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                published_date: row.get("published_date"),
                summary: row.get("summary"),
                image_url: row.get("image_url"),
                content: row.get("content"),
                source_url: row.get("source_url"),
                notes: row.get("notes"),
                parent_id: row.get("parent_id"),
                reading_progress: row.get("reading_progress"),
                first_opened_at: row.get("first_opened_at"),
                last_opened_at: row.get("last_opened_at"),
                saved_at: row.get("saved_at"),
                last_moved_at: row.get("last_moved_at"),
            };
            documents.push(document.into());
        }

        let books_sync = self.get_last_sync(ReadwiseObjectKind::Book).await?.unwrap_or_else(Utc::now);
        let highlights_sync = self.get_last_sync(ReadwiseObjectKind::Highlight).await?.unwrap_or_else(Utc::now);
        let documents_sync = self.get_last_sync(ReadwiseObjectKind::ReaderDocument).await?.unwrap_or_else(Utc::now);

        let overall_last_updated = vec![books_sync, highlights_sync, documents_sync]
            .into_iter()
            .max()
            .unwrap_or_else(Utc::now);

        Ok(Library {
            books,
            highlights,
            documents,
            updated_at: overall_last_updated,
        })
    }
}
