use anyhow::{anyhow, Context, Result};
use clap::Args;
use lazyfetch_auth::resolver::DefaultResolver;
use lazyfetch_auth::NoCache;
use lazyfetch_core::catalog::{Folder, Item, Request};
use lazyfetch_core::env::{Environment, ResolveCtx, VarValue};
use lazyfetch_core::exec::{execute, AuthChain};
use lazyfetch_core::ports::SystemClock;
use lazyfetch_core::secret::SecretRegistry;
use lazyfetch_http::ReqwestSender;
use lazyfetch_storage::collection::FsCollectionRepo;
use lazyfetch_storage::env::FsEnvRepo;
use secrecy::SecretString;
use std::path::PathBuf;

#[derive(Args)]
pub struct RunArgs {
    /// Path within collections: "<collection>/<folder>/.../<request>"
    pub request_path: String,
    #[arg(long)]
    pub env: Option<String>,
    /// Override variable: --set k=v (repeatable)
    #[arg(long)]
    pub set: Vec<String>,
    #[arg(long)]
    pub config_dir: Option<PathBuf>,
}

pub async fn run(args: RunArgs) -> Result<()> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cfg = crate::resolve_config_dir(args.config_dir, &cwd);
    let segments: Vec<&str> = args.request_path.split('/').collect();
    if segments.len() < 2 {
        return Err(anyhow!(
            "request path must be `<collection>/<request>` or deeper"
        ));
    }
    let coll_name = segments[0];
    let req_path = &segments[1..];

    let coll_repo = FsCollectionRepo::new(cfg.join("collections"));
    let coll = coll_repo
        .load_by_name(coll_name)
        .with_context(|| format!("loading collection `{}`", coll_name))?;
    let req = find_request(&coll.root, req_path)
        .ok_or_else(|| anyhow!("request not found: {}", args.request_path))?
        .clone();

    let env = match args.env {
        Some(name) => FsEnvRepo::new(cfg.join("environments"))
            .load_by_name(&name)
            .with_context(|| format!("loading env `{}`", name))?,
        None => Environment {
            id: ulid::Ulid::new(),
            name: "_empty".into(),
            vars: vec![],
        },
    };
    let overrides: Vec<(String, VarValue)> = args
        .set
        .iter()
        .map(|s| {
            let (k, v) = s
                .split_once('=')
                .ok_or_else(|| anyhow!("--set expects k=v: got `{}`", s))?;
            Ok::<_, anyhow::Error>((
                k.to_string(),
                VarValue {
                    value: SecretString::new(v.into()),
                    secret: false,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let coll_vars: Vec<(String, VarValue)> = coll
        .vars
        .iter()
        .map(|kv| {
            (
                kv.key.clone(),
                VarValue {
                    value: SecretString::new(kv.value.clone()),
                    secret: kv.secret,
                },
            )
        })
        .collect();
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &coll_vars,
        overrides: &overrides,
    };

    let resolver = DefaultResolver::new();
    let cache = NoCache;
    let http = ReqwestSender::new();
    let clock = SystemClock;
    let auth_chain = AuthChain {
        folders: &[],
        collection: coll.auth.as_ref(),
    };

    let executed = execute(&req, &ctx, auth_chain, &resolver, &cache, &http, &clock)
        .await
        .map_err(|e| anyhow!("{e}"))?;

    let reg: &SecretRegistry = &executed.secrets;
    println!(
        "{} {}ms",
        executed.response.status,
        executed.response.elapsed.as_millis()
    );
    for (k, v) in &executed.response.headers {
        println!("{}: {}", k, v);
    }
    println!();
    let body = String::from_utf8_lossy(&executed.response.body_bytes);
    println!("{}", reg.redact(&body));
    Ok(())
}

fn find_request<'a>(folder: &'a Folder, path: &[&str]) -> Option<&'a Request> {
    if path.is_empty() {
        return None;
    }
    let head = path[0];
    let rest = &path[1..];
    for item in &folder.items {
        match item {
            Item::Folder(f) if f.name == head => {
                if rest.is_empty() {
                    continue;
                }
                if let Some(r) = find_request(f, rest) {
                    return Some(r);
                }
            }
            Item::Request(r) if r.name == head && rest.is_empty() => return Some(r),
            _ => {}
        }
    }
    None
}
