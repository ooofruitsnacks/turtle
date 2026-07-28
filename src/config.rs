use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model_path: PathBuf,     // kept for metadata
    pub chat_template: PathBuf,  // kept for metadata
    pub context_size: u32,
    pub max_iterations: u32,
    pub project_dir: PathBuf,
    pub language: Language,
    pub debug: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    Rust,
    Odin,
}

