use super::LlmBackend;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;
use std::time::Instant;

pub struct OllamaBackend {
    model_name: String,
    client: reqwest::Client,
    base_url: String,
}

impl OllamaBackend {
    pub fn new(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
            client: reqwest::Client::new(),
            base_url: "http://localhost:11434".to_string(),
        }
    }

    /// Verify Ollama is running and the model is available.
    pub async fn check(&self) -> Result<()> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .context("Cannot connect to Ollama at http://localhost:11434. Is 'ollama serve' running?")?;

        let body: serde_json::Value = resp.json().await.context("Failed to parse Ollama tags")?;
        let models: Vec<String> = body["models"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
            .collect();

        if !models.iter().any(|m| m.starts_with(&self.model_name)) {
            anyhow::bail!(
                "Model '{}' not found in Ollama. Run: ollama pull {}",
                self.model_name,
                self.model_name
            );
        }

        println!("✅ Backend connected. Model '{}' available.", self.model_name);
        Ok(())
    }
}

#[async_trait]
impl LlmBackend for OllamaBackend {
    async fn complete(&self, prompt: &str) -> Result<String> {
        eprintln!("🐢⚒️🦙Thinking...🧠💭 ({} chars)...", prompt.len());
        let start = Instant::now();

        let resp = self
            .client
            .post(format!("{}/api/generate", self.base_url))
            .json(&json!({
                "model": self.model_name,
                "prompt": prompt,
                "stream": false,
                "options": {
                    "temperature": 0.1,
		    "num_predict": 2048,
                    "num_ctx": 8192,
                    "stop": ["\n\nExplanation:", "\n\nNote:"]
                }
            }))
            .send()
            .await
            .context("Failed to send request to Ollama")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Ollama returned HTTP {}: {}", status, text);
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        let content = body["response"]
            .as_str()
            .unwrap_or("")
            .to_string();

        eprintln!(
            "✅ Response received in {:?} ({} chars)",
            start.elapsed(),
            content.len()
        );
        Ok(content)
    }
}

