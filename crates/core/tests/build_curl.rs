use http::Method;
use lazyfetch_core::exec::{build_curl, WireRequest};
use lazyfetch_core::secret::SecretRegistry;

fn req(method: Method, url: &str) -> WireRequest {
    WireRequest {
        method,
        url: url.into(),
        headers: vec![],
        body_bytes: vec![],
        timeout: std::time::Duration::from_secs(30),
        follow_redirects: true,
        max_redirects: 10,
        multipart: None,
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
    // POSIX-portable single-quote escape inside single-quoted strings:
    // the inner `'` becomes `'\''`. Header is wrapped as `'key: value'`,
    // so the rendered output contains `O'\''Brien` (apostrophe split out).
    let mut r = req(Method::GET, "https://api/x");
    r.headers.push(("X-Author".into(), "O'Brien".into()));
    let s = build_curl(&r, &SecretRegistry::new());
    assert!(s.contains("O'\\''Brien"), "got: {s}");
    // And the surrounding quotes are intact: full header arg is quoted on both sides.
    assert!(s.contains("-H 'X-Author: O'\\''Brien'"), "got: {s}");
}
