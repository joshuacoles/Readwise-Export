use std::path::PathBuf;
use chrono::{DateTime, Utc};
use clap::Parser;
use itertools::Itertools;
use obsidian_rust_interface::VaultNote;
use regex::Regex;
use rhai::{AST, Dynamic, Engine, Scope};
use rhai::serde::to_dynamic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tera::{Context, Template, Tera};
use crate::readwise::{Book, Highlight};
use crate::readwise::Resource::Highlights;

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

    /// If custom metadata should be written, a script to generate it
    #[arg(long)]
    metadata_script: Option<PathBuf>,

    /// A template or directory of templates to use for exporting
    #[arg(long)]
    template: PathBuf,
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Request error {0}")]
    RequestError(#[from] reqwest::Error),
}

pub struct Library {
    books: Vec<Book>,
    highlights: Vec<Highlight>,
}

impl Library {
    fn highlights_for(&self, book: &Book) -> Vec<&Highlight> {
        self.highlights.iter()
            .filter(|h| h.book_id == book.id)
            .collect_vec()
    }
}

struct Exporter {
    sanitizer: Regex,
    export_root: PathBuf,
    library: Library,

    templates: Tera,
    metadata_script: Option<AST>,
    engine: Engine,
}

struct NoteToWrite<T> {
    path: PathBuf,
    metadata: T,
    contents: String,
}

impl<T: Serialize> NoteToWrite<T> {
    fn write(&self) -> anyhow::Result<()> {
        let contents = format!("---\n{}---\n{}", serde_yaml::to_string(&self.metadata)?, self.contents);
        std::fs::write(&self.path, contents)?;
        Ok(())
    }
}

impl Exporter {
    fn new(
        library: Library,
        cli: &Cli,
    ) -> Self {
        let engine = rhai::Engine::new();
        let metadata_script = cli.metadata_script
            .as_ref()
            .map(|script| engine.compile_file(script.to_path_buf()).unwrap());

        Exporter {
            library,
            export_root: cli.vault.join(&cli.base_folder),
            templates: {
                let mut tera = Tera::default();
                tera.add_template_file( &cli.template, Some("book")).unwrap();
                tera
            },
            metadata_script,
            engine: Engine::new(),

            sanitizer: Regex::new(r#"[<>"'/\\|?*]+"#).unwrap(),
        }
    }

    fn export(&self) -> anyhow::Result<()> {
        let by_category = self.library.books
            .iter()
            .group_by(|book| book.category.clone());

        let bc = by_category.into_iter();

        for (category, books) in bc {
            let category_root = self.export_root.join(category);
            std::fs::create_dir_all(&category_root)?;
            for book in books {
                self.export_book(&category_root, book).write();
            }
        }

        Ok(())
    }

    fn export_book(&self, root: &PathBuf, book: &Book) -> NoteToWrite<serde_yaml::Value> {
        let title = self.sanitize_title(&book.title);
        let highlights = self.library.highlights_for(book);

        let context = {
            let mut context = Context::from_value(serde_json::to_value(book).unwrap()).unwrap();
            context.insert("book", &book);
            context.insert("highlights", &highlights);
            context
        };

        let contents = self.templates
            .render("book", &context)
            .unwrap();

        let metadata: serde_yaml::Value = {
            let mut scope = {
                let mut scope = Scope::new();

                scope.push_dynamic("book", to_dynamic(book).unwrap());
                scope.push_dynamic("highlights", to_dynamic(highlights).unwrap());

                scope
            };

            if let Some(script) = &self.metadata_script {
                let dynamic: Dynamic = self.engine.eval_ast_with_scope::<Dynamic>(
                    &mut scope,
                    script,
                ).unwrap();

                serde_yaml::to_value(&dynamic).unwrap()
            } else {
                serde_yaml::to_value(&book).unwrap()
            }
        };

        NoteToWrite {
            path: root.join(title).with_extension("md"),
            contents,
            metadata,
        }
    }

    fn sanitize_title(&self, title: &str) -> String {
        self.sanitizer.replace_all(title, "")
            .replace(":", "-")
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();
    // let readwise = readwise::Readwise::new(&cli.api_token);
    // let library = readwise.fetch_library().await?;
    let library = Library {
        books: serde_json::from_reader(std::fs::File::open("books.json")?)?,
        highlights: serde_json::from_reader(std::fs::File::open("highlights.json")?)?,
    };

    let exporter = Exporter::new(library, &cli);
    exporter.export()?;

    Ok(())
}
