use crate::config::Language;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct AgentState {
    pub task: String,
    pub language : Language,
    pub files: HashMap<String, String>,
    pub iteration: u32,
    pub diagnostics: Vec<String>,
    pub done: bool,
}
