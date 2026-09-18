use crate::config::Language;
use anyhow::{ensure, Context, Result};
use std::io::ErrorKind;
use std::path::Path;

pub struct Project;

impl Project {
    pub async fn scaffold(base: &Path, _language: Language, _name: &str) -> Result<()> {
        match tokio::fs::metadata(base).await {
            Ok(metadata) => {
                ensure!(
                    metadata.is_dir(),
                    "--project must point to a directory, not a file: {}\n\
                     Pass the project directory and mention the file in \
                     --task. For an additional reference file, use \
                     --context-file.",
                    base.display()
                );

                return Ok(());
            }

            Err(error) if error.kind() == ErrorKind::NotFound => {}

            Err(error) => {
                return Err(error).with_context(|| {
                    format!("cannot inspect project directory: {}", base.display())
                });
            }
        }

        tokio::fs::create_dir_all(base).await.with_context(|| {
            format!(
                "cannot create project directory: {}. \
                     Check that no parent component is an existing file",
                base.display()
            )
        })?;

        let metadata = tokio::fs::metadata(base).await.with_context(|| {
            format!(
                "cannot inspect created project directory: {}",
                base.display()
            )
        })?;

        ensure!(
            metadata.is_dir(),
            "project path is not a directory: {}",
            base.display()
        );

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
    async fn creates_empty_project_directory() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("new-project");

        Project::scaffold(&project, Language::Python, "ignored")
            .await
            .unwrap();

        assert!(project.is_dir());
        assert_eq!(std::fs::read_dir(project).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn rejects_file_as_project_without_modifying_it() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("main.rs");

        std::fs::write(&source, "fn main() {}\n").unwrap();

        let error = Project::scaffold(&source, Language::Rust, "ignored")
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("--project must point to a directory"));

        assert_eq!(std::fs::read_to_string(source).unwrap(), "fn main() {}\n");
    }
}
