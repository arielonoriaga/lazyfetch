//! cURL command parser. Targets bash / zsh / POSIX shell quoting.
//! Rejects cmd.exe / PowerShell input (`^"` carets, `\""` doubled-quotes).

use lazyfetch_core::auth::AuthSpec;
use lazyfetch_core::catalog::{Body, Request};
use lazyfetch_core::primitives::{KV, Template, UrlTemplate};
use thiserror::Error;
use ulid::Ulid;

#[derive(Debug, Error)]
pub enum CurlError {
    #[error("tokenize: {msg}")]
    Tokenize { msg: String },
    #[error("flag {which}: {msg}")]
    Flag { which: String, msg: String },
    #[error("missing url")]
    MissingUrl,
}

#[derive(Debug, Default, Clone)]
pub struct ImportReport {
    pub warnings: Vec<String>,
}

/// Derive a display name from a URL: prefer the trailing path segment, else host, else `imported`.
fn derive_name(url: &str) -> String {
    let after_scheme = url.split("://").nth(1).unwrap_or(url);
    let path_only = after_scheme.split('?').next().unwrap_or(after_scheme);
    let last = path_only
        .rsplit('/')
        .find(|seg| !seg.is_empty())
        .unwrap_or("");
    if !last.is_empty() && !last.contains('.') {
        return last.to_string();
    }
    let host = after_scheme.split('/').next().unwrap_or("");
    if !host.is_empty() {
        host.to_string()
    } else {
        "imported".to_string()
    }
}

pub fn parse(cmd: &str) -> Result<(Request, ImportReport), CurlError> {
    if cmd.contains("^\"") {
        return Err(CurlError::Tokenize {
            msg: "cmd.exe quoting not supported; use Copy as cURL (bash)".into(),
        });
    }
    let tokens = tokenize(cmd)?;
    assemble(tokens)
}

/// POSIX shell tokenizer. Handles:
/// - Single quotes (literal — no escapes inside)
/// - Double quotes (POSIX escapes: `\\`, `\"`, `\$`, `\``)
/// - `$'...'` ANSI-C strings (`\n`, `\r`, `\t`, `\\`, `\'` decoded)
/// - Backslash line continuations + bare backslash escapes outside quotes
fn tokenize(input: &str) -> Result<Vec<String>, CurlError> {
    let mut it = input.chars().peekable();
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;

    while let Some(&c) = it.peek() {
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                if in_token {
                    out.push(std::mem::take(&mut cur));
                    in_token = false;
                }
                it.next();
            }
            '\\' => {
                it.next();
                match it.next() {
                    None => break,
                    Some('\n') => {} // line continuation
                    Some(n) => {
                        cur.push(n);
                        in_token = true;
                    }
                }
            }
            '\'' => {
                it.next();
                read_single_quoted(&mut it, &mut cur)?;
                in_token = true;
            }
            '"' => {
                it.next();
                read_double_quoted(&mut it, &mut cur)?;
                in_token = true;
            }
            '$' => {
                it.next();
                if matches!(it.peek(), Some('\'')) {
                    it.next();
                    read_ansi_c(&mut it, &mut cur)?;
                } else {
                    cur.push('$');
                }
                in_token = true;
            }
            _ => {
                cur.push(c);
                in_token = true;
                it.next();
            }
        }
    }
    if in_token {
        out.push(cur);
    }
    Ok(out)
}

fn read_single_quoted<I: Iterator<Item = char>>(
    it: &mut std::iter::Peekable<I>,
    cur: &mut String,
) -> Result<(), CurlError> {
    for c in it.by_ref() {
        if c == '\'' {
            return Ok(());
        }
        cur.push(c);
    }
    Err(CurlError::Tokenize {
        msg: "unterminated single quote".into(),
    })
}

fn read_double_quoted<I: Iterator<Item = char>>(
    it: &mut std::iter::Peekable<I>,
    cur: &mut String,
) -> Result<(), CurlError> {
    while let Some(c) = it.next() {
        match c {
            '"' => return Ok(()),
            '\\' => match it.next() {
                None => break,
                Some('\n') => {} // line continuation
                Some(n @ ('\\' | '"' | '$' | '`')) => cur.push(n),
                Some(n) => {
                    cur.push('\\');
                    cur.push(n);
                }
            },
            _ => cur.push(c),
        }
    }
    Err(CurlError::Tokenize {
        msg: "unterminated double quote".into(),
    })
}

fn read_ansi_c<I: Iterator<Item = char>>(
    it: &mut std::iter::Peekable<I>,
    cur: &mut String,
) -> Result<(), CurlError> {
    while let Some(c) = it.next() {
        match c {
            '\'' => return Ok(()),
            '\\' => match it.next() {
                None => break,
                Some('n') => cur.push('\n'),
                Some('r') => cur.push('\r'),
                Some('t') => cur.push('\t'),
                Some('\\') => cur.push('\\'),
                Some('\'') => cur.push('\''),
                Some('"') => cur.push('"'),
                Some('0') => cur.push('\0'),
                Some(other) => {
                    cur.push('\\');
                    cur.push(other);
                }
            },
            _ => cur.push(c),
        }
    }
    Err(CurlError::Tokenize {
        msg: "unterminated $'...' string".into(),
    })
}

fn assemble(mut tokens: Vec<String>) -> Result<(Request, ImportReport), CurlError> {
    if tokens.first().map(|s| s.as_str()) == Some("curl") {
        tokens.remove(0);
    }
    let mut report = ImportReport::default();
    let mut method: Option<http::Method> = None;
    let mut url: Option<String> = None;
    let mut headers: Vec<KV> = Vec::new();
    let mut data_parts: Vec<String> = Vec::new();
    let mut urlencode_parts: Vec<KV> = Vec::new();
    let mut form_parts: Vec<(String, String)> = Vec::new();
    let mut auth: Option<AuthSpec> = None;
    let mut force_get = false;
    let mut max_redirects: u8 = 10;
    let mut follow_redirects = true;

    let mut it = tokens.into_iter().peekable();
    while let Some(t) = it.next() {
        let need_val = |it: &mut std::iter::Peekable<std::vec::IntoIter<String>>,
                        flag: &str|
         -> Result<String, CurlError> {
            it.next().ok_or_else(|| CurlError::Flag {
                which: flag.into(),
                msg: "missing value".into(),
            })
        };
        match t.as_str() {
            "-X" | "--request" => {
                let v = need_val(&mut it, &t)?;
                method = Some(
                    http::Method::from_bytes(v.as_bytes()).map_err(|e| CurlError::Flag {
                        which: t.clone(),
                        msg: format!("{e}"),
                    })?,
                );
            }
            "-H" | "--header" => {
                let v = need_val(&mut it, &t)?;
                if let Some((k, val)) = v.split_once(':') {
                    headers.push(KV {
                        key: k.trim().into(),
                        value: val.trim().into(),
                        enabled: true,
                        secret: false,
                    });
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" => {
                data_parts.push(need_val(&mut it, &t)?);
            }
            "--data-urlencode" => {
                let v = need_val(&mut it, &t)?;
                let (k, val) = v.split_once('=').unwrap_or(("", v.as_str()));
                urlencode_parts.push(KV {
                    key: k.into(),
                    value: val.into(),
                    enabled: true,
                    secret: false,
                });
            }
            "-F" | "--form" => {
                let v = need_val(&mut it, &t)?;
                let (k, val) = v.split_once('=').ok_or_else(|| CurlError::Flag {
                    which: t.clone(),
                    msg: "expected key=value".into(),
                })?;
                form_parts.push((k.into(), val.into()));
            }
            "-G" | "--get" => {
                force_get = true;
            }
            "-u" | "--user" => {
                let v = need_val(&mut it, &t)?;
                let (user, pass) = match v.split_once(':') {
                    Some((u, p)) => (u.to_string(), p.to_string()),
                    None => {
                        report
                            .warnings
                            .push(format!("-u {v}: no password (will prompt at send)"));
                        (v.clone(), String::new())
                    }
                };
                auth = Some(AuthSpec::Basic {
                    user: Template(user),
                    pass: Template(pass),
                });
            }
            "--url" => {
                url = Some(need_val(&mut it, &t)?);
            }
            "--cookie" | "-b" => {
                headers.push(KV {
                    key: "Cookie".into(),
                    value: need_val(&mut it, &t)?,
                    enabled: true,
                    secret: false,
                });
            }
            "-A" | "--user-agent" => {
                headers.push(KV {
                    key: "User-Agent".into(),
                    value: need_val(&mut it, &t)?,
                    enabled: true,
                    secret: false,
                });
            }
            "-e" | "--referer" => {
                headers.push(KV {
                    key: "Referer".into(),
                    value: need_val(&mut it, &t)?,
                    enabled: true,
                    secret: false,
                });
            }
            "--compressed" => {
                headers.push(KV {
                    key: "Accept-Encoding".into(),
                    value: "gzip, deflate, br".into(),
                    enabled: true,
                    secret: false,
                });
            }
            "--max-redirs" => {
                let v = need_val(&mut it, &t)?;
                max_redirects = v.parse().map_err(|e| CurlError::Flag {
                    which: t.clone(),
                    msg: format!("{e}"),
                })?;
            }
            "-L" | "--location" => {
                follow_redirects = true;
            }
            "-k" | "--insecure" => {
                report
                    .warnings
                    .push("--insecure ignored: rustls verification is intentional".into());
            }
            "--proxy" => {
                let _ = need_val(&mut it, &t)?;
                report.warnings.push("--proxy ignored in v0.2".into());
            }
            other if other.starts_with('-') => {
                report.warnings.push(format!("unknown flag: {other}"));
            }
            _ => {
                if url.is_none() {
                    url = Some(t);
                } else {
                    report.warnings.push(format!("extra positional ignored: {t}"));
                }
            }
        }
    }

    let mut url = url.ok_or(CurlError::MissingUrl)?;

    let last_ct = headers
        .iter()
        .rev()
        .find(|h| h.key.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.clone());

    let body: Body;
    if !form_parts.is_empty() {
        let parts: Vec<lazyfetch_core::catalog::Part> = form_parts
            .into_iter()
            .map(|(name, val)| {
                if let Some(path) = val.strip_prefix('@') {
                    lazyfetch_core::catalog::Part {
                        name,
                        content: lazyfetch_core::catalog::PartContent::File(
                            std::path::PathBuf::from(path),
                        ),
                        filename: None,
                    }
                } else {
                    lazyfetch_core::catalog::Part {
                        name,
                        content: lazyfetch_core::catalog::PartContent::Text(val),
                        filename: None,
                    }
                }
            })
            .collect();
        body = Body::Multipart(parts);
    } else if force_get {
        let mut combined = data_parts.clone();
        for kv in &urlencode_parts {
            combined.push(if kv.key.is_empty() {
                kv.value.clone()
            } else {
                format!("{}={}", urlencoding::encode(&kv.key), urlencoding::encode(&kv.value))
            });
        }
        if !combined.is_empty() {
            let qs = combined.join("&");
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str(&qs);
        }
        body = Body::None;
    } else {
        let ct = last_ct.unwrap_or_default();
        let ct_lc = ct.to_ascii_lowercase();
        if !urlencode_parts.is_empty() || ct_lc == "application/x-www-form-urlencoded" {
            let mut rows = urlencode_parts;
            for d in &data_parts {
                if let Some((k, v)) = d.split_once('=') {
                    rows.push(KV {
                        key: k.into(),
                        value: v.into(),
                        enabled: true,
                        secret: false,
                    });
                }
            }
            body = Body::Form(rows);
        } else if !data_parts.is_empty() {
            let text = data_parts.join("&");
            if ct_lc == "application/json" {
                body = Body::Json(text);
            } else if ct.is_empty() {
                body = Body::Raw {
                    mime: "text/plain".into(),
                    text,
                };
            } else {
                body = Body::Raw { mime: ct, text };
            }
        } else {
            body = Body::None;
        }
    }

    let default_method = if matches!(&body, Body::None) {
        http::Method::GET
    } else {
        http::Method::POST
    };
    let final_method = if force_get {
        http::Method::GET
    } else {
        method.unwrap_or(default_method)
    };

    let req = Request {
        id: Ulid::new(),
        name: derive_name(&url),
        method: final_method,
        url: UrlTemplate(Template(url)),
        query: vec![],
        headers,
        body,
        auth,
        notes: None,
        follow_redirects,
        max_redirects,
        timeout_ms: None,
    };
    Ok((req, report))
}
