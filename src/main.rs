use anyhow::{bail, ensure, Context, Result};
use clap::Parser;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use turtle::agent::Agent;
use turtle::config::{ChecksFile, Config, Language, Runtime};
use turtle::llm::ollama::task_lifecycle::TaskBackend;
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

    #[arg(
        long,
        help = "Request model unloading after task completion, error, or cancellation"
    )]
    unload_on_exit: bool,

    #[arg(
        long,
        default_value_t = 300,
        value_parser = clap::value_parser!(u64).range(1..=86_400),
        help = "Idle model timeout with --unload-on-exit; overrides TURTLE_KEEP_ALIVE"
    )]
    idle_unload_secs: u64,

    #[arg(
        long,
        default_value_t = 15,
        value_parser = clap::value_parser!(u64).range(1..=120),
        help = "Maximum seconds allowed for task-end model cleanup"
    )]
    unload_timeout_secs: u64,
}

/// Wait for graceful shutdown.
///
/// On Unix, support both Ctrl+C and SIGTERM.
/// On Windows, support Ctrl+C.
///
/// This does not handle force-kill, power loss, or process aborts.
async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut terminate =
            signal(SignalKind::terminate()).context("could not install SIGTERM handler")?;

        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.context("could not listen for Ctrl+C")
            }

            received = terminate.recv() => {
                received.context("SIGTERM signal stream closed")?;
                Ok(())
            }
        }
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .context("could not listen for Ctrl+C")
    }
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

    let mut backend = OllamaBackend::new(&args.model).with_context_size(config.context_size);

    if args.unload_on_exit {
        backend = backend.with_idle_unload_secs(args.idle_unload_secs);

        eprintln!(
            "Task-end unloading enabled: idle fallback={}s, cleanup deadline={}s.",
            args.idle_unload_secs, args.unload_timeout_secs
        );
    }

    backend.check().await?;

    let task_backend = TaskBackend::new(&backend);

    let (task_result, shutdown_result) = {
        let mut agent = Agent::new(&task_backend, &config);
        let task = agent.run(&prompt);
        tokio::pin!(task);

        tokio::select! {
            biased;

            signal = shutdown_signal() => {
                match &signal {
                    Ok(()) => {
                        eprintln!(
                            "\nCancellation requested. Stopping inference; \
                             waiting for any active edit/check to finish..."
                        );
                    }

                    Err(error) => {
                        eprintln!(
                            "\nShutdown listener failed: {error:#}. \
                             Cancelling the task safely..."
                        );
                    }
                }

                task_backend.cancel();

                let result = task.await;
                (result, Some(signal))
            }

            result = &mut task => {
                (result, None)
            }
        }
    };

    if args.unload_on_exit {
        if let Err(error) = backend
            .unload_after_task(Duration::from_secs(args.unload_timeout_secs))
            .await
        {
            eprintln!(
                "Warning: model cleanup did not complete: {error:#}\n\
                 Ollama's configured idle timeout remains the fallback."
            );
        }
    }

    if let Some(signal) = shutdown_result {
        if let Err(error) = &task_result {
            eprintln!("Task stopped: {error:#}");
        }

        signal.context("shutdown signal listener failed")?;
        bail!("task cancelled; any edits already written remain in the project");
    }

    let state = task_result?;

    println!(
        "Stopped after {} edit action(s). Verification: {:?}.",
        state.iteration, state.verification
    );

    if !state.verification_message.is_empty() {
        println!("{}", state.verification_message);
    }

    Ok(())
}
