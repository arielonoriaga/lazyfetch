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
/// - Double quotes (POSIX escapes: `\\`, `\"`, `\$`, `\``, `\n` — `\n` kept as backslash-n to match user intent in headers)
/// - `$'...'` ANSI-C strings (`\n`, `\r`, `\t`, `\\`, `\'` decoded)
/// - Backslash line continuations
/// - Bare backslash escapes outside quotes
fn tokenize(input: &str) -> Result<Vec<String>, CurlError> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let bytes: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => {
                if in_token {
                    out.push(std::mem::take(&mut cur));
                    in_token = false;
                }
                i += 1;
            }
            '\\' if i + 1 < bytes.len() => {
                let n = bytes[i + 1];
                if n == '\n' {
                    i += 2;
                } else {
                    cur.push(n);
                    in_token = true;
                    i += 2;
                }
            }
            '\'' => {
                in_token = true;
                i += 1;
                while i < bytes.len() && bytes[i] != '\'' {
                    cur.push(bytes[i]);
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(CurlError::Tokenize {
                        msg: "unterminated single quote".into(),
                    });
                }
                i += 1;
            }
            '"' => {
                in_token = true;
                i += 1;
                while i < bytes.len() && bytes[i] != '"' {
                    if bytes[i] == '\\' && i + 1 < bytes.len() {
                        let n = bytes[i + 1];
                        match n {
                            '\\' | '"' | '$' | '`' => {
                                cur.push(n);
                                i += 2;
                            }
                            '\n' => i += 2,
                            _ => {
                                cur.push('\\');
                                cur.push(n);
                                i += 2;
                            }
                        }
                    } else {
                        cur.push(bytes[i]);
                        i += 1;
                    }
                }
                if i >= bytes.len() {
                    return Err(CurlError::Tokenize {
                        msg: "unterminated double quote".into(),
                    });
                }
                i += 1;
            }
            '$' if i + 1 < bytes.len() && bytes[i + 1] == '\'' => {
                in_token = true;
                i += 2;
                while i < bytes.len() && bytes[i] != '\'' {
                    if bytes[i] == '\\' && i + 1 < bytes.len() {
                        let n = bytes[i + 1];
                        match n {
                            'n' => cur.push('\n'),
                            'r' => cur.push('\r'),
                            't' => cur.push('\t'),
                            '\\' => cur.push('\\'),
                            '\'' => cur.push('\''),
                            '"' => cur.push('"'),
                            '0' => cur.push('\0'),
                            other => {
                                cur.push('\\');
                                cur.push(other);
                            }
                        }
                        i += 2;
                    } else {
                        cur.push(bytes[i]);
                        i += 1;
                    }
                }
                if i >= bytes.len() {
                    return Err(CurlError::Tokenize {
                        msg: "unterminated $'...' string".into(),
                    });
                }
                i += 1;
            }
            _ => {
                cur.push(c);
                in_token = true;
                i += 1;
            }
        }
    }
    if in_token {
        out.push(cur);
    }
    Ok(out)
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

    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].clone();
        let take_val = |i: &mut usize| -> Result<String, CurlError> {
            *i += 1;
            tokens
                .get(*i)
                .cloned()
                .ok_or_else(|| CurlError::Flag {
                    which: t.clone(),
                    msg: "missing value".into(),
                })
        };
        match t.as_str() {
            "-X" | "--request" => {
                let v = take_val(&mut i)?;
                method = Some(
                    http::Method::from_bytes(v.as_bytes()).map_err(|e| CurlError::Flag {
                        which: t.clone(),
                        msg: format!("{e}"),
                    })?,
                );
            }
            "-H" | "--header" => {
                let v = take_val(&mut i)?;
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
                data_parts.push(take_val(&mut i)?);
            }
            "--data-urlencode" => {
                let v = take_val(&mut i)?;
                let (k, val) = v.split_once('=').unwrap_or(("", v.as_str()));
                urlencode_parts.push(KV {
                    key: k.into(),
                    value: val.into(),
                    enabled: true,
                    secret: false,
                });
            }
            "-F" | "--form" => {
                let v = take_val(&mut i)?;
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
                let v = take_val(&mut i)?;
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
                url = Some(take_val(&mut i)?);
            }
            "--cookie" | "-b" => {
                headers.push(KV {
                    key: "Cookie".into(),
                    value: take_val(&mut i)?,
                    enabled: true,
                    secret: false,
                });
            }
            "-A" | "--user-agent" => {
                headers.push(KV {
                    key: "User-Agent".into(),
                    value: take_val(&mut i)?,
                    enabled: true,
                    secret: false,
                });
            }
            "-e" | "--referer" => {
                headers.push(KV {
                    key: "Referer".into(),
                    value: take_val(&mut i)?,
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
                let v = take_val(&mut i)?;
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
                let _ = take_val(&mut i)?;
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
        i += 1;
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

    let resolved_method = method.unwrap_or_else(|| {
        if force_get {
            http::Method::GET
        } else if matches!(&body, Body::None) {
            http::Method::GET
        } else {
            http::Method::POST
        }
    });
    let final_method = if force_get {
        http::Method::GET
    } else {
        resolved_method
    };

    let req = Request {
        id: Ulid::new(),
        name: "imported".into(),
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
