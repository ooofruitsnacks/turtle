use super::LlmBackend;
use anyhow::{Context, Result};
use async_trait::async_trait;
use mistralrs::{GgufModelBuilder, PagedAttentionMetaBuilder, TextMessageRole, TextMessages};
use std::path::Path;
use tokio::time::{timeout, Duration};

pub struct MistralRsBackend {
    model: mistralrs::Model,
}

impl MistralRsBackend {
    pub async fn load(model_path: &Path, chat_template: &Path) -> Result<Self> {
        let model_path = model_path
            .canonicalize()
            .with_context(|| format!("Cannot find model file: {}", model_path.display()))?;

        let chat_template = chat_template
            .canonicalize()
            .with_context(|| format!("Cannot find chat template: {}", chat_template.display()))?;

        let model_dir = model_path
            .parent()
            .context("Model file must have a parent directory")?
            .to_path_buf();

        let file_name = model_path
            .file_name()
            .context("Model file must have a file name")?
            .to_string_lossy()
            .to_string();

        println!("🔄 Loading model from: {}/{}", model_dir.display(), file_name);
        println!("⏳ This may take 30-120 seconds...");

        let start = std::time::Instant::now();
        let model = GgufModelBuilder::new(
            model_dir.to_string_lossy(),
            vec![file_name],
        )
        .with_chat_template(chat_template.to_string_lossy())
        .with_paged_attn(PagedAttentionMetaBuilder::default().build()?)
        .build()
        .await?;

        println!("✅ Model loaded in {:?}", start.elapsed());
        Ok(Self { model })
    }
}

#[async_trait]
impl LlmBackend for MistralRsBackend {
    async fn complete(&self, prompt: &str) -> Result<String> {
        println!("🐢Thinking... (be patient please it can take a little)");
        let messages = TextMessages::new().add_message(TextMessageRole::User, prompt);

        // 10-minute timeout so you know if it is truly hung
        let response = timeout(
            Duration::from_secs(600),
            self.model.send_chat_request(messages),
        )
        .await
        .context("Model inference timed out after 10 minutes. It is likely running on CPU solely and too slow.")??;

        let content = response.choices[0]
            .message
            .content
            .clone()
            .unwrap_or_default();

        println!("✅ Response received ({} chars)", content.len());
        Ok(content)
    }
}

