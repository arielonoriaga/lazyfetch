use crate::auth::{AuthCache, AuthError, AuthResolver, AuthSpec};
use crate::catalog::{Body, PartContent, Request};
use crate::dynvars::DynCtx;
use crate::env::{interpolate_with_dyn, ResolveCtx};
use crate::error::CoreError;
use crate::ports::Clock;
use crate::secret::SecretRegistry;

/// Bundles the three context refs needed to render a Request into a wire form.
/// Lives only inside `execute`; callers don't construct it directly.
struct RenderCtx<'a, 'b> {
    env: &'a ResolveCtx<'a>,
    dyn_ctx: &'a DynCtx<'a>,
    reg: &'b mut SecretRegistry,
}

impl RenderCtx<'_, '_> {
    fn interp(&mut self, s: &str) -> Result<String, CoreError> {
        let i = interpolate_with_dyn(s, self.env, self.dyn_ctx)?;
        self.reg.extend(&i.used_secrets);
        Ok(i.value)
    }
}
use chrono::{DateTime, Utc};
use http::Method;
use std::path::PathBuf;
use std::time::Duration;

/// One field of a multipart/form-data body. Lives on `WireRequest` as a sidecar so the
/// reqwest adapter can build a `multipart::Form` (which owns the boundary + chunked
/// streaming for files). `body_bytes` is empty when `multipart` is set.
#[derive(Debug, Clone)]
pub struct MultipartField {
    pub name: String,
    pub kind: MultipartKind,
    pub filename: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MultipartKind {
    Text(String),
    File(PathBuf),
}

#[derive(Debug, Clone)]
pub struct WireRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body_bytes: Vec<u8>,
    /// When `Some`, the adapter sends a multipart/form-data body instead of raw bytes.
    /// Mutually exclusive with non-empty `body_bytes`.
    pub multipart: Option<Vec<MultipartField>>,
    pub timeout: Duration,
    pub follow_redirects: bool,
    pub max_redirects: u8,
}

#[derive(Debug, Clone)]
pub struct WireResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body_bytes: Vec<u8>,
    pub elapsed: Duration,
    pub size: u64,
}

#[async_trait::async_trait]
pub trait HttpSender: Send + Sync {
    async fn send(&self, r: WireRequest) -> Result<WireResponse, SendError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("timeout")]
    Timeout,
    #[error("network: {0}")]
    Net(String),
    #[error("tls: {0}")]
    Tls(String),
    #[error("dns: {0}")]
    Dns(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone)]
pub struct Executed {
    /// Pre-interpolation Request (with `{{var}}` placeholders intact). Used by
    /// repeat-last (`R`): replays against the current env so dyn-vars re-roll.
    pub request_template: Request,
    pub request_snapshot: WireRequest,
    pub response: WireResponse,
    pub at: DateTime<Utc>,
    pub secrets: SecretRegistry,
}

pub fn redact_wire(w: &WireRequest, reg: &SecretRegistry) -> WireRequest {
    let mut r = w.clone();
    for h in &mut r.headers {
        h.1 = reg.redact(&h.1);
    }
    r.url = reg.redact(&r.url);
    if let Ok(body) = std::str::from_utf8(&r.body_bytes) {
        r.body_bytes = reg.redact(body).into_bytes();
    }
    r
}

pub struct AuthChain<'a> {
    pub folders: &'a [&'a AuthSpec],
    pub collection: Option<&'a AuthSpec>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Send(#[from] SendError),
}

#[tracing::instrument(
    target = "lazyfetch::exec",
    skip_all,
    fields(method = %req.method, name = %req.name)
)]
pub async fn execute(
    req: &Request,
    ctx: &ResolveCtx<'_>,
    auth_chain: AuthChain<'_>,
    resolver: &dyn AuthResolver,
    cache: &dyn AuthCache,
    http: &dyn HttpSender,
    clock: &dyn Clock,
) -> Result<Executed, ExecError> {
    let dyn_ctx = DynCtx { clock };
    let mut reg = SecretRegistry::new();
    let mut rc = RenderCtx {
        env: ctx,
        dyn_ctx: &dyn_ctx,
        reg: &mut reg,
    };
    let url_value = rc.interp(&req.url.0 .0)?;
    let mut headers: Vec<(String, String)> = Vec::new();
    for kv in req.headers.iter().filter(|k| k.enabled) {
        headers.push((kv.key.clone(), rc.interp(&kv.value)?));
    }
    let body_bytes = render_body(&req.body, &mut rc)?;
    let url = apply_query(&url_value, &req.query, &mut rc)?;
    let multipart = if let Body::Multipart(parts) = &req.body {
        Some(render_multipart(parts, &mut rc)?)
    } else {
        None
    };
    let mut wire = WireRequest {
        method: req.method.clone(),
        url,
        headers,
        body_bytes,
        multipart,
        timeout: Duration::from_millis(req.timeout_ms.unwrap_or(30_000) as u64),
        follow_redirects: req.follow_redirects,
        max_redirects: req.max_redirects,
    };
    if let Some(spec) =
        crate::auth::effective_auth(req.auth.as_ref(), auth_chain.folders, auth_chain.collection)
    {
        resolver
            .apply(spec, ctx, clock, cache, &mut wire, &mut reg)
            .await?;
    }
    let resp = http.send(wire.clone()).await?;
    Ok(Executed {
        request_template: req.clone(),
        request_snapshot: redact_wire(&wire, &reg),
        response: resp,
        at: clock.now(),
        secrets: reg,
    })
}

fn render_body(b: &Body, rc: &mut RenderCtx) -> Result<Vec<u8>, CoreError> {
    Ok(match b {
        Body::None => Vec::new(),
        Body::Raw { text, .. } | Body::Json { text } => rc.interp(text)?.into_bytes(),
        Body::Form(kvs) => {
            let mut s = String::new();
            for (i, kv) in kvs.iter().filter(|k| k.enabled).enumerate() {
                if i > 0 {
                    s.push('&');
                }
                let v = rc.interp(&kv.value)?;
                s.push_str(&urlencoding::encode(&kv.key));
                s.push('=');
                s.push_str(&urlencoding::encode(&v));
            }
            s.into_bytes()
        }
        Body::Multipart(_) | Body::File(_) => Vec::new(),
        Body::GraphQL { query, variables } => {
            let q = rc.interp(query)?;
            let vars_value: serde_json::Value = if variables.trim().is_empty() {
                serde_json::Value::Object(Default::default())
            } else {
                let v = rc.interp(variables)?;
                serde_json::from_str(&v).map_err(|e| {
                    CoreError::InvalidTemplate(format!("graphql variables: {e}"))
                })?
            };
            let body = serde_json::json!({ "query": q, "variables": vars_value });
            serde_json::to_vec(&body).map_err(|e| CoreError::InvalidTemplate(e.to_string()))?
        }
    })
}

fn render_multipart(
    parts: &[crate::catalog::Part],
    rc: &mut RenderCtx,
) -> Result<Vec<MultipartField>, CoreError> {
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        let kind = match &p.content {
            PartContent::Text(t) => MultipartKind::Text(rc.interp(t)?),
            PartContent::File(path) => MultipartKind::File(path.clone()),
        };
        out.push(MultipartField {
            name: p.name.clone(),
            kind,
            filename: p.filename.clone(),
        });
    }
    Ok(out)
}

fn apply_query(
    url: &str,
    q: &[crate::primitives::KV],
    rc: &mut RenderCtx,
) -> Result<String, CoreError> {
    let mut out = url.to_string();
    let mut first = !out.contains('?');
    for kv in q.iter().filter(|k| k.enabled) {
        out.push(if first { '?' } else { '&' });
        first = false;
        let v = rc.interp(&kv.value)?;
        out.push_str(&urlencoding::encode(&kv.key));
        out.push('=');
        out.push_str(&urlencoding::encode(&v));
    }
    Ok(out)
}

/// Render a redacted, paste-safe cURL command for `req`.
/// Each arg is single-quoted; inner `'` is escaped as `'\''` (POSIX-portable).
/// All header values + URL + body run through `reg.redact()` first.
pub fn build_curl(req: &WireRequest, reg: &SecretRegistry) -> String {
    fn q(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
    let mut out = String::from("curl ");
    if req.method != http::Method::GET {
        out.push_str(&format!("-X {} ", req.method));
    }
    for (k, v) in &req.headers {
        let v = reg.redact(v);
        out.push_str(&format!("-H {} ", q(&format!("{k}: {v}"))));
    }
    if !req.body_bytes.is_empty() {
        let body = std::str::from_utf8(&req.body_bytes)
            .map(|s| reg.redact(s))
            .unwrap_or_else(|_| "<binary>".into());
        out.push_str(&format!("-d {} ", q(&body)));
    }
    out.push_str(&q(&reg.redact(&req.url)));
    out.trim_end().to_string()
}
