use crate::readwise::{Book, Highlight};
use chrono::{DateTime, Utc};
use clap::Parser;
use itertools::Itertools;
use obsidian::NoteToWrite;
use obsidian_rust_interface::{NoteReference, Vault};
use regex::Regex;
use scripting::ScriptType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tera::{Context, Tera};
use tracing::{debug, info};

mod obsidian;
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

    /// If true, will fetch data from the Readwise API, updating the cache
    #[arg(long, short, default_value = "false")]
    refetch: bool,

    /// If custom metadata should be written, a script to generate it
    #[arg(long)]
    metadata_script: Option<PathBuf>,

    /// A template or directory of templates to use for exporting
    #[arg(long)]
    template: PathBuf,

    /// Ignore existing files, all notes will be written to their default locations
    #[arg(long, default_value = "false")]
    ignore_existing: bool,

    /// Mark notes as stranded if they no longer correspond to a Readwise book
    #[arg(long, default_value = "true")]
    mark_stranded: bool,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Library {
    books: Vec<Book>,
    highlights: Vec<Highlight>,
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

    remaining_existing: HashMap<i32, PathBuf>,

    ignore_existing: bool,
}

#[derive(Serialize, Deserialize)]
struct ReadwiseBrand {
    #[serde(rename = "__readwise_fk")]
    id: i32,
}

impl Exporter {
    fn new(library: Library, cli: &Cli) -> anyhow::Result<Self> {
        let metadata_script = match &cli.metadata_script {
            None => None,
            Some(path) => Some(ScriptType::new(path)?),
        };

        Ok(Exporter {
            library,
            export_root: cli.vault.join(&cli.base_folder),
            templates: {
                let mut tera = Tera::default();
                tera.add_template_file(&cli.template, Some("book"))?;

                debug!(
                    "Loaded tera templates for markdown. Templates: {}",
                    tera.get_template_names().join(", ")
                );

                tera
            },
            metadata_script,

            mark_stranded: cli.mark_stranded,
            ignore_existing: cli.ignore_existing,

            sanitizer: Regex::new(r#"[<>"'/\\|?*]+"#).unwrap(),

            remaining_existing: Vault::open(&cli.vault)
                .notes()
                .filter_map(|n| n.ok())
                .filter_map(|n| {
                    n.metadata::<ReadwiseBrand>()
                        .ok()
                        .map(|m| (m.id, n.path().to_path_buf()))
                })
                .collect(),
        })
    }

    fn export(&mut self) -> anyhow::Result<()> {
        let by_category = self
            .library
            .books
            .iter()
            .group_by(|book| book.category.clone());

        let bc = by_category.into_iter();

        for (category, books) in bc {
            debug!("Starting export of category: {}", category);
            let category_title = {
                let mut c = category.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            };

            let category_root = self.export_root.join(category_title);
            std::fs::create_dir_all(&category_root)?;
            for book in books {
                self.export_book(&category_root, book)?.write(
                    self.remaining_existing
                        .remove(&book.id)
                        .filter(|_| !self.ignore_existing)
                        .as_ref(),
                )?;
            }
        }

        Ok(())
    }

    fn export_book(
        &self,
        root: &PathBuf,
        book: &Book,
    ) -> anyhow::Result<NoteToWrite<i32, serde_yaml::Value>> {
        debug!(
            "Starting export of book '{}' into '{:?}'",
            book.title, &root
        );

        let title = self.sanitize_title(&book.title);
        let highlights = self.library.highlights_for(book);
        debug!("Found {} highlights in library", highlights.len());

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

        let contents = self.templates.render("book", &context)?;

        let mut metadata: serde_yaml::Value = match &self.metadata_script {
            None => serde_yaml::to_value(&book)?,
            Some(script) => script.execute(book, &highlights)?,
        };

        // We hardcode the type to 'readwise' so that we can find these documents later.
        metadata.as_mapping_mut().unwrap().insert(
            serde_yaml::Value::from("note-kind"),
            serde_yaml::Value::from("readwise"),
        );

        metadata.as_mapping_mut().unwrap().insert(
            serde_yaml::Value::from("__readwise_fk"),
            serde_yaml::Value::from(book.id),
        );

        debug!("Computed metadata for book {:?} as {:?}", &book, metadata);

        Ok(NoteToWrite {
            readwise_id: book.id,
            default_path: root.join(title).with_extension("md"),
            contents,
            metadata,
        })
    }

    fn sanitize_title(&self, title: &str) -> String {
        self.sanitizer.replace_all(title, "").replace(":", "-")
    }

    fn mark_stranded(&self) {
        let remaining = &self.remaining_existing;
        for i in remaining.values() {
            let mut note = NoteReference::from_path(&i)
                .parse::<serde_yaml::Value>()
                .unwrap();

            note.metadata.as_mapping_mut().unwrap().insert(
                serde_yaml::Value::from("stranded"),
                serde_yaml::Value::from(true),
            );

            note.write().unwrap();
        }
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
                "No cache found at {:?}. Fetching whole library from readwise.",
                cache
            );
            let library: Library = readwise.fetch_library().await?;
            serde_json::to_writer(std::fs::File::create(cache)?, &library)?;
            library
        } else {
            info!("Loading library from cache: {:?}", cache);
            let mut library: Library = serde_json::from_reader(std::fs::File::open(cache)?)?;

            if cli.refetch {
                info!("Fetching updates since {:?}", library.updated_at);
                readwise.update_library(&mut library).await?;
                serde_json::to_writer(std::fs::File::create(cache)?, &library)?;
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

    let mut exporter = Exporter::new(library, &cli)?;
    exporter.export()?;

    if cli.mark_stranded {
        exporter.mark_stranded()?;
    }

    Ok(())
}
