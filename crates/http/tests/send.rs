use lazyfetch_core::exec::{HttpSender, WireRequest};
use lazyfetch_http::ReqwestSender;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn sends_get_and_parses_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(204).insert_header("X-Test", "v"))
        .mount(&server)
        .await;

    let sender = ReqwestSender::new();
    let req = WireRequest {
        method: http::Method::GET,
        url: format!("{}/x", server.uri()),
        headers: vec![],
        body_bytes: vec![],
        timeout: std::time::Duration::from_secs(5),
        follow_redirects: true,
        max_redirects: 10,
    };
    let resp = sender.send(req).await.unwrap();
    assert_eq!(resp.status, 204);
    assert!(resp
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("x-test") && v == "v"));
}
