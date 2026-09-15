#[derive(Debug, Clone)]
pub enum Action {
    Plan { steps: Vec<String> },
    WriteFile { path: String, content: String },
    RunCommand { command: String },
    Fix { explanation: String, changes: Vec<(String, String)> },
    Done { summary: String },
}

