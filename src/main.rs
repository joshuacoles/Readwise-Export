use std::path::PathBuf;
use clap::Parser;

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

fn main() {
    let cli = Cli::parse();
    let readwise = readwise::Readwise::new(cli.api_token);
    println!("Hello, world!");
}
