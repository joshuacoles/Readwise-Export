pub mod db;
pub mod library;

// Re-export commonly used types
pub use db::Database;
pub use library::{Book, Document, Highlight, Library};

// Tag definition used by both API and database operations
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

// Re-export enum needed by both executables
#[derive(Debug, Clone, Copy, serde::Deserialize, Eq, PartialEq)]
pub enum ReadwiseObjectKind {
    Book,
    Highlight,
    ReaderDocument,
}