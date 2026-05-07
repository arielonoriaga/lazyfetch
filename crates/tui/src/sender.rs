use crate::app::AppState;
use lazyfetch_auth::resolver::DefaultResolver;
use lazyfetch_auth::NoCache;
use lazyfetch_core::catalog::{Body, Request};
use lazyfetch_core::env::{Environment, ResolveCtx, VarValue};
use lazyfetch_core::exec::{execute, AuthChain, ExecError, Executed};
use lazyfetch_core::ports::SystemClock;
use lazyfetch_core::primitives::{Template, UrlTemplate};
use lazyfetch_http::ReqwestSender;
use secrecy::SecretString;
use std::sync::mpsc;
use tokio::runtime::Handle;
use ulid::Ulid;

/// Build a `Request` from the current TUI state and dispatch it on the tokio runtime.
/// Returns a sync receiver the event loop polls each tick.
pub fn dispatch(state: &AppState, rt: Handle) -> mpsc::Receiver<Result<Executed, ExecError>> {
    let (tx, rx) = mpsc::channel();
    let req = Request {
        id: Ulid::new(),
        name: "ad-hoc".into(),
        method: state.method.clone(),
        url: UrlTemplate(Template(state.url_buf.clone())),
        query: vec![],
        headers: vec![],
        body: Body::None,
        auth: None,
        notes: None,
        follow_redirects: true,
        max_redirects: 10,
        timeout_ms: None,
    };
    let env = state
        .active_env_ref()
        .cloned()
        .unwrap_or_else(|| Environment {
            id: Ulid::new(),
            name: "_empty".into(),
            vars: vec![],
        });
    let overrides: Vec<(String, VarValue)> = vec![];
    let coll_vars: Vec<(String, VarValue)> = vec![];

    rt.spawn(async move {
        let ctx = ResolveCtx {
            env: &env,
            collection_vars: &coll_vars,
            overrides: &overrides,
        };
        let resolver = DefaultResolver::new();
        let cache = NoCache;
        let http = ReqwestSender::new();
        let clock = SystemClock;
        let chain = AuthChain {
            folders: &[],
            collection: None,
        };
        let result = execute(&req, &ctx, chain, &resolver, &cache, &http, &clock).await;
        let _ = tx.send(result);
        // Touch types so unused warnings stay quiet across feature toggles.
        let _ = SecretString::new(String::new());
    });
    rx
}
