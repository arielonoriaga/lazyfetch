use lazyfetch_core::catalog::{Body, BodyKind};

#[test]
fn body_kind_round_trip() {
    assert_eq!(Body::None.kind(), BodyKind::None);
    assert_eq!(Body::Json { text: "{}".into() }.kind(), BodyKind::Json);
    assert_eq!(
        Body::GraphQL {
            query: "{ me { id } }".into(),
            variables: "{}".into()
        }
        .kind(),
        BodyKind::GraphQL
    );
}

#[test]
fn graphql_serde_uses_lowercase_tag() {
    let b = Body::GraphQL {
        query: "{me{id}}".into(),
        variables: "{}".into(),
    };
    let yaml = serde_yaml::to_string(&b).unwrap();
    assert!(yaml.contains("kind: graphql"), "got:\n{yaml}");
    let back: Body = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.kind(), BodyKind::GraphQL);
}
