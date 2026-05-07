# lazyfetch — Design Spec

**Date:** 2026-05-07
**Status:** Draft (approved for plan, post-review)
**Owner:** Ariel Onoriaga

## §0 Goal

Terminal HTTP client. Postman/Insomnia alternative. Sibling to `lazygit`/`lazydocker`/`lazywifi`. Built in Rust with `ratatui` TUI and `reqwest` HTTP. File-based persistence (YAML/JSONL). Vim keymap.

### v1 scope

- Core HTTP client: GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS, headers, query params, JSON/raw/form/multipart body, response viewer.
- Collections + folders + history.
- Environments + variable interpolation (`{{var}}`).
- Auth: Bearer, Basic, API key (header/query), OAuth2 (Client Credentials + Authorization Code w/ PKCE).
- Import: Postman v2.1 collections + Postman environments + OpenAPI 3 specs.
- Response viewer: pretty-print + syntax highlight (JSON/XML/HTML), plain search, jq filter, save to file.
- Body editor: built-in textarea (vim-keys, syntect highlight) + shell-out to `$EDITOR`.

### Out of scope v1

Pre/post-request scripts, GraphQL builder, WebSocket, gRPC, mock server, code-gen, cloud sync, team workspaces, response spill-to-disk, OS keyring (default), image preview, request chaining/runner, native-format export, cookie jar (per-response display only — session jar phase-2).

---

## §1 Workspace & crates

```
lazyfetch/
├── Cargo.toml                  # workspace
├── crates/
│   ├── core/                   # pure domain, no IO, no tokio
│   ├── http/                   # reqwest adapter (HttpSender port impl)
│   ├── storage/                # fs adapter: collections, envs, history, auth-cache
│   ├── auth/                   # OAuth2 flows + browser launch + loopback callback
│   ├── import/                 # Postman v2.1 + OpenAPI 3 → core
│   ├── tui/                    # ratatui app, widgets, state, keymap
│   └── bin/                    # `lazyfetch` binary, composition root, CLI args
└── docs/
```

### Bounded contexts

- **Catalog** — `Collection`/`Folder`/`Request` tree. `core::catalog`. Persisted by `storage`.
- **Environment** — variable sets, active env, `{{var}}` interpolation. `core::env`.
- **Auth** — auth specs, token lifecycle, OAuth2 flows. `core::auth` (types + ports) + `auth` crate (flow execution, loopback server). `AuthCache` adapter lives in `storage`.
- **Execution** — build wire request from `Request + Env + Auth`, send, capture `Response`. `core::exec` (orchestration + ports) + `http` crate (adapter).
- **History** — executed-request snapshots, ring-buffered. `core::history` + `storage`.
- **Import** — foreign formats → core types. `import` crate.
- **UI** — TUI state machine, panes, dialogs, keymap. `tui` crate.

### Ports (traits in `core`)

- `HttpSender`: `async fn send(WireRequest) -> Result<WireResponse>`.
- `CollectionRepo`, `EnvRepo`, `HistoryRepo` — domain CRUD.
- `AuthCache` — token persistence (`get`/`put`/`evict` keyed by `TokenKey`). Trait in `core::auth`, impl in `storage`.
- `Editor: fn edit(buf: &str, hint: EditHint) -> Result<String>` — inline + `$EDITOR` impls.
- `Clock: fn now() -> DateTime<Utc>` — threaded through `execute`, `OAuth2Resolver::needs_refresh`, history `at` field. **Required** — not decorative.
- `Browser: fn open(url: &str) -> Result<()>` — abstracts `xdg-open`/`open` for OAuth flow + tests.

### Dependency direction

`bin → tui → core ← {http, storage, auth, import}`. `core` has no IO and no tokio dependency. Enforced via `cargo deny` + CI grep on `tokio::`/`std::fs::`/`std::net::` in `core`.

---

## §2 Domain types (core)

```rust
// core::primitives
type Id = ulid::Ulid;
struct KV { key: String, value: String, enabled: bool, secret: bool }
struct Template(String);                       // raw with {{var}} placeholders, parsed lazily
struct UrlTemplate(Template);                  // url-shaped Template
struct Part { name: String, content: PartContent, filename: Option<String> }
type Method = http::Method;                    // re-export of `http` crate Method

// core::catalog
struct Collection { id: Id, name: String, root: Folder, auth: Option<AuthSpec>, vars: VarSet }
struct Folder     { id: Id, name: String, items: Vec<Item>, auth: Option<AuthSpec> }
enum Item { Folder(Folder), Request(Request) }

struct Request {
    id: Id, name: String,
    method: Method,
    url: UrlTemplate,
    query: Vec<KV>,
    headers: Vec<KV>,
    body: Body,
    auth: Option<AuthSpec>,                    // None → inherit
    notes: Option<String>,                     // free-text; populated by import for dropped Postman scripts
    follow_redirects: bool,                    // default true
    max_redirects: u8,                         // default 10
    timeout_ms: Option<u32>,                   // override config default
}

enum Body {
    None,
    Raw       { mime: String, text: String },
    Json(String),
    Form(Vec<KV>),
    Multipart(Vec<Part>),
    File(PathBuf),
}

// core::env
struct VarValue { value: secrecy::SecretString, secret: bool }
type VarSet = Vec<(String, VarValue)>;
struct Environment { id: Id, name: String, vars: VarSet }

struct ResolveCtx<'a> {
    env: &'a Environment,
    collection_vars: &'a VarSet,
    overrides: &'a VarSet,
}
fn interpolate(s: &str, ctx: &ResolveCtx) -> Result<Interpolated, MissingVar>;

/// Result of interpolation: final string + the set of variable names whose values
/// were marked `secret: true`. Carried alongside any string built from secrets.
struct Interpolated { value: String, used_secrets: SecretRegistry }

// core::auth
enum AuthSpec {
    None, Inherit,
    Bearer { token: Template },
    Basic  { user: Template, pass: Template },
    ApiKey { name: String, value: Template, location: ApiKeyIn /* Header | Query */ },
    OAuth2(OAuth2Spec),
}

// core::exec
struct WireRequest  { method: Method, url: String, headers: Vec<KV>, body_bytes: Vec<u8>, timeout: Duration, follow_redirects: bool, max_redirects: u8 }
struct WireResponse { status: u16, headers: Vec<KV>, body_bytes: Vec<u8>, elapsed: Duration, size: u64 }
trait HttpSender { async fn send(&self, r: WireRequest) -> Result<WireResponse>; }

async fn execute(
    req: &Request,
    ctx: &ResolveCtx,
    auth: &dyn AuthResolver,
    http: &dyn HttpSender,
    clock: &dyn Clock,
) -> Result<Executed>;

struct Executed {
    request_snapshot: WireRequest,             // already redacted (see Secrets)
    response: WireResponse,
    at: DateTime<Utc>,
    secrets: SecretRegistry,                   // tracked redaction set, never persisted
}
```

### Variable interpolation

`{{var}}` resolved in: URL, header values, query values, body text, auth templates.

Lookup order: per-request overrides → environment → collection vars → error `MissingVar(name)`.

Returns `Interpolated { value, used_secrets }`. Any consumer downstream (history, log, save, raw-view) **must** call `redact(s, &used_secrets)` before display/persistence.

### Secret discipline (unified)

Single redaction surface:

```rust
fn redact(s: &str, reg: &SecretRegistry) -> String;       // replaces each tracked value with "***"
fn redact_wire(w: &WireRequest, reg: &SecretRegistry) -> WireRequest;
```

Applied at **every** boundary:

- History snapshot persistence.
- Response → Headers raw view (`H` toggle).
- Save → cURL reproduction.
- `tracing` log sink (subscriber filter).

`AuthSpec` template values for `Bearer.token`, `Basic.pass`, `ApiKey.value`, `OAuth2Spec.client_secret` **must** resolve to env vars flagged `secret: true`. Loader rejects non-secret bindings with a config-validation error pointing at the offending field.

`secrecy::SecretString` wraps stored values; `Debug` redacts; `expose_secret()` only at the network boundary inside `auth`/`http`.

---

## §3 Storage layout (filesystem)

Config root: `$XDG_CONFIG_HOME/lazyfetch/` (default `~/.config/lazyfetch/`).
Data root:   `$XDG_DATA_HOME/lazyfetch/` (default `~/.local/share/lazyfetch/`).
State root:  `$XDG_STATE_HOME/lazyfetch/` (default `~/.local/state/lazyfetch/`).

```
~/.config/lazyfetch/
├── config.yaml                 # global: theme, keymap overrides, editor cmd, default timeout, redirect defaults
├── collections/
│   ├── my-api/
│   │   ├── collection.yaml     # name, vars, auth (collection-level)
│   │   └── requests/
│   │       ├── auth/
│   │       │   ├── _folder.yaml
│   │       │   ├── login.yaml
│   │       │   └── refresh.yaml
│   │       └── users/
│   │           └── list.yaml
│   └── another-api/...
└── environments/
    ├── dev.yaml
    └── prod.yaml

~/.local/share/lazyfetch/
├── history.jsonl               # append-only executed-request log
├── auth-cache/                 # OAuth2 tokens (file perm 0600)
│   └── <token-key-hash>.json   # see §5
└── tmp/                        # $EDITOR temp files

~/.local/state/lazyfetch/
└── log                         # rotating tracing log
```

### Format choice

YAML for collections/envs/config (hand-editable, comments, diff-friendly). JSONL for history (cheap append).

### File granularity

File = aggregate root for collections. Each `Request` is its own file; folders are dirs containing `_folder.yaml` for metadata + ordering. Lets users `git init` their collection dir. Filenames are slugified names; `id` field is stable across renames.

### Atomic write protocol

1. Create temp file **in the same directory** as the target (avoid `EXDEV` cross-device rename).
2. `write_all` + `fsync`.
3. `rename` over target.
4. `fsync` parent dir on Unix.
`Drop` guard removes temp on panic.

### External-edit detection (mtime + size + hash)

mtime resolution is filesystem-dependent (1s ext4, 2s FAT); mtime alone is insufficient. On load, capture `(mtime, size, blake3_hash_first_64KB)`. On save, re-stat + compare; mismatch → prompt reload-or-overwrite. Documented limitation: rapid concurrent external edits within the tick window may still slip; users are expected to use `R` (reload) explicitly when editing externally.

### Concurrency on `history.jsonl`

Multiple lazyfetch processes + concurrent in-process writers must not interleave. Single in-process actor task owns the file handle; all callers send `Executed` over an mpsc channel. Cross-process: advisory file lock (`fd-lock`) acquired around each append. Reads (history viewer) take a shared lock and read-to-end.

### Repos (storage crate)

- `FsCollectionRepo { root: PathBuf }`
- `FsEnvRepo { root: PathBuf }`
- `FsHistoryRepo { path: PathBuf, max: usize }` — append, tail, ring-truncate (rewrite tail when over `max`).
- `FsAuthCache { dir: PathBuf }` — perm 0600.

---

## §4 TUI layout & state

### Panes (lazygit-style)

```
┌─ Collections ──┬─ Request ─────────────────────────────────┐
│ ▸ my-api       │ [GET ▾] {{base}}/users/{{id}}             │
│   ▾ users      │ ─ Params ─ Headers ─ Body ─ Auth ─        │
│     • list     │ key            value         [x]          │
│     • get      │ ...                                       │
│   ▾ auth       │                                           │
│ ▸ another-api  ├─ Response ────────────── 200 OK · 142ms ─┤
│                │ ─ Body ─ Headers ─ Cookies ─ Timing ─    │
├─ Environment ──┤ {                                         │
│ [dev ▾]        │   "users": [...]                          │
│ base=...       │ }                                         │
│ token=***      │                                           │
└────────────────┴───────────────────────────────────────────┘
 :send  /search  f filter  e edit body  E $EDITOR  s save  ? help
```

### State machine

```rust
enum Mode { Normal, Insert, Command, Search, Filter, Dialog(DialogKind) }
enum Focus { Collections, Env, Request(ReqTab), Response(ResTab) }

struct AppState {
    mode: Mode,
    focus: Focus,
    catalog: Catalog,
    env_state: EnvState,
    open: Option<RequestEditor>,
    last: Option<Executed>,
    inflight: Option<RequestHandle>,           // single concurrent send (v1)
    history: HistoryView,
    toast: Option<Toast>,
}

struct RequestEditor { req: Request, dirty: bool, last_saved_hash: u64 }
```

### Dirty-buffer policy

- Navigating away from a dirty buffer → modal: `[s]ave  [d]iscard  [c]ancel`. Default Enter = save.
- Sending a dirty request → autosave first (transparent), then send. Send failure ⊥ rollback save.
- Quit (`q`/`:q`) with dirty buffers → list dirty items, prompt save-all/discard-all/cancel.

### Concurrency model

Single tokio runtime. UI runs on main thread (crossterm event loop, ~30 fps tick). HTTP send spawned as task; result returned via `tokio::sync::mpsc` channel polled per tick. Cancel via `AbortHandle` (`Ctrl-c` while inflight). **One inflight send at a time** — second `s` while inflight shows toast "send in progress, Ctrl-c to cancel". Phase-2 may add a runner.

### Terminal lifecycle

- Enter alt-screen + raw-mode + mouse capture on startup.
- `Resize` event invalidates layout cache; layout recomputed every `Resize` (invariant).
- Suspend protocol (`Ctrl-z` and `$EDITOR` shell-out): `disable_raw_mode → leave_alt_screen → flush`, exec, on return `enter_alt_screen → enable_raw_mode → force full redraw`. RAII guard restores even on panic of child.
- `$EDITOR` temp file cleaned via `Drop`-guarded `tempfile::NamedTempFile`, never relying on happy-path delete.
- Panic strategy: render-loop wraps draw in `catch_unwind` for **render-only** safety; tokio task panics surface via `JoinHandle::Err` and propagate to UI as toast + log; unrecoverable panics in `core`/`storage` exit cleanly via `Drop` guard that restores terminal state (no `catch_unwind` swallowing).

### Keymap

- **Global:** `Tab`/`Shift-Tab` cycle pane focus, `1`–`4` jump pane, `:` command, `/` search, `?` help, `q` quit, `Ctrl-c` cancel inflight.
- **Collections:** `j`/`k`, `Enter` open, `a` add, `r` rename, `d` delete, `y` duplicate, `R` reload from disk.
- **Request:** `j`/`k` rows, `i`/`a` insert, `x` toggle row enabled, `Space` switch tab, `e` inline edit body, `E` `$EDITOR` body, `s` send, `Enter` (URL field) → send.
- **Response:** `j`/`k` scroll, `/` body search, `f` jq filter, `S` save, `H` toggle headers raw.
- **Env:** `:` `:env <name>` switch (no single-letter overload of `e`), `a` add var, `m` mark/unmark secret.

### Command mode

`:send`, `:save`, `:import postman <path>`, `:import postman-env <path>`, `:import openapi <path>`, `:env <name>`, `:new collection <name>`, `:new request <name>`, `:history`, `:messages`, `:q`.

---

## §5 Auth resolution & OAuth2

### Resolver pipeline (per send)

1. Walk effective `AuthSpec`: request → folder chain → collection. `Inherit` climbs. `None` stops.
2. Interpolate templates → `Interpolated { value, used_secrets }`.
3. For OAuth2 → `AuthCache.get(TokenKey)`. Hit + `clock.now() + 30s < expires_at` → use. Else run flow → `put`.
4. Apply to `WireRequest`. Add resolved secrets to the request's `SecretRegistry`.

### Trait

```rust
trait AuthResolver {
    async fn apply(
        &self, spec: &AuthSpec, ctx: &ResolveCtx, clock: &dyn Clock,
        cache: &dyn AuthCache, req: &mut WireRequest, reg: &mut SecretRegistry,
    ) -> Result<()>;
}
```

### OAuth2 spec

```rust
enum OAuth2Spec {
    ClientCredentials {
        token_url: Template, client_id: Template, client_secret: Template,
        scopes: Vec<String>, audience: Option<Template>,
    },
    AuthCode {
        auth_url: Template, token_url: Template,
        client_id: Template, client_secret: Option<Template>,
        redirect_uri: String,                  // default http://127.0.0.1:<port>/callback
        scopes: Vec<String>, pkce: bool,       // default true
    },
}
```

### Authorization Code flow (TUI-friendly)

1. Generate cryptographically random `state` (32 bytes, base64url) and PKCE `code_verifier` + `code_challenge` (S256). Bind both to flow handle.
2. Bind ephemeral loopback port (default range `49152..65535`).
3. Build auth URL with `state` + `code_challenge` → `Browser.open(url)`. Print URL fallback when browser launch fails.
4. Modal: "Waiting for browser callback… [Esc cancel]". HTTP send remains cancellable.
5. Loopback hyper server accepts **one** request at `/callback`:
   - Validate `state` against bound value (constant-time compare). Mismatch → reject + flow error.
   - Validate `code` present; consume `code` once (single-use guard).
   - Exchange `code` + `code_verifier` at `token_url`.
   - Reply 200 with "lazyfetch: you can close this tab" HTML.
6. **Bounded timeout 120s** (configurable); on timeout or Esc, abort listener and release port.
7. Listener owned by `Drop` guard so port always released — including panic and task cancellation.
8. Store `Token { access, refresh, expires_at, scopes }` via `AuthCache.put(TokenKey, token)`.

### Refresh

When `clock.now() + 30s > expires_at`, run `grant_type=refresh_token`. On refresh fail (4xx invalid_grant) → evict cache + re-run AuthCode.

### Token cache key & filename

```rust
struct TokenKey { collection_id: Id, auth_id: Id, env_id: Id, scopes: Vec<String> }
fn token_key_hash(k: &TokenKey) -> String;     // blake3 of canonical encoding, hex; NO secret material
```

Filename: `auth-cache/<hex>.json`, perm 0600. File contains `Token` with `access`/`refresh` as `SecretString`. Refresh tokens never logged.

### Crate boundary

`core::auth` defines: `AuthSpec`, `OAuth2Spec`, `Token`, `TokenKey`, `AuthCache` trait, `AuthResolver` trait, `Browser` trait. `auth` crate provides: `OAuth2Resolver` impl, PKCE/state generation, hyper loopback server, `SystemBrowser` impl. `storage` crate provides `FsAuthCache` impl. `core` stays IO-free.

### OS keyring

Optional `keyring` integration behind `auth.use_keyring: true` config flag. Phase-2.

---

## §6 Import (Postman v2.1 + OpenAPI 3)

Crate: `import`. Pure functions:

```rust
postman::parse(json: &str) -> Result<(Collection, ImportReport)>;
openapi::parse(yaml_or_json: &str, opts: ImportOpts) -> Result<(Collection, ImportReport)>;

struct ImportOpts {
    max_input_bytes: usize,                    // default 16 MiB
    max_ref_depth: u8,                         // default 32
    max_schema_nodes: usize,                   // default 50_000
}
```

No IO. Bin layer reads file → passes string after size check.

### DoS / safety bounds

- Reject input larger than `max_input_bytes`.
- OpenAPI `$ref` resolution tracks visited refs; cycles → warning + stub body. Recursion depth capped at `max_ref_depth`.
- Schema → example walker capped at `max_schema_nodes` total visits.

### Postman v2.1 mapping

| Postman | lazyfetch |
| --- | --- |
| `info.name` | `Collection.name` |
| `item[]` (folder) | `Folder` |
| `item[]` (request) | `Request` |
| `request.method/url/header/body` | `Request.method/url/headers/body` |
| `url.variable[]` + `url.query[]` | path vars merged into URL template + `query` |
| `auth` (bearer/basic/apikey/oauth2) | `AuthSpec` |
| `variable[]` (collection-level) | `Collection.vars` |
| `event` (prerequest/test scripts) | dropped; raw script stored as `Request.notes` for visibility |
| `body.mode`: raw / urlencoded / formdata / file / graphql | `Body::Raw`/`Form`/`Multipart`/`File`; graphql → `Body::Json` `{query, variables}` |

Postman environments imported as `Environment` via `:import postman-env <path>`.

### OpenAPI 3 mapping

- `info.title` → collection name.
- Tags → top-level folders (untagged → `_default/`).
- Each `path × method` → `Request`. Path params → `{{param}}` placeholders + collection var stub. Query params → `query` rows (disabled unless `required`). `requestBody.content`: prefer `application/json` → `Body::Json` from example/schema-derived stub; else first content type.
- `servers[0].url` → collection var `base`. URL = `{{base}}{path}`.
- `securitySchemes` → `AuthSpec` template at collection level. OAuth2 flows → `OAuth2Spec` skeleton; user fills `client_id`/`client_secret` (must reference secret env vars).

### Errors

Unknown auth type / unsupported body mode / cycle / cap-exceeded → `ImportReport.warnings: Vec<String>`, surfaced as toast + `:messages`. Skip-and-continue, never abort whole import.

### Roundtrip

Import-only v1, no export. Native YAML format is canonical.

### Tests

Golden fixtures in `crates/import/tests/fixtures/` (real Postman exports + OpenAPI specs incl. petstore + a self-referential schema for cycle test) → snapshot assert (`insta`).

---

## §7 Response viewer

### Pipeline

`WireResponse.body_bytes` → decode `Content-Encoding: gzip|deflate|br` → detect content-type (header → magic bytes → extension hint) → renderer.

### Renderers

- `application/json` → `serde_json` parse → pretty 2-space → `syntect` highlight.
- `application/xml`, `text/xml`, `text/html` → `quick-xml` reformat / minimal HTML pretty → `syntect`.
- `text/*` → raw + line numbers + `syntect` by ext if known.
- Binary (`image/*`, `application/octet-stream`, > N MB) → metadata only (size, type, blake3). `S` save. No image rendering v1.

`syntect` themes/syntaxes loaded once at startup from a precompiled `dumps::from_uncompressed_data` blob bundled at build time (avoids 100ms+ cold load). Theme configurable via `config.yaml`.

### Tabs

- **Body** — rendered output.
- **Headers** — sorted KV; `H` toggles raw wire format (post-redaction).
- **Cookies** — parsed `Set-Cookie` rows from current response only (no session jar v1).
- **Timing** — total + TTFB (v1). DNS / connect / TLS phase-2 via instrumented `hyper` connector.

### Search (`/`)

Plain substring over rendered body; highlight; `n`/`N`. Case-insensitive default; `\C` suffix (vim-style) → case-sensitive.

### Filter (`f`)

Prompt → query string applied to parsed JSON body.

- Engine: `jaq` (jq-compatible, pure Rust).
- Non-JSON body → filter disabled with toast.
- Non-destructive: original body preserved; toggle `f` again clears.
- **Debounce 150 ms** between keystroke and re-evaluation. Re-eval runs on a tokio task; new keystroke aborts the previous task before spawning the next.

### Save (`S`)

Dialog choices:

1. Body only → `<name>.<ext>` (ext from content-type).
2. Full response (status line + headers + body) → `.http` file.
3. cURL command (request reproduction, **redacted**) → `.sh`.

Default dir `$PWD`; remembered across session.

### Redirect policy

`reqwest` redirect policy built from `Request.follow_redirects` + `max_redirects`. Default true / 10. Per-request override via Request fields; global default in `config.yaml`.

### Streaming / large bodies

Cap in-memory body at `response.max_body_mb` (default 50). Above cap → reject with warning v1; spill-to-disk windowed reading is phase-2.

---

## §8 Errors, testing, observability, phasing

### Error model

`thiserror` per crate. Top-level `AppError` in `bin` aggregates. Categories: `Domain`, `Io`, `Net`, `Auth`, `Import`. UI surfaces as toast + detail in `:messages`.

### Panic & terminal restoration

- Render loop wraps draw in `catch_unwind` (render-only safety net).
- Tokio task panics propagate via `JoinHandle::Err` → UI toast + log.
- Any panic from `core`/`storage` exits the process, but a `Drop` guard restores terminal state (alt-screen left, raw-mode disabled, cursor shown) **before** unwinding completes. No swallow.

### Logging

`tracing` + `tracing-subscriber`. File sink `~/.local/state/lazyfetch/log` (rotating). `--debug` flag → verbose.

### Secrets filter (logging layer)

A `SecretRegistry` is maintained per-task and per-process:

- Built from: `Environment.vars` flagged secret + active `Token.access`/`refresh` from `AuthCache` lookups.
- Invalidated on env switch, env-var edit, token eviction, token refresh.
- Subscriber layer rewrites known secret values to `***` in any log event.

### Testing strategy

- **`core`** — pure unit tests, no tokio. `proptest` for interpolation, auth resolution chain, env merge, redaction.
- **`http`** — integration tests with `wiremock`.
- **`storage`** — `tempfile`-backed; atomic-write, mtime+hash detection, `fd-lock` cross-process append (spawn helper bin).
- **`auth`** — OAuth2 flows with `wiremock`; loopback callback ephemeral port; PKCE + state round-trip; CSRF rejection (mismatched state); listener-timeout test; port-release-on-panic test.
- **`import`** — golden fixtures; cycle/recursion-cap test; oversize-input test.
- **`tui`** — `TestBackend` snapshots (`insta`); event-driven state transitions tested without backend; resize event invalidates layout test; suspend/restore protocol around stub editor.
- **E2E smoke** — `expectrl`/`rexpect` driving keys against wiremock; assert exit + last response + redacted log.

### CI

`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `cargo deny check`, grep guard against `tokio::`/`std::fs::`/`std::net::` in `crates/core/`.

### Phases

1. Workspace scaffold + `core` types + interpolation + redaction + tests.
2. `storage` (collections, envs, history) + atomic write + mtime+hash + `fd-lock` history.
3. `http` adapter + `core::exec` + non-OAuth auth (Bearer/Basic/ApiKey) + CLI send (`lazyfetch run <req>`).
4. TUI shell: panes, navigation, request edit, send/render response, terminal lifecycle, dirty-buffer policy.
5. Body editor (inline + `$EDITOR` w/ suspend protocol), response search/filter (debounced)/save, syntax highlight (precompiled syntect dump).
6. OAuth2 (Client Credentials → Authorization Code + PKCE + state + loopback w/ timeout + Drop-guarded listener) + `FsAuthCache`.
7. Import: Postman v2.1, then OpenAPI 3 (with DoS bounds).
8. Polish: detailed timing, history viewer, theming, keymap config.

---

## §9 Key dependencies (proposed)

| Crate | Purpose |
| --- | --- |
| `ratatui` | TUI rendering |
| `crossterm` | Terminal IO + events |
| `tokio` | Async runtime |
| `reqwest` (rustls) | HTTP client |
| `hyper` | OAuth2 loopback callback server |
| `http` | `Method` re-export, header types |
| `serde` + `serde_yaml` + `serde_json` | Persistence + parsing |
| `secrecy` | `SecretString` |
| `ulid` | `Id` |
| `blake3` | Content hash, token key hash |
| `fd-lock` | Cross-process file locking (history) |
| `thiserror` / `anyhow` | Errors |
| `tracing` / `tracing-subscriber` | Logging + secrets-filter layer |
| `syntect` | Syntax highlight (precompiled dump) |
| `jaq` | JSON filter (jq-compatible) |
| `quick-xml` | XML pretty |
| `tui-textarea` | Body editor (inline) |
| `proptest` | Property tests |
| `wiremock` | HTTP mocking |
| `insta` | Snapshot tests |
| `tempfile` | Test fs isolation + `$EDITOR` temp |
| `dirs` | XDG paths |
| `keyring` (opt, phase-2) | OS keyring |
