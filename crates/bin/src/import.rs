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
}

pub fn run(args: ImportArgs) -> Result<()> {
    let cfg = args.config_dir.unwrap_or_else(super::default_config_dir);
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
