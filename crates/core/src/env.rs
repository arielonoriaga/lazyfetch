use crate::error::CoreError;
use crate::primitives::Id;
use crate::secret::SecretRegistry;
use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct VarValue {
    pub value: SecretString,
    pub secret: bool,
}

pub type VarSet = Vec<(String, VarValue)>;

#[derive(Debug, Clone)]
pub struct Environment {
    pub id: Id,
    pub name: String,
    pub vars: VarSet,
}

pub struct ResolveCtx<'a> {
    pub env: &'a Environment,
    pub collection_vars: &'a [(String, VarValue)],
    pub overrides: &'a [(String, VarValue)],
}

#[derive(Debug, Clone)]
pub struct Interpolated {
    pub value: String,
    pub used_secrets: SecretRegistry,
}

fn lookup<'a>(name: &str, ctx: &'a ResolveCtx<'a>) -> Option<&'a VarValue> {
    ctx.overrides
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
        .or_else(|| ctx.env.vars.iter().find(|(k, _)| k == name).map(|(_, v)| v))
        .or_else(|| {
            ctx.collection_vars
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v)
        })
}

use crate::dynvars::{self, Arg, DynCtx, DynError};

const MAX_DEPTH: u8 = 8;

/// Like `interpolate` but also resolves `{{$name(args)}}` dyn-vars and
/// propagates secret tainting through dyn-var arg resolution.
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
        return Err(CoreError::InvalidTemplate(format!(
            "dyn-var depth >{MAX_DEPTH}"
        )));
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
    Ok(Interpolated {
        value: out,
        used_secrets: reg,
    })
}

/// Split off everything up to the matching `}}`. Returns (inner, after).
/// Tracks `{{ ... }}` nesting so `{{$base64({{X}})}}` is one outer token.
fn take_token(s: &str) -> Result<(&str, &str), CoreError> {
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
    let (name, raw_args) = parse_name_args(spec)
        .map_err(|m| CoreError::InvalidTemplate(format!("dyn-var ${spec}: {m}")))?;
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
    Quoted(String),
    VarRef(String),
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
    let after = after
        .strip_prefix('(')
        .ok_or_else(|| format!("expected '(' after ${name}, got '{after}'"))?;
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
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
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
            if c.is_whitespace() {
                chars.next();
            } else {
                break;
            }
        }
        match chars.peek() {
            None => break,
            Some(&',') => {
                chars.next();
            }
            Some(&c) => return Err(format!("unexpected char '{c}' between args")),
        }
    }
    Ok(out)
}

fn parse_quoted(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    quote: char,
) -> Result<RawArg, String> {
    chars.next();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == quote {
            return Ok(RawArg::Quoted(out));
        }
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

fn parse_var_ref(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<RawArg, String> {
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
                if close2 != '}' {
                    return Err("expected '}}'".into());
                }
                return Ok(RawArg::VarRef(name));
            }
            Some(c) => name.push(c),
            None => return Err("unterminated {{var}}".into()),
        }
    }
}

fn parse_bareword(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<RawArg, String> {
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
        return Err(CoreError::InvalidTemplate(format!(
            "dyn-var depth >{MAX_DEPTH}"
        )));
    }
    match raw {
        RawArg::Quoted(s) | RawArg::Bareword(s) => Ok((s.clone(), SecretRegistry::new())),
        RawArg::VarRef(name) => {
            let v = lookup(name, ctx).ok_or_else(|| CoreError::MissingVar(name.clone()))?;
            let val = secrecy::ExposeSecret::expose_secret(&v.value).clone();
            let mut reg = SecretRegistry::new();
            if v.secret {
                reg.insert(val.clone());
            }
            // Only further-recurse if the resolved value itself contains a dyn-var token.
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

pub fn interpolate(s: &str, ctx: &ResolveCtx) -> Result<Interpolated, CoreError> {
    let mut out = String::with_capacity(s.len());
    let mut reg = SecretRegistry::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| CoreError::InvalidTemplate(s.into()))?;
        let name = after[..end].trim();
        let v = lookup(name, ctx).ok_or_else(|| CoreError::MissingVar(name.into()))?;
        let val = v.value.expose_secret();
        out.push_str(val);
        if v.secret {
            reg.insert(val.clone());
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(Interpolated {
        value: out,
        used_secrets: reg,
    })
}
