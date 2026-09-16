use crate::config::Language;
use anyhow::{ensure, Result};
use std::path::Path;

pub struct Project;

impl Project {
    pub async fn scaffold(base: &Path, _language: Language, _name: &str) -> Result<()> {
        tokio::fs::create_dir_all(base).await?;
        ensure!(base.is_dir(), "project path is not a directory");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preserves_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = directory.path().join("Cargo.toml");

        std::fs::write(&manifest, "existing contents").unwrap();

        Project::scaffold(directory.path(), Language::Rust, "ignored")
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(manifest).unwrap(),
            "existing contents"
        );
    }

    #[tokio::test]
    async fn does_not_generate_language_specific_files() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("new-project");

        Project::scaffold(&project, Language::Python, "ignored")
            .await
            .unwrap();

        assert!(project.is_dir());
        assert_eq!(std::fs::read_dir(project).unwrap().count(), 0);
    }
}
