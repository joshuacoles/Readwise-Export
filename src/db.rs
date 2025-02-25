use crate::{Book, Document, Highlight, Library, Tag};
use anyhow::Context;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

mod types {
    use chrono::{DateTime, Utc, NaiveDateTime};
    use crate::{readwise, library};

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
                published_date: self.published_date.map(|dt| dt.and_utc()),
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
                last_highlight_at: self.last_highlight_at.map(|dt| dt.and_utc()),
                updated: self.updated.map(|dt| dt.and_utc()),
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
                highlighted_at: self.highlighted_at,
                url: self.url,
                color: self.color,
                updated: self.updated,
                book_id: self.book_id,
            }
        }
    }
}

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        let pool = SqlitePool::connect(database_url)
            .await
            .context("Failed to connect to database")?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("Failed to run migrations")?;

        Ok(Self { pool })
    }

    pub async fn insert_book(&self, book: &Book) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT INTO books (
                id, title, author, category, num_highlights,
                last_highlight_at, updated, cover_image_url,
                highlights_url, source_url, asin
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                asin = excluded.asin
            "#,
            book.id,
            book.title,
            book.author,
            book.category,
            book.num_highlights,
            book.last_highlight_at,
            book.updated,
            book.cover_image_url,
            book.highlights_url,
            book.source_url,
            book.asin,
        )
        .execute(&mut *tx)
        .await?;

        // Handle tags
        for tag in &book.tags {
            self.insert_tag(&mut tx, tag).await?;
            sqlx::query!(
                r#"
                INSERT INTO book_tags (book_id, tag_id)
                VALUES (?, ?)
                ON CONFLICT DO NOTHING
                "#,
                book.id,
                tag.id,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn insert_highlight(&self, highlight: &Highlight) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT INTO highlights (
                id, text, note, location, location_type,
                highlighted_at, url, color, updated, book_id
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                text = excluded.text,
                note = excluded.note,
                location = excluded.location,
                location_type = excluded.location_type,
                highlighted_at = excluded.highlighted_at,
                url = excluded.url,
                color = excluded.color,
                updated = excluded.updated,
                book_id = excluded.book_id
            "#,
            highlight.id,
            highlight.text,
            highlight.note,
            highlight.location,
            highlight.location_type,
            highlight.highlighted_at,
            highlight.url,
            highlight.color,
            highlight.updated,
            highlight.book_id,
        )
        .execute(&mut *tx)
        .await?;

        // Handle tags
        for tag in &highlight.tags {
            self.insert_tag(&mut tx, tag).await?;
            sqlx::query!(
                r#"
                INSERT INTO highlight_tags (highlight_id, tag_id)
                VALUES (?, ?)
                ON CONFLICT DO NOTHING
                "#,
                highlight.id,
                tag.id,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn insert_document(&self, document: &Document) -> anyhow::Result<()> {
        let published_date = match &document.published_date {
            Some(published_date) => Some(published_date.as_date_time()),
            None => None,
        };

        sqlx::query!(
            r#"
            INSERT INTO documents (
                id, url, title, author, source, category,
                location, site_name, word_count, created_at,
                updated_at, published_date, summary, image_url,
                content, source_url, notes, parent_id,
                reading_progress, first_opened_at, last_opened_at,
                saved_at, last_moved_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                last_moved_at = excluded.last_moved_at
            "#,
            document.id,
            document.url,
            document.title,
            document.author,
            document.source,
            document.category,
            document.location,
            document.site_name,
            document.word_count,
            document.created_at,
            document.updated_at,
            published_date,
            document.summary,
            document.image_url,
            document.content,
            document.source_url,
            document.notes,
            document.parent_id,
            document.reading_progress,
            document.first_opened_at,
            document.last_opened_at,
            document.saved_at,
            document.last_moved_at,
        )
        .execute(&self.pool)
        .await?;

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

    pub async fn update_sync_state(&self, updated_at: DateTime<Utc>) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO sync_state (id, last_updated)
            VALUES (1, ?)
            ON CONFLICT(id) DO UPDATE SET
                last_updated = excluded.last_updated
            "#,
            updated_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_last_sync(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        let result = sqlx::query!(
            r#"
            SELECT last_updated FROM sync_state
            WHERE id = 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result
            .and_then(|record| record.last_updated)
            .map(|last_updated| last_updated.and_utc()))
    }

    pub async fn export_to_library(&self) -> anyhow::Result<Library> {
        let mut books = sqlx::query_as!(types::Book, r#"SELECT * FROM books"#)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|book| book.into())
            .collect();

        let mut highlights = sqlx::query_as!(types::Highlight, r#"SELECT * FROM highlights"#)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|highlight| highlight.into())
            .collect();

        let documents = sqlx::query_as!(types::Document, r#"SELECT * FROM documents"#)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|document| document.into())
            .collect();

        // // Fetch tags for books
        // for book in &mut books {
        //     let tags = sqlx::query_as!(
        //         Tag,
        //         r#"
        //         SELECT t.* FROM tags t
        //         JOIN book_tags bt ON bt.tag_id = t.id
        //         WHERE bt.book_id = ?
        //         "#,
        //         book.id
        //     )
        //     .fetch_all(&self.pool)
        //     .await?;
        //     book.tags = tags;
        // }

        // // Fetch tags for highlights
        // for highlight in &mut highlights {
        //     let tags = sqlx::query_as!(
        //         Tag,
        //         r#"
        //         SELECT t.* FROM tags t
        //         JOIN highlight_tags ht ON ht.tag_id = t.id
        //         WHERE ht.highlight_id = ?
        //         "#,
        //         highlight.id
        //     )
        //     .fetch_all(&self.pool)
        //     .await?;
        //     highlight.tags = tags;
        // }

        let last_updated = self.get_last_sync().await?.unwrap_or_else(Utc::now);

        Ok(Library {
            books,
            highlights,
            documents,
            updated_at: last_updated,
        })
    }
}
