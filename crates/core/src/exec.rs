use crate::auth::{AuthCache, AuthError, AuthResolver, AuthSpec};
use crate::catalog::{Body, Request};
use crate::env::ResolveCtx;
use crate::error::CoreError;
use crate::ports::Clock;
use crate::secret::SecretRegistry;
use chrono::{DateTime, Utc};
use http::Method;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WireRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body_bytes: Vec<u8>,
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
    let url_i = crate::env::interpolate(&req.url.0 .0, ctx)?;
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut reg = SecretRegistry::new();
    reg.extend(&url_i.used_secrets);
    for kv in req.headers.iter().filter(|k| k.enabled) {
        let v = crate::env::interpolate(&kv.value, ctx)?;
        reg.extend(&v.used_secrets);
        headers.push((kv.key.clone(), v.value));
    }
    let body_bytes = render_body(&req.body, ctx, &mut reg)?;
    let url = apply_query(&url_i.value, &req.query, ctx, &mut reg)?;
    let mut wire = WireRequest {
        method: req.method.clone(),
        url,
        headers,
        body_bytes,
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
        request_snapshot: redact_wire(&wire, &reg),
        response: resp,
        at: clock.now(),
        secrets: reg,
    })
}

fn render_body(b: &Body, ctx: &ResolveCtx, reg: &mut SecretRegistry) -> Result<Vec<u8>, CoreError> {
    Ok(match b {
        Body::None => Vec::new(),
        Body::Raw { text, .. } | Body::Json(text) => {
            let i = crate::env::interpolate(text, ctx)?;
            reg.extend(&i.used_secrets);
            i.value.into_bytes()
        }
        Body::Form(kvs) => {
            let mut s = String::new();
            for (i, kv) in kvs.iter().filter(|k| k.enabled).enumerate() {
                if i > 0 {
                    s.push('&');
                }
                let v = crate::env::interpolate(&kv.value, ctx)?;
                reg.extend(&v.used_secrets);
                s.push_str(&urlencoding::encode(&kv.key));
                s.push('=');
                s.push_str(&urlencoding::encode(&v.value));
            }
            s.into_bytes()
        }
        Body::Multipart(_) | Body::File(_) => Vec::new(),
        Body::GraphQL { query, variables } => {
            let q = crate::env::interpolate(query, ctx)?;
            reg.extend(&q.used_secrets);
            let vars_value: serde_json::Value = if variables.trim().is_empty() {
                serde_json::Value::Object(Default::default())
            } else {
                let v = crate::env::interpolate(variables, ctx)?;
                reg.extend(&v.used_secrets);
                serde_json::from_str(&v.value).map_err(|e| {
                    CoreError::InvalidTemplate(format!("graphql variables: {e}"))
                })?
            };
            let body = serde_json::json!({ "query": q.value, "variables": vars_value });
            serde_json::to_vec(&body).map_err(|e| CoreError::InvalidTemplate(e.to_string()))?
        }
    })
}

fn apply_query(
    url: &str,
    q: &[crate::primitives::KV],
    ctx: &ResolveCtx,
    reg: &mut SecretRegistry,
) -> Result<String, CoreError> {
    let mut out = url.to_string();
    let mut first = !out.contains('?');
    for kv in q.iter().filter(|k| k.enabled) {
        out.push(if first { '?' } else { '&' });
        first = false;
        let v = crate::env::interpolate(&kv.value, ctx)?;
        reg.extend(&v.used_secrets);
        out.push_str(&urlencoding::encode(&kv.key));
        out.push('=');
        out.push_str(&urlencoding::encode(&v.value));
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
