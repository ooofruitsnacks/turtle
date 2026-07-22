use anyhow::Result;
use async_trait::async_trait;

pub mod local;
pub mod mock;

#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
}

