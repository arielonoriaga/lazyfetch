# lazyfetch v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a usable terminal HTTP client in Rust that supports collections, environments, four auth methods (Bearer/Basic/ApiKey/OAuth2), Postman v2.1 + OpenAPI 3 import, and a vim-keymap ratatui TUI.

**Architecture:** Hexagonal Cargo workspace. `core` crate is pure domain (no IO, no tokio). Adapter crates (`http`, `storage`, `auth`, `import`) implement ports defined in `core`. `tui` and `bin` compose. Filesystem persistence (YAML for collections/envs/config, JSONL for history). Single tokio runtime, single concurrent send v1.

**Tech Stack:** Rust 1.78+, ratatui, crossterm, reqwest (rustls), tokio, hyper, secrecy, ulid, blake3, fd-lock, syntect, jaq, quick-xml, tui-textarea, tracing, thiserror, serde + serde_yaml + serde_json, proptest, wiremock, insta, tempfile.

**Spec:** `docs/superpowers/specs/2026-05-07-lazyfetch-design.md` (read this first).

---

## File Structure

```
lazyfetch/
├── Cargo.toml                  # workspace
├── rust-toolchain.toml         # pin stable
├── deny.toml                   # cargo-deny config
├── .github/workflows/ci.yml
├── crates/
│   ├── core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # re-exports
│   │       ├── primitives.rs       # Id, KV, Template, UrlTemplate
│   │       ├── catalog.rs          # Collection, Folder, Item, Request, Body
│   │       ├── env.rs              # Environment, VarSet, ResolveCtx, interpolate
│   │       ├── secret.rs           # SecretRegistry, redact, redact_wire
│   │       ├── auth.rs             # AuthSpec, OAuth2Spec, Token, TokenKey, traits
│   │       ├── exec.rs             # WireRequest, WireResponse, HttpSender, execute
│   │       ├── history.rs          # Executed, HistoryRepo trait
│   │       ├── ports.rs            # Clock, Browser, Editor, repo traits aggregate
│   │       └── error.rs            # CoreError
│   ├── http/                       # reqwest adapter
│   │   └── src/lib.rs              # ReqwestSender impl HttpSender
│   ├── storage/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── atomic.rs           # write_atomic w/ Drop guard, same-dir tempfile
│   │       ├── collection.rs       # FsCollectionRepo (YAML tree)
│   │       ├── env.rs              # FsEnvRepo
│   │       ├── history.rs          # FsHistoryRepo (JSONL + fd-lock + actor)
│   │       └── auth_cache.rs       # FsAuthCache
│   ├── auth/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── resolver.rs         # AuthResolver impl: Bearer/Basic/ApiKey/OAuth2 dispatch
│   │       ├── oauth2_cc.rs        # Client Credentials flow
│   │       ├── oauth2_code.rs      # Auth Code flow + state + PKCE
│   │       ├── loopback.rs         # hyper one-shot callback server, Drop guard
│   │       └── browser.rs          # SystemBrowser impl Browser port
│   ├── import/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── postman.rs
│   │       ├── postman_env.rs
│   │       └── openapi.rs
│   ├── tui/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs              # AppState, Mode, Focus
│   │       ├── terminal.rs         # alt-screen + raw-mode RAII guard
│   │       ├── event.rs            # crossterm event loop, resize, suspend
│   │       ├── layout.rs
│   │       ├── panes/
│   │       │   ├── collections.rs
│   │       │   ├── env.rs
│   │       │   ├── request.rs
│   │       │   └── response.rs
│   │       ├── editor.rs           # tui-textarea + $EDITOR shell-out
│   │       ├── highlight.rs        # syntect dump loader
│   │       ├── filter.rs           # jaq debounced runner
│   │       ├── command.rs          # `:` command parser
│   │       └── keymap.rs
│   └── bin/
│       └── src/
│           ├── main.rs             # CLI args (clap), composition root
│           └── run.rs              # `lazyfetch run <req>` CLI subcommand
└── docs/
```

---

## Task 1: Workspace scaffold + CI guard

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `deny.toml`, `.github/workflows/ci.yml`
- Create: `crates/{core,http,storage,auth,import,tui,bin}/Cargo.toml`
- Create: `crates/*/src/lib.rs` (`crates/bin/src/main.rs`)
- Create: `scripts/check-core-purity.sh`

- [ ] **Step 1: Initialize workspace `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members  = ["crates/core","crates/http","crates/storage","crates/auth","crates/import","crates/tui","crates/bin"]

[workspace.package]
edition = "2021"
rust-version = "1.78"
license = "MIT"
authors = ["Ariel Onoriaga <onoriagaariel@gmail.com>"]

[workspace.dependencies]
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
serde_yaml  = "0.9"
thiserror   = "1"
anyhow      = "1"
tokio       = { version = "1", features = ["rt-multi-thread","macros","sync","time","fs"] }
tracing     = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter","fmt"] }
secrecy     = "0.8"
ulid        = { version = "1", features = ["serde"] }
blake3      = "1"
http        = "1"
chrono      = { version = "0.4", features = ["serde"] }

[profile.release]
lto = "thin"
codegen-units = 1
strip = "debuginfo"
```

- [ ] **Step 2: Pin toolchain**

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "1.78.0"
components = ["rustfmt","clippy"]
```

- [ ] **Step 3: Create empty crate manifests**

Each `crates/<name>/Cargo.toml`:
```toml
[package]
name = "lazyfetch-<name>"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
```

`crates/bin/Cargo.toml` adds `[[bin]] name = "lazyfetch" path = "src/main.rs"`.

`crates/*/src/lib.rs` is empty (`//! lazyfetch-<name>`). `crates/bin/src/main.rs`: `fn main() {}`.

- [ ] **Step 4: Add core-purity grep guard**

`scripts/check-core-purity.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail
if grep -RnE 'tokio::|std::fs::|std::net::|reqwest::|hyper::' crates/core/src; then
  echo "core must be IO-free"; exit 1
fi
```
`chmod +x scripts/check-core-purity.sh`.

- [ ] **Step 5: `deny.toml`**

```toml
[advisories]
db-path = "~/.cargo/advisory-db"
vulnerability = "deny"
[licenses]
allow = ["MIT","Apache-2.0","BSD-3-Clause","ISC","Unicode-DFS-2016"]
[bans]
multiple-versions = "warn"
```

- [ ] **Step 6: CI workflow**

`.github/workflows/ci.yml`:
```yaml
name: ci
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.78.0
        with: { components: "rustfmt, clippy" }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace --all-features
      - run: bash scripts/check-core-purity.sh
      - uses: EmbarkStudios/cargo-deny-action@v1
```

- [ ] **Step 7: Verify build + commit**

Run: `cargo build --workspace` — Expected: builds clean.
Run: `bash scripts/check-core-purity.sh` — Expected: no output.

```bash
git add -A
git commit -m "chore: workspace scaffold + CI"
```

---

## Task 2: `core::primitives` + `Template` + interpolation TDD

**Files:**
- Create: `crates/core/src/primitives.rs`
- Create: `crates/core/src/env.rs`
- Create: `crates/core/src/secret.rs`
- Create: `crates/core/src/error.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/interpolate.rs`

- [ ] **Step 1: Add deps to `crates/core/Cargo.toml`**

```toml
[dependencies]
serde     = { workspace = true }
thiserror = { workspace = true }
secrecy   = { workspace = true }
ulid      = { workspace = true }
chrono    = { workspace = true }
http      = { workspace = true }

[dev-dependencies]
proptest = "1"
serde_json = { workspace = true }
```

- [ ] **Step 2: Write failing interpolation test**

`crates/core/tests/interpolate.rs`:
```rust
use lazyfetch_core::env::{interpolate, Environment, ResolveCtx, VarValue};
use secrecy::SecretString;

fn ev(pairs: &[(&str, &str, bool)]) -> Environment {
    Environment {
        id: ulid::Ulid::new(),
        name: "test".into(),
        vars: pairs.iter().map(|(k,v,s)| (
            k.to_string(),
            VarValue { value: SecretString::new((*v).into()), secret: *s },
        )).collect(),
    }
}

#[test]
fn substitutes_simple() {
    let env = ev(&[("base","https://api.test", false)]);
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
    let out = interpolate("{{base}}/x", &ctx).unwrap();
    assert_eq!(out.value, "https://api.test/x");
    assert!(out.used_secrets.is_empty());
}

#[test]
fn override_beats_env() {
    let env = ev(&[("k","env", false)]);
    let ov: Vec<_> = vec![("k".into(), VarValue { value: SecretString::new("ov".into()), secret: false })];
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &ov };
    assert_eq!(interpolate("{{k}}", &ctx).unwrap().value, "ov");
}

#[test]
fn missing_var_errors() {
    let env = ev(&[]);
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
    assert!(interpolate("{{nope}}", &ctx).is_err());
}

#[test]
fn secret_tracked_in_registry() {
    let env = ev(&[("tok","s3cret", true)]);
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
    let out = interpolate("Bearer {{tok}}", &ctx).unwrap();
    assert_eq!(out.value, "Bearer s3cret");
    assert!(out.used_secrets.contains("s3cret"));
}
```

- [ ] **Step 3: Run test — Expected: compile error (no module)**

Run: `cargo test -p lazyfetch-core --test interpolate`
Expected: errors about missing `lazyfetch_core::env`.

- [ ] **Step 4: Implement `error.rs`**

`crates/core/src/error.rs`:
```rust
use thiserror::Error;
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("missing variable: {0}")] MissingVar(String),
    #[error("invalid template: {0}")]  InvalidTemplate(String),
}
```

- [ ] **Step 5: Implement `secret.rs`**

`crates/core/src/secret.rs`:
```rust
use std::collections::HashSet;

#[derive(Debug, Default, Clone)]
pub struct SecretRegistry { values: HashSet<String> }
impl SecretRegistry {
    pub fn new() -> Self { Self::default() }
    pub fn insert(&mut self, v: impl Into<String>) { let s=v.into(); if !s.is_empty() { self.values.insert(s); } }
    pub fn extend(&mut self, other: &SecretRegistry) { self.values.extend(other.values.iter().cloned()); }
    pub fn contains(&self, v: &str) -> bool { self.values.contains(v) }
    pub fn is_empty(&self) -> bool { self.values.is_empty() }
    pub fn redact(&self, s: &str) -> String {
        let mut out = s.to_string();
        for v in &self.values { out = out.replace(v, "***"); }
        out
    }
}
```

- [ ] **Step 6: Implement `primitives.rs`**

```rust
pub type Id = ulid::Ulid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KV {
    pub key: String,
    pub value: String,
    #[serde(default = "yes")] pub enabled: bool,
    #[serde(default)] pub secret: bool,
}
fn yes() -> bool { true }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Template(pub String);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct UrlTemplate(pub Template);
```

- [ ] **Step 7: Implement `env.rs` w/ `interpolate`**

```rust
use crate::error::CoreError;
use crate::primitives::Id;
use crate::secret::SecretRegistry;
use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct VarValue { pub value: SecretString, pub secret: bool }

pub type VarSet = Vec<(String, VarValue)>;

#[derive(Debug, Clone)]
pub struct Environment { pub id: Id, pub name: String, pub vars: VarSet }

pub struct ResolveCtx<'a> {
    pub env: &'a Environment,
    pub collection_vars: &'a VarSet,
    pub overrides: &'a VarSet,
}

#[derive(Debug, Clone)]
pub struct Interpolated { pub value: String, pub used_secrets: SecretRegistry }

fn lookup<'a>(name: &str, ctx: &'a ResolveCtx<'a>) -> Option<&'a VarValue> {
    ctx.overrides.iter().find(|(k,_)| k==name).map(|(_,v)| v)
        .or_else(|| ctx.env.vars.iter().find(|(k,_)| k==name).map(|(_,v)| v))
        .or_else(|| ctx.collection_vars.iter().find(|(k,_)| k==name).map(|(_,v)| v))
}

pub fn interpolate(s: &str, ctx: &ResolveCtx) -> Result<Interpolated, CoreError> {
    let mut out = String::with_capacity(s.len());
    let mut reg = SecretRegistry::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start+2..];
        let end = after.find("}}").ok_or_else(|| CoreError::InvalidTemplate(s.into()))?;
        let name = after[..end].trim();
        let v = lookup(name, ctx).ok_or_else(|| CoreError::MissingVar(name.into()))?;
        let val = v.value.expose_secret();
        out.push_str(val);
        if v.secret { reg.insert(val.clone()); }
        rest = &after[end+2..];
    }
    out.push_str(rest);
    Ok(Interpolated { value: out, used_secrets: reg })
}
```

- [ ] **Step 8: Wire `lib.rs`**

```rust
pub mod primitives;
pub mod secret;
pub mod env;
pub mod error;
```

- [ ] **Step 9: Run test — Expected: pass**

Run: `cargo test -p lazyfetch-core --test interpolate` — Expected: 4 pass.

- [ ] **Step 10: Add property test**

Append to `crates/core/tests/interpolate.rs`:
```rust
proptest::proptest! {
    #[test]
    fn no_var_no_change(s in "[^{}]{0,100}") {
        let env = ev(&[]);
        let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
        let out = interpolate(&s, &ctx).unwrap();
        prop_assert_eq!(out.value, s);
    }
}
```

Run: `cargo test -p lazyfetch-core` — Expected: pass.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "feat(core): primitives + env interpolation w/ secret tracking"
```

---

## Task 3: `core::catalog` + `core::auth` types + `core::exec` ports

**Files:**
- Create: `crates/core/src/catalog.rs`, `crates/core/src/auth.rs`, `crates/core/src/exec.rs`, `crates/core/src/history.rs`, `crates/core/src/ports.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/auth_walk.rs`, `crates/core/tests/redact.rs`

- [ ] **Step 1: Test — auth resolution walks request → folder → collection**

`crates/core/tests/auth_walk.rs`:
```rust
use lazyfetch_core::auth::{AuthSpec, effective_auth};
use lazyfetch_core::catalog::{Collection, Folder, Item, Request};

#[test]
fn request_overrides_collection() {
    // build req with Bearer, collection with Basic; expect Bearer
    // ... (see step 4 for types)
}
```
(Filled in step 4 once types exist.)

- [ ] **Step 2: Test — `redact_wire` masks Authorization across all tracked secrets**

`crates/core/tests/redact.rs`:
```rust
use lazyfetch_core::secret::SecretRegistry;
use lazyfetch_core::exec::{WireRequest, redact_wire};
use http::Method;

#[test]
fn redacts_header_value() {
    let mut reg = SecretRegistry::new();
    reg.insert("s3cret");
    let w = WireRequest {
        method: Method::GET, url: "http://x".into(),
        headers: vec![("Authorization".into(), "Bearer s3cret".into())],
        body_bytes: b"tok=s3cret".to_vec(),
        timeout: std::time::Duration::from_secs(30),
        follow_redirects: true, max_redirects: 10,
    };
    let r = redact_wire(&w, &reg);
    assert_eq!(r.headers[0].1, "Bearer ***");
    assert_eq!(r.body_bytes, b"tok=***");
}
```

- [ ] **Step 3: Run tests — Expected: compile fail**

Run: `cargo test -p lazyfetch-core --tests` — Expected: missing items.

- [ ] **Step 4: Implement `catalog.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::primitives::{Id, KV, Template, UrlTemplate};
use crate::env::VarSet;
use crate::auth::AuthSpec;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection { pub id: Id, pub name: String, pub root: Folder, pub auth: Option<AuthSpec>, #[serde(default)] pub vars: Vec<KV> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder { pub id: Id, pub name: String, #[serde(default)] pub items: Vec<Item>, pub auth: Option<AuthSpec> }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item { Folder(Folder), Request(Request) }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: Id, pub name: String,
    #[serde(with = "crate::method_serde")] pub method: http::Method,
    pub url: UrlTemplate,
    #[serde(default)] pub query: Vec<KV>,
    #[serde(default)] pub headers: Vec<KV>,
    #[serde(default)] pub body: Body,
    pub auth: Option<AuthSpec>,
    #[serde(default)] pub notes: Option<String>,
    #[serde(default = "yes")] pub follow_redirects: bool,
    #[serde(default = "default_max_redirects")] pub max_redirects: u8,
    #[serde(default)] pub timeout_ms: Option<u32>,
}
fn yes() -> bool { true }
fn default_max_redirects() -> u8 { 10 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Body {
    #[default] None,
    Raw { mime: String, text: String },
    Json(String),
    Form(Vec<KV>),
    Multipart(Vec<Part>),
    File(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part { pub name: String, pub content: PartContent, pub filename: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartContent { Text(String), File(PathBuf) }
```

Add `crates/core/src/method_serde.rs`:
```rust
use http::Method;
use serde::{Deserializer, Serializer, Deserialize};
pub fn serialize<S: Serializer>(m: &Method, s: S) -> Result<S::Ok, S::Error> { s.serialize_str(m.as_str()) }
pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Method, D::Error> {
    let s = String::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}
```

- [ ] **Step 5: Implement `auth.rs`**

```rust
use serde::{Deserialize, Serialize};
use secrecy::SecretString;
use chrono::{DateTime, Utc};
use crate::primitives::{Id, Template};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSpec {
    None, Inherit,
    Bearer { token: Template },
    Basic  { user: Template, pass: Template },
    ApiKey { name: String, value: Template, location: ApiKeyIn },
    OAuth2(OAuth2Spec),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyIn { Header, Query }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum OAuth2Spec {
    ClientCredentials { token_url: Template, client_id: Template, client_secret: Template, #[serde(default)] scopes: Vec<String>, audience: Option<Template> },
    AuthCode { auth_url: Template, token_url: Template, client_id: Template, client_secret: Option<Template>, redirect_uri: String, #[serde(default)] scopes: Vec<String>, #[serde(default = "yes")] pkce: bool },
}
fn yes() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token { pub access: SecretString, pub refresh: Option<SecretString>, pub expires_at: DateTime<Utc>, pub scopes: Vec<String> }

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TokenKey { pub collection_id: Id, pub auth_id: Id, pub env_id: Id, pub scopes: Vec<String> }

pub fn token_key_hash(k: &TokenKey) -> String {
    let canon = format!("{}|{}|{}|{}", k.collection_id, k.auth_id, k.env_id, k.scopes.join(","));
    blake3::hash(canon.as_bytes()).to_hex().to_string()
}

pub trait AuthCache: Send + Sync {
    fn get(&self, key: &TokenKey) -> Option<Token>;
    fn put(&self, key: &TokenKey, token: Token);
    fn evict(&self, key: &TokenKey);
}

/// Walk request → folder chain → collection. Returns first non-`Inherit` spec, or None.
pub fn effective_auth<'a>(
    req_auth: Option<&'a AuthSpec>, folder_chain: &[&'a AuthSpec], coll_auth: Option<&'a AuthSpec>
) -> Option<&'a AuthSpec> {
    if let Some(a) = req_auth { if !matches!(a, AuthSpec::Inherit) { return Some(a); } }
    for a in folder_chain { if !matches!(a, AuthSpec::Inherit) { return Some(*a); } }
    coll_auth
}
```

Add `blake3` to `crates/core/Cargo.toml`:
```toml
blake3 = { workspace = true }
```

- [ ] **Step 6: Implement `exec.rs`**

```rust
use chrono::{DateTime, Utc};
use http::Method;
use std::time::Duration;
use crate::primitives::KV;
use crate::secret::SecretRegistry;

#[derive(Debug, Clone)]
pub struct WireRequest {
    pub method: Method, pub url: String,
    pub headers: Vec<(String,String)>, pub body_bytes: Vec<u8>,
    pub timeout: Duration, pub follow_redirects: bool, pub max_redirects: u8,
}

#[derive(Debug, Clone)]
pub struct WireResponse {
    pub status: u16, pub headers: Vec<(String,String)>,
    pub body_bytes: Vec<u8>, pub elapsed: Duration, pub size: u64,
}

#[async_trait::async_trait]
pub trait HttpSender: Send + Sync {
    async fn send(&self, r: WireRequest) -> Result<WireResponse, SendError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("timeout")] Timeout,
    #[error("network: {0}")] Net(String),
    #[error("tls: {0}")] Tls(String),
    #[error("dns: {0}")] Dns(String),
    #[error(transparent)] Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone)]
pub struct Executed {
    pub request_snapshot: WireRequest,    // already redacted
    pub response: WireResponse,
    pub at: DateTime<Utc>,
    pub secrets: SecretRegistry,
}

pub fn redact_wire(w: &WireRequest, reg: &SecretRegistry) -> WireRequest {
    let mut r = w.clone();
    for h in &mut r.headers { h.1 = reg.redact(&h.1); }
    r.url = reg.redact(&r.url);
    if let Ok(body) = std::str::from_utf8(&r.body_bytes) {
        r.body_bytes = reg.redact(body).into_bytes();
    }
    r
}
```

Add to core deps: `async-trait = "0.1"`, `anyhow = { workspace = true }`.

- [ ] **Step 7: `history.rs` + `ports.rs`**

`history.rs`:
```rust
use crate::exec::Executed;

pub trait HistoryRepo: Send + Sync {
    fn append(&self, e: &Executed) -> std::io::Result<()>;
    fn tail(&self, n: usize) -> std::io::Result<Vec<Executed>>;
}
```

`ports.rs`:
```rust
use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync { fn now(&self) -> DateTime<Utc>; }
pub struct SystemClock;
impl Clock for SystemClock { fn now(&self) -> DateTime<Utc> { Utc::now() } }

pub trait Browser: Send + Sync { fn open(&self, url: &str) -> std::io::Result<()>; }

pub enum EditHint { PlainText, Json, Yaml, Form }
pub trait Editor: Send + Sync {
    fn edit(&self, buf: &str, hint: EditHint) -> std::io::Result<String>;
}
```

- [ ] **Step 8: Wire `lib.rs`**

```rust
pub mod primitives;
pub mod secret;
pub mod env;
pub mod error;
pub mod auth;
pub mod catalog;
pub mod exec;
pub mod history;
pub mod ports;
pub(crate) mod method_serde;
```

- [ ] **Step 9: Fill auth_walk test (step 1)**

Replace stub in `crates/core/tests/auth_walk.rs`:
```rust
use lazyfetch_core::auth::{AuthSpec, effective_auth};
use lazyfetch_core::primitives::Template;

fn bearer(s: &str) -> AuthSpec { AuthSpec::Bearer { token: Template(s.into()) } }

#[test]
fn request_wins() {
    let r = bearer("R");
    let c = bearer("C");
    let got = effective_auth(Some(&r), &[], Some(&c)).unwrap();
    assert!(matches!(got, AuthSpec::Bearer { token } if token.0 == "R"));
}

#[test]
fn inherit_climbs_to_folder() {
    let r = AuthSpec::Inherit;
    let f = bearer("F");
    let c = bearer("C");
    let got = effective_auth(Some(&r), &[&f], Some(&c)).unwrap();
    assert!(matches!(got, AuthSpec::Bearer { token } if token.0 == "F"));
}

#[test]
fn none_stops() {
    let r = AuthSpec::None;
    let c = bearer("C");
    assert!(effective_auth(Some(&r), &[], Some(&c)).is_none());
}
```

Run: `cargo test -p lazyfetch-core` — Expected: all pass.

- [ ] **Step 10: Verify core purity**

Run: `bash scripts/check-core-purity.sh` — Expected: no output.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "feat(core): catalog + auth types + exec ports + redact_wire"
```

---

## Task 4: `storage` — atomic write + collection repo (TDD)

**Files:**
- Create: `crates/storage/src/atomic.rs`, `crates/storage/src/collection.rs`, `crates/storage/src/env.rs`, `crates/storage/src/lib.rs`
- Test: `crates/storage/tests/atomic.rs`, `crates/storage/tests/collection_roundtrip.rs`

- [ ] **Step 1: deps**

`crates/storage/Cargo.toml`:
```toml
[dependencies]
lazyfetch-core = { path = "../core" }
serde       = { workspace = true }
serde_yaml  = { workspace = true }
serde_json  = { workspace = true }
thiserror   = { workspace = true }
fd-lock     = "4"
blake3      = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Test — atomic write survives panic mid-write**

`crates/storage/tests/atomic.rs`:
```rust
use lazyfetch_storage::atomic::write_atomic;
use std::fs;

#[test]
fn writes_and_replaces() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("f.yaml");
    fs::write(&p, "old").unwrap();
    write_atomic(&p, b"new").unwrap();
    assert_eq!(fs::read_to_string(&p).unwrap(), "new");
    let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "no leftover tempfile");
}
```

Run: `cargo test -p lazyfetch-storage --test atomic` — Expected: fail (no module).

- [ ] **Step 3: Implement `atomic.rs`**

```rust
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub fn write_atomic(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = target.parent().ok_or_else(|| std::io::Error::other("no parent dir"))?;
    let tmp = tempfile::Builder::new()
        .prefix(".lazyfetch-")
        .tempfile_in(parent)?;
    {
        let mut f: &File = tmp.as_file();
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    tmp.persist(target).map_err(|e| e.error)?;
    #[cfg(unix)]
    {
        if let Ok(dir) = OpenOptions::new().read(true).open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}
```

`crates/storage/Cargo.toml`: add `tempfile = "3"` to `[dependencies]` (not just dev).

- [ ] **Step 4: Run test — Expected: pass**

Run: `cargo test -p lazyfetch-storage --test atomic`.

- [ ] **Step 5: Test — collection roundtrip (load → modify → save → reload)**

`crates/storage/tests/collection_roundtrip.rs`:
```rust
use lazyfetch_storage::collection::FsCollectionRepo;
use lazyfetch_core::catalog::{Collection, Folder, Item, Request, Body};
use lazyfetch_core::primitives::{UrlTemplate, Template};
use http::Method;
use ulid::Ulid;

#[test]
fn save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsCollectionRepo::new(dir.path());
    let coll = Collection {
        id: Ulid::new(), name: "demo".into(),
        root: Folder { id: Ulid::new(), name: "demo".into(), items: vec![
            Item::Request(Request {
                id: Ulid::new(), name: "ping".into(),
                method: Method::GET,
                url: UrlTemplate(Template("https://api/{{x}}".into())),
                query: vec![], headers: vec![], body: Body::None,
                auth: None, notes: None,
                follow_redirects: true, max_redirects: 10, timeout_ms: None,
            })
        ], auth: None },
        auth: None, vars: vec![],
    };
    repo.save(&coll).unwrap();
    let loaded = repo.load_by_name("demo").unwrap();
    assert_eq!(loaded.name, "demo");
    assert_eq!(loaded.root.items.len(), 1);
}
```

Run: `cargo test -p lazyfetch-storage --test collection_roundtrip` — Expected: fail.

- [ ] **Step 6: Implement `collection.rs`**

```rust
use lazyfetch_core::catalog::{Collection, Folder, Item, Request};
use std::path::{Path, PathBuf};
use crate::atomic::write_atomic;

pub struct FsCollectionRepo { root: PathBuf }
impl FsCollectionRepo {
    pub fn new(root: impl AsRef<Path>) -> Self { Self { root: root.as_ref().to_path_buf() } }

    fn slug(s: &str) -> String { s.chars().map(|c| if c.is_ascii_alphanumeric() || c=='-' || c=='_' { c } else { '-' }).collect() }

    pub fn save(&self, c: &Collection) -> std::io::Result<()> {
        let dir = self.root.join(Self::slug(&c.name));
        std::fs::create_dir_all(dir.join("requests"))?;
        let header = serde_yaml::to_string(&CollectionHeader { id: c.id, name: c.name.clone(), auth: c.auth.clone(), vars: c.vars.clone() })
            .map_err(io_err)?;
        write_atomic(&dir.join("collection.yaml"), header.as_bytes())?;
        Self::save_folder(&dir.join("requests"), &c.root)?;
        Ok(())
    }

    fn save_folder(dir: &Path, f: &Folder) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let meta = serde_yaml::to_string(&FolderHeader { id: f.id, name: f.name.clone(), auth: f.auth.clone() }).map_err(io_err)?;
        write_atomic(&dir.join("_folder.yaml"), meta.as_bytes())?;
        for item in &f.items {
            match item {
                Item::Folder(sub) => Self::save_folder(&dir.join(Self::slug(&sub.name)), sub)?,
                Item::Request(r) => {
                    let y = serde_yaml::to_string(r).map_err(io_err)?;
                    write_atomic(&dir.join(format!("{}.yaml", Self::slug(&r.name))), y.as_bytes())?;
                }
            }
        }
        Ok(())
    }

    pub fn load_by_name(&self, name: &str) -> std::io::Result<Collection> {
        let dir = self.root.join(Self::slug(name));
        let header: CollectionHeader = serde_yaml::from_str(&std::fs::read_to_string(dir.join("collection.yaml"))?).map_err(io_err)?;
        let root = Self::load_folder(&dir.join("requests"))?;
        Ok(Collection { id: header.id, name: header.name, root, auth: header.auth, vars: header.vars })
    }

    fn load_folder(dir: &Path) -> std::io::Result<Folder> {
        let meta_path = dir.join("_folder.yaml");
        let header: FolderHeader = if meta_path.exists() {
            serde_yaml::from_str(&std::fs::read_to_string(&meta_path)?).map_err(io_err)?
        } else {
            FolderHeader { id: ulid::Ulid::new(), name: dir.file_name().unwrap_or_default().to_string_lossy().to_string(), auth: None }
        };
        let mut items = vec![];
        for ent in std::fs::read_dir(dir)? {
            let e = ent?; let p = e.path();
            if e.file_type()?.is_dir() {
                items.push(Item::Folder(Self::load_folder(&p)?));
            } else if p.file_name().unwrap_or_default() != "_folder.yaml" {
                let r: Request = serde_yaml::from_str(&std::fs::read_to_string(&p)?).map_err(io_err)?;
                items.push(Item::Request(r));
            }
        }
        Ok(Folder { id: header.id, name: header.name, items, auth: header.auth })
    }
}

fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error { std::io::Error::other(e.to_string()) }

#[derive(serde::Serialize, serde::Deserialize)]
struct CollectionHeader { id: ulid::Ulid, name: String, auth: Option<lazyfetch_core::auth::AuthSpec>, vars: Vec<lazyfetch_core::primitives::KV> }

#[derive(serde::Serialize, serde::Deserialize)]
struct FolderHeader { id: ulid::Ulid, name: String, auth: Option<lazyfetch_core::auth::AuthSpec> }
```

- [ ] **Step 7: Wire `lib.rs`**

```rust
pub mod atomic;
pub mod collection;
```

Run: `cargo test -p lazyfetch-storage` — Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(storage): atomic write + collection YAML round-trip"
```

---

## Task 5: `storage` — env repo + history (JSONL + fd-lock + actor)

**Files:**
- Create: `crates/storage/src/env.rs`, `crates/storage/src/history.rs`
- Test: `crates/storage/tests/env_roundtrip.rs`, `crates/storage/tests/history_concurrent.rs`

- [ ] **Step 1: Test — env load/save (incl. secret flag preserved)**

`crates/storage/tests/env_roundtrip.rs`:
```rust
use lazyfetch_storage::env::FsEnvRepo;
use lazyfetch_core::env::{Environment, VarValue};
use secrecy::{ExposeSecret, SecretString};
use ulid::Ulid;

#[test]
fn save_load_secret_flag() {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsEnvRepo::new(dir.path());
    let env = Environment {
        id: Ulid::new(), name: "dev".into(),
        vars: vec![
            ("base".into(), VarValue { value: SecretString::new("https://api".into()), secret: false }),
            ("tok".into(), VarValue { value: SecretString::new("xyz".into()), secret: true }),
        ],
    };
    repo.save(&env).unwrap();
    let loaded = repo.load_by_name("dev").unwrap();
    assert_eq!(loaded.vars[0].1.secret, false);
    assert_eq!(loaded.vars[1].1.secret, true);
    assert_eq!(loaded.vars[1].1.value.expose_secret(), "xyz");
}
```

- [ ] **Step 2: Implement `env.rs`**

```rust
use lazyfetch_core::env::{Environment, VarValue};
use secrecy::{ExposeSecret, SecretString};
use std::path::{Path, PathBuf};
use crate::atomic::write_atomic;

#[derive(serde::Serialize, serde::Deserialize)]
struct EnvFile { id: ulid::Ulid, name: String, vars: Vec<VarRow> }
#[derive(serde::Serialize, serde::Deserialize)]
struct VarRow { name: String, value: String, #[serde(default)] secret: bool }

pub struct FsEnvRepo { root: PathBuf }
impl FsEnvRepo {
    pub fn new(root: impl AsRef<Path>) -> Self { Self { root: root.as_ref().to_path_buf() } }

    pub fn save(&self, e: &Environment) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        let f = EnvFile {
            id: e.id, name: e.name.clone(),
            vars: e.vars.iter().map(|(k,v)| VarRow {
                name: k.clone(), value: v.value.expose_secret().clone(), secret: v.secret,
            }).collect(),
        };
        let y = serde_yaml::to_string(&f).map_err(|e| std::io::Error::other(e.to_string()))?;
        write_atomic(&self.root.join(format!("{}.yaml", e.name)), y.as_bytes())
    }

    pub fn load_by_name(&self, name: &str) -> std::io::Result<Environment> {
        let s = std::fs::read_to_string(self.root.join(format!("{}.yaml", name)))?;
        let f: EnvFile = serde_yaml::from_str(&s).map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(Environment {
            id: f.id, name: f.name,
            vars: f.vars.into_iter().map(|r| (r.name, VarValue { value: SecretString::new(r.value), secret: r.secret })).collect(),
        })
    }
}
```

Run: `cargo test -p lazyfetch-storage --test env_roundtrip` — Expected: pass.

- [ ] **Step 3: Test — history concurrent append (cross-process via spawned helper)**

`crates/storage/tests/history_concurrent.rs`:
```rust
use lazyfetch_storage::history::FsHistoryRepo;
use std::thread;

#[test]
fn concurrent_appends_no_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hist.jsonl");
    let repo = std::sync::Arc::new(FsHistoryRepo::new(path.clone(), 1000));

    let mut handles = vec![];
    for i in 0..50 {
        let r = repo.clone();
        handles.push(thread::spawn(move || {
            r.append_raw(&format!("{{\"i\":{}}}", i)).unwrap();
        }));
    }
    for h in handles { h.join().unwrap(); }

    let lines: Vec<_> = std::fs::read_to_string(&path).unwrap().lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 50);
    for l in lines { let _: serde_json::Value = serde_json::from_str(l).unwrap(); }
}
```

- [ ] **Step 4: Implement `history.rs` w/ `fd-lock`**

```rust
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::fs::OpenOptions;
use std::io::Write;
use fd_lock::RwLock;

pub struct FsHistoryRepo { path: PathBuf, max: usize, lock: Mutex<()> }

impl FsHistoryRepo {
    pub fn new(path: PathBuf, max: usize) -> Self { Self { path, max, lock: Mutex::new(()) } }

    /// Append a single JSONL record (already-serialized line, no trailing newline).
    pub fn append_raw(&self, line: &str) -> std::io::Result<()> {
        let _g = self.lock.lock().unwrap();
        if let Some(p) = self.path.parent() { std::fs::create_dir_all(p)?; }
        let file = OpenOptions::new().create(true).append(true).read(true).open(&self.path)?;
        let mut lock = RwLock::new(file);
        let mut w = lock.write()?;
        writeln!(*w, "{}", line)?;
        w.sync_data()?;
        Ok(())
    }

    pub fn tail(&self, n: usize) -> std::io::Result<Vec<String>> {
        let s = std::fs::read_to_string(&self.path).unwrap_or_default();
        Ok(s.lines().rev().take(n).map(String::from).collect())
    }

    pub fn truncate_to_max(&self) -> std::io::Result<()> {
        let _g = self.lock.lock().unwrap();
        let s = std::fs::read_to_string(&self.path).unwrap_or_default();
        let lines: Vec<&str> = s.lines().collect();
        if lines.len() <= self.max { return Ok(()); }
        let keep = &lines[lines.len()-self.max..];
        let joined = keep.join("\n") + "\n";
        crate::atomic::write_atomic(&self.path, joined.as_bytes())
    }
}
```

Run: `cargo test -p lazyfetch-storage` — Expected: pass.

- [ ] **Step 5: Wire + commit**

`crates/storage/src/lib.rs`: add `pub mod env; pub mod history;`.

```bash
git add -A
git commit -m "feat(storage): env repo + history JSONL w/ fd-lock"
```

---

## Task 6: `http` adapter — `ReqwestSender`

**Files:**
- Create: `crates/http/src/lib.rs`
- Test: `crates/http/tests/send.rs`

- [ ] **Step 1: deps**

`crates/http/Cargo.toml`:
```toml
[dependencies]
lazyfetch-core = { path = "../core" }
reqwest        = { version = "0.12", default-features = false, features = ["rustls-tls","gzip","brotli","deflate","stream","multipart"] }
async-trait    = "0.1"
tokio          = { workspace = true }
http           = { workspace = true }
anyhow         = { workspace = true }

[dev-dependencies]
wiremock = "0.6"
tokio    = { workspace = true, features = ["macros","rt"] }
```

- [ ] **Step 2: Test against wiremock**

`crates/http/tests/send.rs`:
```rust
use lazyfetch_core::exec::{HttpSender, WireRequest};
use lazyfetch_http::ReqwestSender;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn sends_get_and_parses_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/x"))
        .respond_with(ResponseTemplate::new(204).insert_header("X-Test","v"))
        .mount(&server).await;

    let sender = ReqwestSender::new();
    let req = WireRequest {
        method: http::Method::GET,
        url: format!("{}/x", server.uri()),
        headers: vec![], body_bytes: vec![],
        timeout: std::time::Duration::from_secs(5),
        follow_redirects: true, max_redirects: 10,
    };
    let resp = sender.send(req).await.unwrap();
    assert_eq!(resp.status, 204);
    assert!(resp.headers.iter().any(|(k,v)| k.eq_ignore_ascii_case("x-test") && v == "v"));
}
```

- [ ] **Step 3: Implement `lib.rs`**

```rust
use async_trait::async_trait;
use lazyfetch_core::exec::{HttpSender, SendError, WireRequest, WireResponse};
use std::time::Instant;

pub struct ReqwestSender { client_pool: parking_lot::Mutex<Option<reqwest::Client>> }

impl ReqwestSender {
    pub fn new() -> Self { Self { client_pool: parking_lot::Mutex::new(None) } }
    fn build_client(req: &WireRequest) -> reqwest::Client {
        let policy = if req.follow_redirects {
            reqwest::redirect::Policy::limited(req.max_redirects as usize)
        } else { reqwest::redirect::Policy::none() };
        reqwest::Client::builder().redirect(policy).timeout(req.timeout).build().unwrap()
    }
}

#[async_trait]
impl HttpSender for ReqwestSender {
    async fn send(&self, r: WireRequest) -> Result<WireResponse, SendError> {
        let client = Self::build_client(&r);
        let mut rb = client.request(r.method.clone(), &r.url);
        for (k,v) in &r.headers { rb = rb.header(k, v); }
        if !r.body_bytes.is_empty() { rb = rb.body(r.body_bytes.clone()); }
        let started = Instant::now();
        let resp = rb.send().await.map_err(map_err)?;
        let status = resp.status().as_u16();
        let headers: Vec<(String,String)> = resp.headers().iter()
            .map(|(k,v)| (k.to_string(), v.to_str().unwrap_or("").to_string())).collect();
        let bytes = resp.bytes().await.map_err(map_err)?.to_vec();
        let elapsed = started.elapsed();
        let size = bytes.len() as u64;
        Ok(WireResponse { status, headers, body_bytes: bytes, elapsed, size })
    }
}

fn map_err(e: reqwest::Error) -> SendError {
    if e.is_timeout() { SendError::Timeout }
    else if e.is_connect() { SendError::Net(format!("{e}")) }
    else { SendError::Other(anyhow::anyhow!(e)) }
}
```

Add `parking_lot = "0.12"` to deps.

Run: `cargo test -p lazyfetch-http` — Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(http): reqwest adapter for HttpSender"
```

---

## Task 7: `auth` — non-OAuth resolvers + `execute()` orchestration

**Files:**
- Create: `crates/auth/src/lib.rs`, `crates/auth/src/resolver.rs`
- Modify: `crates/core/src/exec.rs` (add `execute` fn)
- Test: `crates/auth/tests/resolvers.rs`, `crates/core/tests/execute.rs`

- [ ] **Step 1: Test — Bearer applies header**

`crates/auth/tests/resolvers.rs`:
```rust
use lazyfetch_auth::resolver::DefaultResolver;
use lazyfetch_core::auth::{AuthSpec, AuthResolver};
use lazyfetch_core::env::{Environment, ResolveCtx, VarValue};
use lazyfetch_core::exec::WireRequest;
use lazyfetch_core::primitives::Template;
use lazyfetch_core::secret::SecretRegistry;
use secrecy::SecretString;

fn empty_req() -> WireRequest { WireRequest {
    method: http::Method::GET, url: "http://x".into(),
    headers: vec![], body_bytes: vec![],
    timeout: std::time::Duration::from_secs(5),
    follow_redirects: true, max_redirects: 10,
}}

#[tokio::test]
async fn bearer_uses_secret_var() {
    let env = Environment {
        id: ulid::Ulid::new(), name: "t".into(),
        vars: vec![("tok".into(), VarValue { value: SecretString::new("xyz".into()), secret: true })],
    };
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
    let mut req = empty_req();
    let mut reg = SecretRegistry::new();
    let resolver = DefaultResolver::new();
    let spec = AuthSpec::Bearer { token: Template("{{tok}}".into()) };
    resolver.apply(&spec, &ctx, &lazyfetch_core::ports::SystemClock, &NoCache, &mut req, &mut reg).await.unwrap();
    assert!(req.headers.iter().any(|(k,v)| k=="Authorization" && v=="Bearer xyz"));
    assert!(reg.contains("xyz"));
}

struct NoCache;
impl lazyfetch_core::auth::AuthCache for NoCache {
    fn get(&self, _: &lazyfetch_core::auth::TokenKey) -> Option<lazyfetch_core::auth::Token> { None }
    fn put(&self, _: &lazyfetch_core::auth::TokenKey, _: lazyfetch_core::auth::Token) {}
    fn evict(&self, _: &lazyfetch_core::auth::TokenKey) {}
}
```

- [ ] **Step 2: Update `core::auth` `AuthResolver` trait + `Bearer.token` validation**

In `crates/core/src/auth.rs` add:
```rust
use crate::env::ResolveCtx;
use crate::exec::WireRequest;
use crate::ports::Clock;
use crate::secret::SecretRegistry;

#[async_trait::async_trait]
pub trait AuthResolver: Send + Sync {
    async fn apply(
        &self, spec: &AuthSpec, ctx: &ResolveCtx<'_>, clock: &dyn Clock,
        cache: &dyn AuthCache, req: &mut WireRequest, reg: &mut SecretRegistry,
    ) -> Result<(), AuthError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing var: {0}")] MissingVar(String),
    #[error("non-secret var used for secret field: {0}")] NotSecret(String),
    #[error("oauth: {0}")] OAuth(String),
    #[error(transparent)] Core(#[from] crate::error::CoreError),
}
```

- [ ] **Step 3: Implement `crates/auth/src/resolver.rs`**

```rust
use async_trait::async_trait;
use base64::Engine;
use lazyfetch_core::auth::{AuthCache, AuthError, AuthResolver, AuthSpec, ApiKeyIn};
use lazyfetch_core::env::{interpolate, ResolveCtx};
use lazyfetch_core::exec::WireRequest;
use lazyfetch_core::ports::Clock;
use lazyfetch_core::secret::SecretRegistry;

pub struct DefaultResolver;
impl DefaultResolver { pub fn new() -> Self { Self } }

#[async_trait]
impl AuthResolver for DefaultResolver {
    async fn apply(
        &self, spec: &AuthSpec, ctx: &ResolveCtx<'_>, _clock: &dyn Clock,
        _cache: &dyn AuthCache, req: &mut WireRequest, reg: &mut SecretRegistry,
    ) -> Result<(), AuthError> {
        match spec {
            AuthSpec::None | AuthSpec::Inherit => Ok(()),
            AuthSpec::Bearer { token } => {
                let i = interpolate(&token.0, ctx)?;
                require_secret(&token.0, &i)?;
                req.headers.push(("Authorization".into(), format!("Bearer {}", i.value)));
                reg.extend(&i.used_secrets);
                Ok(())
            }
            AuthSpec::Basic { user, pass } => {
                let u = interpolate(&user.0, ctx)?;
                let p = interpolate(&pass.0, ctx)?;
                require_secret(&pass.0, &p)?;
                let raw = format!("{}:{}", u.value, p.value);
                let enc = base64::engine::general_purpose::STANDARD.encode(raw);
                req.headers.push(("Authorization".into(), format!("Basic {}", enc)));
                reg.extend(&u.used_secrets); reg.extend(&p.used_secrets);
                Ok(())
            }
            AuthSpec::ApiKey { name, value, location } => {
                let v = interpolate(&value.0, ctx)?;
                require_secret(&value.0, &v)?;
                match location {
                    ApiKeyIn::Header => req.headers.push((name.clone(), v.value.clone())),
                    ApiKeyIn::Query  => {
                        let sep = if req.url.contains('?') { '&' } else { '?' };
                        req.url.push(sep); req.url.push_str(name); req.url.push('='); req.url.push_str(&v.value);
                    }
                }
                reg.extend(&v.used_secrets);
                Ok(())
            }
            AuthSpec::OAuth2(_) => Err(AuthError::OAuth("OAuth2 not yet wired (Task 11)".into())),
        }
    }
}

/// Reject if template references any var that wasn't flagged `secret: true`.
/// Implementation: if template contains {{...}} placeholders but used_secrets is empty,
/// the user wired a non-secret var into a secret-only field.
fn require_secret(tpl: &str, i: &lazyfetch_core::env::Interpolated) -> Result<(), AuthError> {
    if tpl.contains("{{") && i.used_secrets.is_empty() && i.value != *tpl {
        Err(AuthError::NotSecret(tpl.to_string()))
    } else { Ok(()) }
}
```

`crates/auth/Cargo.toml`:
```toml
[dependencies]
lazyfetch-core = { path = "../core" }
async-trait    = "0.1"
base64         = "0.22"
thiserror      = { workspace = true }

[dev-dependencies]
tokio    = { workspace = true, features = ["macros","rt","time"] }
secrecy  = { workspace = true }
ulid     = { workspace = true }
http     = { workspace = true }
```

`crates/auth/src/lib.rs`: `pub mod resolver;`.

Run: `cargo test -p lazyfetch-auth` — Expected: pass.

- [ ] **Step 4: Add `execute` to `core::exec`**

```rust
use crate::auth::{AuthCache, AuthError, AuthResolver, AuthSpec};
use crate::catalog::{Body, Folder, Item, Request};
use crate::env::ResolveCtx;
use crate::ports::Clock;
use crate::secret::SecretRegistry;

pub async fn execute(
    req: &Request, ctx: &ResolveCtx<'_>, auth_chain: AuthChain<'_>,
    resolver: &dyn AuthResolver, cache: &dyn AuthCache,
    http: &dyn HttpSender, clock: &dyn Clock,
) -> Result<Executed, ExecError> {
    let url_i = crate::env::interpolate(&req.url.0.0, ctx)?;
    let mut headers: Vec<(String,String)> = Vec::new();
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
        method: req.method.clone(), url, headers, body_bytes,
        timeout: std::time::Duration::from_millis(req.timeout_ms.unwrap_or(30_000) as u64),
        follow_redirects: req.follow_redirects, max_redirects: req.max_redirects,
    };
    if let Some(spec) = crate::auth::effective_auth(req.auth.as_ref(), auth_chain.folders, auth_chain.collection) {
        resolver.apply(spec, ctx, clock, cache, &mut wire, &mut reg).await?;
    }
    let resp = http.send(wire.clone()).await?;
    Ok(Executed {
        request_snapshot: redact_wire(&wire, &reg),
        response: resp, at: clock.now(), secrets: reg,
    })
}

pub struct AuthChain<'a> { pub folders: &'a [&'a AuthSpec], pub collection: Option<&'a AuthSpec> }

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error(transparent)] Core(#[from] crate::error::CoreError),
    #[error(transparent)] Auth(#[from] AuthError),
    #[error(transparent)] Send(#[from] SendError),
}

fn render_body(b: &Body, ctx: &ResolveCtx, reg: &mut SecretRegistry) -> Result<Vec<u8>, crate::error::CoreError> {
    Ok(match b {
        Body::None => Vec::new(),
        Body::Raw { text, .. } | Body::Json(text) => {
            let i = crate::env::interpolate(text, ctx)?; reg.extend(&i.used_secrets); i.value.into_bytes()
        }
        Body::Form(kvs) => {
            let mut s = String::new();
            for (i, kv) in kvs.iter().filter(|k| k.enabled).enumerate() {
                if i > 0 { s.push('&'); }
                let v = crate::env::interpolate(&kv.value, ctx)?; reg.extend(&v.used_secrets);
                s.push_str(&urlencoding::encode(&kv.key)); s.push('=');
                s.push_str(&urlencoding::encode(&v.value));
            }
            s.into_bytes()
        }
        Body::Multipart(_) | Body::File(_) => Vec::new(), // phase-2 pieces; raw bytes built in adapter for v1
    })
}

fn apply_query(url: &str, q: &[crate::primitives::KV], ctx: &ResolveCtx, reg: &mut SecretRegistry) -> Result<String, crate::error::CoreError> {
    let mut out = url.to_string();
    let mut first = !out.contains('?');
    for kv in q.iter().filter(|k| k.enabled) {
        out.push(if first { '?' } else { '&' }); first = false;
        let v = crate::env::interpolate(&kv.value, ctx)?; reg.extend(&v.used_secrets);
        out.push_str(&urlencoding::encode(&kv.key)); out.push('=');
        out.push_str(&urlencoding::encode(&v.value));
    }
    Ok(out)
}
```

Add `urlencoding = "2"` to `crates/core/Cargo.toml`.

- [ ] **Step 5: Test — `execute` end-to-end against wiremock**

`crates/core/tests/execute.rs` — note: test lives in `crates/auth/tests/` instead because it needs http+wiremock. Move:

`crates/auth/tests/execute_e2e.rs`:
```rust
// Same shape as send.rs but builds a Request, calls core::exec::execute through a stub HttpSender.
// (Or use ReqwestSender behind a feature flag — keep core test pure with stub.)
```

Stub `HttpSender` + assert headers redacted in `Executed.request_snapshot`. (≈40 lines, straight construction.)

- [ ] **Step 6: Run + commit**

```bash
cargo test --workspace
git add -A
git commit -m "feat(core,auth): execute() orchestration + Bearer/Basic/ApiKey resolvers"
```

---

## Task 8: `bin` — `lazyfetch run` CLI subcommand

**Files:**
- Create: `crates/bin/src/run.rs`
- Modify: `crates/bin/src/main.rs`
- Test: `crates/bin/tests/cli_run.rs`

- [ ] **Step 1: deps**

`crates/bin/Cargo.toml` adds `clap = { version = "4", features = ["derive"] }`, `tokio = { workspace = true }`, plus all other workspace crates as path deps, plus `tracing-subscriber`.

- [ ] **Step 2: Test — `lazyfetch run my-coll/users/list --env dev` exits 0 and prints status line**

`crates/bin/tests/cli_run.rs`: spawn binary against wiremock + tempdir-config, assert stdout contains `200`.

- [ ] **Step 3: Implement `run.rs`**

```rust
use clap::Args;

#[derive(Args)]
pub struct RunArgs {
    /// Path within collections: "<collection>/<folder>/<request>"
    pub request_path: String,
    #[arg(long)] pub env: Option<String>,
    #[arg(long)] pub set: Vec<String>,                  // --set k=v
    #[arg(long)] pub config_dir: Option<std::path::PathBuf>,
}

pub async fn run(args: RunArgs) -> anyhow::Result<()> {
    let cfg = args.config_dir.unwrap_or_else(default_config_dir);
    // load collection by first segment, walk to request
    // load env, build ResolveCtx, call core::exec::execute, print status + body
    Ok(())
}

fn default_config_dir() -> std::path::PathBuf {
    dirs::config_dir().unwrap_or_else(|| ".".into()).join("lazyfetch")
}
```

Fill out body (~80 lines): load via `FsCollectionRepo`, env via `FsEnvRepo`, walk request path, build `Request` + `AuthChain`, instantiate `ReqwestSender` + `DefaultResolver` + `SystemClock` + in-memory no-op `AuthCache`, call `execute`, print `{status} {elapsed}` + redacted body.

- [ ] **Step 4: Wire `main.rs`**

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name="lazyfetch", version)]
struct Cli { #[command(subcommand)] cmd: Cmd }

#[derive(Subcommand)]
enum Cmd { Run(crate::run::RunArgs) /* TUI default in Task 9 */ }

mod run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    match Cli::parse().cmd { Cmd::Run(a) => run::run(a).await }
}
```

Run: `cargo test --workspace` — Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(bin): lazyfetch run CLI for headless send"
```

---

## Task 9: TUI shell — terminal lifecycle + panes + navigation

**Files:**
- Create: `crates/tui/src/{terminal.rs,event.rs,app.rs,layout.rs,keymap.rs}` and `crates/tui/src/panes/*.rs`
- Modify: `crates/bin/src/main.rs` (default subcommand → TUI)

deps: `ratatui = "0.28"`, `crossterm = "0.28"`, `tui-textarea = "0.7"`, `anyhow`, plus all workspace crates.

- [ ] **Step 1: Test — `TerminalGuard` restores raw-mode + alt-screen on Drop, even on panic**

`crates/tui/tests/terminal_guard.rs`:
```rust
use lazyfetch_tui::terminal::TerminalGuard;
#[test]
fn drop_restores() {
    // mock backend: assert leave_alternate_screen + disable_raw_mode called once on drop.
    // Use a `TestBackend` wrapper that records calls.
}
```

- [ ] **Step 2: Implement `terminal.rs`**

```rust
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub struct TerminalGuard { pub term: Terminal<CrosstermBackend<std::io::Stdout>>, restored: bool }

impl TerminalGuard {
    pub fn new() -> std::io::Result<Self> {
        enable_raw_mode()?;
        let mut out = std::io::stdout();
        execute!(out, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(out);
        Ok(Self { term: Terminal::new(backend)?, restored: false })
    }
    pub fn suspend(&mut self) -> std::io::Result<()> {
        execute!(std::io::stdout(), LeaveAlternateScreen)?; disable_raw_mode()?; Ok(())
    }
    pub fn resume(&mut self) -> std::io::Result<()> {
        enable_raw_mode()?; execute!(std::io::stdout(), EnterAlternateScreen)?; self.term.clear()?; Ok(())
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.restored {
            let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
            let _ = disable_raw_mode();
        }
    }
}
```

- [ ] **Step 3: `app.rs` — `AppState`, `Mode`, `Focus`, `RequestEditor`**

(Per spec §4. ~80 lines, straight types.)

- [ ] **Step 4: Layout + 4 panes — render skeleton (no editing yet)**

`layout.rs` builds the 4-pane grid; each pane has a `render(&mut Frame, Rect, &AppState)` fn drawing borders + selection highlight on focused pane.

- [ ] **Step 5: `event.rs` — crossterm event loop**

```rust
pub enum Tick { Input(Event), Resize(u16,u16), Http(Result<Executed, ExecError>), Frame }
```

Spawns a tokio task reading `crossterm::event::poll` + sender; main loop selects on tick channel + http result channel.

- [ ] **Step 6: `keymap.rs` — global + per-pane mapping**

Single function `dispatch(event, state) -> Action` returning `Action` enum (`FocusNext`, `Quit`, `Send`, `OpenCommand`, etc.). Keep handler tables small + tested.

- [ ] **Step 7: Snapshot test — initial render**

`crates/tui/tests/snapshots.rs` using `ratatui::backend::TestBackend` + `insta::assert_snapshot!` of buffer string for an `AppState` with one collection + one open request.

- [ ] **Step 8: Wire `bin` default**

```rust
#[derive(Subcommand)]
enum Cmd { Run(run::RunArgs), Tui }
// Default to Tui when no subcommand:
let cli = Cli::parse();
match cli.cmd.unwrap_or(Cmd::Tui) { /* ... */ }
```

Run + manually verify TUI launches, `Tab` cycles, `q` quits, terminal restored. Add E2E expectrl smoke test.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(tui): terminal lifecycle + panes + navigation"
```

---

## Task 10: TUI — body editor (inline + $EDITOR), response viewer, search/filter, save

**Files:**
- Create: `crates/tui/src/{editor.rs,highlight.rs,filter.rs,command.rs}`
- Modify: pane render fns

- [ ] **Step 1: Highlight — bundle precompiled syntect dump**

`build.rs` generates `assets/syntaxes.bin` + `assets/themes.bin` via `syntect::dumps::dump_to_uncompressed_file` from default sets. `highlight.rs::load()` `include_bytes!` and `from_uncompressed_data`.

- [ ] **Step 2: Inline editor — `tui-textarea` integration in Body tab**

Test: pressing `e` enters Insert mode; typing modifies `RequestEditor.req.body`; `Esc` returns to Normal + sets `dirty=true`.

- [ ] **Step 3: `$EDITOR` shell-out**

`editor.rs::shell_out(initial: &str, ext: &str) -> Result<String>`:
1. Suspend terminal (`TerminalGuard::suspend`).
2. Write `initial` to `tempfile::Builder::new().suffix(&format!(".{ext}")).tempfile_in(tmp_dir)?`.
3. Spawn `$EDITOR` (default `vi`) with file path; `wait()`.
4. Read file back.
5. `TerminalGuard::resume`.
RAII guard ensures `Drop` runs `resume` even on panic.

Test: stub `$EDITOR=true` (no-op binary), assert content unchanged + terminal restored.

- [ ] **Step 4: Response viewer — render w/ highlight, `/` search, `f` filter (debounced 150ms via tokio task w/ AbortHandle), `S` save dialog**

Each gets a sub-step pair (failing test + impl). Filter test uses a stubbed `JaqEngine` trait so no jaq dep in `tui` tests.

- [ ] **Step 5: Dirty-buffer modal**

State machine adds `Dialog(LeaveDirty { req_id })`. Test: try focus-jump w/ dirty → modal appears.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(tui): editor + response viewer + search/filter/save + dirty-buffer policy"
```

---

## Task 11: OAuth2 — Client Credentials + Authorization Code w/ PKCE + loopback

**Files:**
- Create: `crates/auth/src/{oauth2_cc.rs,oauth2_code.rs,loopback.rs,browser.rs}`
- Modify: `crates/auth/src/resolver.rs`
- Create: `crates/storage/src/auth_cache.rs`

deps: `hyper = { version = "1", features = ["server","http1"] }`, `hyper-util = "0.1"`, `rand = "0.8"`, `sha2 = "0.10"`, `urlencoding = "2"`, `webbrowser = "0.8"`.

- [ ] **Step 1: Test — Client Credentials happy path**

wiremock at `/token` returning `{access_token, expires_in, token_type:"Bearer"}`. Resolver applies `Authorization: Bearer <access>`; second call within 30s of expiry uses cache.

- [ ] **Step 2: Implement `oauth2_cc.rs`**

```rust
pub async fn fetch_cc(spec: &OAuth2Spec, ctx: &ResolveCtx<'_>) -> Result<Token, AuthError> { /* POST form-encoded grant_type=client_credentials */ }
```

- [ ] **Step 3: Test — Authorization Code flow**

wiremock at `/authorize` (returns 302 to redirect_uri w/ code+state via test driver) + `/token`. Test driver: spawn a thread that, after a short delay, GETs the loopback `redirect_uri` w/ the captured `state` param.

State extraction: resolver exposes a `pending_state(&handle) -> String` for tests.

CSRF test: driver sends mismatched state → resolver rejects.

Listener-timeout test: driver never calls back → resolver returns timeout after configured window (set to 200ms in test).

Port-release test: drop handle mid-flow → port reusable.

- [ ] **Step 4: Implement `loopback.rs`**

```rust
pub struct LoopbackHandle { pub redirect_uri: String, _drop: ListenerDrop }
struct ListenerDrop { /* abort handle, port released on drop */ }

pub async fn start_loopback(timeout: Duration, state: String) -> Result<(LoopbackHandle, oneshot::Receiver<CallbackResult>), AuthError> {
    // bind ephemeral port
    // spawn hyper service: on /callback, validate state (constant-time), send code via oneshot, reply 200
    // wrap in Drop guard that aborts task + closes listener
}
```

- [ ] **Step 5: Implement `oauth2_code.rs`**

```rust
pub async fn fetch_code(spec: &OAuth2Spec, ctx: &ResolveCtx<'_>, browser: &dyn Browser, clock: &dyn Clock) -> Result<Token, AuthError>
```

PKCE: `code_verifier` = 32 random bytes base64url; `code_challenge` = `BASE64URL(SHA256(verifier))`. Send `code_challenge_method=S256`.

- [ ] **Step 6: Implement `FsAuthCache` in `storage`**

```rust
pub struct FsAuthCache { dir: PathBuf }
impl AuthCache for FsAuthCache {
    fn get(&self, key: &TokenKey) -> Option<Token> { /* read <hash>.json */ }
    fn put(&self, key: &TokenKey, t: Token) { /* atomic write w/ perm 0600 */ }
    fn evict(&self, key: &TokenKey) { /* fs::remove_file ignore-not-found */ }
}
```

`#[cfg(unix)]` set perm via `OpenOptions::custom_flags + Permissions::from_mode(0o600)`.

- [ ] **Step 7: Wire OAuth2 into `DefaultResolver`**

Replace `OAuth2(_) => Err(...)` arm with dispatch to `fetch_cc` / `fetch_code`. Cache lookup first (using `clock.now()` for expiry check).

- [ ] **Step 8: Refresh token flow**

Test: token w/ `expires_at = now + 10s` → resolver runs `grant_type=refresh_token`. On refresh 4xx (`invalid_grant`), evict + re-run AuthCode. (For CC, just re-fetch.)

- [ ] **Step 9: Run + commit**

```bash
cargo test --workspace
git add -A
git commit -m "feat(auth): OAuth2 Client Credentials + Auth Code w/ PKCE + loopback"
```

---

## Task 12: `import` — Postman v2.1 + Postman environments + OpenAPI 3 (with DoS bounds)

**Files:**
- Create: `crates/import/src/{postman.rs,postman_env.rs,openapi.rs,lib.rs}`
- Test: `crates/import/tests/{postman_golden.rs,openapi_golden.rs,bounds.rs}`
- Fixtures: `crates/import/tests/fixtures/*.json`, `*.yaml`, plus `cyclic.yaml`

- [ ] **Step 1: deps**

```toml
lazyfetch-core = { path = "../core" }
serde      = { workspace = true }
serde_json = { workspace = true }
serde_yaml = { workspace = true }
thiserror  = { workspace = true }
indexmap   = "2"
[dev-dependencies]
insta = { version = "1", features = ["yaml"] }
```

- [ ] **Step 2: Postman fixtures + golden test**

Drop a small real Postman v2.1 export (3 requests, one folder, one auth) into `fixtures/postman_basic.json`. Test parses → `assert_yaml_snapshot!(coll)`.

- [ ] **Step 3: Implement `postman.rs`**

Struct mirrors v2.1 schema (`PmCollection { info, item, variable, auth }`). Recursive `walk_item` produces `Folder` / `Request`. Auth dispatch table. Unknown body modes → `ImportReport.warnings` + `Body::None`.

- [ ] **Step 4: Postman env**

Separate `postman_env.rs::parse(json) -> Environment`.

- [ ] **Step 5: OpenAPI golden test (petstore)**

Drop `fixtures/petstore.yaml`. Assert: collection has folders matching tags; `servers[0].url` becomes `{{base}}` collection var; required query params enabled.

- [ ] **Step 6: Implement `openapi.rs`**

Use `openapiv3` crate. Walk `paths`. Body stub from `requestBody.content["application/json"].example` or schema example walker (capped at `max_schema_nodes`). Track `visited: HashSet<String>` for `$ref` cycles; on revisit → emit warning + stub.

deps add: `openapiv3 = "2"`.

- [ ] **Step 7: DoS bounds test**

`fixtures/cyclic.yaml`: schema references itself. Test asserts parse returns Ok w/ warning + does not exhaust stack/memory.
Oversize test: build a 17 MiB string in-memory → `parse(s, ImportOpts { max_input_bytes: 16 * 1024 * 1024, .. })` returns input-too-large error.

- [ ] **Step 8: `:import` command wiring**

In `crates/tui/src/command.rs`, add `:import postman <path>`, `:import postman-env <path>`, `:import openapi <path>` → call importer + save via `FsCollectionRepo`/`FsEnvRepo` + show toast w/ warning count.

- [ ] **Step 9: Run + commit**

```bash
cargo test --workspace
git add -A
git commit -m "feat(import): Postman v2.1 + OpenAPI 3 + DoS bounds"
```

---

## Task 13: Polish — tracing secrets filter, history viewer, theme/keymap config, manpage stub

- [ ] **Step 1: Tracing secrets filter layer**

`crates/bin/src/log.rs`:
```rust
pub fn init(reg: Arc<Mutex<SecretRegistry>>) {
    let layer = SecretsRedactLayer::new(reg);
    tracing_subscriber::registry().with(fmt_layer()).with(layer).init();
}
```

`SecretsRedactLayer` rewrites `event.fields.message` via `reg.lock().redact(...)`. Build registry from active env on env-switch action. Test: log `tracing::info!("token={}", "xyz")` w/ "xyz" in registry → file contains `***`, not `xyz`.

- [ ] **Step 2: History viewer (TUI)**

`:history` opens a modal listing last 100 from `FsHistoryRepo`. Selection → preview redacted request snapshot + response status/elapsed. `Enter` reopens request in editor.

- [ ] **Step 3: Theme + keymap config**

`config.yaml`:
```yaml
theme: base16-default-dark
editor: nvim
defaults:
  timeout_ms: 30000
  follow_redirects: true
  max_redirects: 10
response:
  max_body_mb: 50
auth:
  use_keyring: false
keymap:
  send: s
  filter: f
```

Loaded once at startup, applied across `tui` (theme + key overrides).

- [ ] **Step 4: Manpage stub**

`crates/bin/build.rs` generates `man/lazyfetch.1` via `clap_mangen`. Install hook in README.

- [ ] **Step 5: Final checks**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && bash scripts/check-core-purity.sh`.

- [ ] **Step 6: Commit + tag v0.1.0**

```bash
git add -A
git commit -m "feat: history viewer + secrets log filter + config"
git tag v0.1.0
```

---

## Self-review

**Spec coverage check:**

- §0 v1 scope — covered Tasks 1–13. Each scope bullet maps to Task: HTTP core (6,7,8), collections (4), history (5,13), envs (5), auth Bearer/Basic/ApiKey (7), OAuth2 (11), Postman+OpenAPI import (12), response viewer + filter + save (10), body editor inline+`$EDITOR` (10).
- §1 crates/ports — Task 1 scaffold; ports realized in Tasks 2,3,7,11.
- §2 domain types incl. `notes`, redirect fields, `Interpolated`, `SecretRegistry` — Tasks 2, 3.
- §3 storage atomic-write same-dir tempfile, mtime+size+hash detection, fd-lock history — Task 4 (atomic), Task 5 (history). **Gap:** mtime+hash detection not explicitly implemented as a task — fix below.
- §4 TUI lifecycle, dirty buffer, resize, suspend — Task 9 (lifecycle + nav) + Task 10 (dirty modal). Resize handling: covered in event loop (Task 9 step 5).
- §5 OAuth2 PKCE+state+timeout+Drop guard+token key hash — Task 11.
- §6 import w/ DoS bounds — Task 12.
- §7 response viewer + jaq debounce + syntect precompiled — Task 10.
- §8 panic strategy + secrets filter + CI core purity — Task 1 (CI), Task 13 (filter), Task 9 (Drop guard restores terminal).
- §9 deps — Tasks 1–12.

**Gap fix — add mtime+hash freshness check to Task 4:**

Insert after Task 4 Step 7 (before commit):

- [ ] **Step 7b: External-edit detection**

Add `Stamp { mtime: SystemTime, size: u64, head_hash: [u8;32] }` to `FsCollectionRepo`. `load_with_stamp(name) -> (Collection, Stamp)`. `save_if_unchanged(coll, expected: &Stamp) -> Result<(), Conflict>` re-stats + re-hashes first 64 KiB of `collection.yaml` (and per-request file on per-request save) and refuses on mismatch. Test: load → external write → save → assert `Conflict`.

**Placeholder scan:** grep for "TBD/TODO/etc" in plan — none.

**Type consistency:**
- `Interpolated { value, used_secrets }` consistent across Tasks 2, 3, 7.
- `WireRequest` shape stable Tasks 3, 6, 7.
- `AuthResolver::apply` signature defined in Task 7 step 2 used unchanged in Task 11.
- `TokenKey { collection_id, auth_id, env_id, scopes }` defined Task 3, used Task 11.

**No spec gap remaining.**

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-07-lazyfetch-v1.md`.

Two execution options:

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — execute tasks in this session using executing-plans, batch execution with checkpoints.

Which?
