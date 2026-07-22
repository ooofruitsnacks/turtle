use anyhow::Result;
use clap::Parser;
use turtle::agent::Agent;
use turtle::config::{Config, Language};
use turtle::project::Project;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "turtle",
    about = "Offline agentic coding program for Rust and Odin"
)]
struct Args {
    #[arg(short, long, help = "Path to the local GGUF model file")]
    model: PathBuf,

    #[arg(short, long, help = "Path to the chat template JSON file")]
    chat_template: PathBuf,

    #[arg(short, long, default_value = "4096")]
    context: u32,

    #[arg(short, long, value_enum, default_value = "rust")]
    language: LanguageArg,

    #[arg(short, long, default_value = "./generated")]
    project: PathBuf,

    #[arg(short, long, default_value = "5")]
    iterations: u32,
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

    let config = Config {
        model_path: args.model,
        chat_template: args.chat_template,
        context_size: args.context,
        max_iterations: args.iterations,
        project_dir: args.project.clone(),
        language: args.language.into(),
    };

    Project::scaffold(&config.project_dir, config.language, "generated_project").await?;

    let backend = turtle::llm::local::MistralRsBackend::load(
        &config.model_path,
        &config.chat_template,
    ).await?;

    let mut agent = Agent::new(&backend, &config);

    print!("What do you want to make?? ");
    io::stdout().flush()?;
    let mut prompt = String::new();
    io::stdin().read_line(&mut prompt)?;

    let state = agent.run(&prompt).await?;
    println!("Finished after {} iterations.", state.iteration);

    Ok(())
}

