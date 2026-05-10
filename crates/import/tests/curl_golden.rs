use lazyfetch_core::auth::AuthSpec;
use lazyfetch_core::catalog::{Body, PartContent};
use lazyfetch_import::curl;

#[test]
fn parses_chrome_simple_get() {
    let s = include_str!("fixtures/curl/chrome_simple_get.txt");
    let (req, report) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::GET);
    assert_eq!(req.url.0 .0, "https://api.test/users?page=1");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert!(req.headers.iter().any(|h| h.key == "Accept"));
    assert!(req.headers.iter().any(|h| h.key == "Accept-Encoding"));
}

#[test]
fn parses_chrome_post_json() {
    let s = include_str!("fixtures/curl/chrome_post_json.txt");
    let (req, _) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::POST);
    assert!(matches!(&req.body, Body::Json { text } if text == "{\"name\":\"alice\"}"));
    assert!(req
        .headers
        .iter()
        .any(|h| h.key == "Authorization" && h.value == "Bearer abc.def.ghi"));
}

#[test]
fn parses_firefox_get() {
    let s = include_str!("fixtures/curl/firefox_get.txt");
    let (req, _) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::GET);
    assert_eq!(req.url.0 .0, "https://api.test/x");
}

#[test]
fn parses_safari_get() {
    let s = include_str!("fixtures/curl/safari_get.txt");
    let (req, _) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::GET);
}

#[test]
fn parses_plain_bash() {
    let s = include_str!("fixtures/curl/plain_bash.txt");
    let (req, _) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::PUT);
    assert!(matches!(&req.body, Body::Raw { text, .. } if text == "hello world"));
}

#[test]
fn parses_multipart_file() {
    let s = include_str!("fixtures/curl/multipart_file.txt");
    let (req, _) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::POST);
    let parts = match &req.body {
        Body::Multipart(p) => p,
        _ => panic!("expected multipart"),
    };
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].name, "meta");
    assert!(matches!(&parts[0].content, PartContent::Text(t) if t == "hello"));
    assert_eq!(parts[1].name, "avatar");
    assert!(
        matches!(&parts[1].content, PartContent::File(p) if p.to_str() == Some("/tmp/pic.png"))
    );
}

#[test]
fn parses_data_urlencode() {
    let s = include_str!("fixtures/curl/data_urlencode.txt");
    let (req, _) = curl::parse(s).unwrap();
    let rows = match &req.body {
        Body::Form(r) => r,
        _ => panic!("expected form, got {:?}", req.body),
    };
    assert!(rows.iter().any(|r| r.key == "user" && r.value == "alice"));
    assert!(rows
        .iter()
        .any(|r| r.key == "pass" && r.value == "hunter 2"));
}

#[test]
fn parses_get_force() {
    let s = include_str!("fixtures/curl/get_force.txt");
    let (req, _) = curl::parse(s).unwrap();
    assert_eq!(req.method, http::Method::GET);
    assert!(matches!(&req.body, Body::None));
    assert!(req.url.0 .0.contains("q=rust"));
    assert!(req.url.0 .0.contains("page=2"));
}

#[test]
fn parses_user_pass() {
    let s = include_str!("fixtures/curl/user_pass.txt");
    let (req, report) = curl::parse(s).unwrap();
    assert!(report.warnings.is_empty());
    let (u, p) = match req.auth.as_ref().unwrap() {
        AuthSpec::Basic { user, pass } => (user.0.as_str(), pass.0.as_str()),
        other => panic!("expected basic, got {other:?}"),
    };
    assert_eq!(u, "alice");
    assert_eq!(p, "hunter2");
}

#[test]
fn parses_user_only_warns() {
    let s = include_str!("fixtures/curl/user_only.txt");
    let (req, report) = curl::parse(s).unwrap();
    assert!(matches!(req.auth, Some(AuthSpec::Basic { .. })));
    assert!(
        report.warnings.iter().any(|w| w.contains("no password")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn rejects_cmd_shell() {
    let s = include_str!("fixtures/curl/cmd_shell_rejected.txt");
    let r = curl::parse(s);
    assert!(matches!(r, Err(curl::CurlError::Tokenize { .. })));
}

#[test]
fn last_content_type_wins() {
    let s = include_str!("fixtures/curl/last_content_type_wins.txt");
    let (req, _) = curl::parse(s).unwrap();
    assert!(matches!(&req.body, Body::Json { text } if text == "{\"a\":1}"));
}

#[test]
fn ansi_c_string_decodes_newline() {
    let s = include_str!("fixtures/curl/ansi_c_string.txt");
    let (req, _) = curl::parse(s).unwrap();
    let h = req
        .headers
        .iter()
        .find(|h| h.key == "X-Trace")
        .expect("X-Trace header");
    assert!(h.value.contains('\n'));
}
