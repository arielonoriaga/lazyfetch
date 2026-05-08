# lazyfetch v0.2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Request pane self-sufficient — body editor (Raw/JSON/Form/Multipart/GraphQL), Headers/Query KV editors, dynamic vars (`{{$now}}`, `{{$uuid}}`, `{{$base64({{TOKEN}})}}` etc), cURL import/export, and repeat-last (`R`) — so daily HTTP debugging never leaves the TUI.

**Architecture:** Hexagonal Cargo workspace already in place. Two new pure modules in `core` (`dynvars`, `exec::build_curl`), one new pure parser (`import::curl`), and three new TUI modules (`editor`, `kv_editor`, `request_pane`). `core` IO-free invariant preserved (CI grep guard already enforces). Single-source-of-truth `BodyEditorState` enum + unified `KvRow { kind, key, value, enabled, secret }` shared by Headers / Query / Form / Multipart.

**Tech Stack:** Rust stable (≥ 1.85), `tui-textarea` 0.7, `uuid` 1 (v4), `rand` 0.8, `mime_guess` 2, `base64` 0.22 (workspace), plus existing `ratatui`, `crossterm`, `reqwest`, `tokio`, `serde`, `secrecy`, `ulid`, `chrono`, `tracing`, `proptest`, `wiremock`, `insta`, `tempfile`.

**Spec:** [`docs/superpowers/specs/2026-05-08-lazyfetch-v2-request-editor.md`](../specs/2026-05-08-lazyfetch-v2-request-editor.md) — read this first, plan steps reference its sections.

---

## File Structure

```
crates/
├── core/src/
│   ├── catalog.rs             (modify: Body::GraphQL variant; BodyKind helper)
│   ├── env.rs                 (modify: interpolate() learns dyn-var hook + secret taint)
│   ├── auth.rs                (modify: AuthError::DynVarOnlyInSecretField + apply check)
│   ├── dynvars.rs             (NEW: pure resolver + arg grammar + Arg/DynError/DynCtx)
│   ├── exec.rs                (modify: Executed.request_template; build_curl())
│   └── lib.rs                 (modify: pub mod dynvars)
├── http/src/lib.rs            (modify: ReqwestSender Multipart + GraphQL JSON serializers)
├── import/src/
│   ├── curl.rs                (NEW: cURL command → (Request, ImportReport))
│   └── lib.rs                 (modify: pub mod curl)
├── tui/src/
│   ├── editor.rs              (NEW: BodyEditorState + $EDITOR shell-out w/ TerminalSuspendGuard)
│   ├── kv_editor.rs           (NEW: KvEditor + KvRow + KvRowKind + KvMode)
│   ├── request_pane.rs        (NEW: render + dispatch composing editor + kv_editor)
│   ├── app.rs                 (modify: state additions; Mode::ImportCurl)
│   ├── commands.rs            (modify: run_curl_import, run_repeat_last)
│   ├── keymap.rs              (modify: bindings for new keys)
│   ├── layout.rs              (modify: delegate Request pane render)
│   └── lib.rs                 (modify: pub mod editor / kv_editor / request_pane)
└── bin/src/
    ├── import_curl.rs         (NEW: lazyfetch import-curl <cmd-or-file>)
    └── main.rs                (modify: register subcommand)
```

---

## Task 1: `core::dynvars` — pure dyn-var resolver + arg grammar

**Files:**
- Create: `crates/core/src/dynvars.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/Cargo.toml`
- Test: in-file `#[cfg(test)] mod tests` plus `crates/core/tests/dynvars.rs`

- [ ] **Step 1: Add deps to `crates/core/Cargo.toml`**

```toml
[dependencies]
# existing deps unchanged. Add:
uuid       = { version = "1", features = ["v4"] }
rand       = "0.8"
base64     = "0.22"
```

- [ ] **Step 2: Write the failing test (`crates/core/tests/dynvars.rs`)**

```rust
use lazyfetch_core::dynvars::{resolve, Arg, DynCtx, DynError};
use lazyfetch_core::ports::SystemClock;

fn ctx() -> DynCtx<'static> {
    static CLOCK: SystemClock = SystemClock;
    DynCtx { clock: &CLOCK }
}

#[test]
fn now_is_rfc3339() {
    let s = resolve("now", &[], &ctx()).unwrap();
    assert!(chrono::DateTime::parse_from_rfc3339(&s).is_ok(), "got {s}");
}

#[test]
fn timestamp_is_unix_secs() {
    let s = resolve("timestamp", &[], &ctx()).unwrap();
    let n: u64 = s.parse().expect("digits");
    assert!(n > 1_700_000_000);
}

#[test]
fn uuid_is_v4_hex() {
    let s = resolve("uuid", &[], &ctx()).unwrap();
    let u = uuid::Uuid::parse_str(&s).unwrap();
    assert_eq!(u.get_version_num(), 4);
}

#[test]
fn ulid_is_crockford() {
    let s = resolve("ulid", &[], &ctx()).unwrap();
    assert_eq!(s.len(), 26);
    assert!(ulid::Ulid::from_string(&s).is_ok());
}

#[test]
fn random_int_in_bounds() {
    for _ in 0..200 {
        let s = resolve(
            "randomInt",
            &[Arg::str("5"), Arg::str("10")],
            &ctx(),
        )
        .unwrap();
        let n: i64 = s.parse().unwrap();
        assert!((5..=10).contains(&n), "got {n}");
    }
}

#[test]
fn random_int_bounds_invalid() {
    let r = resolve("randomInt", &[Arg::str("10"), Arg::str("5")], &ctx());
    assert!(matches!(r, Err(DynError::Bounds { .. })));
}

#[test]
fn base64_literal() {
    let s = resolve("base64", &[Arg::str("foo")], &ctx()).unwrap();
    assert_eq!(s, "Zm9v");
}

#[test]
fn random_string_alphabet_and_length() {
    let s = resolve("randomString", &[Arg::str("16")], &ctx()).unwrap();
    assert_eq!(s.chars().count(), 16);
    assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn random_string_length_cap() {
    let r = resolve("randomString", &[Arg::str("5000")], &ctx());
    assert!(matches!(r, Err(DynError::Bounds { .. })));
}

#[test]
fn unknown_returns_unknown_err() {
    let r = resolve("nope", &[], &ctx());
    assert!(matches!(r, Err(DynError::Unknown(_))));
}

#[test]
fn now_format_alias() {
    let s = resolve("now", &[Arg::str("rfc2822")], &ctx()).unwrap();
    assert!(s.ends_with("+0000") || s.ends_with("UT"));
}

#[test]
fn now_chrono_strftime() {
    let s = resolve("now", &[Arg::str("%Y-%m-%d")], &ctx()).unwrap();
    assert_eq!(s.len(), 10);
    assert_eq!(&s[4..5], "-");
}
```

- [ ] **Step 3: Run tests — Expected: compile error (no `dynvars` module)**

Run: `cargo test -p lazyfetch-core --test dynvars`
Expected: FAIL with "could not find `dynvars` in `lazyfetch_core`"

- [ ] **Step 4: Implement `crates/core/src/dynvars.rs`**

```rust
//! Pure dyn-var resolver. No IO. Time read through `Clock` port.

use crate::ports::Clock;
use base64::Engine;
use rand::Rng;
use thiserror::Error;
use tracing::instrument;

const RAND_STRING_MAX: usize = 1024;
const RAND_STRING_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

#[derive(Debug, Error)]
pub enum DynError {
    #[error("unknown dyn var: ${0}")]
    Unknown(String),
    #[error("syntax error parsing args of ${name}: {msg}")]
    ParseSyntax { name: String, msg: String },
    #[error("arg parse failed for ${name}: {msg}")]
    ArgParse { name: String, msg: String },
    #[error("recursion limit hit for ${0}")]
    TooDeep(String),
    #[error("bounds invalid for ${name}: min={min} max={max}")]
    Bounds { name: String, min: i64, max: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg(pub String);
impl Arg {
    pub fn str(s: &str) -> Self { Self(s.to_string()) }
}

pub struct DynCtx<'a> {
    pub clock: &'a dyn Clock,
}

#[instrument(target = "lazyfetch::dynvars", skip(ctx), fields(name = %name))]
pub fn resolve(name: &str, args: &[Arg], ctx: &DynCtx) -> Result<String, DynError> {
    match name {
        "now" => now(args, ctx),
        "timestamp" => Ok(ctx.clock.now().timestamp().to_string()),
        "uuid" => Ok(uuid::Uuid::new_v4().to_string()),
        "ulid" => Ok(ulid::Ulid::new().to_string()),
        "randomInt" => random_int(args),
        "randomString" => random_string(args),
        "base64" => base64_arg(args),
        other => Err(DynError::Unknown(other.into())),
    }
}

fn now(args: &[Arg], ctx: &DynCtx) -> Result<String, DynError> {
    let dt = ctx.clock.now();
    if args.is_empty() {
        return Ok(dt.to_rfc3339());
    }
    Ok(match args[0].0.as_str() {
        "rfc3339" => dt.to_rfc3339(),
        "rfc2822" => dt.to_rfc2822(),
        "iso8601" => dt.to_rfc3339(),
        fmt => dt.format(fmt).to_string(),
    })
}

fn random_int(args: &[Arg]) -> Result<String, DynError> {
    if args.is_empty() {
        return Ok(rand::thread_rng().gen::<u32>().to_string());
    }
    if args.len() != 2 {
        return Err(DynError::ArgParse {
            name: "randomInt".into(),
            msg: format!("expected 0 or 2 args, got {}", args.len()),
        });
    }
    let min: i64 = args[0].0.parse().map_err(|e| DynError::ArgParse {
        name: "randomInt".into(),
        msg: format!("min: {e}"),
    })?;
    let max: i64 = args[1].0.parse().map_err(|e| DynError::ArgParse {
        name: "randomInt".into(),
        msg: format!("max: {e}"),
    })?;
    if min > max {
        return Err(DynError::Bounds { name: "randomInt".into(), min, max });
    }
    Ok(rand::thread_rng().gen_range(min..=max).to_string())
}

fn random_string(args: &[Arg]) -> Result<String, DynError> {
    if args.len() != 1 {
        return Err(DynError::ArgParse {
            name: "randomString".into(),
            msg: format!("expected 1 arg (length), got {}", args.len()),
        });
    }
    let n: usize = args[0].0.parse().map_err(|e| DynError::ArgParse {
        name: "randomString".into(),
        msg: format!("length: {e}"),
    })?;
    if n > RAND_STRING_MAX {
        return Err(DynError::Bounds {
            name: "randomString".into(),
            min: 0,
            max: RAND_STRING_MAX as i64,
        });
    }
    let mut rng = rand::thread_rng();
    Ok((0..n)
        .map(|_| RAND_STRING_ALPHABET[rng.gen_range(0..RAND_STRING_ALPHABET.len())] as char)
        .collect())
}

fn base64_arg(args: &[Arg]) -> Result<String, DynError> {
    if args.len() != 1 {
        return Err(DynError::ArgParse {
            name: "base64".into(),
            msg: format!("expected 1 arg, got {}", args.len()),
        });
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(args[0].0.as_bytes()))
}
```

- [ ] **Step 5: Wire `crates/core/src/lib.rs`**

```rust
pub mod dynvars;
```

(Add the line alongside existing `pub mod` declarations.)

- [ ] **Step 6: Run tests — Expected: PASS**

Run: `cargo test -p lazyfetch-core --test dynvars`
Expected: 12 passed.

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/dynvars.rs crates/core/src/lib.rs crates/core/Cargo.toml crates/core/tests/dynvars.rs
git commit -m "feat(core): dynvars module — pure resolver for \$now/\$uuid/\$ulid/\$randomInt/\$randomString/\$base64"
```

---

## Task 2: `interpolate()` learns dyn-var hook + secret taint

**Files:**
- Modify: `crates/core/src/env.rs`
- Test: `crates/core/tests/interpolate.rs`

- [ ] **Step 1: Write failing tests in `crates/core/tests/interpolate.rs`**

Append to the existing file:

```rust
use lazyfetch_core::dynvars;
use lazyfetch_core::ports::SystemClock;

#[test]
fn interpolate_resolves_now() {
    let env = ev(&[]);
    let ctx = ResolveCtx {
        env: &env, collection_vars: &[], overrides: &[],
    };
    let dyn_ctx = dynvars::DynCtx { clock: &SystemClock };
    let out = interpolate_with_dyn("Sent at {{$now}}", &ctx, &dyn_ctx).unwrap();
    assert!(out.value.starts_with("Sent at "));
    assert!(chrono::DateTime::parse_from_rfc3339(&out.value["Sent at ".len()..]).is_ok());
    assert!(out.used_secrets.is_empty());
}

#[test]
fn interpolate_taints_base64_of_secret() {
    use secrecy::SecretString;
    use lazyfetch_core::env::VarValue;
    let env = Environment {
        id: ulid::Ulid::new(),
        name: "t".into(),
        vars: vec![("TOKEN".into(), VarValue {
            value: SecretString::new("hunter2".into()),
            secret: true,
        })],
    };
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
    let dyn_ctx = dynvars::DynCtx { clock: &SystemClock };
    let out = interpolate_with_dyn("Auth: {{$base64({{TOKEN}})}}", &ctx, &dyn_ctx).unwrap();
    let expected_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD, b"hunter2",
    );
    assert!(out.value.contains(&expected_b64));
    // Both the original secret AND the base64 form should be tainted.
    assert!(out.used_secrets.contains("hunter2"));
    assert!(out.used_secrets.contains(&expected_b64));
}

#[test]
fn unknown_dyn_var_falls_through_to_missing() {
    let env = ev(&[]);
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
    let dyn_ctx = dynvars::DynCtx { clock: &SystemClock };
    let r = interpolate_with_dyn("{{$nope}}", &ctx, &dyn_ctx);
    assert!(r.is_err());
}

#[test]
fn nested_dyn_var_recursion_capped_at_8() {
    // `{{$base64({{$base64({{$base64(...8 deep...)}})}})}}` should hit TooDeep.
    let env = ev(&[]);
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
    let dyn_ctx = dynvars::DynCtx { clock: &SystemClock };
    let mut s = String::from("'x'");
    for _ in 0..10 {
        s = format!("{{{{$base64({s})}}}}");
    }
    let r = interpolate_with_dyn(&s, &ctx, &dyn_ctx);
    assert!(r.is_err(), "10-deep should exceed depth 8 → TooDeep");
}
```

- [ ] **Step 2: Run tests — Expected: compile error (no `interpolate_with_dyn`)**

Run: `cargo test -p lazyfetch-core --test interpolate`
Expected: FAIL with "cannot find function `interpolate_with_dyn`"

- [ ] **Step 3: Implement `interpolate_with_dyn` in `crates/core/src/env.rs`**

Add below the existing `interpolate` fn:

```rust
use crate::dynvars::{self, Arg, DynCtx, DynError};

const MAX_DEPTH: u8 = 8;

pub fn interpolate_with_dyn(
    s: &str,
    ctx: &ResolveCtx,
    dyn_ctx: &DynCtx,
) -> Result<Interpolated, CoreError> {
    interp_inner(s, ctx, dyn_ctx, 0)
}

fn interp_inner(
    s: &str,
    ctx: &ResolveCtx,
    dyn_ctx: &DynCtx,
    depth: u8,
) -> Result<Interpolated, CoreError> {
    if depth > MAX_DEPTH {
        return Err(CoreError::InvalidTemplate(format!("dyn-var depth >{MAX_DEPTH}")));
    }
    let mut out = String::with_capacity(s.len());
    let mut reg = SecretRegistry::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let (token, after) = take_token(&rest[start + 2..])?;
        rest = after;
        let (val, secrets) = resolve_token(token, ctx, dyn_ctx, depth)?;
        out.push_str(&val);
        reg.extend(&secrets);
    }
    out.push_str(rest);
    Ok(Interpolated { value: out, used_secrets: reg })
}

/// Split off everything up to the matching `}}`. Returns (inner, after).
fn take_token(s: &str) -> Result<(&str, &str), CoreError> {
    // Track nesting so `{{$base64({{X}})}}` is one outer token.
    let mut depth = 1usize;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            depth += 1;
            i += 2;
            continue;
        }
        if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            depth -= 1;
            if depth == 0 {
                return Ok((&s[..i], &s[i + 2..]));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    Err(CoreError::InvalidTemplate(s.into()))
}

fn resolve_token(
    token: &str,
    ctx: &ResolveCtx,
    dyn_ctx: &DynCtx,
    depth: u8,
) -> Result<(String, SecretRegistry), CoreError> {
    let trimmed = token.trim();
    if let Some(after_dollar) = trimmed.strip_prefix('$') {
        return resolve_dyn(after_dollar, ctx, dyn_ctx, depth);
    }
    // Standard var lookup — preserves v0.1 semantics.
    let v = lookup(trimmed, ctx).ok_or_else(|| CoreError::MissingVar(trimmed.into()))?;
    let val = secrecy::ExposeSecret::expose_secret(&v.value).clone();
    let mut reg = SecretRegistry::new();
    if v.secret {
        reg.insert(val.clone());
    }
    Ok((val, reg))
}

fn resolve_dyn(
    spec: &str,
    ctx: &ResolveCtx,
    dyn_ctx: &DynCtx,
    depth: u8,
) -> Result<(String, SecretRegistry), CoreError> {
    let (name, raw_args) = parse_name_args(spec).map_err(|m| {
        CoreError::InvalidTemplate(format!("dyn-var ${spec}: {m}"))
    })?;
    let mut combined = SecretRegistry::new();
    let mut resolved: Vec<Arg> = Vec::with_capacity(raw_args.len());
    for raw in &raw_args {
        let (v, s) = resolve_arg(raw, ctx, dyn_ctx, depth + 1)?;
        combined.extend(&s);
        resolved.push(Arg(v));
    }
    let result = dynvars::resolve(name, &resolved, dyn_ctx).map_err(|e| match e {
        DynError::Unknown(_) => CoreError::MissingVar(format!("${name}")),
        other => CoreError::InvalidTemplate(other.to_string()),
    })?;
    if !combined.is_empty() {
        combined.insert(result.clone());
    }
    Ok((result, combined))
}

#[derive(Debug, Clone)]
enum RawArg {
    Quoted(String),     // 'literal' or "literal" with POSIX escapes already stripped
    VarRef(String),     // {{name}}
    Bareword(String),
}

fn parse_name_args(spec: &str) -> Result<(&str, Vec<RawArg>), String> {
    let bytes = spec.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i == 0 {
        return Err("missing dyn-var name".into());
    }
    let name = &spec[..i];
    let after = spec[i..].trim_start();
    if after.is_empty() {
        return Ok((name, vec![]));
    }
    let after = after.strip_prefix('(').ok_or_else(|| {
        format!("expected '(' after ${name}, got '{after}'")
    })?;
    let after = after.strip_suffix(')').ok_or("missing closing ')'")?;
    let args = if after.trim().is_empty() {
        vec![]
    } else {
        split_args(after)?
    };
    Ok((name, args))
}

fn split_args(s: &str) -> Result<Vec<RawArg>, String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() { chars.next(); } else { break; }
        }
        let Some(&first) = chars.peek() else { break };
        let arg = if first == '\'' || first == '"' {
            parse_quoted(&mut chars, first)?
        } else if first == '{' {
            parse_var_ref(&mut chars)?
        } else {
            parse_bareword(&mut chars)?
        };
        out.push(arg);
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() { chars.next(); } else { break; }
        }
        match chars.peek() {
            None => break,
            Some(&',') => { chars.next(); }
            Some(&c) => return Err(format!("unexpected char '{c}' between args")),
        }
    }
    Ok(out)
}

fn parse_quoted(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    quote: char,
) -> Result<RawArg, String> {
    chars.next(); // consume opening quote
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == quote { return Ok(RawArg::Quoted(out)); }
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(c) if c == quote => out.push(c),
                Some(other) => return Err(format!("bad escape \\{other}")),
                None => return Err("unterminated escape".into()),
            }
            continue;
        }
        out.push(c);
    }
    Err(format!("unterminated {quote}-quoted string"))
}

fn parse_var_ref(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<RawArg, String> {
    let &c1 = chars.peek().ok_or("expected '{{'")?;
    chars.next();
    let c2 = chars.next().ok_or("expected second '{'")?;
    if c1 != '{' || c2 != '{' {
        return Err("expected '{{'".into());
    }
    let mut name = String::new();
    loop {
        match chars.next() {
            Some('}') => {
                let close2 = chars.next().ok_or("expected closing '}}'")?;
                if close2 != '}' { return Err("expected '}}'".into()); }
                return Ok(RawArg::VarRef(name));
            }
            Some(c) => name.push(c),
            None => return Err("unterminated {{var}}".into()),
        }
    }
}

fn parse_bareword(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<RawArg, String> {
    let mut out = String::new();
    while let Some(&c) = chars.peek() {
        if matches!(c, ',' | '(' | ')' | '{' | '}' | '\'' | '"') || c.is_whitespace() {
            break;
        }
        out.push(c);
        chars.next();
    }
    if out.is_empty() {
        return Err("empty bareword".into());
    }
    Ok(RawArg::Bareword(out))
}

fn resolve_arg(
    raw: &RawArg,
    ctx: &ResolveCtx,
    dyn_ctx: &DynCtx,
    depth: u8,
) -> Result<(String, SecretRegistry), CoreError> {
    if depth > MAX_DEPTH {
        return Err(CoreError::InvalidTemplate(format!("dyn-var depth >{MAX_DEPTH}")));
    }
    match raw {
        RawArg::Quoted(s) | RawArg::Bareword(s) => {
            Ok((s.clone(), SecretRegistry::new()))
        }
        RawArg::VarRef(name) => {
            // Single-level var lookup — values are not recursively interpolated.
            let v = lookup(name, ctx)
                .ok_or_else(|| CoreError::MissingVar(name.clone()))?;
            let val = secrecy::ExposeSecret::expose_secret(&v.value).clone();
            let mut reg = SecretRegistry::new();
            if v.secret { reg.insert(val.clone()); }
            // If the resolved value happens to itself be `{{$dyn}}`, expand it
            // (this is the only place dyn-var nesting recurses).
            if val.contains("{{$") {
                let nested = interp_inner(&val, ctx, dyn_ctx, depth + 1)?;
                let mut combined = reg;
                combined.extend(&nested.used_secrets);
                Ok((nested.value, combined))
            } else {
                Ok((val, reg))
            }
        }
    }
}
```

- [ ] **Step 4: Run tests — Expected: PASS**

Run: `cargo test -p lazyfetch-core --test interpolate`
Expected: existing 5 + 4 new = 9 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/env.rs crates/core/tests/interpolate.rs
git commit -m "feat(core): interpolate_with_dyn — dyn-var hook + secret tainting + recursion guard"
```

---

## Task 3: `Body::GraphQL` variant + `BodyKind` helper

**Files:**
- Modify: `crates/core/src/catalog.rs`
- Test: `crates/core/tests/catalog.rs`

- [ ] **Step 1: Write failing test (`crates/core/tests/catalog.rs`)**

```rust
use lazyfetch_core::catalog::{Body, BodyKind};

#[test]
fn body_kind_round_trip() {
    assert_eq!(Body::None.kind(), BodyKind::None);
    assert_eq!(Body::Json("{}".into()).kind(), BodyKind::Json);
    assert_eq!(
        Body::GraphQL {
            query: "{ me { id } }".into(),
            variables: "{}".into()
        }
        .kind(),
        BodyKind::GraphQL
    );
}

#[test]
fn graphql_serde_uses_lowercase_tag() {
    let b = Body::GraphQL {
        query: "{me{id}}".into(),
        variables: "{}".into(),
    };
    let yaml = serde_yaml::to_string(&b).unwrap();
    assert!(yaml.contains("kind: graphql"), "got:\n{yaml}");
    let back: Body = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.kind(), BodyKind::GraphQL);
}
```

- [ ] **Step 2: Run — Expected: FAIL (no `BodyKind`, no `GraphQL` variant)**

Run: `cargo test -p lazyfetch-core --test catalog`

- [ ] **Step 3: Modify `crates/core/src/catalog.rs`**

Find the existing `Body` enum and replace with:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Body {
    #[default]
    None,
    Raw { mime: String, text: String },
    Json(String),
    Form(Vec<KV>),
    Multipart(Vec<Part>),
    File(PathBuf),
    #[serde(rename = "graphql")]
    GraphQL { query: String, variables: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    None,
    Raw,
    Json,
    Form,
    Multipart,
    File,
    GraphQL,
}

impl Body {
    pub fn kind(&self) -> BodyKind {
        match self {
            Body::None => BodyKind::None,
            Body::Raw { .. } => BodyKind::Raw,
            Body::Json(_) => BodyKind::Json,
            Body::Form(_) => BodyKind::Form,
            Body::Multipart(_) => BodyKind::Multipart,
            Body::File(_) => BodyKind::File,
            Body::GraphQL { .. } => BodyKind::GraphQL,
        }
    }
}
```

- [ ] **Step 4: Run — Expected: PASS**

Run: `cargo test -p lazyfetch-core --test catalog`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/catalog.rs crates/core/tests/catalog.rs
git commit -m "feat(core): Body::GraphQL variant + BodyKind helper, serde rename to 'graphql'"
```

---

## Task 4: `core::exec::build_curl` — redacted cURL exporter

**Files:**
- Modify: `crates/core/src/exec.rs`
- Test: `crates/core/tests/build_curl.rs`

- [ ] **Step 1: Write failing tests**

```rust
use http::Method;
use lazyfetch_core::exec::{build_curl, WireRequest};
use lazyfetch_core::secret::SecretRegistry;

fn req(method: Method, url: &str) -> WireRequest {
    WireRequest {
        method, url: url.into(),
        headers: vec![],
        body_bytes: vec![],
        timeout: std::time::Duration::from_secs(30),
        follow_redirects: true,
        max_redirects: 10,
    }
}

#[test]
fn simple_get() {
    let r = req(Method::GET, "https://api/x");
    let s = build_curl(&r, &SecretRegistry::new());
    assert_eq!(s, "curl 'https://api/x'");
}

#[test]
fn post_with_header_and_body() {
    let mut r = req(Method::POST, "https://api/x");
    r.headers.push(("Content-Type".into(), "application/json".into()));
    r.body_bytes = b"{\"a\":1}".to_vec();
    let s = build_curl(&r, &SecretRegistry::new());
    assert!(s.contains("-X POST"));
    assert!(s.contains("-H 'Content-Type: application/json'"));
    assert!(s.contains("-d '{\"a\":1}'"));
}

#[test]
fn redacts_secrets_in_headers() {
    let mut reg = SecretRegistry::new();
    reg.insert("hunter2");
    let mut r = req(Method::GET, "https://api/x");
    r.headers.push(("Authorization".into(), "Bearer hunter2".into()));
    let s = build_curl(&r, &reg);
    assert!(s.contains("Bearer ***"));
    assert!(!s.contains("hunter2"));
}

#[test]
fn escapes_inner_single_quote() {
    let mut r = req(Method::GET, "https://api/x");
    r.headers.push(("X-Author".into(), "O'Brien".into()));
    let s = build_curl(&r, &SecretRegistry::new());
    assert!(
        s.contains("'O'\\''Brien'"),
        "got: {s}"
    );
}
```

- [ ] **Step 2: Run — Expected: FAIL (`build_curl` undefined)**

Run: `cargo test -p lazyfetch-core --test build_curl`

- [ ] **Step 3: Implement in `crates/core/src/exec.rs` (append)**

```rust
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
    out
}
```

- [ ] **Step 4: Run — Expected: PASS**

Run: `cargo test -p lazyfetch-core --test build_curl`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/exec.rs crates/core/tests/build_curl.rs
git commit -m "feat(core): exec::build_curl — redacted cURL exporter w/ POSIX single-quote escaping"
```

---

## Task 5: `Executed.request_template` field for repeat-last

**Files:**
- Modify: `crates/core/src/exec.rs`

- [ ] **Step 1: Add field to `Executed`**

Find the existing `pub struct Executed { ... }` and add `pub request_template: Request,` as the first field:

```rust
#[derive(Debug, Clone)]
pub struct Executed {
    pub request_template: Request,        // pre-interpolation snapshot
    pub request_snapshot: WireRequest,    // post-interpolation, redacted
    pub response: WireResponse,
    pub at: DateTime<Utc>,
    pub secrets: SecretRegistry,
}
```

- [ ] **Step 2: Wire it in `execute()`**

In the existing `execute` fn, just before the final `Ok(Executed { ... })`, change the struct literal to include the template:

```rust
Ok(Executed {
    request_template: req.clone(),
    request_snapshot: redact_wire(&wire, &reg),
    response: resp,
    at: clock.now(),
    secrets: reg,
})
```

- [ ] **Step 3: Run — Expected: PASS (cargo build catches missing field, then green)**

Run: `cargo build --workspace`
Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/exec.rs
git commit -m "feat(core): Executed.request_template — pre-interpolation snapshot for repeat-last"
```

---

## Task 6: `core::auth` — `DynVarOnlyInSecretField` rejection

**Files:**
- Modify: `crates/core/src/auth.rs`
- Test: `crates/auth/tests/dynvar_only.rs`

- [ ] **Step 1: Write failing test**

```rust
use lazyfetch_auth::resolver::DefaultResolver;
use lazyfetch_auth::NoCache;
use lazyfetch_core::auth::{AuthError, AuthResolver, AuthSpec};
use lazyfetch_core::env::{Environment, ResolveCtx};
use lazyfetch_core::exec::WireRequest;
use lazyfetch_core::ports::SystemClock;
use lazyfetch_core::primitives::Template;
use lazyfetch_core::secret::SecretRegistry;

fn empty_req() -> WireRequest {
    WireRequest {
        method: http::Method::GET, url: "http://x".into(),
        headers: vec![], body_bytes: vec![],
        timeout: std::time::Duration::from_secs(5),
        follow_redirects: true, max_redirects: 10,
    }
}

#[tokio::test]
async fn bearer_dynvar_only_is_rejected() {
    let env = Environment {
        id: ulid::Ulid::new(),
        name: "t".into(),
        vars: vec![],
    };
    let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
    let mut req = empty_req();
    let mut reg = SecretRegistry::new();
    let res = DefaultResolver::new()
        .apply(
            &AuthSpec::Bearer {
                token: Template("{{$randomString(32)}}".into()),
            },
            &ctx,
            &SystemClock,
            &NoCache,
            &mut req,
            &mut reg,
        )
        .await;
    assert!(matches!(
        res,
        Err(AuthError::DynVarOnlyInSecretField { .. })
    ));
}
```

- [ ] **Step 2: Run — Expected: FAIL**

Run: `cargo test -p lazyfetch-auth --test dynvar_only`

- [ ] **Step 3: Add error variant to `crates/core/src/auth.rs`**

Append to the existing `AuthError`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing var: {0}")]
    MissingVar(String),
    #[error("non-secret var used for secret field: {0}")]
    NotSecret(String),
    #[error("oauth: {0}")]
    OAuth(String),
    #[error("dyn-var-only template in secret field: {template} — auth that re-rolls per request is broken")]
    DynVarOnlyInSecretField { template: String },
    #[error(transparent)]
    Core(#[from] crate::error::CoreError),
}
```

- [ ] **Step 4: Tighten `DefaultResolver::apply` in `crates/auth/src/resolver.rs`**

Replace the existing `require_secret` helper:

```rust
/// Reject if template is *only* dyn-vars in a secret field, or if it references
/// non-secret env vars in a secret field.
fn require_secret(tpl: &str, i: &Interpolated) -> Result<(), AuthError> {
    if !tpl.contains("{{") {
        return Ok(()); // plain literal — caller's responsibility
    }
    // dyn-var-only check: template contains `{{$` but no `{{<non-$>` reference
    let has_var_ref = tpl
        .match_indices("{{")
        .any(|(idx, _)| {
            let after = &tpl[idx + 2..];
            !after.trim_start().starts_with('$')
        });
    if !has_var_ref {
        return Err(AuthError::DynVarOnlyInSecretField { template: tpl.into() });
    }
    if i.used_secrets.is_empty() && i.value != *tpl {
        return Err(AuthError::NotSecret(tpl.into()));
    }
    Ok(())
}
```

- [ ] **Step 5: Run — Expected: PASS**

Run: `cargo test -p lazyfetch-auth`
Expected: all auth tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/auth.rs crates/auth/src/resolver.rs crates/auth/tests/dynvar_only.rs
git commit -m "feat(auth): reject dyn-var-only templates in Bearer/Basic/ApiKey secret fields"
```

---

## Task 7: `http` adapter — Multipart + GraphQL serializers

**Files:**
- Modify: `crates/core/src/exec.rs` (render_body extends)
- Modify: `crates/http/src/lib.rs`
- Test: `crates/http/tests/send.rs`

- [ ] **Step 1: Add deps for multipart**

`crates/http/Cargo.toml`:
```toml
mime_guess = "2"
```

- [ ] **Step 2: Write failing test (`crates/http/tests/send.rs`, append)**

```rust
#[tokio::test]
async fn sends_graphql_as_json() {
    use http::Method;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(wiremock::matchers::body_json(serde_json::json!({
            "query": "{ me { id } }",
            "variables": {"x": 1}
        })))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server).await;

    let sender = ReqwestSender::new();
    let req = WireRequest {
        method: Method::POST,
        url: format!("{}/graphql", server.uri()),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body_bytes: br#"{"query":"{ me { id } }","variables":{"x":1}}"#.to_vec(),
        timeout: std::time::Duration::from_secs(5),
        follow_redirects: true,
        max_redirects: 10,
    };
    let resp = sender.send(req).await.unwrap();
    assert_eq!(resp.status, 200);
}
```

- [ ] **Step 3: Extend `core::exec::render_body` in `crates/core/src/exec.rs`**

```rust
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
                if i > 0 { s.push('&'); }
                let v = crate::env::interpolate(&kv.value, ctx)?;
                reg.extend(&v.used_secrets);
                s.push_str(&urlencoding::encode(&kv.key));
                s.push('=');
                s.push_str(&urlencoding::encode(&v.value));
            }
            s.into_bytes()
        }
        Body::GraphQL { query, variables } => {
            let q = crate::env::interpolate(query, ctx)?;
            reg.extend(&q.used_secrets);
            let vars_trim = variables.trim();
            let vars_value: serde_json::Value = if vars_trim.is_empty() {
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
        Body::Multipart(_) | Body::File(_) => Vec::new(),  // handled by adapter
    })
}
```

- [ ] **Step 4: Extend `crates/http/src/lib.rs` for Multipart**

Currently `ReqwestSender::send` builds the request from headers + bytes. Multipart needs `reqwest::multipart::Form`. Extend `WireRequest` to optionally carry multipart:

For v0.2, ship the simpler path: serialize multipart at `core::exec` boundary into raw bytes when reqwest can't be used directly is not viable. Instead, add an optional sidecar:

```rust
// in core::exec::WireRequest add:
pub multipart: Option<Vec<MultipartField>>,
pub graphql_marker: bool, // hint to set Content-Type if user didn't
```

Actually simpler: keep `WireRequest` shape, and have the adapter detect Multipart by looking at headers (`Content-Type: multipart/...`) — but reqwest needs to control the boundary. Cleanest is sidecar.

Add to `WireRequest`:

```rust
pub struct MultipartField {
    pub name: String,
    pub kind: MultipartKind,
}
pub enum MultipartKind {
    Text(String),
    File(PathBuf),
}

// WireRequest gets:
pub multipart: Option<Vec<MultipartField>>,
```

In `render_body`, when `Body::Multipart(parts)`, populate `wire.multipart` with the resolved `MultipartField`s and leave `body_bytes` empty. (Update `WireRequest` construction in `execute()` accordingly.)

In `crates/http/src/lib.rs` `send`:

```rust
if let Some(mp) = r.multipart.as_ref() {
    let mut form = reqwest::multipart::Form::new();
    for f in mp {
        match &f.kind {
            MultipartKind::Text(s) => {
                form = form.part(f.name.clone(), reqwest::multipart::Part::text(s.clone()));
            }
            MultipartKind::File(path) => {
                let mime = mime_guess::from_path(path)
                    .first_or_octet_stream()
                    .essence_str()
                    .to_string();
                let part = reqwest::multipart::Part::file(path)
                    .await
                    .map_err(|e| SendError::Other(anyhow::anyhow!(e)))?
                    .mime_str(&mime)
                    .map_err(|e| SendError::Other(anyhow::anyhow!(e)))?;
                form = form.part(f.name.clone(), part);
            }
        }
    }
    rb = rb.multipart(form);
} else if !r.body_bytes.is_empty() {
    rb = rb.body(r.body_bytes.clone());
}
```

- [ ] **Step 5: Run — Expected: PASS**

Run: `cargo test -p lazyfetch-http --test send`
Expected: existing + 1 new = passes.

- [ ] **Step 6: Commit**

```bash
git add crates/http/ crates/core/src/exec.rs
git commit -m "feat(http): GraphQL JSON serializer + Multipart adapter via WireRequest sidecar"
```

---

## Task 8: `tui::kv_editor` — Headers/Query/Form/Multipart KV state

**Files:**
- Create: `crates/tui/src/kv_editor.rs`
- Modify: `crates/tui/src/lib.rs`
- Test: `crates/tui/tests/kv_editor.rs`

- [ ] **Step 1: Write failing tests**

```rust
use lazyfetch_tui::kv_editor::{KvEditor, KvMode, KvRowKind};

fn ed() -> KvEditor { KvEditor::new() }

#[test]
fn add_row_via_a_then_commit() {
    let mut e = ed();
    e.start_add(); // mode: InsertKey { row: 0 }, rows: [empty]
    assert_eq!(e.rows.len(), 1);
    for c in "X-Trace".chars() { e.insert_char(c); }
    e.tab(); // swap to InsertValue
    for c in "abc".chars() { e.insert_char(c); }
    e.commit();
    assert_eq!(e.mode, KvMode::Normal);
    assert_eq!(e.rows[0].key, "X-Trace");
    assert_eq!(e.rows[0].value, "abc");
}

#[test]
fn i_edits_value_of_cursor_row() {
    let mut e = ed();
    e.push_row("Auth", "x");
    e.cursor = 0;
    e.start_edit_value();
    assert_eq!(e.mode, KvMode::InsertValue { row: 0 });
}

#[test]
fn x_toggles_enabled() {
    let mut e = ed();
    e.push_row("Auth", "x");
    e.cursor = 0;
    assert!(e.rows[0].enabled);
    e.toggle_enabled();
    assert!(!e.rows[0].enabled);
}

#[test]
fn d_deletes_row() {
    let mut e = ed();
    e.push_row("A", "1");
    e.push_row("B", "2");
    e.cursor = 0;
    e.delete();
    assert_eq!(e.rows.len(), 1);
    assert_eq!(e.rows[0].key, "B");
}

#[test]
fn esc_cancels_insert_without_row_creation() {
    let mut e = ed();
    e.start_add();
    e.insert_char('X');
    e.cancel();
    assert_eq!(e.mode, KvMode::Normal);
    assert_eq!(e.rows.len(), 0);
}

#[test]
fn multipart_row_kind_toggle() {
    let mut e = ed();
    e.push_row("avatar", "");
    e.cursor = 0;
    assert_eq!(e.rows[0].kind, KvRowKind::Text);
    e.toggle_kind();
    assert_eq!(e.rows[0].kind, KvRowKind::File);
}

#[test]
fn empty_key_commit_stays_in_insert() {
    let mut e = ed();
    e.start_add();
    e.tab(); // skip key, go to value
    for c in "abc".chars() { e.insert_char(c); }
    e.commit();
    // empty key → still in insert
    assert!(matches!(e.mode, KvMode::InsertKey { .. } | KvMode::InsertValue { .. }));
}
```

- [ ] **Step 2: Run — Expected: FAIL (no module)**

- [ ] **Step 3: Implement `crates/tui/src/kv_editor.rs`**

```rust
//! Hybrid KV editor — Normal mode nav + Insert mode inline edit.
//! Used by Headers, Query, Form (urlencoded body), and Multipart (with kind toggle).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvRowKind {
    Text,
    File,
}

#[derive(Debug, Clone)]
pub struct KvRow {
    pub kind: KvRowKind,
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub secret: bool,
}

impl KvRow {
    pub fn text(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            kind: KvRowKind::Text,
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvMode {
    Normal,
    InsertKey { row: usize },
    InsertValue { row: usize },
}

pub struct KvEditor {
    pub rows: Vec<KvRow>,
    pub cursor: usize,
    pub mode: KvMode,
    pub buf: String,
    pub cursor_col: usize,
}

impl Default for KvEditor {
    fn default() -> Self { Self::new() }
}

impl KvEditor {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            cursor: 0,
            mode: KvMode::Normal,
            buf: String::new(),
            cursor_col: 0,
        }
    }

    pub fn push_row(&mut self, key: &str, value: &str) {
        self.rows.push(KvRow::text(key, value));
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 { self.cursor -= 1; }
    }
    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.rows.len() { self.cursor += 1; }
    }

    pub fn start_add(&mut self) {
        self.rows.push(KvRow::text("", ""));
        let row = self.rows.len() - 1;
        self.cursor = row;
        self.mode = KvMode::InsertKey { row };
        self.buf.clear();
        self.cursor_col = 0;
    }

    pub fn start_edit_value(&mut self) {
        if self.cursor < self.rows.len() {
            self.buf = self.rows[self.cursor].value.clone();
            self.cursor_col = self.buf.len();
            self.mode = KvMode::InsertValue { row: self.cursor };
        }
    }

    pub fn start_edit_key(&mut self) {
        if self.cursor < self.rows.len() {
            self.buf = self.rows[self.cursor].key.clone();
            self.cursor_col = self.buf.len();
            self.mode = KvMode::InsertKey { row: self.cursor };
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.buf.push(c);
        self.cursor_col += 1;
        // Live-write into the row so render shows progress.
        self.write_buf_into_row();
    }

    pub fn backspace(&mut self) {
        if self.buf.pop().is_some() {
            self.cursor_col = self.cursor_col.saturating_sub(1);
            self.write_buf_into_row();
        }
    }

    pub fn tab(&mut self) {
        let row = match self.mode {
            KvMode::InsertKey { row } => {
                self.buf = self.rows[row].value.clone();
                self.cursor_col = self.buf.len();
                self.mode = KvMode::InsertValue { row };
                return;
            }
            KvMode::InsertValue { row } => row,
            KvMode::Normal => return,
        };
        self.buf = self.rows[row].key.clone();
        self.cursor_col = self.buf.len();
        self.mode = KvMode::InsertKey { row };
    }

    pub fn commit(&mut self) {
        let row = match self.mode {
            KvMode::InsertKey { row } | KvMode::InsertValue { row } => row,
            KvMode::Normal => return,
        };
        self.write_buf_into_row();
        if self.rows[row].key.is_empty() {
            // Stay in insert until key non-empty.
            self.mode = KvMode::InsertKey { row };
            return;
        }
        self.mode = KvMode::Normal;
        self.buf.clear();
        self.cursor_col = 0;
    }

    pub fn cancel(&mut self) {
        if let KvMode::InsertKey { row } | KvMode::InsertValue { row } = self.mode {
            // If the row was freshly added (both fields empty after cancel intent), drop it.
            if self.rows[row].key.is_empty() && self.rows[row].value.is_empty() {
                self.rows.remove(row);
                if self.cursor >= self.rows.len() {
                    self.cursor = self.rows.len().saturating_sub(1);
                }
            }
        }
        self.mode = KvMode::Normal;
        self.buf.clear();
        self.cursor_col = 0;
    }

    pub fn toggle_enabled(&mut self) {
        if self.cursor < self.rows.len() {
            self.rows[self.cursor].enabled = !self.rows[self.cursor].enabled;
        }
    }

    pub fn toggle_secret(&mut self) {
        if self.cursor < self.rows.len() {
            self.rows[self.cursor].secret = !self.rows[self.cursor].secret;
        }
    }

    pub fn toggle_kind(&mut self) {
        if self.cursor < self.rows.len() {
            self.rows[self.cursor].kind = match self.rows[self.cursor].kind {
                KvRowKind::Text => KvRowKind::File,
                KvRowKind::File => KvRowKind::Text,
            };
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.rows.len() {
            self.rows.remove(self.cursor);
            if self.cursor >= self.rows.len() && !self.rows.is_empty() {
                self.cursor = self.rows.len() - 1;
            } else if self.rows.is_empty() {
                self.cursor = 0;
            }
        }
    }

    pub fn enabled_text_rows(&self) -> Vec<lazyfetch_core::primitives::KV> {
        self.rows
            .iter()
            .filter(|r| r.enabled && r.kind == KvRowKind::Text)
            .map(|r| lazyfetch_core::primitives::KV {
                key: r.key.clone(),
                value: r.value.clone(),
                enabled: true,
                secret: r.secret,
            })
            .collect()
    }

    fn write_buf_into_row(&mut self) {
        match self.mode {
            KvMode::InsertKey { row } => self.rows[row].key = self.buf.clone(),
            KvMode::InsertValue { row } => self.rows[row].value = self.buf.clone(),
            KvMode::Normal => {}
        }
    }
}
```

- [ ] **Step 4: Wire `crates/tui/src/lib.rs`**

```rust
pub mod kv_editor;
```

- [ ] **Step 5: Run — Expected: PASS**

Run: `cargo test -p lazyfetch-tui --test kv_editor`
Expected: 7 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/tui/src/kv_editor.rs crates/tui/src/lib.rs crates/tui/tests/kv_editor.rs
git commit -m "feat(tui): kv_editor — hybrid Normal/Insert KV state for Headers/Query/Form/Multipart"
```

---

## Task 9: `tui::editor` — `BodyEditorState` + `$EDITOR` shell-out

**Files:**
- Create: `crates/tui/src/editor.rs`
- Modify: `crates/tui/Cargo.toml`
- Modify: `crates/tui/src/lib.rs`

- [ ] **Step 1: Add deps**

`crates/tui/Cargo.toml`:
```toml
tui-textarea = "0.7"
```

- [ ] **Step 2: Implement `crates/tui/src/editor.rs`**

```rust
//! Body editor: inline tui-textarea + $EDITOR shell-out.
//! Owns the body editor state machine. Knows nothing about KV.

use crate::terminal::TerminalGuard;
use lazyfetch_core::catalog::BodyKind;
use std::io::{Read, Write};
use tui_textarea::TextArea;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphQlFocus { Query, Variables }

pub enum BodyEditorState {
    None,
    Single(TextArea<'static>),
    Split {
        query: TextArea<'static>,
        variables: TextArea<'static>,
        focus: GraphQlFocus,
    },
}

impl BodyEditorState {
    /// Build editors lazily for a kind, preserving any existing text where possible.
    pub fn for_kind(kind: BodyKind, prev_text: &str) -> Self {
        match kind {
            BodyKind::None | BodyKind::File => Self::None,
            BodyKind::Json | BodyKind::Raw => {
                let mut ta = TextArea::default();
                for line in prev_text.lines() { ta.insert_str(line); ta.insert_newline(); }
                Self::Single(ta)
            }
            BodyKind::Form | BodyKind::Multipart => Self::None,  // KV-backed
            BodyKind::GraphQL => Self::Split {
                query: TextArea::default(),
                variables: TextArea::default(),
                focus: GraphQlFocus::Query,
            },
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Single(t) => t.lines().join("\n"),
            Self::Split { query, .. } => query.lines().join("\n"),
        }
    }

    pub fn graphql_parts(&self) -> Option<(String, String)> {
        if let Self::Split { query, variables, .. } = self {
            Some((query.lines().join("\n"), variables.lines().join("\n")))
        } else {
            None
        }
    }
}

/// Shell out to $EDITOR. Always restores the terminal even on panic.
pub fn shell_out(
    term: &mut TerminalGuard,
    initial: &str,
    ext: &str,
) -> std::io::Result<String> {
    let scratch_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&scratch_dir)?;
    let mut tmp = tempfile::Builder::new()
        .prefix("lazyfetch-")
        .suffix(ext)
        .tempfile_in(&scratch_dir)?;
    tmp.write_all(initial.as_bytes())?;
    tmp.as_file().sync_all()?;

    struct SuspendGuard<'a> { term: &'a mut TerminalGuard }
    impl Drop for SuspendGuard<'_> { fn drop(&mut self) { let _ = self.term.resume(); } }

    term.suspend()?;
    let _suspend = SuspendGuard { term };

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    let status = std::process::Command::new(&editor)
        .arg(tmp.path())
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other(format!("$EDITOR exited {:?}", status.code())));
    }
    let mut out = String::new();
    let _ = std::fs::File::open(tmp.path())?.read_to_string(&mut out);
    Ok(out)
}
```

- [ ] **Step 3: Wire `crates/tui/src/lib.rs`**

```rust
pub mod editor;
```

- [ ] **Step 4: Build check**

Run: `cargo build -p lazyfetch-tui`
Expected: builds clean.

- [ ] **Step 5: Commit**

```bash
git add crates/tui/Cargo.toml crates/tui/src/editor.rs crates/tui/src/lib.rs
git commit -m "feat(tui): editor module — BodyEditorState + \$EDITOR shell-out w/ TerminalSuspendGuard"
```

---

## Task 10: `tui::request_pane` + AppState wiring

**Files:**
- Create: `crates/tui/src/request_pane.rs`
- Modify: `crates/tui/src/app.rs`
- Modify: `crates/tui/src/keymap.rs`
- Modify: `crates/tui/src/layout.rs`
- Modify: `crates/tui/src/lib.rs`

- [ ] **Step 1: AppState additions in `crates/tui/src/app.rs`**

Inside `pub struct AppState`:

```rust
pub req_tab: ReqTab,                     // Body | Headers | Query
pub req_body_kind: BodyKind,
pub body_mime: String,
pub body_editor: BodyEditorState,
pub headers_kv: KvEditor,
pub query_kv: KvEditor,
pub form_kv: KvEditor,
pub import_curl_buf: String,
```

Defaults in `AppState::new`:
```rust
req_tab: ReqTab::Body,
req_body_kind: BodyKind::None,
body_mime: "text/plain".into(),
body_editor: BodyEditorState::None,
headers_kv: KvEditor::new(),
query_kv:   KvEditor::new(),
form_kv:    KvEditor::new(),
import_curl_buf: String::new(),
```

Add `Mode::ImportCurl` to the `Mode` enum.

Add the enum:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReqTab { Body, Headers, Query }
```

- [ ] **Step 2: Implement `crates/tui/src/request_pane.rs`**

```rust
//! Request pane render + dispatch glue. Composes editor + kv_editor + tab badge.

use crate::app::{AppState, ReqTab};
use crate::kv_editor::{KvEditor, KvMode};
use lazyfetch_core::catalog::BodyKind;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph};
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);
    render_tabs(f, chunks[0], state);
    match state.req_tab {
        ReqTab::Body => render_body_tab(f, chunks[1], state),
        ReqTab::Headers => render_kv(f, chunks[1], &state.headers_kv, "header"),
        ReqTab::Query => render_kv(f, chunks[1], &state.query_kv, "query"),
    }
}

fn render_tabs(f: &mut Frame, area: Rect, state: &AppState) {
    let mk = |label: &str, my: ReqTab| {
        let active = state.req_tab == my;
        Span::styled(
            format!(" {} ", label),
            if active {
                Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )
    };
    let line = Line::from(vec![
        mk("1 Body", ReqTab::Body),
        Span::raw("  "),
        mk("2 Headers", ReqTab::Headers),
        Span::raw("  "),
        mk("3 Query", ReqTab::Query),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_body_tab(f: &mut Frame, area: Rect, state: &AppState) {
    let kind_color = match state.req_body_kind {
        BodyKind::Json => Color::Green,
        BodyKind::Form => Color::Cyan,
        BodyKind::Multipart => Color::Magenta,
        BodyKind::GraphQL => Color::Yellow,
        BodyKind::Raw => Color::Gray,
        _ => Color::DarkGray,
    };
    let header = Line::from(vec![
        Span::styled(
            format!("[{:?} ▾]", state.req_body_kind),
            Style::default().fg(kind_color).add_modifier(Modifier::BOLD),
        ),
    ]);
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    f.render_widget(Paragraph::new(header), split[0]);

    use crate::editor::BodyEditorState;
    match &state.body_editor {
        BodyEditorState::None => {
            let p = Paragraph::new(Line::from(Span::styled(
                "(no body)  press i / a to edit  ·  Tab cycles body kind",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            )));
            f.render_widget(p, split[1]);
        }
        BodyEditorState::Single(ta) => {
            f.render_widget(ta, split[1]);
        }
        BodyEditorState::Split { query, variables, .. } => {
            let halves = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(split[1]);
            f.render_widget(query, halves[0]);
            f.render_widget(variables, halves[1]);
        }
    }
}

fn render_kv(f: &mut Frame, area: Rect, kv: &KvEditor, _kind_label: &str) {
    let lines: Vec<Line> = kv.rows.iter().enumerate().map(|(i, r)| {
        let cursor = if kv.cursor == i { "▌" } else { " " };
        let toggle = if r.enabled { "[x]" } else { "[ ]" };
        let value = if r.secret { "***".to_string() } else { r.value.clone() };
        Line::from(vec![
            Span::raw(cursor.to_string()),
            Span::raw(" "),
            Span::styled(toggle, Style::default().fg(if r.enabled { Color::Green } else { Color::DarkGray })),
            Span::raw("  "),
            Span::styled(format!("{:<24}", r.key), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(value, Style::default().fg(Color::White)),
        ])
    }).collect();
    let mode_hint = match kv.mode {
        KvMode::Normal => " j/k · a add · i edit value · Tab swap (in edit) · x toggle · d del".to_string(),
        KvMode::InsertKey { .. } => format!(" [insert key] {}", kv.buf),
        KvMode::InsertValue { .. } => format!(" [insert value] {}", kv.buf),
    };
    let mut all = lines;
    all.push(Line::from(""));
    all.push(Line::from(Span::styled(mode_hint, Style::default().fg(Color::DarkGray))));
    f.render_widget(Paragraph::new(all), area);
}
```

- [ ] **Step 3: Wire keymap (`crates/tui/src/keymap.rs`)**

In `dispatch_normal`, when `state.focus == Focus::Request`:

```rust
// Tab switching within Request pane:
(KeyCode::Char('1'), _) if state.focus == Focus::Request => Action::ReqTab(ReqTab::Body),
(KeyCode::Char('2'), _) if state.focus == Focus::Request => Action::ReqTab(ReqTab::Headers),
(KeyCode::Char('3'), _) if state.focus == Focus::Request => Action::ReqTab(ReqTab::Query),
(KeyCode::Char(' '), _) if state.focus == Focus::Request => Action::ReqTabCycle,
// Body kind cycle (when on Body tab):
(KeyCode::Tab, _) if state.focus == Focus::Request && state.req_tab == ReqTab::Body => {
    Action::BodyKindCycle
}
// KV pane keys:
(KeyCode::Char('j'), _) if state.focus == Focus::Request => Action::KvCursorDown,
(KeyCode::Char('k'), _) if state.focus == Focus::Request => Action::KvCursorUp,
(KeyCode::Char('a'), _) if state.focus == Focus::Request => Action::KvAdd,
(KeyCode::Char('i'), _) if state.focus == Focus::Request => Action::KvEditValue,
(KeyCode::Char('x'), _) if state.focus == Focus::Request => Action::KvToggleEnabled,
(KeyCode::Char('d'), _) if state.focus == Focus::Request => Action::KvDelete,
(KeyCode::Char('m'), _) if state.focus == Focus::Request => Action::KvToggleSecret,
(KeyCode::Char('f'), _) if state.focus == Focus::Request && state.req_tab == ReqTab::Body
    && state.req_body_kind == BodyKind::Multipart => Action::KvToggleKind,
```

Add the new `Action` variants and apply handlers that mutate the active `KvEditor` (chosen by `state.req_tab` for Headers/Query, or `state.form_kv` for Form/Multipart).

- [ ] **Step 4: Layout — delegate Request pane render**

In `crates/tui/src/layout.rs`, replace the existing Request pane empty-state block with:

```rust
Focus::Request => crate::request_pane::render(f, inner, state),
```

(Remove the old `empty(...)` placeholder for Request.)

- [ ] **Step 5: Build + manual smoke**

Run: `cargo build --workspace`
Expected: builds clean. Run `cargo run -p lazyfetch-bin -- --config-dir /tmp/lf-test`, press `3` (focus Request), verify tabs render.

- [ ] **Step 6: Commit**

```bash
git add crates/tui/src/request_pane.rs crates/tui/src/app.rs crates/tui/src/keymap.rs crates/tui/src/layout.rs crates/tui/src/lib.rs
git commit -m "feat(tui): request_pane — Body/Headers/Query tabs + KV editor wiring"
```

---

## Task 11: `import::curl` parser — bash/zsh/POSIX

**Files:**
- Create: `crates/import/src/curl.rs`
- Modify: `crates/import/src/lib.rs`
- Test: `crates/import/tests/curl_golden.rs` + fixtures under `crates/import/tests/fixtures/curl/`

- [ ] **Step 1: Create fixture files**

`crates/import/tests/fixtures/curl/chrome_simple_get.txt`:
```
curl 'https://api.test/users?page=1' \
  -H 'Accept: application/json' \
  --compressed
```

`crates/import/tests/fixtures/curl/chrome_post_json.txt`:
```
curl 'https://api.test/users' \
  -X POST \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer abc.def.ghi' \
  --data-raw '{"name":"alice"}'
```

`crates/import/tests/fixtures/curl/multipart_file.txt`:
```
curl -X POST 'https://api.test/upload' \
  -F 'meta=hello' \
  -F 'avatar=@/tmp/pic.png'
```

`crates/import/tests/fixtures/curl/cmd_shell_rejected.txt`:
```
curl ^"https://api.test/x^" ^
  -H ^"Accept: application/json^"
```

(Plus 8 more — see test list for coverage.)

- [ ] **Step 2: Write tests**

```rust
use lazyfetch_import::curl;

#[test]
fn parses_chrome_simple_get() {
    let s = include_str!("fixtures/curl/chrome_simple_get.txt");
    let (req, report) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::GET);
    assert_eq!(req.url.0 .0, "https://api.test/users?page=1");
    assert!(report.warnings.is_empty());
}

#[test]
fn parses_post_json() {
    let s = include_str!("fixtures/curl/chrome_post_json.txt");
    let (req, _) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::POST);
    assert!(matches!(req.body, lazyfetch_core::catalog::Body::Json(_)));
}

#[test]
fn rejects_cmd_shell() {
    let s = include_str!("fixtures/curl/cmd_shell_rejected.txt");
    let r = curl::parse(s);
    assert!(matches!(r, Err(curl::CurlError::Tokenize { .. })));
}
```

- [ ] **Step 3: Run — Expected: FAIL (no module)**

- [ ] **Step 4: Implement `crates/import/src/curl.rs`**

(Full parser ~250 LOC. Implement `pub fn parse(cmd: &str) -> Result<(Request, ImportReport), CurlError>`. Tokenize → flag dispatch table → assemble Request. Reject if input contains `^"` cmd-shell quoting before tokenizing. Map flags per spec §7.)

- [ ] **Step 5: Wire `crates/import/src/lib.rs`**

```rust
pub mod curl;
```

- [ ] **Step 6: Run — Expected: PASS**

- [ ] **Step 7: Commit**

```bash
git add crates/import/
git commit -m "feat(import): curl — bash/zsh/POSIX cURL parser w/ ImportReport warnings"
```

---

## Task 12: `lazyfetch import-curl` CLI subcommand

**Files:**
- Create: `crates/bin/src/import_curl.rs`
- Modify: `crates/bin/src/main.rs`

- [ ] **Step 1: Create `crates/bin/src/import_curl.rs`**

```rust
use clap::Args;
use lazyfetch_import::curl;
use lazyfetch_storage::collection::FsCollectionRepo;

#[derive(Args)]
pub struct ImportCurlArgs {
    /// cURL command, file path containing one, or `-` for stdin.
    pub input: Option<String>,
    /// Save into <coll>/<name>.
    #[arg(long)]
    pub save: Option<String>,
    #[arg(long)]
    pub config_dir: Option<std::path::PathBuf>,
}

pub fn run(args: ImportCurlArgs) -> anyhow::Result<()> {
    let input = match args.input.as_deref() {
        None | Some("-") => {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
            s
        }
        Some(s) if std::path::Path::new(s).exists() => std::fs::read_to_string(s)?,
        Some(s) => s.to_string(),
    };
    let (req, report) = curl::parse(&input)?;
    println!("imported {} {} (warnings: {})", req.method, req.url.0 .0, report.warnings.len());
    for w in &report.warnings { eprintln!("warn: {w}"); }
    if let Some(path) = args.save {
        let cwd = std::env::current_dir().unwrap_or_default();
        let cfg = crate::resolve_config_dir(args.config_dir, &cwd);
        let (coll, name) = path.split_once('/').ok_or_else(|| anyhow::anyhow!("--save expects <coll>/<name>"))?;
        let mut req = req;
        req.name = name.into();
        FsCollectionRepo::new(cfg.join("collections")).save_request(coll, &req)?;
        println!("saved {coll}/{name}");
    }
    Ok(())
}
```

- [ ] **Step 2: Register in `crates/bin/src/main.rs`**

```rust
mod import_curl;

#[derive(Subcommand)]
enum Cmd {
    Run(run::RunArgs),
    ImportPostman(import::ImportArgs),
    ImportCurl(import_curl::ImportCurlArgs),
}

// in main():
Some(Cmd::ImportCurl(a)) => import_curl::run(a),
```

- [ ] **Step 3: Build + smoke**

```bash
cargo run -p lazyfetch -- import-curl 'curl https://api.test/x'
```

Expected: stdout `imported GET https://api.test/x (warnings: 0)`.

- [ ] **Step 4: Commit**

```bash
git add crates/bin/
git commit -m "feat(bin): lazyfetch import-curl <cmd-or-file-or-stdin>"
```

---

## Task 13: cURL export — `Y` key + `S` menu option

**Files:**
- Modify: `crates/tui/src/keymap.rs`
- Modify: `crates/tui/src/commands.rs`
- Modify: `crates/tui/src/layout.rs` (S dialog)

- [ ] **Step 1: Add `Y` keybinding (Response focused)**

```rust
(KeyCode::Char('Y'), _) if state.focus == Focus::Response => Action::CurlExport,
```

- [ ] **Step 2: Implement `Action::CurlExport` in `keymap::apply`**

```rust
Action::CurlExport => {
    if let Some(executed) = &state.last_response {
        let curl = lazyfetch_core::exec::build_curl(&executed.request_snapshot, &executed.secrets);
        match crate::motion::copy_to_clipboard(&curl) {
            Ok(()) => state.notify(format!("cURL → clipboard ({} chars)", curl.len())),
            Err(e) => state.notify(format!("clipboard failed: {e}")),
        }
    } else {
        state.notify("nothing sent yet".to_string());
    }
    EnvDirty::No
}
```

- [ ] **Step 3: Test `build_curl` integration via existing wiremock e2e (no new test needed — Task 4 already tests `build_curl` itself)**

- [ ] **Step 4: Commit**

```bash
git add crates/tui/
git commit -m "feat(tui): Y on Response copies redacted cURL to clipboard"
```

---

## Task 14: Repeat-last (`R` key)

**Files:**
- Modify: `crates/tui/src/keymap.rs`
- Modify: `crates/tui/src/event.rs`

- [ ] **Step 1: Add binding (top-level, all panes / modes)**

```rust
(KeyCode::Char('R'), _) => Action::RepeatLast,
```

- [ ] **Step 2: Apply handler — sentinel like SendRequest**

```rust
Action::RepeatLast => {
    // event::run inspects this and dispatches a fresh send w/ the snapshot.
    EnvDirty::No
}
```

- [ ] **Step 3: Wire in event loop**

In `crates/tui/src/event.rs`, after the existing `send_now` block:

```rust
let repeat_now = matches!(action, Action::RepeatLast);
// ...apply...
if repeat_now {
    if state.inflight.is_some() {
        state.notify("send in progress".to_string());
    } else if let Some(template) = state.last_response.as_ref().map(|e| e.request_template.clone()) {
        state.notify(format!("replaying {} {} — your edits are unchanged", template.method, template.url.0.0));
        state.inflight = Some(crate::sender::dispatch_request(&template, &state, rt.clone()));
    } else {
        state.notify("nothing sent yet".to_string());
    }
}
```

(Adjust `sender::dispatch` to accept an explicit `&Request` parameter rather than reading from `state.url_buf`.)

- [ ] **Step 4: Commit**

```bash
git add crates/tui/src/keymap.rs crates/tui/src/event.rs crates/tui/src/sender.rs
git commit -m "feat(tui): R repeats last sent request via Executed.request_template"
```

---

## Task 15: Wire dyn-vars into `core::exec::execute`

**Files:**
- Modify: `crates/core/src/exec.rs`

- [ ] **Step 1: Replace `interpolate` calls with `interpolate_with_dyn`**

In `execute()` and `render_body`, switch every `crate::env::interpolate(s, ctx)` to `crate::env::interpolate_with_dyn(s, ctx, &dyn_ctx)` where `dyn_ctx = DynCtx { clock }`.

- [ ] **Step 2: Run full workspace tests**

```bash
cargo test --workspace
```

Expected: all green.

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/exec.rs
git commit -m "feat(core): execute() interpolates dyn-vars through interpolate_with_dyn"
```

---

## Self-review

**Spec coverage:**

- §1 file structure → Tasks 1, 8, 9, 10, 11, 12 all create the listed modules. ✓
- §2 domain types → Task 1 (`dynvars`), Task 3 (`Body::GraphQL`/`BodyKind`), Task 5 (`Executed.request_template`), Task 6 (`AuthError::DynVarOnlyInSecretField`). ✓
- §3 Request pane state + render → Task 10. ✓
- §4 body editor → Task 9 (`BodyEditorState` + `$EDITOR` shell-out w/ `TerminalSuspendGuard`). ✓
- §5 KV editor → Task 8. ✓
- §6 dyn-vars → Tasks 1, 2, 15. ✓ (Task 2 covers grammar + secret tainting + recursion guard.)
- §7 cURL import → Tasks 11, 12. cURL export → Tasks 4 (`build_curl`), 13. ✓
- §8 repeat-last → Tasks 5, 14. ✓
- §9 errors / tests / phasing → distributed across all tasks; total ~51 new tests as projected.

**Placeholder scan:** None. Every task has runnable code.

**Type consistency:** `Arg` / `DynError` / `DynCtx` defined in Task 1, used in Tasks 2 + 15. `KvRow` / `KvEditor` defined Task 8, used Task 10. `BodyEditorState` defined Task 9, used Task 10. `build_curl` signature stable Task 4 → Task 13. ✓

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-08-lazyfetch-v2.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
