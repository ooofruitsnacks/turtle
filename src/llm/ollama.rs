use super::LlmBackend;
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Write};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
pub mod task_lifecycle;
/// Response schema for Turtle's action protocol.
///
/// Both new-file creation and existing-file replacement use "edit".
/// Semantic validation, including path restrictions and duplicate-file
/// checks, must still be performed by the agent's action parser.
///
/// This backend is configured for action generation, not free-form chat.
fn action_response_schema() -> Value {
    json!({
        "anyOf": [
            {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["edit"]
                    },
                    "files": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 12,
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": {
                                    "type": "string",
                                    "minLength": 1
                                },
                                "content": {
                                    "type": "string"
                                }
                            },
                            "required": ["path", "content"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["action", "files"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["stop"]
                    },
                    "reason": {
                        "type": "string",
                        "minLength": 1
                    }
                },
                "required": ["action", "reason"],
                "additionalProperties": false
            }
        ]
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_owned(),
            content: content.into(),
        }
    }
}

pub struct OllamaBackend {
    model_name: String,
    client: reqwest::Client,
    base_url: String,
    history: Mutex<Vec<ChatMessage>>,
    request_lock: Mutex<()>,
    context_tokens: u32,
    recent_turns: usize,
    keep_alive: String,
    preview: bool,
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref() {
        Some("1" | "true" | "yes") => true,
        Some("0" | "false" | "no") => false,
        _ => default,
    }
}

fn estimated_tokens(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .map(|message| message.content.len().div_ceil(3) + 32)
        .sum::<usize>()
        + 256
}

fn remove_last_turn(history: &mut Vec<ChatMessage>) {
    let length = history.len();

    if length >= 3 && history[length - 1].role == "assistant" && history[length - 2].role == "user"
    {
        history.truncate(length - 2);
    }
}

impl OllamaBackend {
    pub fn new(model_name: &str) -> Self {
        let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://127.0.0.1:11434".into());

        let base_url = if host.starts_with("http://") || host.starts_with("https://") {
            host
        } else {
            format!("http://{host}")
        };

        let timeout = env_u32("TURTLE_REQUEST_TIMEOUT_SECS", 600).clamp(10, 3600);

        Self {
            model_name: model_name.to_owned(),
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(timeout as u64))
                .build()
                .expect("failed to construct Ollama HTTP client"),
            base_url: base_url.trim_end_matches('/').to_owned(),
            history: Mutex::new(vec![ChatMessage::new("system", "")]),
            request_lock: Mutex::new(()),
            context_tokens: env_u32("TURTLE_NUM_CTX", 8192).clamp(4096, 131072),
            recent_turns: env_u32("TURTLE_HISTORY_TURNS", 1).min(8) as usize,
            keep_alive: std::env::var("TURTLE_KEEP_ALIVE").unwrap_or_else(|_| "5m".into()),
            preview: env_bool("TURTLE_STREAM_PREVIEW", true),
        }
    }

    pub fn with_context_size(mut self, tokens: u32) -> Self {
        self.context_tokens = tokens.clamp(4096, 131072);
        self
    }

    pub async fn check(&self) -> Result<()> {
        let body: Value = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .with_context(|| format!("cannot connect to Ollama at {}", self.base_url))?
            .error_for_status()
            .context("Ollama model listing failed")?
            .json()
            .await?;

        let default_tag = format!("{}:latest", self.model_name);

        let installed = body["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|model| model["name"].as_str())
            .any(|name| {
                name == self.model_name || (!self.model_name.contains(':') && name == default_tag)
            });

        ensure!(
            installed,
            "model {:?} is not installed in this Ollama instance",
            self.model_name
        );

        eprintln!(
            "Ollama connected: model={}, context={}, retained_turns={}",
            self.model_name, self.context_tokens, self.recent_turns
        );

        Ok(())
    }

    async fn chat_turn(&self, prompt: &str, requested_output: u32) -> Result<String> {
        let _guard = self.request_lock.lock().await;

        ensure!(requested_output > 0, "output budget must be positive");

        let output_tokens = requested_output.min(self.context_tokens / 2);
        let history = self.history.lock().await.clone();

        let mut messages = vec![history
            .first()
            .cloned()
            .unwrap_or_else(|| ChatMessage::new("system", ""))];

        let keep = self.recent_turns.saturating_mul(2);
        let start = history.len().saturating_sub(keep).max(1);

        if start < history.len() {
            messages.extend_from_slice(&history[start..]);
        }

        messages.push(ChatMessage::new("user", prompt));

        while estimated_tokens(&messages) + output_tokens as usize > self.context_tokens as usize
            && messages.len() > 2
        {
            messages.drain(1..3);
        }

        let input_estimate = estimated_tokens(&messages);

        ensure!(
            input_estimate + output_tokens as usize <= self.context_tokens as usize,
            "approximate context budget exceeded: input≈{}, output={}, \
             context={}. Reduce source/task size or increase --context",
            input_estimate,
            output_tokens,
            self.context_tokens
        );

        let mut request = json!({
            "model": self.model_name,
            "messages": messages,
            "stream": true,
            "format": action_response_schema(),
            "keep_alive": self.keep_alive,
            "options": {
                "num_ctx": self.context_tokens,
                "num_predict": output_tokens,
                "temperature": 0
            }
        });

        if let Ok(value) = std::env::var("TURTLE_THINK") {
            request["think"] = match value.as_str() {
                "true" => json!(true),
                "false" => json!(false),
                "low" | "medium" | "high" | "max" => json!(value),
                _ => bail!("invalid TURTLE_THINK setting"),
            };
        }

        let started = Instant::now();
        let mut first_content_ms = None;

        eprintln!(
            "Generating: input≈{} tokens, output_limit={}",
            input_estimate, output_tokens
        );

        let mut response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .context("Ollama chat request failed")?
            .error_for_status()
            .context("Ollama returned an HTTP error")?;

        let mut pending = Vec::new();
        let mut content = String::new();
        let mut final_frame = None;

        while let Some(chunk) = response.chunk().await? {
            pending.extend_from_slice(&chunk);

            ensure!(
                pending.len() <= 4 * 1024 * 1024,
                "stream frame exceeded safety limit"
            );

            let mut consumed = 0;

            while let Some(relative_end) =
                pending[consumed..].iter().position(|byte| *byte == b'\n')
            {
                let end = consumed + relative_end;

                consume_frame(
                    &pending[consumed..end],
                    &mut content,
                    &mut final_frame,
                    &mut first_content_ms,
                    started,
                    self.preview,
                )?;

                consumed = end + 1;
            }

            if consumed > 0 {
                pending.drain(..consumed);
            }

            if final_frame.is_some() {
                break;
            }
        }

        if final_frame.is_none() && !pending.is_empty() {
            consume_frame(
                &pending,
                &mut content,
                &mut final_frame,
                &mut first_content_ms,
                started,
                self.preview,
            )?;
        }

        if self.preview {
            eprintln!();
        }

        let metrics = final_frame.context("stream ended without a completion frame")?;

        ensure!(
            metrics["done_reason"].as_str() != Some("length"),
            "generation reached its output limit. No conversation turn \
             was committed. Split the change into smaller tasks"
        );

        ensure!(
            !content.trim().is_empty(),
            "Ollama returned no usable response content"
        );

        eprintln!(
            "{}",
            json!({
                "event": "turtle_inference",
                "model": self.model_name,
                "wall_ms": started.elapsed().as_millis(),
                "first_content_ms": first_content_ms,
                "prompt_tokens": metrics["prompt_eval_count"],
                "generated_tokens": metrics["eval_count"],
                "load_ns": metrics["load_duration"],
                "prompt_eval_ns": metrics["prompt_eval_duration"],
                "eval_ns": metrics["eval_duration"]
            })
        );

        messages.push(ChatMessage::new("assistant", content.clone()));

        *self.history.lock().await = messages;

        Ok(content)
    }
}

fn consume_frame(
    bytes: &[u8],
    content: &mut String,
    final_frame: &mut Option<Value>,
    first_content_ms: &mut Option<u128>,
    started: Instant,
    preview: bool,
) -> Result<()> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }

    ensure!(final_frame.is_none(), "data received after completion");

    let frame: Value = serde_json::from_slice(bytes).context("invalid stream JSON")?;

    if let Some(error) = frame["error"].as_str() {
        bail!("Ollama error: {error}");
    }

    if let Some(text) = frame["message"]["content"].as_str() {
        if !text.is_empty() && first_content_ms.is_none() {
            *first_content_ms = Some(started.elapsed().as_millis());
        }

        ensure!(
            content.len().saturating_add(text.len()) <= 2 * 1024 * 1024,
            "generated response exceeded safety limit"
        );

        content.push_str(text);

        if preview {
            let safe: String = text
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                .collect();

            eprint!("{safe}");
            let _ = io::stderr().flush();
        }
    }

    if frame["done"].as_bool() == Some(true) {
        *final_frame = Some(frame);
    }

    Ok(())
}

#[async_trait]
impl LlmBackend for OllamaBackend {
    async fn complete(&self, prompt: &str) -> Result<String> {
        self.chat_turn(prompt, 2048).await
    }

    async fn complete_with_budget(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        self.chat_turn(prompt, max_tokens).await
    }

    async fn set_system(&self, system_prompt: &str) {
        let _guard = self.request_lock.lock().await;

        *self.history.lock().await = vec![ChatMessage::new("system", system_prompt)];
    }

    async fn reset_context(&self) {
        let _guard = self.request_lock.lock().await;
        self.history.lock().await.truncate(1);
    }

    async fn pop_last(&self) {
        let _guard = self.request_lock.lock().await;
        let mut history = self.history.lock().await;
        remove_last_turn(&mut history);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_schema_defines_edit_and_stop_shapes() {
        let schema = action_response_schema();
        let alternatives = schema["anyOf"]
            .as_array()
            .expect("schema must contain action alternatives");

        assert_eq!(alternatives.len(), 2);

        let edit = &alternatives[0];
        assert_eq!(edit["type"], json!("object"));
        assert_eq!(edit["properties"]["action"]["enum"], json!(["edit"]));
        assert_eq!(edit["required"], json!(["action", "files"]));
        assert_eq!(edit["additionalProperties"], json!(false));

        let files = &edit["properties"]["files"];
        assert_eq!(files["type"], json!("array"));
        assert_eq!(files["minItems"], json!(1));
        assert_eq!(files["maxItems"], json!(12));
        assert_eq!(files["items"]["required"], json!(["path", "content"]));
        assert_eq!(files["items"]["additionalProperties"], json!(false));

        let stop = &alternatives[1];
        assert_eq!(stop["type"], json!("object"));
        assert_eq!(stop["properties"]["action"]["enum"], json!(["stop"]));
        assert_eq!(stop["required"], json!(["action", "reason"]));
        assert_eq!(stop["additionalProperties"], json!(false));
    }

    #[test]
    fn rollback_removes_complete_assistant_turn() {
        let mut history = vec![
            ChatMessage::new("system", "rules"),
            ChatMessage::new("user", "first"),
            ChatMessage::new("assistant", "accepted"),
            ChatMessage::new("user", "second"),
            ChatMessage::new("assistant", "rejected"),
        ];

        remove_last_turn(&mut history);

        assert_eq!(history.len(), 3);
        assert_eq!(history[2].content, "accepted");
    }

    #[test]
    fn incomplete_turn_is_not_removed() {
        let mut history = vec![
            ChatMessage::new("system", "rules"),
            ChatMessage::new("user", "request"),
        ];

        remove_last_turn(&mut history);

        assert_eq!(history.len(), 2);
    }

    #[test]
    fn streaming_frames_accumulate_content() {
        let mut content = String::new();
        let mut final_frame = None;
        let mut first_content_ms = None;
        let started = Instant::now();

        consume_frame(
            br#"{"message":{"content":"hello"},"done":false}"#,
            &mut content,
            &mut final_frame,
            &mut first_content_ms,
            started,
            false,
        )
        .unwrap();

        consume_frame(
            br#"{"message":{"content":""},"done":true,"done_reason":"stop"}"#,
            &mut content,
            &mut final_frame,
            &mut first_content_ms,
            started,
            false,
        )
        .unwrap();

        assert_eq!(content, "hello");
        assert!(final_frame.is_some());
        assert!(first_content_ms.is_some());
    }
}
