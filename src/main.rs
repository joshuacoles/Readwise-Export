use std::arch::aarch64::vqrshlb_s8;
use std::path::PathBuf;
use std::thread::scope;
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
    metadata_script: AST,
    engine: Engine,
}

struct NoteToWrite<T> {
    path: PathBuf,
    metadata: T,
    contents: String,
}

impl Exporter {
    fn export(&self) -> anyhow::Result<()> {
        let by_category = self.library.books
            .iter()
            .group_by(|book| book.category.clone());

        let bc = by_category.into_iter();

        for (category, books) in bc {
            let category_root = self.export_root.join(category);
            std::fs::create_dir_all(&category_root)?;
            for book in books {
                self.export_book(&category_root, book);
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
            .render("header", &context)
            .unwrap();

        let mut scope = {
            let mut scope = Scope::new();

            scope.push_dynamic("book", to_dynamic(book).unwrap());
            scope.push_dynamic("highlights", to_dynamic(highlights).unwrap());

            scope
        };

        let metadata: Dynamic = self.engine.eval_ast_with_scope::<Dynamic>(
            &mut scope,
            &self.metadata_script,
        ).unwrap();

        let metadata = serde_yaml::to_value(&metadata).unwrap();

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
    let readwise = readwise::Readwise::new(cli.api_token);
    let library = readwise.fetch_library().await?;
    let export_root = cli.vault.join(cli.base_folder);

    Ok(())
}
