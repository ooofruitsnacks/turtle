use super::LlmBackend;
use anyhow::Result;
use async_trait::async_trait;

pub struct MockLlmBackend;

#[async_trait]
impl LlmBackend for MockLlmBackend {
    async fn complete(&self, prompt: &str) -> Result<String> {
        let first_line = prompt.lines().next().unwrap_or("");
        Ok(format!(
            "```src/main.rs\n// Mock implementation for: {first_line}\nfn main() {{\n    println!\"Hello from turtle\");\n}}\n```"
        ))
    }
}

