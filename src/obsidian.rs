use serde::Serialize;
use std::path::{Path, PathBuf};
use anyhow::anyhow;
use tracing::debug;
use crate::obsidian::WriteOutcome::{Created, Updated};

pub struct NoteToWrite<T> {
    pub readwise_id: i32,
    pub default_path: PathBuf,
    pub metadata: T,
    pub contents: String,
}

#[derive(Clone, Copy, Debug)]
pub enum WriteOutcome {
    Created,
    Updated
}

impl<T: Serialize> NoteToWrite<T> {
    pub fn write(&self, existing: Option<&PathBuf>) -> anyhow::Result<WriteOutcome> {
        let (outcome, path) = if let Some(existing) = existing {
            (Updated, existing)
        } else {
            let parent = self.default_path.parent()
                .filter(|p| *p != Path::new(""))
                .ok_or(anyhow!("Invalid note location, lacks meaningful parent"))?;

            std::fs::create_dir_all(parent)?;
            (Created, &self.default_path)
        };

        debug!("Writing note to {:?}", &path);

        let contents = format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&self.metadata)?,
            self.contents
        );

        std::fs::write(&path, contents)?;
        Ok(outcome)
    }
}
