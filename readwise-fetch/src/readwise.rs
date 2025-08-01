use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use futures::stream::Stream;
use reqwest::header::AUTHORIZATION;
use reqwest::{StatusCode, Url};
use std::fmt::{Display, Formatter};
use std::pin::Pin;
use std::time::Duration;

pub struct Readwise {
    token: String,
    api_endpoint: Url,
    api_page_size: i64,
}

use readwise_common::{Library, ReadwiseObjectKind, Tag};
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

impl From<Book> for readwise_common::Book {
    fn from(book: Book) -> Self {
        readwise_common::Book {
            id: book.id,
            title: book.title,
            author: book.author,
            category: book.category,
            num_highlights: book.num_highlights,
            last_highlight_at: book
                .last_highlight_at
                .as_deref()
                .map(|s| s.parse().ok())
                .flatten(),
            updated: book.updated.as_deref().map(|s| s.parse().ok()).flatten(),
            cover_image_url: book.cover_image_url,
            highlights_url: book.highlights_url,
            source_url: book.source_url,
            asin: book.asin,
            tags: book.tags,
        }
    }
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

impl From<Highlight> for readwise_common::Highlight {
    fn from(highlight: Highlight) -> Self {
        readwise_common::Highlight {
            id: highlight.id,
            text: highlight.text,
            note: highlight.note,
            location: highlight.location,
            location_type: highlight.location_type,
            highlighted_at: highlight
                .highlighted_at
                .as_deref()
                .map(|s| s.parse().ok())
                .flatten(),
            url: highlight.url,
            color: highlight.color,
            updated: highlight.updated.parse().unwrap(),
            book_id: highlight.book_id,
            tags: highlight.tags,
        }
    }
}

// Tag is now imported from readwise_common

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

    pub fn fetch_books_stream(
        &self,
        last_updated: Option<DateTime<Utc>>,
    ) -> Pin<Box<dyn Stream<Item = Result<Vec<Book>, anyhow::Error>> + Send + '_>> {
        self.fetch_paged_stream(Resource::Books, last_updated)
    }

    pub fn fetch_highlights_stream(
        &self,
        last_updated: Option<DateTime<Utc>>,
    ) -> Pin<Box<dyn Stream<Item = Result<Vec<Highlight>, anyhow::Error>> + Send + '_>> {
        self.fetch_paged_stream(Resource::Highlights, last_updated)
    }

    pub(crate) fn fetch_paged_stream<T: DeserializeOwned + Send + 'static>(
        &self,
        resource: Resource,
        last_updated: Option<DateTime<Utc>>,
    ) -> Pin<Box<dyn Stream<Item = Result<Vec<T>, anyhow::Error>> + Send + '_>> {
        let token = self.token.clone();
        let api_endpoint = self.api_endpoint.clone();
        let api_page_size = self.api_page_size;

        info!(
            "Starting streaming fetch of {} from Readwise, since {}",
            resource,
            last_updated
                .map(|v| v.to_rfc3339())
                .unwrap_or("[all]".to_string())
        );

        let mut url = api_endpoint;
        url.path_segments_mut().unwrap().push(match resource {
            Resource::Books => "books",
            Resource::Highlights => "highlights",
        });

        url.query_pairs_mut()
            .append_pair("page_size", &api_page_size.to_string());

        if let Some(last_updated) = last_updated {
            url.query_pairs_mut()
                .append_pair("updated__gt", &last_updated.to_rfc3339());
        }

        debug!("Readwise api url: {}", url);

        let stream = async_stream::stream! {
            let mut next_url = Some(url);

            while let Some(current_url) = next_url {
                loop {
                    let response = match reqwest::Client::new()
                        .get(current_url.clone())
                        .header(AUTHORIZATION, format!("Token {}", token))
                        .send()
                        .await
                    {
                        Ok(response) => response,
                        Err(e) => {
                            yield Err(anyhow::anyhow!("Request failed: {}", e));
                            return;
                        }
                    };

                    if response.status() == StatusCode::TOO_MANY_REQUESTS {
                        let retry_delay = response
                            .headers()
                            .get("Retry-After")
                            .map(|v| v.to_str().unwrap_or("5"))
                            .map(|v| v.parse::<u64>().unwrap_or(5))
                            .unwrap_or(5);

                        debug!("Rate limited, retrying in {} seconds", retry_delay);
                        tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                        continue;
                    } else if !response.status().is_success() {
                        yield Err(anyhow::anyhow!("Unexpected response: {:?}", response));
                        return;
                    }

                    let response_json = match response.json::<CollectionResponse<T>>().await {
                        Ok(json) => json,
                        Err(e) => {
                            yield Err(anyhow::anyhow!("Failed to parse JSON: {}", e));
                            return;
                        }
                    };

                    debug!(
                        "Received api response: count={count}, next={next:?}, previous={previous:?}, results={results}",
                        count = response_json.count,
                        next = response_json.next,
                        previous = response_json.previous,
                        results = response_json.results.len(),
                    );

                    // Yield the current page of results
                    yield Ok(response_json.results);

                    // Set up for next iteration
                    if let Some(next) = response_json.next {
                        match Url::parse(&next) {
                            Ok(parsed_url) => {
                                next_url = Some(parsed_url);
                                break; // Break the retry loop, continue with next page
                            }
                            Err(e) => {
                                yield Err(anyhow::anyhow!("Failed to parse next URL: {}", e));
                                return;
                            }
                        }
                    } else {
                        next_url = None;
                        break; // No more pages
                    }
                }
            }
        };

        Box::pin(stream)
    }

    pub fn fetch_documents_stream(
        &self,
        updated_after: Option<DateTime<Utc>>,
        location: Option<String>,
    ) -> Pin<Box<dyn Stream<Item = Result<Vec<Document>, anyhow::Error>> + Send + '_>> {
        let token = self.token.clone();

        info!(
            "Starting streaming fetch of reader documents from Readwise, since {}",
            updated_after
                .map(|v| v.to_rfc3339())
                .unwrap_or("[all]".to_string())
        );

        let stream = async_stream::stream! {
            let base_url = match Url::parse("https://readwise.io/api/v3/list") {
                Ok(url) => url,
                Err(e) => {
                    yield Err(anyhow::anyhow!("Failed to parse base URL: {}", e));
                    return;
                }
            };
            
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

                loop {
                    let response = match reqwest::Client::new()
                        .get(url.clone())
                        .header(AUTHORIZATION, format!("Token {}", token))
                        .send()
                        .await
                    {
                        Ok(response) => response,
                        Err(e) => {
                            yield Err(anyhow::anyhow!("Request failed: {}", e));
                            return;
                        }
                    };

                    if response.status() == StatusCode::TOO_MANY_REQUESTS {
                        let retry_delay = response
                            .headers()
                            .get("Retry-After")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(5);

                        debug!("Rate limited, retrying in {} seconds", retry_delay);
                        tokio::time::sleep(Duration::from_secs(retry_delay)).await;
                        continue;
                    } else if !response.status().is_success() {
                        yield Err(anyhow::anyhow!("Unexpected response: {:?}", response));
                        return;
                    }

                    let raw = match response.json::<Value>().await {
                        Ok(json) => json,
                        Err(e) => {
                            yield Err(anyhow::anyhow!("Failed to parse JSON: {}", e));
                            return;
                        }
                    };
                    
                    debug!("Raw result {:?}", raw);

                    let response_json: DocumentListResponse = match serde_json::from_value(raw) {
                        Ok(response) => response,
                        Err(e) => {
                            yield Err(anyhow::anyhow!("Failed to deserialize document list response: {}", e));
                            return;
                        }
                    };

                    debug!(
                        "Received api response: results={}, next_cursor={:?}",
                        response_json.results.len(),
                        response_json.next_page_cursor
                    );

                    // Yield the current page of results
                    yield Ok(response_json.results);

                    // Set up for next iteration
                    next_page_cursor = response_json.next_page_cursor;

                    if next_page_cursor.is_none() {
                        return; // No more pages
                    } else {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        break; // Break the retry loop, continue with next page
                    }
                }
            }
        };

        Box::pin(stream)
    }

    // fetch_document_list removed - only streaming methods are used now
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

impl From<Document> for readwise_common::Document {
    fn from(document: Document) -> Self {
        readwise_common::Document {
            id: document.id,
            url: document.url,
            title: document.title,
            author: document.author,
            source: document.source,
            category: document.category,
            location: document.location,
            site_name: document.site_name,
            word_count: document.word_count,
            created_at: document.created_at.parse().unwrap(),
            updated_at: document.updated_at.parse().unwrap(),
            published_date: document.published_date.map(|pd| pd.as_date_time()),
            summary: document.summary,
            image_url: document.image_url,
            content: document.content,
            source_url: document.source_url,
            notes: document.notes,
            parent_id: document.parent_id,
            reading_progress: document.reading_progress,
            first_opened_at: document.first_opened_at.map(|s| s.parse().unwrap()),
            last_opened_at: document.last_opened_at.map(|s| s.parse().unwrap()),
            saved_at: document.saved_at.parse().unwrap(),
            last_moved_at: document.last_moved_at.parse().unwrap(),
        }
    }
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
            PublishedDate::String(s) => s
                .parse()
                .or_else(|_| {
                    NaiveDate::parse_from_str(&s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
                })
                .unwrap(),
        }
    }
}

#[test]
fn test_p_d() {
    let x = "2023-11-24";
    dbg!(PublishedDate::String(x.to_string()).as_date_time());
}
