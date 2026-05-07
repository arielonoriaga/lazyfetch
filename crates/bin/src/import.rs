use anyhow::{Context, Result};
use clap::Args;
use lazyfetch_import::postman;
use lazyfetch_storage::collection::FsCollectionRepo;
use std::path::PathBuf;

#[derive(Args)]
pub struct ImportArgs {
    /// Path to a Postman v2.1 collection JSON
    pub file: PathBuf,
    #[arg(long)]
    pub config_dir: Option<PathBuf>,
    /// Save into the project-local `.lazyfetch/` of the current directory
    /// (creates it if absent). Ignored when `--config-dir` is set.
    #[arg(long)]
    pub local: bool,
}

pub fn run(args: ImportArgs) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cfg = if args.config_dir.is_some() {
        super::resolve_config_dir(args.config_dir, &cwd)
    } else if args.local {
        let p = cwd.join(".lazyfetch");
        std::fs::create_dir_all(&p).with_context(|| format!("creating {:?}", p))?;
        p
    } else {
        super::resolve_config_dir(None, &cwd)
    };
    let json =
        std::fs::read_to_string(&args.file).with_context(|| format!("reading {:?}", args.file))?;
    let (coll, report) = postman::parse(&json).context("parsing Postman collection")?;
    let repo = FsCollectionRepo::new(cfg.join("collections"));
    repo.save(&coll).context("saving collection")?;
    println!(
        "imported '{}' ({} warnings)",
        coll.name,
        report.warnings.len()
    );
    for w in &report.warnings {
        eprintln!("warn: {}", w);
    }
    Ok(())
}
