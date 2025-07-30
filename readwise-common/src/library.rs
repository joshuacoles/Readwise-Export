use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use itertools::Itertools;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_date: Option<DateTime<Utc>>,
    pub summary: Option<String>,
    pub image_url: Option<String>,
    pub content: Option<String>,
    pub source_url: Option<String>,
    pub notes: Option<String>,
    pub parent_id: Option<String>,
    pub reading_progress: f64,
    pub first_opened_at: Option<DateTime<Utc>>,
    pub last_opened_at: Option<DateTime<Utc>>,
    pub saved_at: DateTime<Utc>,
    pub last_moved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    pub category: String,
    pub num_highlights: i64,
    pub last_highlight_at: Option<DateTime<Utc>>,
    pub updated: Option<DateTime<Utc>>,
    pub cover_image_url: Option<String>,
    pub highlights_url: Option<String>,
    pub source_url: Option<String>,
    pub asin: Option<String>,
    pub tags: Vec<crate::Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Highlight {
    pub id: i64,
    pub text: String,
    pub note: String,
    pub location: i64,
    pub location_type: String,
    pub highlighted_at: Option<DateTime<Utc>>,
    pub url: Option<String>,
    pub color: String,
    pub updated: DateTime<Utc>,
    pub book_id: i64,
    pub tags: Vec<crate::Tag>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Library {
    #[serde(default)]
    pub books: Vec<Book>,

    #[serde(default)]
    pub highlights: Vec<Highlight>,

    #[serde(default)]
    pub documents: Vec<Document>,

    pub updated_at: DateTime<Utc>,
}

impl Library {
    pub fn highlights_for(&self, book: &Book) -> Vec<&Highlight> {
        self.highlights
            .iter()
            .filter(|h| h.book_id == book.id)
            .collect_vec()
    }
}
