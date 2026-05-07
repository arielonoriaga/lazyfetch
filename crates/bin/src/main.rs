use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod import;
mod run;

pub fn default_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lazyfetch")
}

#[derive(Parser)]
#[command(name = "lazyfetch", version, about = "Terminal HTTP client")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Send a saved request headlessly
    Run(run::RunArgs),
    /// Import a Postman v2.1 collection
    ImportPostman(import::ImportArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Some(Cmd::Run(a)) => run::run(a).await,
        Some(Cmd::ImportPostman(a)) => import::run(a),
        None => {
            tokio::task::spawn_blocking(|| {
                lazyfetch_tui::event::run(lazyfetch_tui::app::AppState::new())
            })
            .await?
        }
    }
}
