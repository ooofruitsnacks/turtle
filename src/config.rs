use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model_path: PathBuf,
    pub chat_template: PathBuf,
    pub context_size: u32,
    pub max_iterations: u32,
    pub project_dir: PathBuf,
    pub language: Language,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    Rust,
    Odin,
}

