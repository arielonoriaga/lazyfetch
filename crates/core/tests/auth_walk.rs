use lazyfetch_core::auth::{effective_auth, AuthSpec};
use lazyfetch_core::primitives::Template;

fn bearer(s: &str) -> AuthSpec {
    AuthSpec::Bearer {
        token: Template(s.into()),
    }
}

#[test]
fn request_wins() {
    let r = bearer("R");
    let c = bearer("C");
    let got = effective_auth(Some(&r), &[], Some(&c)).unwrap();
    assert!(matches!(got, AuthSpec::Bearer { token } if token.0 == "R"));
}

#[test]
fn inherit_climbs_to_folder() {
    let r = AuthSpec::Inherit;
    let f = bearer("F");
    let c = bearer("C");
    let got = effective_auth(Some(&r), &[&f], Some(&c)).unwrap();
    assert!(matches!(got, AuthSpec::Bearer { token } if token.0 == "F"));
}

#[test]
fn none_stops() {
    let r = AuthSpec::None;
    let c = bearer("C");
    assert!(effective_auth(Some(&r), &[], Some(&c)).is_none());
}
