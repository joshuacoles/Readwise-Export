use std::time::Duration;
use chrono::{DateTime, Utc};
use reqwest::header::AUTHORIZATION;
use reqwest::{StatusCode, Url};

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum ContentType {
    Books,
    Highlights,
}

pub struct Readwise {
    token: String,
    api_endpoint: Url,
    api_page_size: i32,
}

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Book {
    pub id: i32,
    pub title: String,
    pub author: String,
    pub category: String,
    pub num_highlights: i32,
    pub last_highlight_at: String,
    pub updated: String,
    pub cover_image_url: String,
    pub highlights_url: String,
    pub source_url: Option<String>,
    pub asin: String,
    pub highlights: Vec<Highlight>,
    pub tags: Vec<Tag>,
}

#[derive(Serialize, Deserialize)]
pub struct Highlight {
    pub id: i32,
    pub text: String,
    pub note: String,
    pub location: i32,
    pub location_type: String,
    pub highlighted_at: String,
    pub url: Option<String>,
    pub color: String,
    pub updated: String,
    pub book_id: i32,
    pub tags: Vec<Tag>,
}

#[derive(Serialize, Deserialize)]
pub struct Tag {
    pub id: i32,
    pub name: String,
}

impl Readwise {
    pub fn new(token: String) -> Self {
        Self {
            token,
            api_endpoint: "https://readwise.io/api/v2".parse().unwrap(),
            api_page_size: 1000,
        }
    }

    pub async fn fetch_books(
        &self,
        last_updated: Option<DateTime<Utc>>,
    ) -> Result<Vec<Book>, anyhow::Error> {
        let mut url = self.api_endpoint.clone();
        url.path_segments_mut()
            .unwrap().push("books");

        url.query_pairs_mut()
            .append_pair("page_size", &self.api_page_size.to_string());

        if let Some(last_updated) = last_updated {
            url.query_pairs_mut()
                .append_pair("updated__gt", &last_updated.to_rfc3339());
        }

        let mut books = vec![];
        let mut next_url = url;

        loop {
            let mut response = reqwest::Client::new()
                .get(next_url.clone())
                .header(AUTHORIZATION, format!("Token {}", self.token))
                .send()
                .await?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry_delay = response.headers()
                    .get("Retry-After")
                    .map(|v| v.to_str().unwrap())
                    .map(|v| v.parse::<u64>().unwrap())
                    .unwrap_or(5);

                tracing::warn!("Rate limited, retrying in {} seconds", retry_delay);
                tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                continue;
            } else if !response.status().is_success() {
                return Err(anyhow::anyhow!("Unexpected response: {:?}", response));
            }

            let mut response = response.json::<CollectionResponse<Book>>()
                .await?;

            books.append(&mut response.results);

            if let Some(next) = response.next {
                next_url = Url::parse(&next).unwrap();
            } else {
                break;
            }
        }

        return Ok(books);
    }
}

#[derive(Debug, Deserialize)]
struct CollectionResponse<T> {
    count: i32,
    next: Option<String>,
    previous: Option<String>,
    results: Vec<T>,
}