use lazyfetch_core::auth::AuthSpec;
use lazyfetch_core::catalog::{Body, Item};
use lazyfetch_import::postman::parse;

#[test]
fn parses_basic_collection() {
    let json = include_str!("fixtures/postman_basic.json");
    let (coll, report) = parse(json).unwrap();
    assert_eq!(coll.name, "demo");
    assert_eq!(coll.vars.len(), 1);
    assert_eq!(coll.vars[0].key, "base");
    assert!(matches!(coll.auth, Some(AuthSpec::Bearer { .. })));

    let users = match &coll.root.items[0] {
        Item::Folder(f) => f,
        _ => panic!("expected folder"),
    };
    assert_eq!(users.name, "users");
    assert_eq!(users.items.len(), 2);

    let list = match &users.items[0] {
        Item::Request(r) => r,
        _ => panic!(),
    };
    assert_eq!(list.name, "list");
    assert_eq!(list.method, http::Method::GET);
    assert_eq!(list.query.len(), 1);

    let create = match &users.items[1] {
        Item::Request(r) => r,
        _ => panic!(),
    };
    assert!(matches!(create.body, Body::Json(_)));

    let ping = match &coll.root.items[1] {
        Item::Request(r) => r,
        _ => panic!(),
    };
    assert_eq!(ping.url.0 .0, "https://api.test/ping");

    assert!(report.warnings.is_empty());
}

#[test]
fn rejects_oversize_input() {
    use lazyfetch_import::postman::{parse_with_limit, PostmanError};
    let big = "x".repeat(1024);
    let res = parse_with_limit(&big, 100);
    assert!(matches!(res, Err(PostmanError::TooLarge(_))));
}
