use serde::Serialize;
use std::path::PathBuf;
use tracing::debug;

pub struct NoteToWrite<T> {
    pub path: PathBuf,
    pub metadata: T,
    pub contents: String,
}

impl<T: Serialize> NoteToWrite<T> {
    pub fn write(&self) -> anyhow::Result<()> {
        debug!("Writing note to {:?}", self.path);
        let contents = format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&self.metadata)?,
            self.contents
        );
        std::fs::write(&self.path, contents)?;
        Ok(())
    }
}
