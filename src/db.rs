use anyhow::Context;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use crate::{Book, Document, Highlight, Library, Tag};

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

            // Handle PublishedDate enum
            match &document.published_date {
                Some(date) => match date {
                    crate::readwise::PublishedDate::Integer(i) => i.to_string(),
                    crate::readwise::PublishedDate::String(s) => s.clone(),
                },
                None => None,
            },

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

    async fn insert_tag<'a>(&self, tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, tag: &Tag) -> anyhow::Result<()> {
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
            updated_at.to_rfc3339(),
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

        match result {
            Some(row) => Ok(Some(DateTime::parse_from_rfc3339(&row.last_updated)?.with_timezone(&Utc))),
            None => Ok(None),
        }
    }

    pub async fn export_to_library(&self) -> anyhow::Result<Library> {
        let mut books = sqlx::query_as!(
            Book,
            r#"SELECT * FROM books"#
        )
        .fetch_all(&self.pool)
        .await?;

        let mut highlights = sqlx::query_as!(
            Highlight,
            r#"SELECT * FROM highlights"#
        )
        .fetch_all(&self.pool)
        .await?;

        let documents = sqlx::query_as!(
            Document,
            r#"SELECT * FROM documents"#
        )
        .fetch_all(&self.pool)
        .await?;

        // Fetch tags for books
        for book in &mut books {
            let tags = sqlx::query_as!(
                Tag,
                r#"
                SELECT t.* FROM tags t
                JOIN book_tags bt ON bt.tag_id = t.id
                WHERE bt.book_id = ?
                "#,
                book.id
            )
            .fetch_all(&self.pool)
            .await?;
            book.tags = tags;
        }

        // Fetch tags for highlights
        for highlight in &mut highlights {
            let tags = sqlx::query_as!(
                Tag,
                r#"
                SELECT t.* FROM tags t
                JOIN highlight_tags ht ON ht.tag_id = t.id
                WHERE ht.highlight_id = ?
                "#,
                highlight.id
            )
            .fetch_all(&self.pool)
            .await?;
            highlight.tags = tags;
        }

        let last_updated = self.get_last_sync().await?.unwrap_or_else(Utc::now);

        Ok(Library {
            books,
            highlights,
            documents,
            updated_at: last_updated,
        })
    }
}
