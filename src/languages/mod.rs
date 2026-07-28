use crate::agent::AgentState;
use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

pub mod odin;
pub mod rust;

#[async_trait]
pub trait LanguageExpert: Send + Sync {
    fn name(&self) -> &'static str;
    fn system_prompt(&self) -> String;
    fn plan_prompt(&self, task: &str) -> String;
    fn code_prompt(&self, task: &str, step: &str, project_context: &str) -> String;
    fn fix_prompt(&self, state: &AgentState, project_context: &str) -> String;
    async fn check_project(&self, project_dir: &Path) -> Result<Vec<String>>;
}

