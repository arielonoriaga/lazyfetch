use http::Method;
use lazyfetch_core::catalog::{Body, Collection, Folder, Item, Request};
use lazyfetch_core::primitives::{Template, UrlTemplate, KV};
use lazyfetch_storage::collection::FsCollectionRepo;
use std::process::Command;
use ulid::Ulid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("lazyfetch")
}

#[tokio::test]
async fn run_sends_and_prints_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let cfg = tempfile::tempdir().unwrap();
    let coll_root = cfg.path().join("collections");
    let repo = FsCollectionRepo::new(&coll_root);

    let coll = Collection {
        id: Ulid::new(),
        name: "demo".into(),
        root: Folder {
            id: Ulid::new(),
            name: "demo".into(),
            items: vec![Item::Request(Request {
                id: Ulid::new(),
                name: "ping".into(),
                method: Method::GET,
                url: UrlTemplate(Template(format!("{}/ping", server.uri()))),
                query: vec![],
                headers: vec![],
                body: Body::None,
                auth: None,
                notes: None,
                follow_redirects: true,
                max_redirects: 10,
                timeout_ms: None,
            })],
            auth: None,
        },
        auth: None,
        vars: vec![],
    };
    repo.save(&coll).unwrap();

    // run binary
    let out = Command::new(bin())
        .args([
            "run",
            "demo/ping",
            "--config-dir",
            cfg.path().to_str().unwrap(),
        ])
        .output()
        .expect("spawn lazyfetch");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "non-zero exit: stderr={}", stderr);
    assert!(
        stdout.starts_with("204 "),
        "expected 204 status line, got: {}",
        stdout
    );

    // suppress unused warnings
    let _ = KV {
        key: String::new(),
        value: String::new(),
        enabled: true,
        secret: false,
    };
}

#[tokio::test]
async fn run_discovers_project_local_dotlazyfetch() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let project = tempfile::tempdir().unwrap();
    let cfg = project.path().join(".lazyfetch");
    std::fs::create_dir(&cfg).unwrap();
    let repo = FsCollectionRepo::new(cfg.join("collections"));
    let coll = Collection {
        id: Ulid::new(),
        name: "local".into(),
        root: Folder {
            id: Ulid::new(),
            name: "local".into(),
            items: vec![Item::Request(Request {
                id: Ulid::new(),
                name: "ping".into(),
                method: Method::GET,
                url: UrlTemplate(Template(format!("{}/ping", server.uri()))),
                query: vec![],
                headers: vec![],
                body: Body::None,
                auth: None,
                notes: None,
                follow_redirects: true,
                max_redirects: 10,
                timeout_ms: None,
            })],
            auth: None,
        },
        auth: None,
        vars: vec![],
    };
    repo.save(&coll).unwrap();

    let nested = project.path().join("a").join("b");
    std::fs::create_dir_all(&nested).unwrap();

    let out = Command::new(bin())
        .args(["run", "local/ping"])
        .current_dir(&nested)
        .output()
        .expect("spawn lazyfetch");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "non-zero exit: stderr={}", stderr);
    assert!(
        stdout.starts_with("200 "),
        "expected 200 status line, got: {}",
        stdout
    );
}
