use anyhow::{anyhow, Context as _};
use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use futures::stream::StreamExt;
use readwise_common::{Database, ReadwiseObjectKind};
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{info, warn};

mod readwise;

#[derive(Debug, Parser, Deserialize)]
struct Cli {
    /// Database URL (sqlite://path/to/db.sqlite or postgresql://user:pass@host/db)
    #[arg(long, env = "DATABASE_URL", default_value = "./readwise.db")]
    database_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Parser, Deserialize)]
enum Commands {
    /// Fetch data from Readwise API
    Fetch(FetchCommand),
}

#[derive(Debug, Parser, Deserialize)]
struct FetchCommand {
    /// Readwise API token
    #[arg(long, env = "READWISE_API_TOKEN")]
    api_token: String,

    /// The strategy to use when fetching data from the Readwise API
    #[arg(long, default_value = "update")]
    strategy: FetchStrategy,

    /// Only export the listed kind of records from readwise. Allows multiple.
    #[arg(long, short)]
    kind: Vec<ReadwiseObjectKind>,

    /// The location of the library cache file (deprecated, for compatibility)
    #[arg(long)]
    library: Option<PathBuf>,
}

#[derive(ValueEnum, Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
enum FetchStrategy {
    /// Ask for updates from the Readwise API since the last update to the library cache
    Update,

    /// Refetch the whole library from the Readwise API
    Refetch,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let db = Database::new(&cli.database_url).await?;

    match &cli.command {
        Commands::Fetch(fetch_cmd) => {
            let kinds_to_fetch = if fetch_cmd.kind.is_empty() {
                vec![
                    ReadwiseObjectKind::ReaderDocument,
                    ReadwiseObjectKind::Book,
                    ReadwiseObjectKind::Highlight,
                ]
            } else {
                fetch_cmd.kind.clone()
            };

            let readwise = readwise::Readwise::new(&fetch_cmd.api_token);

            for kind in kinds_to_fetch {
                let last_sync = match fetch_cmd.strategy {
                    FetchStrategy::Update => db.get_last_sync(kind).await?,
                    FetchStrategy::Refetch => None,
                };

                if let Some(last_sync_time) = last_sync {
                    info!("Fetching {:?} updates since {}", kind, last_sync_time);
                } else {
                    info!("Fetching all {:?} from readwise", kind);
                }

                match kind {
                    ReadwiseObjectKind::Book => {
                        info!("Starting to stream books from Readwise API");
                        let mut book_stream = readwise.fetch_books_stream(last_sync);
                        
                        while let Some(chunk_result) = book_stream.next().await {
                            match chunk_result {
                                Ok(books_chunk) => {
                                    if !books_chunk.is_empty() {
                                        info!("Processing {} books in current chunk", books_chunk.len());
                                        // Convert readwise books to library books
                                        let library_books: Vec<_> = books_chunk.into_iter().map(Into::into).collect();
                                        let book_refs: Vec<&_> = library_books.iter().collect();
                                        db.insert_books(&book_refs).await?;
                                    }
                                }
                                Err(e) => return Err(anyhow!("Failed to fetch books chunk: {}", e)),
                            }
                        }
                        db.update_sync_state(ReadwiseObjectKind::Book, Utc::now()).await?;
                        info!("Finished processing all book chunks");
                    }
                    ReadwiseObjectKind::Highlight => {
                        info!("Starting to stream highlights from Readwise API");
                        let mut highlight_stream = readwise.fetch_highlights_stream(last_sync);
                        
                        while let Some(chunk_result) = highlight_stream.next().await {
                            match chunk_result {
                                Ok(highlights_chunk) => {
                                    if !highlights_chunk.is_empty() {
                                        info!("Processing {} highlights in current chunk", highlights_chunk.len());
                                        // Convert readwise highlights to library highlights
                                        let library_highlights: Vec<_> = highlights_chunk.into_iter().map(Into::into).collect();
                                        let highlight_refs: Vec<&_> = library_highlights.iter().collect();
                                        db.insert_highlights(&highlight_refs).await?;
                                    }
                                }
                                Err(e) => return Err(anyhow!("Failed to fetch highlights chunk: {}", e)),
                            }
                        }
                        db.update_sync_state(ReadwiseObjectKind::Highlight, Utc::now()).await?;
                        info!("Finished processing all highlight chunks");
                    }
                    ReadwiseObjectKind::ReaderDocument => {
                        info!("Starting to stream documents from Readwise API");
                        let mut document_stream = readwise.fetch_documents_stream(last_sync, None);
                        
                        while let Some(chunk_result) = document_stream.next().await {
                            match chunk_result {
                                Ok(documents_chunk) => {
                                    if !documents_chunk.is_empty() {
                                        info!("Processing {} documents in current chunk", documents_chunk.len());
                                        // Convert readwise documents to library documents
                                        let library_documents: Vec<_> = documents_chunk.into_iter().map(Into::into).collect();
                                        let document_refs: Vec<&_> = library_documents.iter().collect();
                                        db.insert_documents(&document_refs).await?;
                                    }
                                }
                                Err(e) => return Err(anyhow!("Failed to fetch documents chunk: {}", e)),
                            }
                        }
                        db.update_sync_state(ReadwiseObjectKind::ReaderDocument, Utc::now()).await?;
                        info!("Finished processing all document chunks");
                    }
                }
            }

            // If legacy library file is specified, export to JSON for compatibility
            if let Some(library_path) = &fetch_cmd.library {
                info!("Exporting to legacy JSON format at {:?}", library_path);
                let library = db.export_to_library().await?;
                serde_json::to_writer(std::fs::File::create(library_path)?, &library)?;
            }
        }
    }

    Ok(())
}