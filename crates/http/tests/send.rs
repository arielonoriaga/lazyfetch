use lazyfetch_core::exec::{HttpSender, WireRequest};
use lazyfetch_http::ReqwestSender;
use wiremock::matchers::{body_json, header, method, path};
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
        multipart: None,
    };
    let resp = sender.send(req).await.unwrap();
    assert_eq!(resp.status, 204);
    assert!(resp
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("x-test") && v == "v"));
}

#[tokio::test]
async fn sends_graphql_json_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gql"))
        .and(header("content-type", "application/json"))
        .and(body_json(serde_json::json!({
            "query": "query { me { id } }",
            "variables": { "x": 1 }
        })))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let body = br#"{"query":"query { me { id } }","variables":{"x":1}}"#.to_vec();
    let req = WireRequest {
        method: http::Method::POST,
        url: format!("{}/gql", server.uri()),
        headers: vec![("content-type".into(), "application/json".into())],
        body_bytes: body,
        multipart: None,
        timeout: std::time::Duration::from_secs(5),
        follow_redirects: true,
        max_redirects: 10,
    };
    let resp = ReqwestSender::new().send(req).await.unwrap();
    assert_eq!(resp.status, 200);
}
