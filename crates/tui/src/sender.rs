use crate::app::AppState;
use lazyfetch_core::catalog::{Body, Request};
use lazyfetch_core::env::{Environment, ResolveCtx, VarValue};
use lazyfetch_core::exec::{execute, AuthChain, ExecError, Executed};
use lazyfetch_core::primitives::{Template, UrlTemplate};
use std::sync::mpsc;
use tokio::runtime::Handle;
use ulid::Ulid;

/// Build a `Request` from the current TUI state and dispatch it on the tokio runtime.
/// Returns a sync receiver the event loop polls each tick.
pub fn dispatch(state: &AppState, rt: Handle) -> mpsc::Receiver<Result<Executed, ExecError>> {
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
    dispatch_request(&req, state, rt)
}

/// Dispatch an explicit `Request` (used by repeat-last replays). Re-interpolates
/// against the current env so dyn-vars re-roll and env switches take effect.
pub fn dispatch_request(
    req: &Request,
    state: &AppState,
    rt: Handle,
) -> mpsc::Receiver<Result<Executed, ExecError>> {
    let (tx, rx) = mpsc::channel();
    let req = req.clone();
    let env = state
        .active_env_ref()
        .cloned()
        .unwrap_or_else(|| Environment {
            id: Ulid::new(),
            name: "_empty".into(),
            vars: vec![],
        });
    // Clone the injected adapter handles so the spawned future is 'static.
    let adapters = state.adapters.clone();

    rt.spawn(async move {
        let overrides: Vec<(String, VarValue)> = vec![];
        let coll_vars: Vec<(String, VarValue)> = vec![];
        let ctx = ResolveCtx {
            env: &env,
            collection_vars: &coll_vars,
            overrides: &overrides,
        };
        let chain = AuthChain {
            folders: &[],
            collection: None,
        };
        let result = execute(
            &req,
            &ctx,
            chain,
            &*adapters.auth_resolver,
            &*adapters.auth_cache,
            &*adapters.http,
            &*adapters.clock,
        )
        .await;
        let _ = tx.send(result);
    });
    rx
}
