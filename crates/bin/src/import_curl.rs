use clap::Args;
use lazyfetch_import::curl;
use lazyfetch_storage::collection::FsCollectionRepo;

#[derive(Args)]
pub struct ImportCurlArgs {
    /// cURL command, file path, or `-` for stdin (default if omitted).
    pub input: Option<String>,
    /// Save into <coll>/<name>.
    #[arg(long)]
    pub save: Option<String>,
    #[arg(long)]
    pub config_dir: Option<std::path::PathBuf>,
}

pub fn run(args: ImportCurlArgs) -> anyhow::Result<()> {
    let input = match args.input.as_deref() {
        None | Some("-") => {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
            s
        }
        Some(s) if std::path::Path::new(s).exists() => std::fs::read_to_string(s)?,
        Some(s) => s.to_string(),
    };
    let (mut req, report) = curl::parse(&input)?;
    println!(
        "imported {} {} (warnings: {})",
        req.method,
        req.url.0 .0,
        report.warnings.len()
    );
    for w in &report.warnings {
        eprintln!("warn: {w}");
    }
    if let Some(path) = args.save {
        let cwd = std::env::current_dir().unwrap_or_default();
        let cfg = crate::resolve_config_dir(args.config_dir, &cwd);
        let (coll, name) = path
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("--save expects <coll>/<name>"))?;
        req.name = name.into();
        FsCollectionRepo::new(cfg.join("collections")).save_request(coll, &req)?;
        println!("saved {coll}/{name}");
    }
    Ok(())
}
