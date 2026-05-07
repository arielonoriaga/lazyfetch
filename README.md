<div align="center">

# lazyfetch

<a href="https://github.com/arielonoriaga/lazyfetch">
  <img src="https://readme-typing-svg.demolab.com?font=JetBrains+Mono&weight=600&size=18&duration=3500&pause=1200&color=58A6FF&center=true&vCenter=true&width=620&lines=Postman+in+your+terminal.;Vim+keys.+No+Electron.+No+account.;YAML+collections+%E2%80%94+git-friendly+by+design.;Hexagonal+Rust.+IO-free+core.+24+tests+green." alt="typing SVG" />
</a>

**A terminal-first HTTP client. Sibling to `lazygit` and `lazydocker`.**
Send requests, manage collections, switch environments, import from Postman — all without leaving the keyboard.

<p>
  <img alt="Rust" src="https://img.shields.io/badge/rust-stable-orange?style=flat-square&logo=rust" />
  <img alt="ratatui" src="https://img.shields.io/badge/TUI-ratatui-58A6FF?style=flat-square" />
  <img alt="reqwest" src="https://img.shields.io/badge/HTTP-reqwest%20%2B%20rustls-009688?style=flat-square" />
  <img alt="tokio" src="https://img.shields.io/badge/async-tokio-369?style=flat-square&logo=tokio" />
  <img alt="license" src="https://img.shields.io/badge/license-MIT-yellow?style=flat-square" />
  <img alt="status" src="https://img.shields.io/badge/status-v0.1%20alpha-FF5D01?style=flat-square" />
</p>

| 🦀 7 crates | ✅ 24 tests | 🧪 IO-free core | 🔐 secret-aware redaction |
|:---:|:---:|:---:|:---:|
| Hexagonal workspace | wiremock + insta + proptest | `cargo deny` + grep guard | unified across log / save / history |

</div>

---

## Why

Postman and Insomnia are powerful but heavy: GUI app, account, cloud sync you didn't ask for, opaque storage. The `lazy*` family (`lazygit`, `lazydocker`) proved that terminal-native UX wins for developer tools. `lazyfetch` does the same for HTTP.

```
┌─ Collections ──┬─ Request ─────────────────────────────────┐
│ ▸ my-api       │ [GET ▾] {{base}}/users/{{id}}             │
│   ▾ users      │ ─ Params ─ Headers ─ Body ─ Auth ─        │
│     • list     │ key            value         [x]          │
│     • get      │                                           │
│ ▸ stripe       ├─ Response ────────────── 200 OK · 142ms ─┤
│                │ ─ Body ─ Headers ─ Cookies ─ Timing ─    │
├─ Environment ──┤ {                                         │
│ [dev ▾]        │   "users": [...]                          │
│ base=...       │ }                                         │
│ token=***      │                                           │
└────────────────┴───────────────────────────────────────────┘
 :send  /search  e edit  s send  S save  ? help  q quit
```

---

## Install

```bash
git clone git@github.com:arielonoriaga/lazyfetch.git
cd lazyfetch
cargo install --path crates/bin
```

Requires Rust stable (≥ 1.85). No system deps — `rustls` everywhere, no OpenSSL.

---

## Use

### Headless — `lazyfetch run`

```bash
# Send a saved request
lazyfetch run my-api/users/list --env dev

# Override variables on the fly
lazyfetch run my-api/users/get --env dev --set id=42

# Custom config dir (default: ~/.config/lazyfetch)
lazyfetch run my-api/ping --config-dir ./fixtures
```

### Import from Postman

```bash
lazyfetch import-postman ./postman_collection.json
# → imported 'demo' (0 warnings)
```

Postman v2.1 collections become first-class YAML files under `~/.config/lazyfetch/collections/<name>/`. `git init` the directory and share with your team. No cloud, no account, no lock-in.

### Interactive TUI

```bash
lazyfetch
```

`Tab` cycles panes, `q` quits. (Editor + send + response viewer land in v0.2.)

---

## Storage — your data, your repo

```
~/.config/lazyfetch/
├── config.yaml
├── collections/
│   └── my-api/
│       ├── collection.yaml          # name, vars, auth
│       └── requests/
│           ├── users/
│           │   ├── _folder.yaml
│           │   ├── list.yaml        # one Request = one file
│           │   └── get.yaml
│           └── ping.yaml
└── environments/
    ├── dev.yaml
    └── prod.yaml
```

YAML is hand-editable, diff-friendly, comment-friendly. Each `Request` is its own file so a 200-request collection produces 200 small diffs, not one giant blob.

---

## Architecture

Hexagonal Cargo workspace. `core` is pure domain — no `tokio`, no `std::fs`, no network. Adapters live in their own crates. CI greps `core` for IO calls and fails on hits.

```
┌────────────────────────────────────────────────────────────┐
│ bin   →   tui   →   core   ←   { http, storage, auth, import } │
└────────────────────────────────────────────────────────────┘
```

| Crate | Responsibility |
|---|---|
| `core` | `Collection`, `Request`, `AuthSpec`, `WireRequest`, ports (`HttpSender`, `AuthCache`, `Clock`, `Browser`, `Editor`), `interpolate()`, `execute()`, `redact_wire()` |
| `http` | `reqwest` adapter, redirect policy, error mapping |
| `storage` | YAML collections (file-per-Request), env round-trip, JSONL history with `fd-lock`, atomic write (same-dir tempfile + `Drop` guard) |
| `auth` | `Bearer` / `Basic` / `ApiKey` resolvers with secret-only validation. OAuth2 stubbed for v0.2. |
| `import` | Postman v2.1 → core types, `ImportReport` warnings, DoS-bound parser |
| `tui` | `ratatui` + `crossterm`, alt-screen + raw-mode `Drop` guard, 4-pane layout |
| `bin` | composition root + CLI (`run`, `import-postman`) |

`{{var}}` interpolation lookup: per-request overrides → environment → collection vars. Every interpolated value carries a `SecretRegistry`; redaction is unified across history snapshots, raw-view toggles, log sinks, and `S`-save dialogs. Templates referencing non-secret variables in secret-only fields (`Bearer.token`, `Basic.pass`, `ApiKey.value`, `OAuth2.client_secret`) are **rejected at apply time**.

---

## Roadmap

| Version | Status |
|---|---|
| **v0.1 alpha** | ✅ Backend + CLI + Postman import + TUI shell |
| v0.2 | TUI body editor (`tui-textarea` + `$EDITOR`), response viewer (`syntect` + `jaq` filter), `/`-search, `S`-save |
| v0.3 | OAuth2 (Client Credentials + Authorization Code w/ PKCE + loopback callback) |
| v0.4 | OpenAPI 3 import, history viewer, theme + keymap config |
| v0.5 | Cookie jar, detailed timings (DNS / connect / TLS / TTFB), OS keyring |

Spec: [`docs/superpowers/specs/2026-05-07-lazyfetch-design.md`](docs/superpowers/specs/2026-05-07-lazyfetch-design.md)
Plan: [`docs/superpowers/plans/2026-05-07-lazyfetch-v1.md`](docs/superpowers/plans/2026-05-07-lazyfetch-v1.md)

---

## Develop

```bash
cargo test --workspace                       # 24 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
bash scripts/check-core-purity.sh            # enforce IO-free core
```

CI runs all of the above plus `cargo deny check` on every push.

<details>
<summary><strong>Test inventory</strong></summary>

| Crate | Tests |
|---|---|
| `core` | 5 interpolation (incl. proptest) + 3 auth-walk + 1 wire redaction |
| `storage` | atomic write + collection round-trip + env round-trip + 50-thread concurrent JSONL append |
| `http` | wiremock GET → status + headers |
| `auth` | Bearer (secret + non-secret reject) + Basic encoding + ApiKey query |
| `bin` | end-to-end binary spawn → wiremock → status assert |
| `tui` | keymap dispatch + `TestBackend` snapshot |
| `import` | Postman golden fixture + DoS oversize reject |

</details>

---

## Philosophy

- **Domain-driven, hexagonal.** Bounded contexts as crates. Ports as traits. Adapters at the edge.
- **TDD throughout.** Tests drive the design, not document it after the fact.
- **YAGNI ruthlessly.** No speculative abstraction. Three similar lines beat a premature trait.
- **Secrets are first-class.** Every secret value flows through `SecretRegistry`. There is no path that prints `Authorization: Bearer <token>` verbatim — including logs and history.
- **Plain files win.** YAML + JSONL. `git init` your collections. Open them in your editor. Diff them. Share them.

---

## Tech

`rust` `ratatui` `crossterm` `tokio` `reqwest` `rustls` `hyper` `serde` `serde_yaml` `secrecy` `ulid` `blake3` `fd-lock` `tempfile` `tracing` `thiserror` `proptest` `wiremock` `insta` `clap` `dirs`

---

<div align="center">

Built by [@arielonoriaga](https://github.com/arielonoriaga). MIT.

</div>
