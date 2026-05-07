# lazyfetch — Design Spec

**Date:** 2026-05-07
**Status:** Draft (approved for plan)
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

Pre/post-request scripts, GraphQL builder, WebSocket, gRPC, mock server, code-gen, cloud sync, team workspaces, response spill-to-disk, OS keyring (default), image preview, request chaining/runner, native-format export.

---

## §1 Workspace & crates

```
lazyfetch/
├── Cargo.toml                  # workspace
├── crates/
│   ├── core/                   # pure domain, no IO, no tokio
│   ├── http/                   # reqwest adapter (HttpSender port impl)
│   ├── storage/                # fs adapter (collections, envs, history, auth-cache)
│   ├── auth/                   # auth resolvers (Bearer/Basic/ApiKey/OAuth2)
│   ├── import/                 # Postman v2.1 + OpenAPI 3 → core
│   ├── tui/                    # ratatui app, widgets, state, keymap
│   └── bin/                    # `lazyfetch` binary, composition root, CLI args
└── docs/
```

### Bounded contexts

- **Catalog** — `Collection`/`Folder`/`Request` tree. `core::catalog`. Persisted by `storage`.
- **Environment** — variable sets, active env, `{{var}}` interpolation. `core::env`.
- **Auth** — auth specs, token cache, OAuth2 flows. `core::auth` (types) + `auth` crate (flows, cache).
- **Execution** — build wire request from `Request + Env + Auth`, send, capture `Response`. `core::exec` (orchestration + ports) + `http` crate (adapter).
- **History** — executed-request snapshots, ring-buffered. `core::history` + `storage`.
- **Import** — foreign formats → core types. `import` crate.
- **UI** — TUI state machine, panes, dialogs, keymap. `tui` crate.

### Ports (traits in `core`)

- `HttpSender`: `async fn send(WireRequest) -> Result<WireResponse>`
- `CollectionRepo`, `EnvRepo`, `HistoryRepo`, `AuthCache`
- `Editor`: `fn edit(buf: &str) -> String` (inline + `$EDITOR` impls)
- `Clock` (deterministic tests)

### Dependency direction

`bin → tui → core ← {http, storage, auth, import}`. `core` has no IO and no tokio dependency.

---

## §2 Domain types (core)

```rust
// core::catalog
struct Collection { id: Id, name: String, root: Folder, auth: Option<AuthSpec>, vars: VarSet }
struct Folder { id: Id, name: String, items: Vec<Item>, auth: Option<AuthSpec> }
enum Item { Folder(Folder), Request(Request) }

struct Request {
    id: Id, name: String,
    method: Method,                // GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS
    url: UrlTemplate,              // raw + parsed {{var}} segments
    query: Vec<KV>,                // enabled flag per row
    headers: Vec<KV>,
    body: Body,
    auth: Option<AuthSpec>,        // None → inherit
}

enum Body {
    None,
    Raw { mime: String, text: String },
    Json(String),
    Form(Vec<KV>),
    Multipart(Vec<Part>),
    File(PathBuf),
}

// core::env
struct Environment { id: Id, name: String, vars: VarSet }   // VarSet = Vec<(String, Secret<String>)>
struct ResolveCtx<'a> {
    env: &'a Environment,
    collection_vars: &'a VarSet,
    overrides: &'a VarSet,
}
fn interpolate(s: &str, ctx: &ResolveCtx) -> Result<String, MissingVar>;

// core::auth
enum AuthSpec {
    None, Inherit,
    Bearer { token: Template },
    Basic  { user: Template, pass: Template },
    ApiKey { name: String, value: Template, location: ApiKeyIn /* Header | Query */ },
    OAuth2(OAuth2Spec),
}

// core::exec
struct WireRequest  { method: Method, url: String, headers: Vec<KV>, body_bytes: Vec<u8>, timeout: Duration }
struct WireResponse { status: u16, headers: Vec<KV>, body_bytes: Vec<u8>, elapsed: Duration, size: u64 }
trait HttpSender { async fn send(&self, r: WireRequest) -> Result<WireResponse>; }
async fn execute(req: &Request, ctx: &ResolveCtx, auth: &dyn AuthResolver, http: &dyn HttpSender) -> Result<Executed>;
struct Executed { request_snapshot: WireRequest, response: WireResponse, at: DateTime<Utc> }
```

### Variable interpolation

`{{var}}` resolved in: URL, header values, query values, body text, auth templates.

Lookup order: per-request overrides → environment → collection vars → error `MissingVar(name)`.

### Secrets

Values flagged `secret: true` are never written to history snapshots verbatim (masked as `***`). Never logged. `Secret<String>` wrapper redacts in `Debug`.

---

## §3 Storage layout (filesystem)

Config root: `$XDG_CONFIG_HOME/lazyfetch/` (default `~/.config/lazyfetch/`).
Data root: `$XDG_DATA_HOME/lazyfetch/` (default `~/.local/share/lazyfetch/`).
State root: `$XDG_STATE_HOME/lazyfetch/` (default `~/.local/state/lazyfetch/`).

```
~/.config/lazyfetch/
├── config.yaml                 # global: theme, keymap overrides, editor cmd, default timeout
├── collections/
│   ├── my-api/
│   │   ├── collection.yaml     # name, vars, auth (collection-level)
│   │   └── requests/
│   │       ├── auth/
│   │       │   ├── _folder.yaml    # folder name, auth, order
│   │       │   ├── login.yaml      # Request
│   │       │   └── refresh.yaml
│   │       └── users/
│   │           └── list.yaml
│   └── another-api/...
└── environments/
    ├── dev.yaml
    ├── staging.yaml
    └── prod.yaml

~/.local/share/lazyfetch/
├── history.jsonl               # append-only executed-request log
├── auth-cache/                 # OAuth2 tokens (file perm 0600)
│   └── <collection-id>-<auth-id>.json
└── tmp/                        # $EDITOR temp files

~/.local/state/lazyfetch/
└── log                         # rotating tracing log
```

### Format choice

- YAML for collections/envs/config (hand-editable, comment-friendly, diff-friendly).
- JSONL for history (cheap append, no full rewrite).

### File granularity

File = aggregate root for collections. Each `Request` is its own file; folders are dirs containing `_folder.yaml` for metadata + ordering. Lets users `git init` their collection dir and share via repo. Filenames are slugified names; `id` field is stable across renames.

### Repos (storage crate)

- `FsCollectionRepo { root: PathBuf }` — load/save `Collection` tree, watch for external edits.
- `FsEnvRepo { root: PathBuf }`
- `FsHistoryRepo { path: PathBuf, max: usize }` — append, tail, truncate ring.
- `FsAuthCache { dir: PathBuf }` — read/write tokens, file perm 0600.

### Concurrency & integrity

Writes use `tempfile + atomic rename`. External-edit detection via mtime check on load; if dirty, prompt reload before overwrite.

---

## §4 TUI layout & state

### Panes (lazygit-style)

```
┌─ Collections ──┬─ Request ─────────────────────────────────┐
│ ▸ my-api       │ [GET ▾] {{base}}/users/{{id}}             │
│   ▾ users      │ ─ Params ─ Headers ─ Body ─ Auth ─ Tests ─│
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
 :send  /filter  e edit body  E $EDITOR  s save  ? help
```

- Left top: `Collections` (tree).
- Left bottom: `Environment` (active env + var list).
- Right top: `Request` (tabs: Params / Headers / Body / Auth / Tests-stub).
- Right bottom: `Response` (tabs: Body / Headers / Cookies / Timing).

### State machine (top-level)

```rust
enum Mode { Normal, Insert, Command, Search, Filter, Dialog(DialogKind) }
enum Focus { Collections, Env, Request(ReqTab), Response(ResTab) }

struct AppState {
    mode: Mode,
    focus: Focus,
    catalog: Catalog,
    env_state: EnvState,
    open: Option<RequestEditor>,        // current request buffer (dirty flag)
    last: Option<Executed>,
    inflight: Option<RequestHandle>,    // tokio task + cancel
    history: HistoryView,
    toast: Option<Toast>,
}
```

### Async model

Single tokio runtime. UI runs on main thread (crossterm event loop, ~30 fps tick). HTTP send spawned as task; result returned via `tokio::sync::mpsc` channel polled per tick. Cancel via `AbortHandle` (`Ctrl-c` while inflight).

### Keymap (vim-ish, lazygit-style per-pane)

- **Global:** `Tab`/`Shift-Tab` cycle pane focus, `1`–`4` jump to pane, `:` command, `/` search, `?` help, `q` quit.
- **Collections:** `j`/`k` nav, `Enter` open req, `a` add, `r` rename, `d` delete, `y` duplicate, `R` reload from disk.
- **Request:** `j`/`k` rows, `i`/`a` insert, `x` toggle row enabled, `Space` toggle tab, `e` inline edit body, `E` `$EDITOR` body, `Enter` (URL field) → send, `s` send.
- **Response:** `j`/`k` scroll, `/` body search, `f` jq filter, `S` save, `H` toggle headers raw.
- **Env:** `e` switch env, `a` add var, `s` mark secret.

### Command mode (`:`)

`:send`, `:save`, `:import postman <path>`, `:import postman-env <path>`, `:import openapi <path>`, `:env <name>`, `:new collection <name>`, `:new request <name>`, `:history`, `:messages`, `:q`.

---

## §5 Auth resolution & OAuth2

### Resolver pipeline (per send)

1. Walk effective `AuthSpec`: request → folder chain → collection. `Inherit` climbs. `None` stops.
2. Interpolate templates against `ResolveCtx` (env + collection vars).
3. For OAuth2 → `AuthCache` lookup. Hit + valid → use. Miss/expired → run flow → store.
4. Apply to `WireRequest` (header or query param).

### Trait

```rust
trait AuthResolver {
    async fn apply(&self, spec: &AuthSpec, ctx: &ResolveCtx, req: &mut WireRequest) -> Result<()>;
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

1. Bind ephemeral loopback port (default range `49152..65535`).
2. Build auth URL with PKCE challenge → `xdg-open`/`open` in user browser. Print URL fallback.
3. Block UI with modal: "Waiting for browser callback… [Esc cancel]". HTTP send remains cancellable.
4. Tiny `hyper` server captures `?code=...&state=...` → exchange code at `token_url` → store `Token { access, refresh, expires_at, scopes }`.
5. Refresh: when `expires_at - now < 30s`, run `grant_type=refresh_token`. On refresh fail → re-run AuthCode.

### Token storage

Path: `~/.local/share/lazyfetch/auth-cache/<coll>-<auth-hash>.json`. File perm 0600. Refresh tokens never logged. Optional OS keyring (`keyring` crate) behind config flag `auth.use_keyring: true` — phase-2.

### Crate boundary

`core::auth` defines `AuthSpec` + `AuthResolver` trait + `Token` types. `auth` crate provides `OAuth2Resolver` impl + `FsAuthCache` (or thin wrapper around `storage`). `core` stays IO-free.

---

## §6 Import (Postman v2.1 + OpenAPI 3)

Crate: `import`. Pure functions:

```rust
postman::parse(json: &str) -> Result<(Collection, ImportReport)>
openapi::parse(yaml_or_json: &str) -> Result<(Collection, ImportReport)>
```

No IO. Bin layer reads file → passes string.

### Postman v2.1 mapping

| Postman | lazyfetch |
| --- | --- |
| `info.name` | `Collection.name` |
| `item[]` (folder w/ `item[]`) | `Folder` |
| `item[]` (request) | `Request` |
| `request.method/url/header/body` | `Request.method/url/headers/body` |
| `url.variable[]` + `url.query[]` | path vars merged into url template + `query` |
| `auth` (bearer/basic/apikey/oauth2) | `AuthSpec` |
| `variable[]` (collection-level) | `Collection.vars` |
| `event` (prerequest/test scripts) | dropped v1, recorded as `notes` field on `Request` for visibility |
| `body.mode`: raw / urlencoded / formdata / file / graphql | `Body::Raw`/`Form`/`Multipart`/`File`; graphql → `Body::Json` with `{query, variables}` |

Postman environments: separate `postman_environment.json` files imported as `Environment`. Command: `:import postman-env <path>`.

### OpenAPI 3 mapping

- `info.title` → collection name.
- Tags → top-level folders (untagged ops → `_default/`).
- Each `path × method` → `Request`. Path params → `{{param}}` placeholders + collection var stub. Query params → `query` rows (disabled by default unless `required`). `requestBody.content`: prefer `application/json` → `Body::Json` with example/schema-derived stub; else first content type.
- `servers[0].url` → collection var `base`. URL template = `{{base}}{path}`.
- `securitySchemes` → `AuthSpec` template at collection level: bearer → `Bearer { token: {{token}} }`; apiKey → `ApiKey { ... }`; oauth2 flows → `OAuth2Spec` skeleton (user fills client id/secret).

### Errors

Unknown auth type / unsupported body mode → warning collected in `ImportReport { warnings: Vec<String> }`, surfaced in toast + `:messages` log. Skip-and-continue, never abort whole import.

### Roundtrip

Import-only v1, no export. Native YAML format is canonical.

### Tests

Golden fixtures in `crates/import/tests/fixtures/` (real Postman exports + OpenAPI specs e.g. petstore) → assert deserialized `Collection` snapshot via `insta`.

---

## §7 Response viewer

### Pipeline

`WireResponse.body_bytes` → detect content-type (header → magic bytes → extension hint) → decoder → renderer.

### Renderers

- `application/json` → serde parse → pretty 2-space → `syntect` highlight (theme: `base16-default-dark`, configurable).
- `application/xml`, `text/xml`, `text/html` → format via `quick-xml` reformatter / minimal HTML pretty → `syntect`.
- `text/*` → raw + line numbers + `syntect` by ext if known.
- Binary (`image/*`, `application/octet-stream`, > N MB) → show metadata (size, type, sha256), offer `S` save. No image rendering v1.
- Decode `Content-Encoding: gzip|deflate|br` before render.

### Tabs (Response pane)

- **Body** — rendered output.
- **Headers** — sorted KV; `H` toggles raw wire format.
- **Cookies** — parsed `Set-Cookie` rows.
- **Timing** — DNS / connect / TLS / TTFB / total. v1 ships **total + TTFB only**; remaining metrics phase-2.

### Search (`/`)

Plain substring over rendered body; highlight matches; `n`/`N` jump. Case-insensitive default; `\C` suffix (vim-style) → case-sensitive.

### Filter (`f`)

Opens prompt → query string applied to parsed JSON.

- Engine: `jaq` crate (jq-compatible, pure Rust). Reason: drop-in jq syntax, no C dependency.
- Non-JSON body → filter disabled with toast.
- Filter is non-destructive: original body preserved; toggle `f` again clears.

### Save (`S`)

Dialog choices:

1. Body only → `<name>.<ext>` (ext from content-type).
2. Full response (status line + headers + body) → `.http` file.
3. cURL command (request reproduction) → `.sh`.

Default dir: `$PWD`; remembered across session.

### Streaming / large bodies

Cap in-memory body at `response.max_body_mb` (default 50). Above cap → reject + warning v1; spill-to-disk windowed reading is phase-2.

---

## §8 Errors, testing, observability, phasing

### Error model

`thiserror` per crate. Top-level `AppError` in `bin` aggregates. Categories:

- `Domain` — validation, missing var.
- `Io` — fs, parse.
- `Net` — timeout, dns, tls, status-as-error opt-in.
- `Auth` — oauth fail, refresh fail, missing creds.
- `Import` — warn-collect, no abort.

UI surfaces as toast + detail in `:messages`. No panics in render loop — `catch_unwind` guard around frame draw, error → fallback screen.

### Logging

`tracing` + `tracing-subscriber`. File sink `~/.local/state/lazyfetch/log` (rotating). `--debug` flag → verbose. Secrets filter layer redacts known secret values pre-write.

### Testing strategy

- **`core`** — pure unit tests, no tokio. Property tests (`proptest`) for interpolation, auth resolution chain, env merge.
- **`http`** — integration tests with `wiremock` — real network mocked.
- **`storage`** — `tempfile`-backed tests, atomic-write + reload + mtime detection.
- **`auth`** — OAuth2 flows with `wiremock`; loopback callback with ephemeral port; PKCE verifier round-trip.
- **`import`** — golden fixtures (Postman exports, petstore OpenAPI) → snapshot assert (`insta`).
- **`tui`** — ratatui `TestBackend` snapshots for key screens (collection list, request edit, response render); event-driven state transitions tested without backend.
- **E2E smoke** — spawn lazyfetch with `expectrl` or `rexpect`, drive keys, hit wiremock, assert exit + last response.

### CI

`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `cargo deny check`.

### Phases

1. Workspace scaffold + `core` types + interpolation + tests.
2. `storage` (collections, envs, history) + YAML round-trip.
3. `http` adapter + `core::exec` + non-OAuth auth + CLI send (`lazyfetch run <req>`).
4. TUI shell: panes, navigation, request edit, send/render response.
5. Body editor (inline + `$EDITOR`), response search/filter/save, syntax highlight.
6. OAuth2 (Client Credentials → Authorization Code + PKCE + loopback).
7. Import: Postman v2.1, then OpenAPI 3.
8. Polish: timing details, history viewer, theming, keymap config.

---

## §9 Key dependencies (proposed)

| Crate | Purpose |
| --- | --- |
| `ratatui` | TUI rendering |
| `crossterm` | Terminal IO + events |
| `tokio` | Async runtime |
| `reqwest` (rustls) | HTTP client |
| `hyper` | OAuth2 loopback callback server |
| `serde` + `serde_yaml` + `serde_json` | Persistence + parsing |
| `thiserror` / `anyhow` | Errors |
| `tracing` / `tracing-subscriber` | Logging |
| `syntect` | Syntax highlight |
| `jaq` | JSON filter (jq-compatible) |
| `quick-xml` | XML pretty |
| `tui-textarea` | Body editor (inline) |
| `proptest` | Property tests |
| `wiremock` | HTTP mocking |
| `insta` | Snapshot tests |
| `tempfile` | Test fs isolation |
| `keyring` (opt) | OS keyring (phase-2) |
| `directories` | XDG paths |
| `secrecy` | `Secret<T>` wrapper |
