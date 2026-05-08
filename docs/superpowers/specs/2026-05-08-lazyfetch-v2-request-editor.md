# lazyfetch v0.2 — Request editor + dyn vars + cURL interop

**Date:** 2026-05-08
**Status:** Draft (approved for plan, post-irondev review)
**Owner:** Ariel Onoriaga
**Anchor:** Make the Request pane self-sufficient — body editor (Raw/JSON/Form/Multipart/GraphQL), Headers/Query KV editors, dynamic vars, cURL import/export, repeat-last.

## §0 Goal

v0.1 ships a Request pane that is read-only. v0.2 closes the biggest UX gap: the user can build, edit, and persist a full Request from inside the TUI with zero context-switch. cURL import/export bridges the rest of the internet's HTTP examples. Dynamic vars (`{{$uuid}}`, `{{$now}}`, …) make retry/replay loops painless. Repeat-last (`R`) closes the dev inner loop.

### v0.2 scope

- Body editor: `Raw`, `Json`, `Form` (urlencoded), `Multipart` (with file fields), `GraphQL` (split query/variables)
- Inline `tui-textarea` editor + `$EDITOR` shell-out (`E`)
- Hybrid `KvEditor` reused for Headers, Query, and Form bodies — Normal-mode nav (`j/k/x/a/d/m/r`), Insert-mode inline edit (`i` for value, `a` adds row, `Tab` swap fields, `Enter` commit, `Esc` cancel)
- Dynamic vars resolved by `interpolate()` ahead of standard var lookup with formal grammar + recursion guard + secret tainting: `$now[(format)]`, `$timestamp`, `$uuid`, `$ulid`, `$randomInt[(min,max)]`, `$base64('text')`, `$base64({{var}})`, `$randomString(n)`
- cURL import: `lazyfetch import-curl '<cmd>'` CLI + `:import curl` TUI popup. Bash/zsh/POSIX cURL only. Parses Chrome (bash) / Firefox / Safari "Copy as cURL" outputs.
- cURL export: 4th option in Response `S` save dialog + dedicated `Y` (capital) key copies redacted cURL to clipboard.
- Repeat-last: `R` re-runs the last sent `Request` with current env (re-interpolated so `{{$uuid}}` re-rolls). Ignores current editor state.

### Out of scope v0.2 (deferred)

Pre-request scripts, response tests/assertions, OAuth2 wiring, OpenAPI 3 import, mock server, collection runner, GraphQL introspection, response diffing, history viewer pane, WebSocket / gRPC, code generation beyond cURL, plugins, theme/keymap config, Insomnia/Bruno import, mTLS, proxy, cookie jar, **cmd.exe-quoted cURL parsing**, **Auth tab editing** (read-only display in current spec also dropped — wired in v0.3 with OAuth2).

---

## §1 Workspace & files

```
crates/
├── core/src/
│   ├── catalog.rs              # Body enum gains GraphQL variant; BodyKind helper
│   ├── env.rs                  # interpolate() learns dyn-var lookup w/ recursion guard
│   ├── dynvars.rs              # NEW — pure dyn-var resolvers + arg grammar
│   └── exec.rs                 # build_curl() for export
├── http/src/lib.rs             # ReqwestSender extends to Multipart + GraphQL JSON
├── import/src/
│   ├── postman.rs
│   └── curl.rs                 # NEW — cURL command → Request parser, ImportReport
├── tui/src/
│   ├── editor.rs               # NEW — BodyEditorState enum + $EDITOR shell-out
│   ├── kv_editor.rs            # NEW — hybrid Headers/Query/Form/Multipart KV editor
│   ├── request_pane.rs         # NEW — Request pane render + dispatch
│   ├── commands.rs             # +run_curl_import, +run_repeat_last
│   ├── motion.rs               # unchanged
│   └── layout.rs               # delegates Request pane render to request_pane
└── bin/src/
    └── import_curl.rs          # NEW — `lazyfetch import-curl <cmd-or-file>` subcommand
```

### Bounded contexts

- **Dynamic vars** → `core::dynvars`. Pure. No IO. Takes `DynCtx { clock }`.
- **cURL parsing** → `import::curl`. Pure. Returns `(Request, ImportReport)`.
- **Body editor** → `tui::editor`. Owns `tui_textarea::TextArea` instances per kind via `BodyEditorState`. Knows nothing about KV.
- **KV editor** → `tui::kv_editor`. Owns row state, cursor, edit buffer. Shared by Headers / Query / Form / Multipart via a unified `KvRow`.
- **Request pane** → `tui::request_pane`. Composes editor + kv_editor + tab badge.

### Dependency direction

`bin → tui → core ← {http, storage, auth, import}`. `core` IO-free invariant preserved — `dynvars` uses `Clock` port for `$now`/`$timestamp`, no `std::time`.

### New deps

- `tui-textarea = "0.7"` (already in v0.1 spec, not yet pulled)
- `uuid = { version = "1", features = ["v4"] }` — for `$uuid` (v4 hex). `ulid` (already a dep) used separately for `$ulid`.
- `rand = "0.8"` for `$randomInt` / `$randomString`
- `base64` already in `auth` — promoted to workspace dep
- `mime_guess = "2"` for multipart File row Content-Type detection (extension-based, deterministic, no I/O on the file)

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
    #[serde(rename = "graphql")]
    GraphQL { query: String, variables: String },   // variables = JSON text
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind { None, Raw, Json, Form, Multipart, File, GraphQL }

impl Body {
    pub fn kind(&self) -> BodyKind { /* match */ }
}
```

The explicit `#[serde(rename = "graphql")]` overrides the snake_case default which would otherwise mangle the variant to `graph_q_l`.

### `core::dynvars`

```rust
pub struct DynCtx<'a> {
    pub clock: &'a dyn crate::ports::Clock,
}

#[derive(Debug, thiserror::Error)]
pub enum DynError {
    #[error("unknown dyn var: ${0}")] Unknown(String),
    #[error("syntax error parsing args of ${name}: {msg}")]
    ParseSyntax { name: String, msg: String },
    #[error("arg parse failed for ${name}: {msg}")]
    ArgParse { name: String, msg: String },
    #[error("recursion limit hit for ${0}")] TooDeep(String),
    #[error("bounds invalid for ${name}: min={min} max={max}")]
    Bounds { name: String, min: i64, max: i64 },
}

#[tracing::instrument(target = "lazyfetch::dynvars", skip(ctx), fields(name = %name))]
pub fn resolve(name: &str, args: &[Arg], ctx: &DynCtx) -> Result<String, DynError>;
```

`Arg` is the *resolved* (string-typed) argument value. The lexer/parser produces `Arg`s before `resolve` runs — separation between syntax errors and resolution errors.

### `interpolate()` extension

Hook in `core::env`:

```rust
pub struct Interpolated {
    pub value: String,
    pub used_secrets: SecretRegistry,
}
```

Algorithm:

1. Find `{{...}}` token (existing).
2. Trim inner whitespace.
3. If trimmed starts with `$`: parse `name(args)` per the grammar in §6.
4. Resolve each arg (single level only — see §6 recursion rules).
5. Call `dynvars::resolve(name, &args, ctx)`.
   - On `Err(DynError::Unknown)` → fall through to standard var lookup.
   - On other `Err` → propagate as `MissingVar(<wrapped>)`.
6. Collect each arg's `used_secrets` into a combined set. If non-empty, the dyn-var **output** is added to the outer `SecretRegistry` (secret tainting — see §6).

---

## §3 Request pane — state & render

### Layout

```
┌ 3  Request ────────────────────────────────────────────────┐
│ ─ Body ─ Headers ─ Query ─                                 │
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

Three tabs in v0.2: `Body` · `Headers` · `Query`. Auth tab dropped — wired in v0.3 with OAuth2.

### State (additions to `AppState`)

```rust
pub req_tab: ReqTab,                          // Body | Headers | Query
pub req_body_kind: BodyKind,
pub body_mime: String,                        // for Body::Raw

/// Single source of truth for the body editor — replaces the parallel
/// `body_editor: Option<TextArea>` + `graphql_editor: Option<(_, _)>` slots
/// from earlier draft. Switching kind builds the appropriate variant lazily.
pub body_editor: BodyEditorState,

pub headers_kv: KvEditor,
pub query_kv:   KvEditor,
pub form_kv:    KvEditor,                     // for BodyKind::Form / Multipart

/// cURL-import popup state. Active when `Mode == ImportCurl`.
pub import_curl_buf: String,
```

```rust
pub enum BodyEditorState {
    None,
    Single(tui_textarea::TextArea<'static>),
    Split {
        query: tui_textarea::TextArea<'static>,
        variables: tui_textarea::TextArea<'static>,
        focus: GraphQlFocus,                  // Query | Variables
    },
}
```

```rust
pub enum Mode {
    Normal, Command, Insert, Search, SaveAs, Rename, Move,
    ImportCurl,                                // NEW — cURL paste popup
}
```

### Tab keymap (Request focused, Normal mode)

- `1` Body, `2` Headers, `3` Query
- `Space` cycles tabs
- `Tab` (when Body kind dropdown row focused) cycles `BodyKind`
- `i` / `a` enter editor (textarea or `KvEditor::InsertKey`)
- `E` (Body tab only) shell-out to `$EDITOR`
- `Esc` exits editor → Normal mode in pane

Method-color badge: `Body [JSON ▾]`. Color matches body kind: json=green, form=cyan, multipart=magenta, graphql=yellow, raw=gray.

---

## §4 Body editor

### Inline (`tui-textarea`)

- Lazy-built per kind via `BodyEditorState`
- Line numbers gutter
- `syntect` JSON tokenization when `kind=Json`
- Auto-indent on `{` / `[` / newline (textarea built-in)
- Tab inserts 2 spaces (configurable in v0.4 config)
- Vim input mode by default
- `Ctrl-c` while editor focused returns to pane Normal mode (does not quit)
- `Esc` returns to pane Normal mode without leaving the pane

### `$EDITOR` shell-out (`E`) — terminal restoration discipline

Two distinct guards, stack-allocated, drop in reverse order on panic:

```rust
struct TerminalSuspendGuard<'a> { term: &'a mut TerminalGuard }
impl Drop for TerminalSuspendGuard<'_> {
    fn drop(&mut self) {
        // Always restore even if the spawn / read panicked.
        let _ = self.term.resume();
    }
}

let _scratch = tempfile::Builder::new()
    .prefix("lazyfetch-")
    .suffix(ext_for(kind))                    // .json / .txt / .graphql / .form
    .tempfile_in(scratch_dir())?;             // see below
let _suspend = {
    state.terminal.suspend()?;
    TerminalSuspendGuard { term: &mut state.terminal }
};

// write buf → scratch → fsync
// Command::new(env::var("EDITOR").unwrap_or_else(|_| "vi".into()))
//     .arg(scratch.path()).status()?;
// read scratch back → replace buffer
// _suspend dropped here → terminal::resume()
// _scratch dropped here → file unlinked
```

Both guards are *unconditional* — terminal is always restored, scratch is always cleaned, regardless of whether the editor exited cleanly, was killed, or panicked.

### Scratch directory

Tempfiles for `$EDITOR` go to `XDG_RUNTIME_DIR` if set, else `std::env::temp_dir()`. **Not** `state.config_dir.join("tmp")` — config dir is settings, not scratch. (v0.1 spec §3 already established this; following the established pattern.)

### Multipart

Reuses the same `KvRow` struct as Headers/Query/Form. Each row has a `kind: KvRowKind = Text | File`.

- Multipart consumers respect `kind`; Headers / Query / Form ignore the field (always `Text`).
- File rows render as `📎 path/to/file.png` in the value column.
- `f` key on cursor row → opens single-line file-path popup, sets `kind = File`.
- On send: `Part::text(value)` for Text rows, `Part::file(path).mime_str(mime_guess::from_path(path).first_or_octet_stream().essence_str())` for File rows.

### GraphQL split editor

`BodyEditorState::Split { query, variables, focus }`. Two stacked TextAreas inside Body tab. `Tab` (in Body tab) cycles focus query → variables → query. On send: `Body::GraphQL { query, variables }` serialized as `{"query": "<q>", "variables": <vars JSON>}` with `Content-Type: application/json`. Empty/whitespace `variables` → field omitted from JSON.

### On send

```rust
match body_kind {
    BodyKind::None      => Body::None,
    BodyKind::Raw       => Body::Raw { mime: state.body_mime.clone(), text: editor.text() },
    BodyKind::Json      => Body::Json(editor.text()),
    BodyKind::Form      => Body::Form(state.form_kv.enabled_text_rows()),
    BodyKind::Multipart => Body::Multipart(parts_from_kv(&state.form_kv)?),
    BodyKind::GraphQL   => Body::GraphQL { query, variables },
    BodyKind::File      => Body::File(state.body_file.clone()),
}
```

All bodies run through `interpolate()` first so `{{var}}` and `{{$dynvar}}` work uniformly.

---

## §5 KV editor (Headers / Query / Form / Multipart)

```rust
pub enum KvMode {
    Normal,
    InsertKey { row: usize },
    InsertValue { row: usize },
}

pub enum KvRowKind { Text, File }

pub struct KvRow {
    pub kind: KvRowKind,
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub secret: bool,
}

pub struct KvEditor {
    pub rows: Vec<KvRow>,
    pub cursor: usize,
    pub mode: KvMode,
    pub buf: String,
    pub cursor_col: usize,                    // cursor position within `buf`
}
```

`cursor_col` was previously named `field_anchor` — renamed to match what it actually is.

### Render

```
  [x] Authorization        Bearer {{TOKEN}}
▌ [x] Content-Type         application/json
  [ ] X-Trace-Id           {{$uuid}}
```

- `▌` cursor row marker
- `[x]` / `[ ]` enabled toggle
- Width-balanced: key fixed 24-char, value flexible
- Disabled rows dimmed gray; enabled white
- Secret rows show value as `***` (toggle `r` to reveal — same as Env pane)
- **No sentinel `+ add` row** — `a` adds + jumps to InsertKey directly

### Keys (Normal mode)

| Key | Action |
| --- | --- |
| `j` / `k` | row cursor |
| `x` | toggle enabled |
| `m` | toggle secret |
| `r` | reveal secret (transient) |
| `d` | delete row |
| `a` | **add new row** at end → `InsertKey` |
| `i` | **edit value** of cursor row → `InsertValue` |
| `e` | popup-style edit modal (same as Env pane `e`) |
| `f` | (Multipart only) toggle `kind` Text↔File on cursor row |

`i` and `a` now have distinct meanings: `i` edits, `a` adds. Vim-aligned.

### Insert modes

- Inline editing on the cursor row — value/key shown in-place with `▏` cursor
- `Tab` swaps `key ↔ value` field on same row
- `Enter` commits → back to Normal
- `Esc` cancels → discards buf, back to Normal
- Backspace deletes; printable chars append

### Validation

- Empty key on commit → toast `header key is empty` and stays in Insert
- Duplicate key → no error (HTTP allows multi-value), sigil (`²`) shown next to second occurrence

### Persistence

Headers / Query saved as part of the `Request` via `FsCollectionRepo::save_request` (existing). `Ctrl-w` from Request pane saves the **full** state (URL + method + headers + query + body), not just URL+method.

---

## §6 Dynamic vars

### Built-in table

| Token | Resolves to | Notes |
|---|---|---|
| `{{$now}}` | `2026-05-08T01:30:42Z` | RFC 3339 UTC via `clock.now().to_rfc3339()` |
| `{{$now('rfc2822')}}` | `Fri, 08 May 2026 01:30:42 +0000` | named format alias |
| `{{$now('%Y-%m-%d')}}` | `2026-05-08` | chrono format string (literal single-quoted arg) |
| `{{$timestamp}}` | `1778201442` | Unix seconds u64 |
| `{{$uuid}}` | `550e8400-e29b-41d4-a716-446655440000` | `uuid::Uuid::new_v4()` (v4 hex) |
| `{{$ulid}}` | `01HX...` | `ulid::Ulid::new()` (Crockford base32) |
| `{{$randomInt}}` | `42` | full `u32` from `rand::thread_rng()` |
| `{{$randomInt(1,100)}}` | `73` | bounded `i64,i64`; `min ≤ max` enforced — else `Bounds` |
| `{{$base64('foo')}}` | `Zm9v` | literal arg encoded |
| `{{$base64({{TOKEN}})}}` | `<base64 of resolved TOKEN>` | nested var resolved first; **secret-tainted** if TOKEN was secret |
| `{{$randomString(16)}}` | `Xk2pQ7sLm0vBnW9R` | n-char from alphabet `[A-Za-z0-9]`; `n` capped at 1024 |

### Argument grammar (formal)

```
ArgList   := Arg ( ',' Arg )*
Arg       := Quoted | VarRef | Bareword
Quoted    := "'" QChar* "'"   |   '"' QChar* '"'
QChar     := <any char except backslash and matching quote> | "\\" EscChar
EscChar   := "'" | '"' | "\\" | "n" | "r" | "t"
VarRef    := "{{" Ident "}}"                  ;; one level of var lookup, no recursion
Bareword  := <chars not in: , ( ) { } ' "  whitespace>+
```

Errors → `DynError::ParseSyntax { name, msg }` distinct from `ArgParse` (semantic — e.g. `randomInt('abc', 5)`).

### Recursion / depth invariants (locked)

- **`{{var}}` arg resolution does exactly one level of var lookup.** The looked-up *value* is **not** recursively interpolated. (Stock interpolate behaviour preserved — values are literal.)
- **Recursion depth 8** applies *only* to nested dyn-vars, e.g. `{{$base64({{$base64('x')}})}}`. Tracked via a counter passed through arg evaluation.
- Beyond depth 8 → `DynError::TooDeep`.

### Secret tainting

A dyn-var output is tainted secret if any of its arg's `used_secrets` registry was non-empty.

```rust
fn eval_dynvar(name, raw_args, ctx, depth) -> (String, SecretRegistry) {
    let mut combined_secrets = SecretRegistry::new();
    let mut resolved_args = vec![];
    for raw in raw_args {
        let Interpolated { value, used_secrets } = resolve_arg(raw, ctx, depth + 1)?;
        combined_secrets.extend(&used_secrets);
        resolved_args.push(value);
    }
    let out = dynvars::resolve(name, &resolved_args, ctx)?;
    let mut out_secrets = combined_secrets;
    if !out_secrets.is_empty() {
        out_secrets.insert(out.clone());      // taint output as secret
    }
    Ok((out, out_secrets))
}
```

This closes a real leak: `{{$base64({{TOKEN}})}}` would otherwise produce an unredacted base64-of-secret in cURL exports / history / log.

### Auth-spec dyn-var rule (locked)

`Bearer.token`, `Basic.pass`, `ApiKey.value`, `OAuth2.client_secret` templates that contain *only* dyn-var references (no `{{var}}` env reference, no plain text) are **rejected at apply time** with `AuthError::DynVarOnlyInSecretField`. A token that re-rolls every request is broken auth, not a feature. Test added.

### Tracing

`#[tracing::instrument(target = "lazyfetch::dynvars", skip(ctx), fields(name = %name))]` on `resolve` so users can debug "why is my `{{$randomInt(5,1)}}` failing" via `RUST_LOG=lazyfetch::dynvars=trace`.

### Tests (`core::dynvars`)

- 7 happy-path: each builtin
- Unknown fallthrough: `{{$xyz}}` falls through to standard var lookup
- Recursion: `{{$base64({{$base64('x')}})}}` works at depth 2; depth >8 → `TooDeep`
- Bounds: `{{$randomInt(10,5)}}` → `Bounds`
- ParseSyntax: unbalanced quotes, unbalanced parens, comma in bareword, escape outside quotes — 5 negative tests
- Property: `randomInt(min,max)` always in `[min,max]` (proptest, 1000 iters)
- Auth-spec rejection: `Bearer { token: "{{$randomString(32)}}" }` → `AuthError::DynVarOnlyInSecretField`
- Secret taint: `{{$base64({{TOKEN}})}}` w/ TOKEN=secret → output appears in `used_secrets`
- `$now('%Y-%m-%d')` parses via chrono format string

---

## §7 cURL import + export

### Scope

**In:** bash / zsh / POSIX-shell-quoted cURL. Examples: Chrome "Copy as cURL (bash)", Firefox, Safari, plain bash.

**Out:** cmd.exe / PowerShell quoting (`^"` / `\""` escape forms). Detected → `CurlError::Tokenize { msg: "cmd.exe quoting not supported; use Copy as cURL (bash)" }`.

### Import CLI

```bash
lazyfetch import-curl '<curl command>'      # positional: starts w/ 'curl' or '-' → command
lazyfetch import-curl path/to/cmd.sh        # positional: starts w/ '/' or no leading flag → file
echo 'curl ...' | lazyfetch import-curl     # stdin if no positional
```

Heuristic: positional arg `arg`. If `arg` is a path that exists → file. If `arg` starts with `curl` or `-` → command. Else → file (with helpful error if not found).

### TUI popup

`:import curl` → centered popup w/ multi-line input. State: `Mode::ImportCurl` + `import_curl_buf: String`. Keys: typing appends, `Enter` (without modifier) submits, `Shift-Enter` inserts newline, `Esc` cancels. Submit parses + loads into URL/method/headers/body. Does **not** auto-save — user can `Ctrl-w` to persist.

### Parser (`import::curl`)

```rust
pub fn parse(cmd: &str) -> Result<(Request, ImportReport), CurlError>;
```

Tokenizer:
- Optional `curl` prefix
- `\` line continuations stripped
- Single + double quotes (POSIX escapes inside: `\'`, `\"`, `\\`, `\n`, `\r`, `\t`)
- `$'...'` ANSI-C strings (Chrome devtools "copy as cURL (bash)")
- Bash heredocs

### Flag table

| Flags | Maps to |
| --- | --- |
| `-X`, `--request` | `Request.method` |
| `-H`, `--header` | append to `headers` |
| `-d`, `--data`, `--data-raw`, `--data-binary` | body text (kind chosen by Content-Type rule below) |
| `--data-urlencode` | `Body::Form` row |
| `-F`, `--form` | `Body::Multipart` row (`-F 'file=@path'` → `KvRowKind::File`) |
| `-G`, `--get` | force GET, accumulated `-d` becomes query string |
| `-u user[:pass]` / `--user` | `AuthSpec::Basic { user, pass }` (empty pass + warning if no `:`) |
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

### Content-Type / body-kind resolution rule

After parsing all flags:

1. The *last* `-H Content-Type:` value is the canonical Content-Type.
2. Body kind decided by:
   - `application/json` → `Body::Json(text)`
   - `application/x-www-form-urlencoded` → `Body::Form` (collected from `-d` / `--data-urlencode` rows)
   - `multipart/form-data` → `Body::Multipart` (from `-F` rows)
   - Anything else (incl. unset) → `Body::Raw { mime, text }` where `mime` is the Content-Type or `text/plain`
3. If `-G` is present and there's body text, body becomes empty and text is appended to URL as query string.

Deterministic regardless of flag order.

### Export

**Response pane `S` save dialog adds option 4:**
```
1. Body only          → <name>.<ext>
2. Full response      → .http
3. cURL command       → .sh                   (file)
4. cURL → clipboard                            (NEW)
```

**Dedicated `Y` (capital) on Response pane:** copies cURL to clipboard via `motion::copy_to_clipboard`. Toast: `cURL → clipboard (84 chars)`.

### `core::exec::build_curl`

```rust
pub fn build_curl(req: &WireRequest, reg: &SecretRegistry) -> String;
```

Always redacts via the `SecretRegistry` the request was sent with — never spits out raw bearer tokens.

**Quoting / escape rules (locked):**
- Each header / data arg wrapped in single quotes.
- Inner single-quote replaced by `'\''` (POSIX-portable single-quote escape inside single-quoted strings).
- URL escaped the same way.
- No double-quoting (avoids shell-variable expansion in zsh).
- Newlines in header values rendered as `\n` literal (preserves single-quoted form).

Test fixture: header value `O'Brien` round-trips as `'O'\''Brien'`.

---

## §8 Repeat-last-request

### Snapshot lives on `Executed`

```rust
// core::exec::Executed
pub struct Executed {
    pub request_template: Request,            // pre-interpolation snapshot (NEW)
    pub request_snapshot: WireRequest,        // post-interpolation, redacted
    pub response: WireResponse,
    pub at: DateTime<Utc>,
    pub secrets: SecretRegistry,
}
```

Single source of truth — no separate `last_sent` field on `AppState`. The "last sent" Request is always `state.last_response.as_ref().map(|e| &e.request_template)`.

### Key

`R` (capital) — any pane, any mode (including `Insert` / `SaveAs` modals it bails out of). Bound at top-level dispatch alongside `F5`.

### Behavior (locked)

- No `last_response` → toast: `nothing sent yet`
- `inflight` → toast: `send in progress`
- Otherwise → dispatch via existing `sender::dispatch` with a clone of `last_response.request_template`. Re-interpolated against the **current** env so `{{$uuid}}` re-rolls and env switches take effect.
- **URL bar + method + body + headers in the editor are NOT touched.** R replays the snapshot, ignoring anything the user is currently editing. Toast: `replaying GET /users/42 — your edits are unchanged`.
- If the user wants to send their edited request, they press `s` / `F5`.

### Edge case

If the last request belonged to a now-deleted collection, `request_template` is a `Request` clone, not an index reference — still works.

---

## §9 Errors, tests, phasing

### Error model

- `core::dynvars::DynError` — `Unknown(name)`, `ParseSyntax { name, msg }`, `ArgParse { name, msg }`, `TooDeep(name)`, `Bounds { name, min, max }`
- `core::auth::AuthError` adds `DynVarOnlyInSecretField { template }`
- `import::curl::CurlError` — `Tokenize { msg }`, `Flag { which, msg }`, `MissingUrl`, `InvalidUtf8`. Returns `(Request, ImportReport)` on partial success.

UI surfaces all errors as toasts + `:messages` history (already wired in v0.1).

### Tests

| Crate | New tests | Count |
| --- | --- | --- |
| `core::dynvars` | each builtin, unknown fallthrough, recursion guard, bounds, 5 negative parse-syntax, randomInt proptest, secret taint, `$now` format alias | ~14 |
| `core::env` | interpolate w/ `$now`, w/ `$base64({{X}})`, recursion limit, secret-taint propagation through interpolate | 4 |
| `core::auth` | DynVarOnlyInSecretField rejection on Bearer / Basic / ApiKey / OAuth2.client_secret | 4 |
| `core::catalog` | `BodyKind::from(&Body)` round-trip, `Body::GraphQL` serde rename to `"graphql"` | 2 |
| `core::exec` | `build_curl` redacts, single-quote escape (`O'Brien`) | 2 |
| `import::curl` | golden fixtures: Chrome bash, Firefox, Safari, plain bash, multipart `-F`, `--data-urlencode`, `-G`, `-u user:pass`, `-u user`, cmd-shell rejected, last-Content-Type wins, ANSI-C string | 12 |
| `tui::editor` | body kind switch preserves text per kind, `$EDITOR=cat` round-trip + restore-on-panic, multipart File row toggle | 3 |
| `tui::kv_editor` | Normal nav, `i` edits value, `a` adds row, Insert→Tab swap, Insert→Enter commit, `x` toggle, `d` delete | 7 |
| `tui::commands` | `repeat_last` ignores edits, `curl_import` via stub state | 2 |
| `bin` (e2e) | spawn `lazyfetch import-curl '<cmd>'` against tempdir, verify file content | 1 |

Total: ~51 new tests (current 54 → ~105).

### CI

Existing `cargo test --workspace` + `clippy -D warnings` + `fmt --check` + `cargo deny` + core-purity grep guard. cURL fixture goldens committed via `insta`.

### Phases (each phase ships independently, branch + merge)

1. **`core::dynvars` + interpolate hook + secret taint + ~14 tests** (foundation — everything else uses this)
2. `Body::GraphQL` variant + `core::exec::build_curl` + `http` adapter Multipart serializer + 4 tests
3. **`tui::kv_editor` standalone module + Headers/Query tabs wired into Request pane** (phases 3+5 merged per review — KvEditor without consumers is dead code)
4. Body editor (`tui-textarea` wrap + `$EDITOR` shell-out w/ TerminalSuspendGuard) — Raw/JSON only first
5. Multipart + Form bodies plug into `kv_editor` w/ Text/File row kind + multipart serializer in `http`
6. GraphQL split editor UI
7. cURL import parser + CLI subcommand + TUI popup (`Mode::ImportCurl`) + 12 fixture tests
8. cURL export (`Y` key + 4th option in `S` menu) + `build_curl` quoting tests
9. Repeat-last (`R`) + `Executed.request_template` field

Estimated **~2,500 LOC** + 51 tests. Each phase ~250 LOC, mergeable in 1-2 days.

---

## §10 Key new dependencies

| Crate | Purpose |
| --- | --- |
| `tui-textarea` 0.7 | inline body editor (vim-keys + JSON syntax) |
| `uuid` 1 (`v4`) | `$uuid` dyn var (v4 hex) |
| `rand` 0.8 | `$randomInt`, `$randomString` |
| `base64` (workspace) | `$base64` dyn var (already in `auth`) |
| `mime_guess` 2 | multipart File row Content-Type (extension-based) |
