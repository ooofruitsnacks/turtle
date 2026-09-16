use anyhow::{ensure, Context, Result};
use clap::Parser;
use std::io::{self, Write};
use std::path::PathBuf;
use turtle::agent::Agent;
use turtle::config::{ChecksFile, Config, Language, Runtime};
use turtle::llm::ollama::OllamaBackend;
use turtle::project::Project;

#[derive(Parser, Debug)]
#[command(name = "turtle", about = "Local multilingual coding assistant")]
struct Args {
    #[arg(short, long, help = "Installed Ollama model name")]
    model: String,

    #[arg(
        short,
        long,
        value_enum,
        value_delimiter = ',',
        default_value = "rust",
        help = "One or more languages, separated by commas"
    )]
    language: Vec<Language>,

    #[arg(long, value_enum, default_value = "auto")]
    runtime: Runtime,

    #[arg(short, long, default_value = "./generated")]
    project: PathBuf,

    #[arg(short, long, default_value_t = 3)]
    iterations: u32,

    #[arg(long, help = "Context size; otherwise use TURTLE_NUM_CTX or 8192")]
    context: Option<u32>,

    #[arg(long, help = "Trusted checks JSON file outside the target project")]
    checks: Option<PathBuf>,

    #[arg(
        long,
        help = "Authorize configured builds/tests, which can execute project code"
    )]
    allow_checks: bool,

    #[arg(long, help = "Task text; otherwise prompt interactively")]
    task: Option<String>,

    #[arg(long, help = "Print a configuration summary")]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let language = *args.language.first().context("no language selected")?;

    let context_size = args.context.unwrap_or_else(|| {
        std::env::var("TURTLE_NUM_CTX")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(8192)
    });

    Project::scaffold(&args.project, language, "generated_project").await?;
    let project_dir = args.project.canonicalize()?;

    let checks = if let Some(path) = &args.checks {
        let path = path
            .canonicalize()
            .with_context(|| format!("cannot locate checks file: {}", path.display()))?;

        ensure!(
            !path.starts_with(&project_dir),
            "keep the trusted checks file outside the target project"
        );

        ensure!(
            std::fs::metadata(&path)?.len() <= 64 * 1024,
            "checks file is too large"
        );

        let text = std::fs::read_to_string(&path)?;
        let parsed: ChecksFile = serde_json::from_str(&text)
            .with_context(|| format!("invalid checks JSON: {}", path.display()))?;

        Some(parsed.checks)
    } else {
        None
    };

    let config = Config {
        model_path: PathBuf::from(&args.model),
        context_size,
        max_iterations: args.iterations,
        project_dir,
        language,
        additional_languages: args.language.into_iter().skip(1).collect(),
        runtime: args.runtime,
        allow_checks: args.allow_checks,
        checks,
        debug: args.debug,
        ..Config::default()
    };

    config.validate()?;

    if config.debug {
        eprintln!(
            "Languages: {:?}; runtime: {:?}; context: {}; checks authorized: {}",
            config.languages(),
            config.runtime,
            config.context_size,
            config.allow_checks
        );
    }

    let prompt = match args.task {
        Some(task) => task,
        None => {
            print!("Enter your coding prompt: ");
            io::stdout().flush()?;

            let mut task = String::new();
            io::stdin().read_line(&mut task)?;
            task
        }
    };

    ensure!(!prompt.trim().is_empty(), "task must not be empty");

    let backend = OllamaBackend::new(&args.model).with_context_size(config.context_size);

    backend.check().await?;

    let mut agent = Agent::new(&backend, &config);
    let state = agent.run(&prompt).await?;

    println!(
        "Stopped after {} edit action(s). Verification: {:?}.",
        state.iteration, state.verification
    );

    if !state.verification_message.is_empty() {
        println!("{}", state.verification_message);
    }

    Ok(())
}
