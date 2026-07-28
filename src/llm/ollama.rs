use super::LlmBackend;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    fn system(s: &str) -> Self { Self { role: "system".into(), content: s.into() } }
    fn user(s: &str) -> Self { Self { role: "user".into(), content: s.into() } }
    fn assistant(s: String) -> Self { Self { role: "assistant".into(), content: s } }
}

pub struct OllamaBackend {
    model_name: String,
    client: reqwest::Client,
    base_url: String,
    history: Mutex<Vec<ChatMessage>>,
}

impl OllamaBackend {
    pub fn new(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(180))
                .build()
                .expect("failed to build reqwest client"),
            base_url: "http://localhost:11434".to_string(),
            history: Mutex::new(Vec::new()),
        }
    }

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

    async fn chat_turn(&self, user_content: &str) -> Result<String> {
        let messages: Vec<ChatMessage> = {
            let mut hist = self.history.lock().await;
            hist.push(ChatMessage::user(user_content));
            hist.clone()
        };

        eprintln!(
            "🐢⚒️🦙Thinking…🧠💭 ({} messages, {} chars in latest turn)…",
            messages.len(),
            user_content.len()
        );
        let start = Instant::now();

        let resp = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&json!({
                "model": self.model_name,
                "messages": messages,
                "stream": false,
                "keep_alive": "30m",
                "options": {
                    "temperature": 0.1,
                    "num_predict": 2048,
                    "num_ctx": 8192,
                    "stop": ["\n\nExplanation:", "\n\nNote:"]
                }
            }))
            .send()
            .await
            .context("Failed to send request to Ollama");

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                self.history.lock().await.pop();
                return Err(e);
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            self.history.lock().await.pop();
            anyhow::bail!("Ollama returned HTTP {}: {}", status, text);
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        let content = body["message"]["content"].as_str().unwrap_or("").to_string();

        if let Some(pe) = body["prompt_eval_count"].as_u64() {
            let ee = body["eval_count"].as_u64().unwrap_or(0);
            eprintln!("✅ {} prompt tok / {} gen tok in {:?}", pe, ee, start.elapsed());
        } else {
            eprintln!("✅ Response received in {:?} ({} chars)", start.elapsed(), content.len());
        }

        self.history.lock().await.push(ChatMessage::assistant(content.clone()));
        Ok(content)
    }
}

#[async_trait]
impl LlmBackend for OllamaBackend {
    async fn complete(&self, prompt: &str) -> Result<String> {
        self.chat_turn(prompt).await
    }

    async fn set_system(&self, system_prompt: &str) {
        let mut hist = self.history.lock().await;
        hist.clear();
        hist.push(ChatMessage::system(system_prompt));
    }

    async fn reset_context(&self) {
        let mut hist = self.history.lock().await;
        hist.retain(|m| m.role == "system");
    }
}

