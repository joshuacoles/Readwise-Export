use chrono::{DateTime, TimeZone, Utc};
use reqwest::header::AUTHORIZATION;
use reqwest::{StatusCode, Url};
use std::fmt::{Display, Formatter};
use std::time::Duration;

pub struct Readwise {
    token: String,
    api_endpoint: Url,
    api_page_size: i64,
}

use crate::{Library, ReadwiseObjectKind};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub author: Option<String>,
    pub category: String,
    pub num_highlights: i64,
    pub last_highlight_at: Option<String>,
    pub updated: Option<String>,
    pub cover_image_url: Option<String>,
    pub highlights_url: Option<String>,
    pub source_url: Option<String>,
    pub asin: Option<String>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Highlight {
    pub id: i64,
    pub text: String,
    pub note: String,
    pub location: i64,
    pub location_type: String,
    pub highlighted_at: Option<String>,
    pub url: Option<String>,
    pub color: String,
    pub updated: String,
    pub book_id: i64,
    pub tags: Vec<Tag>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Resource {
    Books,
    Highlights,
}

impl Display for Resource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Resource::Books => "books",
            Resource::Highlights => "highlights",
        };

        write!(f, "{}", str)
    }
}

impl Readwise {
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
            api_endpoint: "https://readwise.io/api/v2".parse().unwrap(),
            api_page_size: 1000,
        }
    }

    pub async fn fetch_library(&self, kinds: &[ReadwiseObjectKind]) -> anyhow::Result<Library> {
        Ok(Library {
            books: if kinds.contains(&ReadwiseObjectKind::Book) {
                self.fetch_books(None).await?
            } else {
                vec![]
            },
            highlights: if kinds.contains(&ReadwiseObjectKind::Highlight) {
                self.fetch_highlights(None).await?
            } else {
                vec![]
            },

            documents: if kinds.contains(&ReadwiseObjectKind::ReaderDocument) {
                self.fetch_document_list(None, None).await?
            } else {
                vec![]
            },
            updated_at: Utc::now(),
        })
    }

    pub async fn update_library(
        &self,
        library: &mut Library,
        kinds: &[ReadwiseObjectKind],
    ) -> anyhow::Result<()> {
        let last_updated = library.updated_at;

        if kinds.contains(&ReadwiseObjectKind::Book) {
            library
                .books
                .extend(self.fetch_books(Some(last_updated)).await?);
        }

        if kinds.contains(&ReadwiseObjectKind::Highlight) {
            library
                .highlights
                .extend(self.fetch_highlights(Some(last_updated)).await?);
        }

        if kinds.contains(&ReadwiseObjectKind::ReaderDocument) {
            library
                .documents
                .extend(self.fetch_document_list(Some(last_updated), None).await?);
        }

        library.updated_at = Utc::now();

        Ok(())
    }

    pub async fn fetch_books(
        &self,
        last_updated: Option<DateTime<Utc>>,
    ) -> Result<Vec<Book>, anyhow::Error> {
        self.fetch_paged(Resource::Books, last_updated).await
    }

    pub async fn fetch_highlights(
        &self,
        last_updated: Option<DateTime<Utc>>,
    ) -> Result<Vec<Highlight>, anyhow::Error> {
        self.fetch_paged(Resource::Highlights, last_updated).await
    }

    pub(crate) async fn fetch_paged<T: DeserializeOwned>(
        &self,
        resource: Resource,
        last_updated: Option<DateTime<Utc>>,
    ) -> Result<Vec<T>, anyhow::Error> {
        info!(
            "Fetching {} from Readwise, since {}",
            resource,
            last_updated
                .map(|v| v.to_rfc3339())
                .unwrap_or("[all]".to_string())
        );

        let mut url = self.api_endpoint.clone();
        url.path_segments_mut().unwrap().push(match resource {
            Resource::Books => "books",
            Resource::Highlights => "highlights",
        });

        url.query_pairs_mut()
            .append_pair("page_size", &self.api_page_size.to_string());

        if let Some(last_updated) = last_updated {
            url.query_pairs_mut()
                .append_pair("updated__gt", &last_updated.to_rfc3339());
        }

        debug!("Readwise api url: {}", url);

        let mut entities = vec![];
        let mut next_url = url.clone();

        loop {
            let response = reqwest::Client::new()
                .get(next_url.clone())
                .header(AUTHORIZATION, format!("Token {}", self.token))
                .send()
                .await?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry_delay = response
                    .headers()
                    .get("Retry-After")
                    .map(|v| v.to_str().unwrap())
                    .map(|v| v.parse::<u64>().unwrap())
                    .unwrap_or(5);

                debug!("Rate limited, retrying in {} seconds", retry_delay);

                tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                continue;
            } else if !response.status().is_success() {
                return Err(anyhow::anyhow!("Unexpected response: {:?}", response));
            }

            let mut response = response.json::<CollectionResponse<T>>().await?;

            debug!(
                "Received api response: count={count}, next={next:?}, previous={previous:?}",
                count = response.count,
                next = response.next,
                previous = response.previous,
            );

            entities.append(&mut response.results);

            if let Some(next) = response.next {
                next_url = Url::parse(&next).unwrap();
            } else {
                break;
            }
        }

        return Ok(entities);
    }

    pub async fn fetch_document_list(
        &self,
        updated_after: Option<DateTime<Utc>>,
        location: Option<String>,
    ) -> Result<Vec<Document>, anyhow::Error> {
        info!(
            "Fetching reader documents from Readwise, since {}",
            updated_after
                .map(|v| v.to_rfc3339())
                .unwrap_or("[all]".to_string())
        );

        let base_url = Url::parse("https://readwise.io/api/v3/list").unwrap();
        let mut full_data = Vec::new();
        let mut next_page_cursor: Option<String> = None;

        loop {
            let mut url = base_url.clone();

            {
                let mut query_params = url.query_pairs_mut();

                if let Some(cursor) = &next_page_cursor {
                    query_params.append_pair("pageCursor", cursor);
                }

                if let Some(updated) = updated_after {
                    query_params.append_pair("updatedAfter", &updated.to_rfc3339());
                }

                if let Some(loc) = &location {
                    query_params.append_pair("location", loc);
                }
            }

            debug!(
                "Making export api request with params: {}",
                url.query().unwrap_or("")
            );

            let response = reqwest::Client::new()
                .get(url.clone())
                .header(AUTHORIZATION, format!("Token {}", self.token))
                .send()
                .await?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry_delay = response
                    .headers()
                    .get("Retry-After")
                    .map(|v| v.to_str().unwrap())
                    .map(|v| v.parse::<u64>().unwrap())
                    .unwrap_or(5);

                debug!("Rate limited, retrying in {} seconds", retry_delay);

                tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                continue;
            } else if !response.status().is_success() {
                return Err(anyhow::anyhow!("Unexpected response: {:?}", response));
            }

            let raw = response.json::<Value>().await?;
            debug!("Raw result {:?}", raw);

            let response_json: DocumentListResponse = serde_json::from_value(raw)?;

            debug!(
                "Received api response: results={}, next_cursor={:?}",
                response_json.results.len(),
                response_json.next_page_cursor
            );

            full_data.extend(response_json.results);
            next_page_cursor = response_json.next_page_cursor;

            if next_page_cursor.is_none() {
                break;
            } else {
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }

        debug!("Fetched {} documents total", full_data.len());

        Ok(full_data)
    }
}

#[derive(Debug, Deserialize)]
struct CollectionResponse<T> {
    count: i64,
    next: Option<String>,
    previous: Option<String>,
    results: Vec<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentListResponse {
    results: Vec<Document>,
    next_page_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub source: Option<String>,
    pub category: Option<String>,
    pub location: Option<String>,
    pub tags: Option<Value>,
    pub site_name: Option<String>,
    pub word_count: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub published_date: Option<PublishedDate>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PublishedDate {
    Integer(i64),
    String(String),
}

impl PublishedDate {
    pub(crate) fn as_date_time(&self) -> chrono::DateTime<Utc> {
        match self {
            PublishedDate::Integer(i) => Utc.timestamp_millis_opt(*i).unwrap(),
            PublishedDate::String(s) => s.parse().unwrap(),
        }
    }
}
