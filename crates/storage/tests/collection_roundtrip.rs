use http::Method;
use lazyfetch_core::catalog::{Body, Collection, Folder, Item, Request};
use lazyfetch_core::primitives::{Template, UrlTemplate};
use lazyfetch_storage::collection::FsCollectionRepo;
use ulid::Ulid;

#[test]
fn save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsCollectionRepo::new(dir.path());
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
                url: UrlTemplate(Template("https://api/{{x}}".into())),
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
    let loaded = repo.load_by_name("demo").unwrap();
    assert_eq!(loaded.name, "demo");
    assert_eq!(loaded.root.items.len(), 1);
}
