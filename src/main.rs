use crate::readwise::{Book, Document, Highlight};
use anyhow::{anyhow, Context as _};
use chrono::{DateTime, Utc};
use clap::{Parser, ValueEnum};
use itertools::Itertools;
use obsidian_rust_interface::{NoteReference, Vault};
use regex::Regex;
use scripting::ScriptType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use obsidian_rust_interface::joining::JoinedNote;
use obsidian_rust_interface::joining::strategies::TypeAndKey;
use tera::{Context, Tera};
use tracing::{debug, info, warn};

mod readwise;
mod scripting;

#[derive(Debug, Parser, Deserialize)]
struct Cli {
    /// The root of the obsidian vault
    #[arg(long)]
    vault: PathBuf,

    /// The location within the obsidian vault where the Readwise files are stored, relative to the
    /// vault root.
    #[arg(long)]
    base_folder: String,

    /// Readwise API token
    #[arg(long)]
    api_token: String,

    /// Store library data in a JSON file for caching between executions
    #[arg(long)]
    library: Option<PathBuf>,

    /// The strategy to use when fetching data from the Readwise API
    #[arg(long, default_value = "cache")]
    fetch: FetchStrategy,

    /// If true, will not export any notes, only fetch data from the Readwise API
    #[arg(long, short, default_value = "false")]
    no_export: bool,

    /// If custom metadata should be written, a script to generate it
    #[arg(long)]
    metadata_script: Option<PathBuf>,

    /// The template used for the initial contents of a book note. The highlights will be rendered directly after this
    /// initial content.
    #[arg(long)]
    book_template: PathBuf,

    /// The template used for each highlight in a book note. These will be rendered after the end of the book note
    /// template, with an inserted %% HIGHLIGHTS_BEGIN %% tag separating the two sections.
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
    /// Do not fetch any data from the Readwise API, only use the library cache
    Cache,

    /// Ask for updates from the Readwise API since the last update to the library cache
    Update,

    /// Refetch the whole library from the Readwise API
    Refetch,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Library {
    books: Vec<Book>,
    highlights: Vec<Highlight>,
    updated_at: DateTime<Utc>,
    documents: Vec<Document>,
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

    remaining_existing: HashMap<i32, NoteReference>,

    replacement_strategy: ReplacementStrategy,
    skip_empty: bool,
    filter_category: Option<String>,
}

impl Exporter {
    fn new(library: Library, cli: &Cli) -> anyhow::Result<Self> {
        let metadata_script = match &cli.metadata_script {
            None => None,
            Some(path) => Some(ScriptType::new(path)?),
        };

        let vault = Vault::open(&cli.vault);
        let existing = obsidian_rust_interface::joining::find_by::<_, i32>(
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

            let category_title = category_title
                .ok_or(anyhow!("Invalid category {category}"))?;

            let category_root = self.export_root.join(category_title);
            std::fs::create_dir_all(&category_root)?;

            for book in books {
                let existing_note = self.remaining_existing
                    .remove(&book.id);

                let existing_file = existing_note
                    .clone()
                    .map(|n| n.to_path_buf());

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
                            debug!("Ignoring existing file '{:?}' for book '{}'", existing_file_path, &book.title);
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
            let highlights_begin_index = existing_file_contents.find(highlights_begin_token).unwrap_or_else(|| {
                warn!("Existing note for book '{}' did not contain highlights begin token", &book.title);
                0
            });

            let persisted_contents = existing_file_contents.split_at(highlights_begin_index).0;

            persisted_contents.to_string()
        } else {
            self.templates.render("book", &template_context)?
        };

        let highlight_contents = highlights.iter().rev().map(|highlight| {
            let mut highlight_context = template_context.clone();
            highlight_context.insert("highlight", &highlight);

            self.templates.render("highlight", &highlight_context)
        }).collect::<Result<Vec<String>, _>>()?;

        let highlight_contents = highlight_contents.join("\n\n");
        let highlight_contents = highlight_contents.trim();

        Ok(format!("{}\n\n%% HIGHLIGHTS_BEGIN %%\n\n{}\n", contents.trim(), highlight_contents))
    }

    fn export_book(
        &self,
        root: &PathBuf,
        book: &Book,
        existing_note: Option<&NoteReference>,
    ) -> anyhow::Result<JoinedNote<i32, serde_yml::Value>> {
        debug!(
            "Starting export of book '{}' into '{:?}'",
            book.title, &root
        );

        let title = self.sanitize_title(&book.title);
        let highlights = self.library.highlights_for(book);
        debug!("Found {} highlights in library", highlights.len());

        let contents = self.render_templates(
            &book,
            &highlights,
            existing_note,
        )?;

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

    fn create_template_context(book: &&Book, highlights: &Vec<&Highlight>) -> anyhow::Result<Context> {
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
        self.sanitizer.replace_all(title, "").replace(":", "-")
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
    // Install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    debug!("Parsed CLI: {:?}", &cli);

    let readwise = readwise::Readwise::new(&cli.api_token);

    let library = if let Some(cache) = &cli.library {
        if !cache.exists() {
            info!(
                "No cache found at {:?}. Fetching whole library from readwise. Ignoring any refetch options.",
                cache
            );
            let library: Library = readwise.fetch_library().await?;
            serde_json::to_writer(std::fs::File::create(cache)?, &library)?;
            library
        } else {
            info!("Loading library from cache: {:?}", cache);
            let mut library: Library = serde_json::from_reader(std::fs::File::open(cache)?)?;

            match cli.fetch {
                FetchStrategy::Cache => {}

                FetchStrategy::Update => {
                    info!("Fetching updates since {:?}", library.updated_at);
                    readwise.update_library(&mut library).await?;
                    serde_json::to_writer(std::fs::File::create(cache)?, &library)?;
                }

                FetchStrategy::Refetch => {
                    info!("Fetching whole library from readwise");
                    readwise.update_library(&mut library).await?;
                    serde_json::to_writer(std::fs::File::create(cache)?, &library)?;
                }
            }

            library
        }
    } else {
        info!("Fetching whole library from readwise. No persistence configured.");
        readwise.fetch_library().await?
    };

    info!(
        "Collected library of {} books and {} highlights",
        library.books.len(),
        library.highlights.len()
    );

    if cli.no_export {
        info!("No export requested, exiting");
        return Ok(());
    }

    let mut exporter = Exporter::new(library, &cli)?;
    exporter.export()?;

    if cli.mark_stranded {
        exporter.mark_stranded()?;
    }

    Ok(())
}
