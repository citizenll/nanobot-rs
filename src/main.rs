use anyhow::Result;
use clap::{Parser, Subcommand};
use nanobot_rs::VERSION;
use nanobot_rs::agent::AgentLoop;
use nanobot_rs::bus::MessageBus;
use nanobot_rs::config::{Config, get_config_path, load_config, providers_status, save_config};
use nanobot_rs::providers::openai::OpenAIProvider;
use nanobot_rs::utils::get_workspace_path;
use std::io::BufRead;
use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(name = "nanobot-rs", about = "nanobot: Rust port of the lightweight personal AI assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Onboard,
    Agent {
        #[arg(short, long)]
        message: Option<String>,
        #[arg(short, long, default_value = "cli:default")]
        session: String,
    },
    Status,
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Onboard => cmd_onboard()?,
        Commands::Status => cmd_status()?,
        Commands::Version => {
            println!("nanobot-rs v{VERSION}");
        }
        Commands::Agent { message, session } => {
            cmd_agent(message, &session).await?;
        }
    }
    Ok(())
}

fn cmd_onboard() -> Result<()> {
    let config_path = get_config_path()?;
    if config_path.exists() {
        println!("Config already exists at {}", config_path.display());
        return Ok(());
    }

    let config = Config::default();
    save_config(&config, Some(&config_path))?;
    println!("Created config at {}", config_path.display());

    let workspace = get_workspace_path(Some(&config.agents.defaults.workspace))?;
    println!("Created workspace at {}", workspace.display());

    let templates = [
        (
            "AGENTS.md",
            "# Agent Instructions\n\nYou are a helpful AI assistant. Be concise and accurate.\n",
        ),
        (
            "SOUL.md",
            "# Soul\n\nI am nanobot-rs, a lightweight Rust AI assistant.\n",
        ),
        (
            "USER.md",
            "# User\n\nRecord user preferences and context here.\n",
        ),
    ];
    for (name, content) in templates {
        let path = workspace.join(name);
        if !path.exists() {
            std::fs::write(&path, content)?;
            println!("Created {}", path.display());
        }
    }

    let memory_dir = workspace.join("memory");
    std::fs::create_dir_all(&memory_dir)?;
    let memory_file = memory_dir.join("MEMORY.md");
    if !memory_file.exists() {
        std::fs::write(
            &memory_file,
            "# Long-term Memory\n\nThis file stores important information across sessions.\n",
        )?;
        println!("Created {}", memory_file.display());
    }

    println!("nanobot-rs is ready.");
    println!("Next steps:");
    println!("1. Add your API key to {}", config_path.display());
    println!("2. Chat: nanobot-rs agent -m \"Hello!\"");
    Ok(())
}

fn cmd_status() -> Result<()> {
    let config_path = get_config_path()?;
    let config = load_config(Some(&config_path)).unwrap_or_default();
    let workspace = config.workspace_path();

    println!("nanobot-rs Status");
    println!(
        "Config: {} {}",
        config_path.display(),
        if config_path.exists() { "OK" } else { "MISSING" }
    );
    println!(
        "Workspace: {} {}",
        workspace.display(),
        if workspace.exists() { "OK" } else { "MISSING" }
    );
    println!("Model: {}", config.agents.defaults.model);

    let status = providers_status(&config);
    println!(
        "OpenRouter API: {}",
        if status.get("openrouter").and_then(|v| v.as_bool()).unwrap_or(false) {
            "SET"
        } else {
            "NOT SET"
        }
    );
    println!(
        "Anthropic API: {}",
        if status.get("anthropic").and_then(|v| v.as_bool()).unwrap_or(false) {
            "SET"
        } else {
            "NOT SET"
        }
    );
    println!(
        "OpenAI API: {}",
        if status.get("openai").and_then(|v| v.as_bool()).unwrap_or(false) {
            "SET"
        } else {
            "NOT SET"
        }
    );
    println!(
        "Gemini API: {}",
        if status.get("gemini").and_then(|v| v.as_bool()).unwrap_or(false) {
            "SET"
        } else {
            "NOT SET"
        }
    );
    println!(
        "vLLM/Local: {}",
        if status.get("vllm").and_then(|v| v.as_bool()).unwrap_or(false) {
            "SET"
        } else {
            "NOT SET"
        }
    );

    Ok(())
}

async fn cmd_agent(message: Option<String>, session: &str) -> Result<()> {
    let config = load_config(None).unwrap_or_default();
    let model = config.agents.defaults.model.clone();
    let is_bedrock = model.starts_with("bedrock/");
    let api_key = config.get_api_key(Some(&model));
    if api_key.is_none() && !is_bedrock {
        println!("Error: No API key configured.");
        println!("Set one in ~/.nanobot/config.json under providers.*.apiKey");
        return Ok(());
    }

    let bus = Arc::new(MessageBus::new(1024));
    let provider = Arc::new(OpenAIProvider::new(
        api_key.unwrap_or_else(|| "dummy".to_string()),
        config.get_api_base(Some(&model)),
        model.clone(),
    ));
    let agent_loop = AgentLoop::new(
        bus,
        provider,
        config.workspace_path(),
        Some(model),
        config.agents.defaults.max_tool_iterations,
        if config.tools.web.search.api_key.is_empty() {
            None
        } else {
            Some(config.tools.web.search.api_key.clone())
        },
        config.tools.exec.timeout,
        config.tools.restrict_to_workspace,
    )?;

    if let Some(content) = message {
        let response = agent_loop
            .process_direct(&content, Some(session), None, None)
            .await?;
        println!("nanobot-rs: {response}");
    } else {
        println!("nanobot-rs interactive mode (Ctrl+C to exit)");
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let input = line?;
            if input.trim().is_empty() {
                continue;
            }
            let response = agent_loop
                .process_direct(&input, Some(session), None, None)
                .await?;
            println!("nanobot-rs: {response}");
        }
    }
    Ok(())
}
