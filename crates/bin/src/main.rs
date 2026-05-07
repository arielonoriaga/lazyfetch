use clap::{Parser, Subcommand};

mod run;

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    match cli.cmd {
        Some(Cmd::Run(a)) => run::run(a).await,
        None => tokio::task::spawn_blocking(|| {
            lazyfetch_tui::event::run(lazyfetch_tui::app::AppState::new())
        })
        .await?,
    }
}
