use http::Method;
use lazyfetch_core::exec::{redact_wire, WireRequest};
use lazyfetch_core::secret::SecretRegistry;

#[test]
fn redacts_header_and_body() {
    let mut reg = SecretRegistry::new();
    reg.insert("s3cret");
    let w = WireRequest {
        method: Method::GET,
        url: "http://x".into(),
        headers: vec![("Authorization".into(), "Bearer s3cret".into())],
        body_bytes: b"tok=s3cret".to_vec(),
        timeout: std::time::Duration::from_secs(30),
        follow_redirects: true,
        max_redirects: 10,
        multipart: None,
    };
    let r = redact_wire(&w, &reg);
    assert_eq!(r.headers[0].1, "Bearer ***");
    assert_eq!(r.body_bytes, b"tok=***");
}
