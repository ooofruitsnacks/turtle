use anyhow::Result;
use clap::Parser;
use turtle::agent::Agent;
use turtle::config::{Config, Language};
use turtle::project::Project;
use std::io::{self, Write};

#[derive(Parser, Debug)]
#[command(name = "turtle", about = "Offline coding assistant")]
struct Args {
    #[arg(short, long, help = "Ollama model name, e.g. qwen2.5-coder:32b")]
    model: String,

    #[arg(short, long, value_enum, default_value = "rust")]
    language: LanguageArg,

    #[arg(short, long, default_value = "./generated")]
    project: std::path::PathBuf,

    #[arg(short, long, default_value = "5")]
    iterations: u32,

    #[arg(long, help = "Enable verbose debug tracing of prompts and actions")]
    debug: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum LanguageArg {
    Rust,
    Odin,
}

impl From<LanguageArg> for Language {
    fn from(arg: LanguageArg) -> Self {
        match arg {
            LanguageArg::Rust => Language::Rust,
            LanguageArg::Odin => Language::Odin,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Ollama backend — no file paths needed
    let backend = turtle::llm::ollama::OllamaBackend::new(&args.model);
    backend.check().await?;

    let config = Config {
        model_path: std::path::PathBuf::from(&args.model), // kept for metadata
        chat_template: std::path::PathBuf::new(), // Ollama handles templating
        context_size: 8192,
        max_iterations: args.iterations,
        project_dir: args.project.clone(),
        language: args.language.into(),
        debug: args.debug,
    };

    Project::scaffold(&config.project_dir, config.language, "generated_project").await?;

    let mut agent = Agent::new(&backend, &config);

    print!("Enter your coding prompt: ");
    io::stdout().flush()?;
    let mut prompt = String::new();
    io::stdin().read_line(&mut prompt)?;

    let state = agent.run(&prompt).await?;
    println!("Finished after {} iterations.", state.iteration);

    Ok(())
}

