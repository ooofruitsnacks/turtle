use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;

#[derive(Clone)]
pub struct Embedder {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl Embedder {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
        }
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vecs = self.embed_batch(&[text.to_string()]).await?;
        vecs.pop().context("empty embedding response")
    }

    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let resp = self
            .client
            .post(format!("{}/api/embed", self.base_url))
            .json(&json!({
                "model": self.model,
                "input": texts
            }))
            .send()
            .await
            .context("Failed to reach Ollama backend. Is 'ollama serve & ollama pull *model*' running?")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama embed error {}: {}", status, body);
        }

        let parsed: EmbedResponse = resp.json().await.context("Failed to parse embed response")?;
        Ok(parsed.embeddings)
    }
}

