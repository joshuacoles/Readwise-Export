use crate::library::{Book, Document, Highlight};
use anyhow::{anyhow, Context as _};
use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use futures::stream::StreamExt;
use itertools::Itertools;
use obsidian_rust_interface::joining::strategies::TypeAndKey;
use obsidian_rust_interface::joining::JoinedNote;
use obsidian_rust_interface::{NoteReference, Vault};
use regex::Regex;
use scripting::ScriptType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tera::{Context, Tera};
use tracing::{debug, info, warn};

mod db;
mod readwise;
mod scripting;
mod library;

#[derive(Debug, Parser, Deserialize)]
struct Cli {
    /// The location of the library cache file (deprecated, use --database-url instead)
    #[arg(long)]
    library: Option<PathBuf>,

    /// SQLite database path
    #[arg(long, env = "DATABASE_PATH", default_value = "./readwise.db")]
    database_path: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Parser, Deserialize)]
enum Commands {
    /// Fetch data from Readwise API
    Fetch(FetchCommand),

    /// Export highlights to markdown files
    Export(ExportCommand),

    /// Export database to JSON format
    ExportJson(ExportJsonCommand),
}

#[derive(Debug, Parser, Deserialize)]
struct ExportJsonCommand {
    /// Path to output JSON file
    #[arg(long)]
    output: PathBuf,
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
}

#[derive(ValueEnum, Debug, Clone, Copy, Deserialize, Eq, PartialEq)]
enum ReadwiseObjectKind {
    Book,
    Highlight,
    ReaderDocument,
}

#[derive(Debug, Parser, Deserialize)]
struct ExportCommand {
    /// The root of the obsidian vault
    #[arg(long)]
    vault: PathBuf,

    /// The location within the obsidian vault where the Readwise files are stored, relative to the
    /// vault root.
    #[arg(long)]
    base_folder: String,

    /// If custom metadata should be written, a script to generate it
    #[arg(long)]
    metadata_script: Option<PathBuf>,

    /// The template used for the initial contents of a book note. The highlights will be rendered
    /// directly after this initial content.
    #[arg(long)]
    book_template: PathBuf,

    /// The template used for each highlight in a book note. These will be rendered after the end
    /// of the book note template, with an inserted %% HIGHLIGHTS_BEGIN %% tag separating the two
    /// sections.
    #[arg(long)]
    highlight_template: PathBuf,

    /// The strategy to use when replacing existing notes
    #[arg(long, default_value = "update")]
    replacement_strategy: ReplacementStrategy,

    /// Mark notes as stranded if they no longer correspond to a Readwise book
    #[arg(long)]
    mark_stranded: bool,

    /// If true, will skip exporting books with no highlights
    #[arg(long, default_value = "true")]
    skip_empty: bool,

    /// If set, will only export books from this category
    #[arg(long)]
    filter_category: Option<String>,
}

#[derive(ValueEnum, Debug, Clone, Deserialize)]
enum ReplacementStrategy {
    /// Update the highlights in the existing files wherever they are located, create new files for new books in the
    /// default location.
    Update,

    /// Replace the contents of the existing files for books which already exist but leave them where they are located,
    /// create new files for new books in the default location.
    Replace,

    /// Create new files for all books in the default location, ignoring existing files.
    IgnoreExisting,
}

#[derive(ValueEnum, Debug, Clone, Deserialize)]
enum FetchStrategy {
    /// Ask for updates from the Readwise API since the last update to the library cache
    Update,

    /// Refetch the whole library from the Readwise API
    Refetch,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Library {
    #[serde(default)]
    books: Vec<Book>,

    #[serde(default)]
    highlights: Vec<Highlight>,

    #[serde(default)]
    documents: Vec<Document>,

    updated_at: DateTime<Utc>,
}

impl Library {
    fn highlights_for(&self, book: &Book) -> Vec<&Highlight> {
        self.highlights
            .iter()
            .filter(|h| h.book_id == book.id)
            .collect_vec()
    }
}

struct Exporter {
    sanitizer: Regex,
    export_root: PathBuf,
    library: Library,

    templates: Tera,
    metadata_script: Option<ScriptType>,

    remaining_existing: HashMap<i64, NoteReference>,

    replacement_strategy: ReplacementStrategy,
    skip_empty: bool,
    filter_category: Option<String>,
}

impl Exporter {
    fn new(library: Library, cli: &ExportCommand) -> anyhow::Result<Self> {
        let metadata_script = match &cli.metadata_script {
            None => None,
            Some(path) => Some(ScriptType::new(path)?),
        };

        let vault = Vault::open(&cli.vault);
        let existing = obsidian_rust_interface::joining::find_by::<_, i64>(
            &vault,
            &TypeAndKey {
                type_key: "note-kind".to_string(),
                note_type: "readwise".to_string(),
                id_key: "__readwise_fk".to_string(),
            },
        );

        debug!("Found {} existing notes", existing.len());

        Ok(Exporter {
            library,
            export_root: cli.vault.join(&cli.base_folder),
            templates: {
                let mut tera = Tera::default();
                tera.add_template_file(&cli.book_template, Some("book"))?;
                tera.add_template_file(&cli.highlight_template, Some("highlight"))?;

                debug!(
                    "Loaded tera templates for markdown. Templates: {}",
                    tera.get_template_names().join(", ")
                );

                tera
            },
            metadata_script,

            replacement_strategy: cli.replacement_strategy.clone(),
            sanitizer: Regex::new(r#"[<>"'/\\|?*]+"#).unwrap(),
            remaining_existing: existing,
            skip_empty: cli.skip_empty,
            filter_category: cli.filter_category.clone(),
        })
    }

    fn export(&mut self) -> anyhow::Result<()> {
        let by_category = self
            .library
            .books
            .iter()
            .filter(|book| {
                if self.skip_empty {
                    // No need to collect all highlights for the book now, just see if there are any
                    self.library.highlights.iter().any(|h| h.book_id == book.id)
                } else {
                    return true;
                }
            })
            .filter(|book| {
                if let Some(filtered_category) = &self.filter_category {
                    book.category == *filtered_category
                } else {
                    return true;
                }
            })
            .chunk_by(|book| book.category.clone());

        for (category, books) in by_category.into_iter() {
            debug!("Starting export of category: {}", category);

            let category_title = {
                let mut c = category.chars();
                match c.next() {
                    None => None,
                    Some(f) => Some(f.to_uppercase().collect::<String>() + c.as_str()),
                }
            };

            let category_title = category_title.ok_or(anyhow!("Invalid category {category}"))?;

            let category_root = self.export_root.join(category_title);
            std::fs::create_dir_all(&category_root)?;

            for book in books {
                let existing_note = self.remaining_existing.remove(&book.id);

                let existing_file = existing_note.clone().map(|n| n.to_path_buf());

                match self.replacement_strategy {
                    ReplacementStrategy::Update => {
                        self.export_book(&category_root, book, existing_note.as_ref())?
                            .write(existing_file.as_ref())?;
                    }

                    ReplacementStrategy::Replace => {
                        self.export_book(&category_root, book, None)?
                            .write(existing_file.as_ref())?;
                    }

                    ReplacementStrategy::IgnoreExisting => {
                        if let Some(existing_file_path) = &existing_file {
                            debug!(
                                "Ignoring existing file '{:?}' for book '{}'",
                                existing_file_path, &book.title
                            );
                        }

                        self.export_book(&category_root, book, None)?.write(None)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn render_templates(
        &self,
        book: &&Book,
        highlights: &Vec<&Highlight>,
        existing_note: Option<&NoteReference>,
    ) -> anyhow::Result<String> {
        let template_context = Self::create_template_context(&book, &highlights)?;
        let highlights_begin_token = "%% HIGHLIGHTS_BEGIN %%";

        let contents = if let Some(existing_note) = existing_note {
            let existing_file_contents = existing_note.parts::<serde_yml::Mapping>()?.1;
            let highlights_begin_index = existing_file_contents
                .find(highlights_begin_token)
                .unwrap_or_else(|| {
                    warn!(
                        "Existing note for book '{}' did not contain highlights begin token",
                        &book.title
                    );
                    0
                });

            let persisted_contents = existing_file_contents.split_at(highlights_begin_index).0;

            persisted_contents.to_string()
        } else {
            self.templates.render("book", &template_context)?
        };

        let highlight_contents = highlights
            .iter()
            .rev()
            .map(|highlight| {
                let mut highlight_context = template_context.clone();
                highlight_context.insert("highlight", &highlight);

                self.templates.render("highlight", &highlight_context)
            })
            .collect::<Result<Vec<String>, _>>()?;

        let highlight_contents = highlight_contents.join("\n\n");
        let highlight_contents = highlight_contents.trim();

        Ok(format!(
            "{}\n\n%% HIGHLIGHTS_BEGIN %%\n\n{}\n",
            contents.trim(),
            highlight_contents
        ))
    }

    fn export_book(
        &self,
        root: &PathBuf,
        book: &Book,
        existing_note: Option<&NoteReference>,
    ) -> anyhow::Result<JoinedNote<i64, serde_yml::Value>> {
        debug!(
            "Starting export of book '{}' into '{:?}'",
            book.title, &root
        );

        let title = self.sanitize_title(&book.title);
        let highlights = self.library.highlights_for(book);
        debug!("Found {} highlights in library", highlights.len());

        let contents = self.render_templates(&book, &highlights, existing_note)?;

        let mut metadata: serde_yml::Value = match &self.metadata_script {
            None => serde_yml::to_value(&book)?,
            Some(script) => script.execute(book, &highlights)?,
        };

        {
            let metadata = metadata
                .as_mapping_mut()
                .expect("Metadata was not a mapping, this is invalid");

            metadata.insert(
                serde_yml::Value::from("note-kind"),
                serde_yml::Value::from("readwise"),
            );

            metadata.insert(
                serde_yml::Value::from("__readwise_fk"),
                serde_yml::Value::from(book.id),
            );
        }

        debug!("Computed metadata for book {:?} as {:?}", &book, metadata);

        Ok(JoinedNote {
            note_id: book.id,
            default_path: root.join(title).with_extension("md"),
            contents,
            metadata,
        })
    }

    fn create_template_context(
        book: &&Book,
        highlights: &Vec<&Highlight>,
    ) -> anyhow::Result<Context> {
        let context = {
            let mut context = Context::from_value(serde_json::to_value(book)?)?;
            let augmented_highlights = highlights.iter()
                .sorted_by_key(|h| h.location)
                .map(|highlight| {
                    let mut v = serde_json::to_value(highlight).unwrap();
                    if let Some(asin) = &book.asin {
                        v.as_object_mut()
                            .unwrap()
                            .insert(
                                String::from("location_url"),
                                tera::Value::from(format!(
                                    "https://readwise.io/to_kindle?action=open&asin={asin}&location={location}",
                                    asin = asin,
                                    location = &highlight.location,
                                )),
                            );
                    }

                    v
                })
                .collect_vec();

            context.insert("book", &book);
            context.insert("highlights", &augmented_highlights);
            context
        };
        Ok(context)
    }

    fn sanitize_title(&self, title: &str) -> String {
        self.sanitizer
            .replace_all(title, "")
            .replace(":", "-")
            .replace(".", "-") // Logic for determining file extensions breaks if we have dots in the title
    }

    fn mark_stranded(&self) -> anyhow::Result<()> {
        let remaining = &self.remaining_existing;
        for note_reference in remaining.values() {
            let mut note = note_reference
                .parse::<serde_yml::Value>()
                .context("Failed to parse note metadata")?;

            note.metadata
                .as_mapping_mut()
                .expect("Metadata was not a mapping, this is invalid")
                .insert(
                    serde_yml::Value::from("stranded"),
                    serde_yml::Value::from(true),
                );

            note.write()?;
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    debug!("Parsed CLI: {:?}", &cli);

    let db = db::Database::new(&cli.database_path).await?;

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
                                        let book_refs: Vec<&_> = books_chunk.iter().collect();
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
                                        let highlight_refs: Vec<&_> = highlights_chunk.iter().collect();
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
                        info!("Fetching reader documents from Readwise API");
                        let documents = readwise.fetch_document_list(last_sync, None).await?;
                        if !documents.is_empty() {
                            info!("Processing {} documents", documents.len());
                            let document_refs: Vec<&_> = documents.iter().collect();
                            db.insert_documents(&document_refs).await?;
                        }
                        db.update_sync_state(ReadwiseObjectKind::ReaderDocument, Utc::now()).await?;
                        info!("Finished processing reader documents");
                    }
                }
            }

            // If legacy library file is specified, export to JSON for compatibility
            if let Some(library_path) = &cli.library {
                info!("Exporting to legacy JSON format at {:?}", library_path);
                let library = db.export_to_library().await?;
                serde_json::to_writer(std::fs::File::create(library_path)?, &library)?;
            }
        }

        Commands::Export(export_cmd) => {
            let library = db.export_to_library().await?;
            let mut exporter = Exporter::new(library, export_cmd)?;
            exporter.export()?;

            if export_cmd.mark_stranded {
                exporter.mark_stranded()?;
            }
        }

        Commands::ExportJson(export_cmd) => {
            let library = db.export_to_library().await?;
            serde_json::to_writer_pretty(std::fs::File::create(&export_cmd.output)?, &library)?;
            info!("Exported library to {:?}", export_cmd.output);
        }
    }

    Ok(())
}
