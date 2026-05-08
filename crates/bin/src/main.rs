use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod config;
mod import;
mod import_curl;
mod run;

pub use config::resolve as resolve_config_dir;

#[derive(Parser)]
#[command(name = "lazyfetch", version, about = "Terminal HTTP client")]
struct Cli {
    /// Override config dir (otherwise: nearest .lazyfetch/ → ~/.config/lazyfetch)
    #[arg(long, global = true)]
    config_dir: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Send a saved request headlessly
    Run(run::RunArgs),
    /// Import a Postman v2.1 collection
    ImportPostman(import::ImportArgs),
    /// Import a cURL command (positional / file / stdin) into a Request
    ImportCurl(import_curl::ImportCurlArgs),
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
        Some(Cmd::ImportCurl(a)) => import_curl::run(a),
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let cfg = resolve_config_dir(cli.config_dir, &cwd);
            let rt = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                lazyfetch_tui::event::run(lazyfetch_tui::app::AppState::new(cfg), rt)
            })
            .await?
        }
    }
}
