use crate::config::Language;
use anyhow::Result;
use std::path::Path;

pub struct Project;

impl Project {
    pub async fn scaffold(base: &Path, language: Language, name: &str) -> Result<()> {
        match language {
            Language::Rust => Self::scaffold_rust(base, name).await,
            Language::Odin => Self::scaffold_odin(base).await,
        }
    }

    async fn scaffold_rust(base: &Path, name: &str) -> Result<()> {
        let cargo_toml = format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
"#
        );

        tokio::fs::create_dir_all(base.join("src")).await?;
        tokio::fs::write(base.join("Cargo.toml"), cargo_toml).await?;
        tokio::fs::write(
            base.join("src/main.rs"),
            "fn main() {\n    println!(\"Hello, world!\");\n}\n",
        )
        .await?;
        Ok(())
    }

    async fn scaffold_odin(base: &Path) -> Result<()> {
        tokio::fs::create_dir_all(base).await?;
        tokio::fs::write(
            base.join("main.odin"),
            "package main\n\nimport \"core:fmt\"\n\nmain :: proc() {\n\tfmt.println(\"Hello, world!\")\n}\n",
        )
        .await?;
        Ok(())
    }
}

