pub mod mock;
pub mod ollama;

use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
}

