<div align="center">

# lazyfetch

<a href="https://github.com/arielonoriaga/lazyfetch">
  <img src="https://readme-typing-svg.demolab.com?font=JetBrains+Mono&weight=600&size=18&duration=3500&pause=1200&color=58A6FF&center=true&vCenter=true&width=620&lines=Postman+in+your+terminal.;Vim+keys.+No+Electron.+No+account.;YAML+collections+%E2%80%94+git-friendly+by+design.;Hexagonal+Rust.+IO-free+core.+54+tests+green." alt="typing SVG" />
</a>

**A terminal-first HTTP client. Sibling to `lazygit` and `lazydocker`.**
Send requests, manage collections + envs, vim-navigate the JSON response, import from Postman — all without leaving the keyboard.

<p>
  <img alt="Rust" src="https://img.shields.io/badge/rust-stable-orange?style=flat-square&logo=rust" />
  <img alt="ratatui" src="https://img.shields.io/badge/TUI-ratatui-58A6FF?style=flat-square" />
  <img alt="reqwest" src="https://img.shields.io/badge/HTTP-reqwest%20%2B%20rustls-009688?style=flat-square" />
  <img alt="tokio" src="https://img.shields.io/badge/async-tokio-369?style=flat-square&logo=tokio" />
  <img alt="license" src="https://img.shields.io/badge/license-MIT-yellow?style=flat-square" />
  <img alt="status" src="https://img.shields.io/badge/status-v0.1%20alpha-FF5D01?style=flat-square" />
</p>

| 🦀 7 crates | ✅ 54 tests | 🧪 IO-free core | 🔐 secret redaction | 🖱️ mouse + vim |
|:---:|:---:|:---:|:---:|:---:|
| Hexagonal workspace | wiremock + insta + proptest | `cargo deny` + grep guard | unified across log / save / history / yank | clicks + `hjkl` + `vy` |

</div>

---

## Why

Postman and Insomnia are powerful but heavy: GUI app, account, cloud sync you didn't ask for, opaque storage. The `lazy*` family proved that terminal-native UX wins for developer tools. `lazyfetch` does the same for HTTP.

```
┌ 1  Collections ──────────┬ 2  URL ────────────────────────────────────┐
│ ▾ api (3)                │ GET     {{API_URL}}/users/{{id}}▏          │
│   ✓ GET    list          ├ 3  Request ────────────────────────────────┤
│ ▌ ✓ GET    get           │                                             │
│     POST   create        │ (no request open)                           │
│ ▸ stripe (5)             │                                             │
├ 5  Environment ──────────├ 4  Response ───────────────────────────────┤
│ [dev]  (1/2 envs)        │ 200 · 142ms · 1.4 KiB · json                │
│ ▌ API_URL = https://...  │                                             │
│   🔒 TOKEN = ***         │ {                                           │
│                          │   "users": [                                │
│                          │     { "id": 1, "name": "alice" }            │
│                          │   ]                                         │
└──────────────────────────┴─────────────────────────────────────────────┘
 NORMAL  Tab cycle · ? help · :messages · q quit
```

---

## Install

```bash
git clone git@github.com:arielonoriaga/lazyfetch.git
cd lazyfetch
cargo install --path crates/bin
```

Requires Rust stable (≥ 1.85). No system deps — `rustls` everywhere, no OpenSSL. Optional clipboard helpers: `wl-copy` (Wayland) / `xclip` (X11) / `pbcopy` (macOS) / `clip.exe` (Windows).

---

## Use

### Interactive TUI

```bash
lazyfetch
```

Press `?` for help anywhere — it's a filterable popup.

### Headless CLI

```bash
lazyfetch run my-api/users/list --env dev               # send a saved request
lazyfetch run my-api/users/get --env dev --set id=42    # override a var
lazyfetch import-postman ./collection.json              # → global config
lazyfetch import-postman ./collection.json --local      # → ./.lazyfetch
```

### Project-local collections (`.lazyfetch/`)

Drop a `.lazyfetch/` directory next to your `.git/`. lazyfetch walks up from your cwd and uses the nearest match:

```
my-app/
├── .git/
├── .lazyfetch/                    ← discovered automatically
│   ├── collections/
│   │   └── api/
│   │       ├── collection.yaml
│   │       └── requests/
│   │           ├── health.yaml
│   │           └── users/
│   │               └── list.yaml
│   └── environments/
│       ├── dev.yaml
│       └── prod.yaml
└── src/
```

Resolution: `--config-dir` flag → nearest `.lazyfetch/` ancestor → `~/.config/lazyfetch/`. Commit `.lazyfetch/` next to your code; every contributor gets the same requests + envs.

---

## Panes

Five panes in a 2x3 grid (URL bar spans the right top row):

| # | Pane | Purpose |
|---|---|---|
| 1 | **Collections** | Tree of saved requests grouped by collection |
| 2 | **URL** | Method badge + URL template (`{{var}}` autocomplete on `{{`) |
| 3 | **Request** | Headers, query, body editor *(v0.2)* |
| 4 | **Response** | Status, body, vim-navigable, JSON-colorized, search/filter |
| 5 | **Environment** | Active env vars w/ secret-aware reveal toggle |

`Tab` / `Shift-Tab` cycle. `1`–`5` jump directly. `h`/`j`/`k`/`l` and arrows move spatially when not on Response. Click any pane to focus.

---

## Keys

### Universal

| Key | Action |
|---|---|
| `?` | toggle filterable help popup |
| `:` | command mode |
| `q` / `Ctrl-c` | quit |
| `F5` | **send request** (any pane / any mode) |
| `Ctrl-s` | send |
| `Ctrl-w` | save URL+method as request → popup |
| `R` | repeat last sent request (snapshot replay; ignores edits) |
| `Y` | (Response pane) copy redacted cURL to clipboard |
| `:messages` | scrollable history of all toasts (last 64) |

### URL bar (pane 2)

| Key | Action |
|---|---|
| typing | edit URL |
| `Enter` | send |
| `Alt-↑` / `Alt-↓` | cycle HTTP method (GET → POST → PUT → PATCH → DELETE → HEAD → OPTIONS) |
| `:method DELETE` | set method by name |
| `{{` | open variable autocomplete; `↑`/`↓` pick, `Tab`/`Enter` accept |

### Response pane (pane 4) — vim-style

| Key | Action |
|---|---|
| `j` / `k` / `↓` / `↑` | line up/down |
| `h` / `l` / `←` / `→` | char left/right |
| `0` / `$` | line start / end |
| `w` / `b` | word forward / back |
| `Ctrl-d` / `Ctrl-u` | half page |
| `Ctrl-f` / `Ctrl-b` | full page |
| `gg` / `G` | top / bottom |
| `{` / `}` | prev / next blank line |
| `H` / `M` / `L` | viewport top / mid / bottom |
| `%` | matching brace `{ } [ ]` |
| `]` / `[` | next / prev sibling block (same indent) |
| `v` | toggle visual select |
| `y` | yank selection (or line) → clipboard |
| `/` | search; `n` / `N` next/prev match |
| **left-click** | move cursor to clicked cell |
| **scroll wheel** | scroll cursor ±3 lines |

JSON bodies are color-coded (keys cyan, strings green, numbers magenta, bool yellow, null red). Status line: `200 · 142ms · 1.4 KiB · json`.

### Environment pane (pane 5)

| Key | Action |
|---|---|
| `j` / `k` | row cursor |
| `a` / `A` | add variable / secret variable (popup) |
| `e` | edit selected (popup, pre-filled) |
| `d` | delete selected |
| `m` | toggle secret flag |
| `r` | reveal / hide secret value (transient, in-memory only) |
| `:env <name>` | switch active environment |
| `:newenv <name>` | create new environment |

### Request pane (pane 3)

Three tabs: `1` Body · `2` Headers · `3` Query · `Space` cycles.

| Key | Action |
|---|---|
| `Tab` (Body tab) | cycle body kind (`None → Raw → Json → Form → Multipart → GraphQL → File → None`) |
| `j` / `k` / `↓` / `↑` | row cursor (Headers / Query / Form / Multipart) |
| `a` | add new row |
| `i` | edit value of cursor row |
| `x` | toggle row enabled |
| `d` | delete row |
| `m` | toggle row secret |
| `f` | toggle Text↔File (Multipart only) |
| **insert mode** typing | edit cell |
| `Tab` (insert) | swap key↔value |
| `Enter` (insert) | commit row (rejects empty key) |
| `Esc` (insert) | cancel; freshly-added row drops |

### Collections pane (pane 1)

| Key | Action |
|---|---|
| `j` / `k` | row cursor |
| `Space` | expand / collapse collection |
| `Enter` | open request → loads URL + method into URL bar |
| `r` | rename collection / request (popup) |
| `x` | mark / unmark request |
| `M` | move marked (or cursor) requests → another collection (popup) |

---

## Variables

`{{var}}` placeholders interpolate at send time, scoped:

```
--set k=v         (CLI override, highest)
   ↓
environments/<env>.yaml   (--env flag picks one; or `:env` in TUI)
   ↓
collection.yaml vars       (default fallback)
   ↓
MissingVar error           (no match)
```

**Secret discipline.** Variables flagged `secret: true` flow through a single `SecretRegistry`. Every output surface — history, raw-view toggle, save dialog, log sinks, clipboard yank — runs through one redactor. Auth fields (`Bearer.token`, `Basic.pass`, `ApiKey.value`, `OAuth2.client_secret`) are **rejected at apply time** if their template references a non-secret variable.

Env files are saved with `0600` permissions on Unix.

---

## Storage

```
~/.config/lazyfetch/             (or .lazyfetch/ in your project)
├── config.yaml
├── collections/
│   └── my-api/
│       ├── collection.yaml          # name, vars, auth
│       └── requests/
│           ├── _folder.yaml
│           ├── ping.yaml            # one Request = one file
│           └── users/
│               ├── _folder.yaml
│               ├── list.yaml
│               └── get.yaml
└── environments/
    ├── dev.yaml                     # 0600 on Unix
    └── prod.yaml
```

YAML is hand-editable, diff-friendly, comment-friendly. Each `Request` is its own file so a 200-request collection produces 200 small diffs.

History: `~/.local/share/lazyfetch/history.jsonl` (append-only, `fd-lock` guarded).

---

## Architecture

Hexagonal Cargo workspace. `core` is pure domain — no `tokio`, no `std::fs`, no network. Adapters live in their own crates. CI greps `core` for IO calls and fails on hits.

```
bin → tui → core ← { http, storage, auth, import }
```

| Crate | Responsibility |
|---|---|
| `core` | `Collection`, `Request`, `AuthSpec`, `WireRequest`, ports (`HttpSender`, `AuthCache`, `Clock`, `Browser`, `Editor`), `interpolate()`, `execute()`, `redact_wire()` |
| `http` | `reqwest` adapter, redirect policy, error mapping |
| `storage` | YAML collections (file-per-Request), env round-trip with 0600 perms, JSONL history with `fd-lock`, atomic write (same-dir tempfile + `Drop` guard), rename / move helpers with collision detection |
| `auth` | `Bearer` / `Basic` / `ApiKey` resolvers with secret-only validation. OAuth2 stubbed for v0.3. |
| `import` | Postman v2.1 → core types, `ImportReport` warnings, DoS-bound parser |
| `tui` | `ratatui` + `crossterm`, alt-screen + raw-mode `Drop` guard, 5 panes + 5 modal popups, mouse + vim navigation, search, JSON colorizer |
| `bin` | composition root + CLI (`run`, `import-postman`, `import-curl`) |

---

## v0.2 — Request editor (shipped on `feat/v2-impl`)

### Request pane (Focus → `3`)

Three tabs. `1` Body · `2` Headers · `3` Query · `Space` cycles. Tab on Body cycles body kind: `None → Raw → Json → Form → Multipart → GraphQL → File`.

KV editor (Headers · Query · Form · Multipart): `j`/`k` move · `a` add row · `i` edit value · `x` enable/disable · `d` delete · `m` toggle secret · `f` Text↔File (Multipart). Insert mode: type to edit, `Tab` swap key/value, `Enter` commit, `Esc` cancel.

Body editor: `tui-textarea` inline for Raw/JSON. GraphQL splits 60/40 query+variables. `$EDITOR` shell-out via `crate::editor::shell_out` (terminal suspend/resume guarded against panic).

### Dynamic vars

Resolve at send time inside any `{{...}}`:

| Name | Args | Output |
|---|---|---|
| `$now` | `(rfc3339 \| iso8601 \| http \| unix \| <strftime>)` | timestamp string |
| `$timestamp` | — | unix seconds |
| `$uuid` | — | UUID v4 |
| `$ulid` | — | ULID |
| `$randomInt` | `(min, max)` inclusive | integer |
| `$randomString` | `(len)` (≤256) | alphanumeric |
| `$base64` | `(arg)` | b64 of arg (nesting + secret-tainting) |

Auth fields refuse dyn-var-only templates (`AuthError::DynVarOnlyInSecretField`) — token re-rolling per request breaks auth.

### cURL import

```bash
lazyfetch import-curl 'curl https://api.test/x'      # literal command
lazyfetch import-curl path/to/cmd.sh                 # file
echo 'curl ...' | lazyfetch import-curl              # stdin
lazyfetch import-curl '<cmd>' --save my-coll/login   # persist as Request
```

Parser handles bash/zsh/POSIX quoting (single, double, `$'...'` ANSI-C, `\` continuations), the full flag table (`-X`/`-H`/`-d`/`--data-raw`/`--data-binary`/`--data-urlencode`/`-F`/`-G`/`-u`/`--cookie`/`-A`/`-e`/`--compressed`/`--max-redirs`/`-L`), warnings for `-k`/`--proxy`/unknown flags. Body kind picked from the *last* `Content-Type` header. cmd.exe / PowerShell input rejected up front.

### cURL export

`Y` (capital) on Response pane — copies a redacted cURL command via `core::exec::build_curl` to the clipboard. Single-quote escaping is POSIX-portable (`O'Brien` → `'O'\''Brien'`). Secrets replaced via the request's `SecretRegistry` before the string ever touches the clipboard helper.

### Repeat-last

`R` (capital) on any pane / mode replays `state.last_response.request_template` — the pre-interpolation snapshot — through `sender::dispatch_request`. Re-interpolates against the *current* env so dyn-vars re-roll. URL bar / method / body / headers in the editor are not consulted; toast: `replaying GET /users/42 — your edits are unchanged`.

---

## Roadmap

| Version | Status |
|---|---|
| **v0.1 alpha** | ✅ Backend + CLI + TUI w/ env+collection management, vim navigation, search, mouse, JSON colorize, save / rename / move popups, autocomplete, `:messages` |
| **v0.2 alpha** | ✅ Request editor (Body kinds incl. GraphQL · Hybrid KV editor · `$EDITOR` shell-out · dyn-vars · cURL import + export · repeat-last) |
| v0.3 | OAuth2 (Client Credentials + Authorization Code w/ PKCE + loopback callback) + OS keyring · `jaq` filter expressions · OpenAPI 3 import |
| v0.4 | History viewer pane, theme + keymap config, nested folder navigation in Collections |
| v0.5 | Cookie jar, detailed timings (DNS / connect / TLS / TTFB), session export to cURL |

Specs: [v0.1](docs/superpowers/specs/2026-05-07-lazyfetch-design.md) · [v0.2](docs/superpowers/specs/2026-05-08-lazyfetch-v2-request-editor.md). Plans: [v0.1](docs/superpowers/plans/2026-05-07-lazyfetch-v1.md) · [v0.2](docs/superpowers/plans/2026-05-08-lazyfetch-v2.md)

---

## Develop

```bash
cargo test --workspace                                          # 99 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
bash scripts/check-core-purity.sh                               # enforce IO-free core
```

CI runs all of the above plus `cargo deny check` on every push.

<details>
<summary><strong>Test inventory (54)</strong></summary>

| Crate | Tests |
|---|---|
| `core` | 5 interpolation (incl. proptest) + 3 auth-walk + 1 wire redaction |
| `storage` | atomic write + collection round-trip + env round-trip + 50-thread concurrent JSONL append + **10 mutation tests** (save_request scaffold, rename collection / request, move_request — happy + collision + missing) |
| `http` | wiremock GET → status + headers |
| `auth` | Bearer (secret + non-secret reject) + Basic encoding + ApiKey query |
| `bin` | end-to-end binary spawn → wiremock → status assert + project-local discovery via nested cwd |
| `tui` | 16 keymap dispatch tests + `TestBackend` snapshot |
| `import` | Postman golden fixture + DoS oversize reject |

</details>

---

## Philosophy

- **Domain-driven, hexagonal.** Bounded contexts as crates. Ports as traits. Adapters at the edge. CI greps `core` for `tokio::` / `std::fs::` and fails on hits.
- **TDD throughout.** Tests drive the design.
- **YAGNI ruthlessly.** No speculative abstraction. Three similar lines beat a premature trait. Big refactors land when they pay for themselves.
- **Secrets are first-class.** Single `SecretRegistry` per request. Every surface — history, log, raw view, save, clipboard — redacts through one path. Env files are 0600.
- **Plain files win.** YAML + JSONL. `git init` your collections. Open them in your editor. Diff them. Share them.
- **Atomic writes everywhere.** Same-directory tempfile + `rename`. Drop guards clean up on panic. Slug collisions detected and refused.

---

## Tech

`rust` `ratatui` `crossterm` `tokio` `reqwest` `rustls` `hyper` `serde` `serde_yaml` `secrecy` `ulid` `blake3` `fd-lock` `tempfile` `tracing` `thiserror` `proptest` `wiremock` `insta` `clap` `dirs` `jaq` `arboard`/`wl-copy`/`xclip`

---

<div align="center">

Built by [@arielonoriaga](https://github.com/arielonoriaga). MIT.

</div>
