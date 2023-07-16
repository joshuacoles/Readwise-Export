use std::fmt::{Display, Formatter};
use std::time::Duration;
use chrono::{DateTime, Utc};
use reqwest::header::AUTHORIZATION;
use reqwest::{StatusCode, Url};

pub struct Readwise {
    token: String,
    api_endpoint: Url,
    api_page_size: i32,
}

use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use tracing::{debug, info};
use crate::Library;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub id: i32,
    pub title: String,
    pub author: Option<String>,
    pub category: String,
    pub num_highlights: i32,
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
    pub id: i32,
    pub text: String,
    pub note: String,
    pub location: i32,
    pub location_type: String,
    pub highlighted_at: Option<String>,
    pub url: Option<String>,
    pub color: String,
    pub updated: String,
    pub book_id: i32,
    pub tags: Vec<Tag>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tag {
    pub id: i32,
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

    pub async fn fetch_library(
        &self,
    ) -> anyhow::Result<Library> {
        Ok(Library {
            books: self.fetch_books(None).await?,
            highlights: self.fetch_highlights(None).await?,
            updated_at: Utc::now(),
        })
    }

    pub async fn update_library(
        &self,
        library: &mut Library,
    ) -> anyhow::Result<()> {
        let last_updated = library.updated_at;

        library.books.extend(self.fetch_books(Some(last_updated)).await?);
        library.highlights.extend(self.fetch_highlights(Some(last_updated)).await?);
        library.updated_at = Utc::now();

        Ok(())
    }

    pub async fn fetch_books(
        &self,
        last_updated: Option<DateTime<Utc>>,
    ) -> Result<Vec<Book>, anyhow::Error> {
        self.fetch_paged(
            Resource::Books,
            last_updated,
        ).await
    }

    pub async fn fetch_highlights(
        &self,
        last_updated: Option<DateTime<Utc>>,
    ) -> Result<Vec<Highlight>, anyhow::Error> {
        self.fetch_paged(
            Resource::Highlights,
            last_updated,
        ).await
    }

    pub(crate) async fn fetch_paged<T: DeserializeOwned>(&self, resource: Resource, last_updated: Option<DateTime<Utc>>) -> Result<Vec<T>, anyhow::Error> {
        info!(
            "Fetching {} from Readwise, since {}",
            resource,
            last_updated.map(|v| v.to_rfc3339()).unwrap_or("[all]".to_string())
        );

        let mut url = self.api_endpoint.clone();
        url.path_segments_mut()
            .unwrap().push(match resource {
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
                .get(dbg!(next_url.clone()))
                .header(AUTHORIZATION, format!("Token {}", self.token))
                .send()
                .await?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                let retry_delay = response.headers()
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

            let mut response = response.json::<CollectionResponse<T>>()
                .await?;

            debug!(
                "Received api response: count={count}, next={next:?}, previous={previous:?}",
                count=response.count,
                next=response.next,
                previous=response.previous,
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
}

#[derive(Debug, Deserialize)]
struct CollectionResponse<T> {
    count: i32,
    next: Option<String>,
    previous: Option<String>,
    results: Vec<T>,
}
