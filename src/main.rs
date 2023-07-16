use std::cell::RefCell;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use clap::Parser;
use itertools::Itertools;
use regex::Regex;
use rhai::{AST, Dynamic, Engine, Scope};
use rhai::serde::to_dynamic;
use serde::{Deserialize, Serialize};
use serde_json::{json};
use tera::{Context, Tera};
use tracing::{debug, info};
use crate::readwise::{Book, Highlight};

mod readwise;

#[derive(Parser)]
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
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Library {
    books: Vec<Book>,
    highlights: Vec<Highlight>,
    updated_at: DateTime<Utc>,
}

impl Library {
    fn highlights_for(&self, book: &Book) -> Vec<&Highlight> {
        self.highlights.iter()
            .filter(|h| h.book_id == book.id)
            .collect_vec()
    }
}

enum ScriptType {
    Rhai {
        metadata_script: AST,
        engine: Engine,
    },

    Javascript {
        script: RefCell<js_sandbox::Script>
    },
}

impl ScriptType {
    fn execute(&self, book: &Book, highlights: &[&Highlight]) -> anyhow::Result<serde_yaml::Value> {
        match self {
            ScriptType::Rhai { metadata_script, engine } => {
                let mut scope = {
                    let mut scope = Scope::new();

                    let book: Dynamic = to_dynamic(book)?;
                    let highlights = to_dynamic(highlights)?;

                    scope.push_dynamic("book", book);
                    scope.push_dynamic("highlights", highlights);

                    scope
                };

                let dynamic: Dynamic = engine.eval_ast_with_scope::<Dynamic>(
                    &mut scope,
                    metadata_script,
                )?;

                Ok(serde_yaml::to_value(&dynamic)?)
            }

            ScriptType::Javascript { script } => {
                let a: serde_json::Value = script.borrow_mut()
                    .call("metadata", &json!({
                        "book": book,
                        "highlights": highlights,
                    }))?;

                Ok(serde_yaml::to_value(&a)?)
            }
        }
    }
}

struct Exporter {
    sanitizer: Regex,
    export_root: PathBuf,
    library: Library,

    templates: Tera,
    metadata_script: Option<ScriptType>,
}

struct NoteToWrite<T> {
    path: PathBuf,
    metadata: T,
    contents: String,
}

impl<T: Serialize> NoteToWrite<T> {
    fn write(&self) -> anyhow::Result<()> {
        debug!("Writing note to {:?}", self.path);
        let contents = format!("---\n{}---\n{}", serde_yaml::to_string(&self.metadata)?, self.contents);
        std::fs::write(&self.path, contents)?;
        Ok(())
    }
}

impl Exporter {
    fn new(
        library: Library,
        cli: &Cli,
    ) -> anyhow::Result<Self> {
        let metadata_script = match &cli.metadata_script {
            None => None,
            Some(path) if path.extension().unwrap() == "js" => {
                debug!("Loading javascript metadata script from {:?}", path);
                let script = js_sandbox::Script::from_file(path)?;
                Some(ScriptType::Javascript { script: RefCell::new(script) })
            }

            Some(path) => {
                debug!("Loading rhai metadata script from {:?}", path);
                let engine = Engine::new();
                let metadata_script = engine.compile_file(path.to_path_buf())?;
                Some(ScriptType::Rhai { metadata_script, engine })
            }
        };

        Ok(Exporter {
            library,
            export_root: cli.vault.join(&cli.base_folder),
            templates: {
                let mut tera = Tera::default();
                tera.add_template_file(&cli.template, Some("book"))?;
                debug!("Loaded tera templates for markdown. Templates: {}", tera.get_template_names().join(", "));

                tera
            },
            metadata_script,

            sanitizer: Regex::new(r#"[<>"'/\\|?*]+"#).unwrap(),
        })
    }

    fn export(&self) -> anyhow::Result<()> {
        let by_category = self.library.books
            .iter()
            .group_by(|book| book.category.clone());

        let bc = by_category.into_iter();

        for (category, books) in bc {
            debug!("Starting export of category: {}", category);
            let category_root = self.export_root.join(category);
            std::fs::create_dir_all(&category_root)?;
            for book in books {
                self.export_book(&category_root, book)?
                    .write()?;
            }
        }

        Ok(())
    }

    fn export_book(&self, root: &PathBuf, book: &Book) -> anyhow::Result<NoteToWrite<serde_yaml::Value>> {
        debug!("Starting export of book '{}' into '{:?}'", book.title, &root);

        let title = self.sanitize_title(&book.title);
        let highlights = self.library.highlights_for(book);
        debug!("Found {} highlights in library", highlights.len());

        let context = {
            let mut context = Context::from_value(serde_json::to_value(book)?)?;
            context.insert("book", &book);
            context.insert("highlights", &highlights);
            context
        };

        let contents = self.templates
            .render("book", &context)?;

        let metadata: serde_yaml::Value = match &self.metadata_script {
            None => serde_yaml::to_value(&book)?,
            Some(script) => script.execute(book, &highlights)?,
        };

        debug!("Computed metadata for book {:?} as {:?}", &book, metadata);

        Ok(NoteToWrite {
            path: root.join(title).with_extension("md"),
            contents,
            metadata,
        })
    }

    fn sanitize_title(&self, title: &str) -> String {
        self.sanitizer.replace_all(title, "")
            .replace(":", "-")
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let readwise = readwise::Readwise::new(&cli.api_token);

    let library = if let Some(cache) = &cli.library {
        info!("Loading library from cache: {:?}", cache);
        let mut library: Library = serde_json::from_reader(std::fs::File::open(cache)?)?;

        if cli.refetch {
            info!("Fetching updates since {:?}", library.updated_at);
            readwise.update_library(&mut library).await?;
            serde_json::to_writer(std::fs::File::create(cache)?, &library)?;
        }

        library
    } else {
        info!("Fetching whole library from readwise. No persistence configured.");
        readwise.fetch_library().await?
    };

    info!("Collected library of {} books and {} highlights", library.books.len(), library.highlights.len());

    let exporter = Exporter::new(library, &cli)?;
    exporter.export()?;

    Ok(())
}
