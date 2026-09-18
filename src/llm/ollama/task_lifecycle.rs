use super::OllamaBackend;
use crate::llm::LlmBackend;
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::watch;

/// A task-scoped wrapper around the Ollama backend.
///
/// Cancellation interrupts inference only. It does not drop the agent's
/// file-edit or verification futures.
///
/// Create a fresh TaskBackend for each independent task.
pub struct TaskBackend<'a> {
    inner: &'a OllamaBackend,
    cancelled: watch::Sender<bool>,
}

impl<'a> TaskBackend<'a> {
    pub fn new(inner: &'a OllamaBackend) -> Self {
        let (cancelled, _) = watch::channel(false);
        Self { inner, cancelled }
    }

    /// Request cooperative cancellation.
    ///
    /// Active inference is interrupted. Future inference calls fail
    /// immediately. An edit or verification already in progress is
    /// allowed to finish.
    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    async fn generate(&self, prompt: &str, budget: Option<u32>) -> Result<String> {
        ensure!(!self.is_cancelled(), "task cancelled");

        // Subscribe before starting inference so cancellation cannot be
        // missed between checking the flag and polling the request.
        let cancellation = self.cancelled.subscribe();

        tokio::select! {
            biased;

            _ = wait_for_cancellation(cancellation) => {
                bail!("task cancelled");
            }

            result = async {
                match budget {
                    Some(tokens) => {
                        self.inner.complete_with_budget(prompt, tokens).await
                    }
                    None => self.inner.complete(prompt).await,
                }
            } => result,
        }
    }
}

async fn wait_for_cancellation(mut receiver: watch::Receiver<bool>) {
    loop {
        let cancelled = *receiver.borrow_and_update();

        if cancelled {
            return;
        }

        if receiver.changed().await.is_err() {
            // Treat the disappearance of the owner as cancellation.
            return;
        }
    }
}

#[async_trait]
impl LlmBackend for TaskBackend<'_> {
    async fn complete(&self, prompt: &str) -> Result<String> {
        self.generate(prompt, None).await
    }

    async fn complete_with_budget(&self, prompt: &str, max_tokens: u32) -> Result<String> {
        self.generate(prompt, Some(max_tokens)).await
    }

    async fn set_system(&self, system_prompt: &str) {
        self.inner.set_system(system_prompt).await;
    }

    async fn reset_context(&self) {
        self.inner.reset_context().await;
    }

    async fn pop_last(&self) {
        self.inner.pop_last().await;
    }
}

impl OllamaBackend {
    /// Use a finite idle timeout during the task.
    ///
    /// This intentionally overrides TURTLE_KEEP_ALIVE when explicitly
    /// selected by the CLI's task-unloading mode.
    ///
    /// A finite timeout remains useful if cleanup cannot run, for example
    /// after a crash or forced process termination.
    pub fn with_idle_unload_secs(mut self, seconds: u64) -> Self {
        self.keep_alive = format!("{}s", seconds.clamp(1, 86_400));
        self
    }

    /// Clear task conversation history and request model unloading.
    ///
    /// The deadline covers:
    /// - waiting for this backend's inference lock;
    /// - clearing conversation history;
    /// - the HTTP request;
    /// - reading Ollama's response.
    ///
    /// Call only after the task has finished or cooperatively cancelled.
    ///
    /// This lock coordinates this backend instance only. It cannot
    /// coordinate other Turtle processes or other Ollama clients.
    pub async fn unload_after_task(&self, deadline: Duration) -> Result<()> {
        ensure!(!deadline.is_zero(), "cleanup timeout must be positive");

        tokio::time::timeout(deadline, async {
            let _request_guard = self.request_lock.lock().await;

            // Drop task-specific messages and their backing allocation.
            // A subsequent Agent::run must install its system prompt again.
            {
                let mut history = self.history.lock().await;
                *history = Vec::new();
            }

            let response: Value = self
                .client
                .post(format!("{}/api/generate", self.base_url))
                .timeout(deadline)
                .json(&json!({
                    "model": self.model_name,
                    "stream": false,
                    "keep_alive": 0
                }))
                .send()
                .await
                .context("could not send Ollama unload request")?
                .error_for_status()
                .context("Ollama rejected the unload request")?
                .json()
                .await
                .context("invalid JSON in Ollama unload response")?;

            if let Some(error) = response["error"].as_str() {
                bail!("Ollama unload error: {error}");
            }

            ensure!(
                response["done"].as_bool() == Some(true),
                "Ollama did not acknowledge completion of the unload request"
            );

            eprintln!(
                "Ollama acknowledged unload request for {}.",
                self.model_name
            );

            Ok::<(), anyhow::Error>(())
        })
        .await
        .with_context(|| {
            format!(
                "model cleanup exceeded its {}-second deadline",
                deadline.as_secs_f64()
            )
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_is_seen_by_new_subscribers() {
        let (sender, _) = watch::channel(false);
        sender.send_replace(true);

        tokio::time::timeout(
            Duration::from_secs(1),
            wait_for_cancellation(sender.subscribe()),
        )
        .await
        .expect("cancellation should already be visible");
    }

    #[tokio::test]
    async fn cancellation_is_seen_by_existing_subscribers() {
        let (sender, receiver) = watch::channel(false);
        sender.send_replace(true);

        tokio::time::timeout(Duration::from_secs(1), wait_for_cancellation(receiver))
            .await
            .expect("subscriber should observe cancellation");
    }

    #[tokio::test]
    async fn cancelled_task_does_not_start_inference() {
        let backend = OllamaBackend::new("unused-test-model");
        let task = TaskBackend::new(&backend);

        task.cancel();

        let error = task
            .complete("This must not make an HTTP request.")
            .await
            .expect_err("cancelled tasks must reject inference");

        assert!(error.to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn cleanup_timeout_includes_waiting_for_request_lock() {
        let backend = OllamaBackend::new("unused-test-model");

        // Holding this lock prevents cleanup from reaching the network.
        let _guard = backend.request_lock.lock().await;

        let error = backend
            .unload_after_task(Duration::from_millis(20))
            .await
            .expect_err("cleanup must time out while the lock is held");

        assert!(error.to_string().contains("deadline"));
    }

    #[test]
    fn idle_timeout_is_finite_and_positive() {
        let backend = OllamaBackend::new("unused-test-model").with_idle_unload_secs(0);

        assert_eq!(backend.keep_alive, "1s");

        let backend = backend.with_idle_unload_secs(u64::MAX);
        assert_eq!(backend.keep_alive, "86400s");
    }
}
