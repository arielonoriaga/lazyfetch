# lazyfetch v0.2 — Request editor + dyn vars + cURL interop

**Date:** 2026-05-08
**Status:** Draft (approved for plan)
**Owner:** Ariel Onoriaga
**Anchor:** Make the Request pane self-sufficient — body editor (Raw/JSON/Form/Multipart/GraphQL), Headers/Query KV editors, dynamic vars, cURL import/export, repeat-last.

## §0 Goal

v0.1 ships a Request pane that is read-only. v0.2 closes the biggest UX gap: the user can build, edit, and persist a full Request from inside the TUI with zero context-switch. cURL import/export bridges the rest of the internet's HTTP examples. Dynamic vars (`{{$uuid}}`, `{{$now}}`, …) make retry/replay loops painless. Repeat-last (`R`) closes the dev inner loop.

### v0.2 scope

- Body editor: `Raw`, `Json`, `Form` (urlencoded), `Multipart` (with file fields), `GraphQL` (split query/variables)
- Inline `tui-textarea` editor + `$EDITOR` shell-out (`E`)
- Hybrid `KvEditor` reused for Headers, Query, and Form bodies — Normal-mode nav (`j/k/x/a/d/m/r`), Insert-mode inline edit (`i`/`a`, `Tab` swap fields, `Enter` commit, `Esc` cancel)
- Dynamic vars resolved by `interpolate()` ahead of standard var lookup: `$now`, `$timestamp`, `$uuid`, `$ulid`, `$randomInt[(min,max)]`, `$base64('text')`, `$base64({{var}})`, `$randomString(n)`
- cURL import: `lazyfetch import-curl '<cmd>'` CLI + `:import curl` TUI popup. Parses Chrome/Firefox/Safari "Copy as cURL" outputs.
- cURL export: 4th option in Response `S` save dialog + dedicated `Y` (capital) key copies redacted cURL to clipboard.
- Repeat-last: `R` re-runs the last sent `Request` with current env (re-interpolated so `{{$uuid}}` re-rolls).

### Out of scope v0.2 (deferred)

Pre-request scripts, response tests/assertions, OAuth2 wiring, OpenAPI 3 import, mock server, collection runner, GraphQL introspection, response diffing, history viewer pane, WebSocket / gRPC, code generation beyond cURL, plugins, theme/keymap config, Insomnia/Bruno import, mTLS, proxy, cookie jar.

---

## §1 Workspace & files

```
crates/
├── core/src/
│   ├── catalog.rs              # Body enum gains GraphQL variant; BodyKind helper
│   ├── env.rs                  # interpolate() learns dyn-var lookup w/ recursion guard
│   ├── dynvars.rs              # NEW — pure dyn-var resolvers ($now, $uuid, $randomInt, $base64, $timestamp, $ulid, $randomString)
│   └── exec.rs                 # build_curl() for export
├── http/src/lib.rs             # ReqwestSender extends to Multipart (reqwest::multipart::Form) + GraphQL JSON
├── import/src/
│   ├── postman.rs
│   └── curl.rs                 # NEW — cURL command → Request parser, ImportReport
├── tui/src/
│   ├── editor.rs               # NEW — body editor state machine + $EDITOR shell-out
│   ├── kv_editor.rs            # NEW — hybrid Headers/Query/Form KV editor
│   ├── request_pane.rs         # NEW — Request pane render + dispatch
│   ├── commands.rs             # +run_curl_import, +run_repeat_last
│   ├── motion.rs               # unchanged
│   └── layout.rs               # delegates Request pane render to request_pane
└── bin/src/
    └── import_curl.rs          # NEW — `lazyfetch import-curl '<cmd>'` subcommand
```

### Bounded contexts

- **Dynamic vars** → `core::dynvars`. Pure. No IO. Takes `DynCtx { clock }`.
- **cURL parsing** → `import::curl`. Pure. Returns `(Request, ImportReport)`.
- **Body editor** → `tui::editor`. Owns `tui_textarea::TextArea` instances per kind. Knows nothing about KV.
- **KV editor** → `tui::kv_editor`. Owns row state, cursor, edit buffer. Knows nothing about Body.
- **Request pane** → `tui::request_pane`. Composes editor + kv_editor + tab badge. The only place that knows the layout.

### Dependency direction

`bin → tui → core ← {http, storage, auth, import}`. `core` IO-free invariant preserved — `dynvars` uses `Clock` port for `$now`/`$timestamp`, no `std::time`.

### New deps

- `tui-textarea = "0.7"` (already declared in v0.1 spec, not yet pulled)
- `uuid = { version = "1", features = ["v4"] }` (or reuse `ulid` for `$uuid` rendered as v4-shaped — adds `uuid` for spec accuracy)
- `rand = "0.8"` for `$randomInt` / `$randomString` (transitive already, made explicit)
- `base64 = "0.22"` already in `auth` crate — promoted to workspace dep

---

## §2 Domain types (core)

### `Body` extension

```rust
// core::catalog
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Body {
    #[default] None,
    Raw       { mime: String, text: String },
    Json(String),
    Form(Vec<KV>),
    Multipart(Vec<Part>),
    File(PathBuf),
    GraphQL { query: String, variables: String },   // NEW — variables = JSON text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind { None, Raw, Json, Form, Multipart, File, GraphQL }

impl Body {
    pub fn kind(&self) -> BodyKind { /* match */ }
}
```

### `core::dynvars`

```rust
pub struct DynCtx<'a> {
    pub clock: &'a dyn crate::ports::Clock,
}

#[derive(Debug, thiserror::Error)]
pub enum DynError {
    #[error("unknown dyn var: ${0}")] Unknown(String),
    #[error("arg parse failed for ${name}: {msg}")]
    ArgParse { name: String, msg: String },
    #[error("recursion limit hit for ${0}")] TooDeep(String),
    #[error("bounds invalid for ${name}: min={min} max={max}")]
    Bounds { name: String, min: i64, max: i64 },
}

pub fn resolve(name: &str, args: &[String], ctx: &DynCtx) -> Result<String, DynError>;
```

Built-ins (v0.2): `$now`, `$timestamp`, `$uuid`, `$ulid`, `$randomInt[(min,max)]`, `$base64('text'|{{var}})`, `$randomString(n)`.

### `interpolate()` extension

Hook in `core::env`:

1. Find `{{...}}` token (existing).
2. If inner trimmed starts with `$`: parse `name(args)` — name `[A-Za-z_][A-Za-z0-9_]*`, optional parenthesized comma-separated arg list. Each arg is `'literal'` / `"literal"` / `{{var}}` (recursively resolved first) / unquoted bareword.
3. Call `dynvars::resolve(name, args, ctx)`. On `Err(Unknown)` fall through to standard var lookup so user-defined env var named `$foo` still works (rare edge case).
4. **Recursion guard:** the recursive `{{var}}` resolution inside dyn-var args is depth-limited to **8**. Beyond → `DynError::TooDeep`.
5. Dyn-var output is **never** marked secret in `SecretRegistry` — values are non-deterministic and would defeat redaction lookup anyway.

---

## §3 Request pane — state & render

### Layout

```
┌ 3  Request ────────────────────────────────────────────────┐
│ ─ Body ─ Headers ─ Query ─ Auth ─                          │
│  [JSON ▾]                                                   │
│ ┌────────────────────────────────────────────────────────┐ │
│ │ {                                                      │ │
│ │   "name": "alice",                                     │ │
│ │   "id": "{{$uuid}}",                                   │ │
│ │   "ts": {{$timestamp}}                                 │ │
│ │ }                                                      │ │
│ └────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────┘
```

### State (additions to `AppState`)

```rust
pub req_tab: ReqTab,                          // Body | Headers | Query | Auth
pub req_body_kind: BodyKind,
pub body_mime: String,                        // for Body::Raw
pub body_editor: Option<tui_textarea::TextArea<'static>>,
pub graphql_editor: Option<(TextArea<'static>, TextArea<'static>)>,  // (query, variables)
pub headers_kv: KvEditor,
pub query_kv:   KvEditor,
pub form_kv:    KvEditor,                     // for BodyKind::Form / Multipart
pub last_sent: Option<Request>,               // for R repeat
```

### Tab keymap (Request focused, Normal mode)

- `1` Body, `2` Headers, `3` Query, `4` Auth
- `Space` cycles tabs
- `Tab` (when Body kind dropdown row focused) cycles `BodyKind`
- `i` / `a` enter editor (textarea or `KvEditor::InsertKey`)
- `E` (Body tab only) shell-out to `$EDITOR`
- `Esc` exits editor → Normal mode in pane

Method-color badge: `Body [JSON ▾]`. Color matches body kind: json=green, form=cyan, multipart=magenta, graphql=yellow, raw=gray.

---

## §4 Body editor

### Inline (`tui-textarea`)

- One `TextArea<'static>` per kind, lazy-built on first focus
- Line numbers gutter
- `syntect` JSON tokenization when `kind=Json`
- Auto-indent on `{` / `[` / newline (textarea built-in)
- Tab inserts 2 spaces (configurable in v0.4 config)
- Vim input mode by default (`tui-textarea` ships with this)
- `Ctrl-c` while editor focused never quits — intercepted, returns to pane Normal mode
- `Esc` returns to pane Normal mode without leaving the pane

### `$EDITOR` shell-out (`E`)

1. `terminal::TerminalGuard::suspend()` — leave alt-screen, disable raw mode
2. `tempfile::Builder::new().suffix(ext).tempfile_in(state.config_dir.join("tmp"))`
   - ext per kind: `.json`, `.txt`, `.graphql`, `.form`
3. Write current buf, `fsync`
4. `Command::new(env::var("EDITOR").unwrap_or("vi")).arg(temp.path()).status()`
5. Read file back, replace buffer
6. `Drop` guard cleans temp; `terminal::resume()` restores TUI on return (or panic)

### Multipart

Reuses `KvEditor` shape. Each row has a kind toggle: `T` (text) / `F` (file).

- File rows render as `📎 path/to/file.png` instead of plain value
- `f` key on cursor row → opens single-line file-path popup, value becomes `PartContent::File`

### GraphQL split editor

Two stacked `TextArea`s inside Body tab:

- Top: `Query` heading
- Bottom: `Variables (JSON)` heading
- `Tab` (when Body focused) cycles query → variables → query
- On send: serialized as `{"query": "<query>", "variables": <variables JSON>}` with `Content-Type: application/json`. If variables is empty/whitespace, omit the field.

### On send

```rust
match body_kind {
    BodyKind::None      => Body::None,
    BodyKind::Raw       => Body::Raw { mime: state.body_mime.clone(), text: editor.text() },
    BodyKind::Json      => Body::Json(editor.text()),
    BodyKind::Form      => Body::Form(state.form_kv.enabled_rows()),
    BodyKind::Multipart => Body::Multipart(parts_from_kv(&state.form_kv)?),
    BodyKind::GraphQL   => Body::GraphQL { query, variables },
    BodyKind::File      => Body::File(state.body_file.clone()),
}
```

`http` adapter extends `WireRequest` build:

- **Multipart:** uses `reqwest::multipart::Form`. Text rows → `Part::text()`. File rows → `Part::file().mime_str(detect_mime(path))`.
- **GraphQL:** body serialized to JSON before send.
- **Form:** existing urlencoded path.

All bodies run through `interpolate()` first so `{{var}}` and `{{$dynvar}}` work uniformly.

---

## §5 KV editor (Headers / Query / Form)

Single `KvEditor` reused 3×.

```rust
pub enum KvMode {
    Normal,
    InsertKey { row: usize },
    InsertValue { row: usize },
}

pub struct KvEditor {
    pub rows: Vec<KV>,
    pub cursor: usize,
    pub mode: KvMode,
    pub buf: String,                          // current edit buffer
    pub field_anchor: usize,                  // cursor col within buf
}
```

### Render

```
  [x] Authorization        Bearer {{TOKEN}}
▌ [x] Content-Type         application/json
  [ ] X-Trace-Id           {{$uuid}}
  + add header
```

- `▌` cursor row marker
- `[x]` / `[ ]` enabled toggle
- Width-balanced: key fixed 24-char, value flexible
- Disabled rows dimmed gray; enabled white
- Secret rows show value as `***` (toggle `r` to reveal — same semantics as Env pane)
- Last entry is always `+ add ...` placeholder; `Enter`/`a` on it acts like `a`

### Keys (Normal mode)

| Key | Action |
| --- | --- |
| `j` / `k` | row cursor |
| `x` | toggle enabled |
| `m` | toggle secret |
| `r` | reveal secret (transient, in-memory) |
| `d` | delete row |
| `a` | add new row → `InsertKey` |
| `i` | edit cursor row → `InsertKey` |
| `e` | popup-style edit modal (same as Env pane) |

### Insert modes

- Inline editing on the cursor row — value/key shown in-place with `▏` cursor
- `Tab` swaps `key ↔ value` field on same row
- `Enter` commits → back to Normal
- `Esc` cancels → discards buf, back to Normal
- Backspace deletes; printable chars append

### Validation

- Empty key on commit → toast `header key is empty` and stays in Insert
- Duplicate key → no error (HTTP allows multi-value), but a sigil (`²`) shown next to the second occurrence

### Persistence

Headers / Query saved as part of the `Request` via `FsCollectionRepo::save_request` — same path as today. `Ctrl-w` from Request pane saves the **full** state (URL + method + headers + query + body), not just URL+method.

---

## §6 Dynamic vars

| Token | Resolves to | Notes |
|---|---|---|
| `{{$now}}` | `2026-05-08T01:30:42Z` | RFC 3339 UTC via `chrono::Utc::now().to_rfc3339()` |
| `{{$timestamp}}` | `1778201442` | Unix seconds u64 |
| `{{$uuid}}` | `550e8400-...` | `uuid::Uuid::new_v4()` |
| `{{$ulid}}` | `01HX...` | `ulid::Ulid::new()` (already a dep) |
| `{{$randomInt}}` | `42` | full `u32` from `rand::thread_rng()` |
| `{{$randomInt(1,100)}}` | `73` | bounded — args parsed as `i64,i64`; min ≤ max enforced |
| `{{$base64('foo')}}` | `Zm9v` | literal arg encoded |
| `{{$base64({{TOKEN}})}}` | `<base64 of resolved TOKEN>` | nested var resolved first |
| `{{$randomString(16)}}` | `Xk2pQ7sLm...` | n-char alphanumeric |

### Parser

In `core::env::interpolate`:

1. Find `{{...}}` token.
2. If inner trimmed starts with `$`: split `name(args)` — `name = [A-Za-z_][A-Za-z0-9_]*`, optional `(...)` arg list.
3. Args: comma-split, each is `'literal'` / `"literal"` (raw string, no interpolation) / `{{var}}` (resolved first, recursion-limited) / unquoted bareword (treated as literal).
4. Pass to `dynvars::resolve(name, args, ctx)`.
5. On `Err(DynError::Unknown)` → fall through to standard var lookup.
6. On other `Err` → `MissingVar` w/ wrapped error message.

### Recursion guard

Depth limit 8. Tracked through a counter passed into recursive `interpolate` calls during arg resolution. Beyond → `DynError::TooDeep`.

### Secret tracking

Dyn-var output is never marked secret. Values are non-deterministic and don't round-trip in the `SecretRegistry` lookup model.

---

## §7 cURL import + export

### Import

**CLI:**
```bash
lazyfetch import-curl '<command>'
lazyfetch import-curl --file cmd.sh
echo 'curl ...' | lazyfetch import-curl --stdin
```

**TUI:** `:import curl` opens a centered popup with a multi-line input. Paste, Enter parses + loads into URL/method/headers/body. Result lands in the current Request state (does **not** auto-save — user can `Ctrl-w` to persist).

### Parser (`import::curl`)

```rust
pub fn parse(cmd: &str) -> Result<(Request, ImportReport), CurlError>;
```

Tokenizer handles:
- Optional `curl` prefix
- `\` line continuations stripped
- Single + double quotes (POSIX-style escapes inside)
- `$'...'` ANSI-C strings (Chrome devtools "copy as cURL (bash)")
- Bash heredocs

Flag table:

| Flags | Maps to |
| --- | --- |
| `-X`, `--request` | `Request.method` |
| `-H`, `--header` | append to `headers` |
| `-d`, `--data`, `--data-raw`, `--data-binary` | `Body::Raw` (or `Body::Json` if Content-Type indicates) |
| `--data-urlencode` | `Body::Form` row |
| `-F`, `--form` | `Body::Multipart` row (`-F 'file=@path'` → `PartContent::File`) |
| `-G`, `--get` | force GET, `-d` becomes query string |
| `-u`, `--user` | `AuthSpec::Basic { user, pass }` |
| `--url` / first non-flag arg | `Request.url` |
| `--cookie`, `-b` | header `Cookie:` |
| `-A`, `--user-agent` | header `User-Agent:` |
| `-e`, `--referer` | header `Referer:` |
| `--compressed` | header `Accept-Encoding: gzip, deflate, br` |
| `--max-redirs N` | `Request.max_redirects = N` |
| `-L`, `--location` | `follow_redirects = true` (default anyway) |
| `-k`, `--insecure` | warning, ignored (rustls strictness intentional) |
| `--proxy` | warning, ignored v0.2 |

Unknown flags → `ImportReport.warnings`. Request still imported.

### Export

**Response pane `S` save dialog adds option 4:**
```
1. Body only          → <name>.<ext>
2. Full response      → .http
3. cURL command       → .sh                   (already exists)
4. cURL → clipboard                            (NEW)
```

**Dedicated `Y` (capital) on Response pane:** copies cURL command to clipboard via `motion::copy_to_clipboard`. Toast: `cURL → clipboard (84 chars)`.

### `core::exec::build_curl`

```rust
pub fn build_curl(req: &WireRequest, reg: &SecretRegistry) -> String;
```

Always redacts via the `SecretRegistry` the request was sent with — never spits out raw bearer tokens.

---

## §8 Repeat-last-request

```rust
// AppState
pub last_sent: Option<Request>,    // pre-interpolation snapshot
```

Snapshot is the un-interpolated `Request` struct (with `{{var}}` placeholders intact). Re-running with `R` re-interpolates against the **current** env — so `{{$uuid}}` re-rolls and env switches take effect.

### Key

`R` (capital) — any pane, any mode (including `Insert` / `SaveAs` modals it bails out of). Bound at top-level dispatch alongside `F5`.

### Behavior

- No `last_sent` → toast: `nothing sent yet`
- `inflight` → toast: `send in progress`
- Otherwise → dispatch via existing `sender::dispatch` with the cached `Request`. URL bar + method are **not** repopulated. Toast: `replaying GET /users/42 …`

### Edge case

If the last request belonged to a now-deleted collection, the snapshot still works — the snapshot is a `Request` clone, not an index reference.

---

## §9 Errors, tests, phasing

### Error model

- `core::dynvars::DynError` — `Unknown(name)`, `ArgParse { name, msg }`, `TooDeep(name)`, `Bounds { name, min, max }`
- `import::curl::CurlError` — `Tokenize`, `Flag { which, msg }`, `MissingUrl`, `InvalidUtf8`. Returns `(Request, ImportReport)` on partial success.

UI surfaces all errors as toasts + `:messages` history (already wired in v0.1).

### Tests

| Crate | New tests | Count |
| --- | --- | --- |
| `core::dynvars` | each builtin happy path, unknown fallthrough, recursion guard, arg parse error, `randomInt` bounds proptest | 7 |
| `core::env` | interpolate w/ `$now`, w/ `$base64({{X}})`, recursion limit | 3 |
| `core::catalog` | `BodyKind::from(&Body)` round-trip | 1 |
| `import::curl` | golden fixtures from real Chrome / Firefox / Safari / plain bash "Copy as cURL" outputs | 12 |
| `tui::editor` | body kind switch preserves text per kind, `$EDITOR=cat` round-trip, multipart File row toggle | 3 |
| `tui::kv_editor` | Normal nav, Insert→Tab swap, Insert→Enter commit, `x` toggle, `d` delete | 5 |
| `tui::commands` | `repeat_last`, `curl_import` via stub state | 2 |
| `bin` (e2e) | spawn `lazyfetch import-curl '<cmd>'` against tempdir, verify file content | 1 |

Total: ~34 new tests (current 54 → ~88).

### CI

Existing `cargo test --workspace` + `clippy -D warnings` + `fmt --check` + `cargo deny` + core-purity grep guard. cURL fixture goldens committed to repo so changes show up in PR review.

### Phases (each phase ships independently, branch + merge)

1. `core::dynvars` + interpolate hook + 10 tests
2. `Body::GraphQL` variant + serializer in `core::exec` + `http` adapter
3. `tui::kv_editor` standalone module + 5 tests (no integration yet)
4. Body editor (`tui-textarea` wrap + `$EDITOR` shell-out) — Raw/JSON only first
5. Headers / Query tabs wired into Request pane using `kv_editor`
6. Multipart + Form bodies plug into `kv_editor` + multipart serializer in `http`
7. GraphQL split editor UI
8. cURL import parser + CLI subcommand + TUI popup
9. cURL export (button in `S` menu + `Y` key) + `build_curl` in core
10. Repeat-last (`R`)

Estimated 1,500-1,800 LOC + 34 tests. Each phase ~150 LOC, mergeable in a day.

---

## §10 Key new dependencies

| Crate | Purpose |
| --- | --- |
| `tui-textarea` 0.7 | inline body editor (vim-keys + JSON syntax) |
| `uuid` 1 (`v4`) | `$uuid` dyn var |
| `rand` 0.8 | `$randomInt`, `$randomString` |
| `base64` (workspace) | `$base64` dyn var (already in `auth`) |

No deferred items leak into v0.2 — all roadmap entries (OAuth2, OpenAPI, history viewer, …) stay on their original v0.3+ slots.
