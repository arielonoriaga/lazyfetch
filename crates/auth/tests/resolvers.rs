use lazyfetch_auth::resolver::DefaultResolver;
use lazyfetch_auth::NoCache;
use lazyfetch_core::auth::{ApiKeyIn, AuthResolver, AuthSpec};
use lazyfetch_core::env::{Environment, ResolveCtx, VarValue};
use lazyfetch_core::exec::WireRequest;
use lazyfetch_core::ports::SystemClock;
use lazyfetch_core::primitives::Template;
use lazyfetch_core::secret::SecretRegistry;
use secrecy::SecretString;

fn empty_req() -> WireRequest {
    WireRequest {
        method: http::Method::GET,
        url: "http://x".into(),
        headers: vec![],
        body_bytes: vec![],
        timeout: std::time::Duration::from_secs(5),
        follow_redirects: true,
        max_redirects: 10,
    }
}

fn env(pairs: &[(&str, &str, bool)]) -> Environment {
    Environment {
        id: ulid::Ulid::new(),
        name: "t".into(),
        vars: pairs
            .iter()
            .map(|(k, v, s)| {
                (
                    k.to_string(),
                    VarValue {
                        value: SecretString::new((*v).into()),
                        secret: *s,
                    },
                )
            })
            .collect(),
    }
}

#[tokio::test]
async fn bearer_uses_secret_var() {
    let e = env(&[("tok", "xyz", true)]);
    let ctx = ResolveCtx {
        env: &e,
        collection_vars: &[],
        overrides: &[],
    };
    let mut req = empty_req();
    let mut reg = SecretRegistry::new();
    DefaultResolver::new()
        .apply(
            &AuthSpec::Bearer {
                token: Template("{{tok}}".into()),
            },
            &ctx,
            &SystemClock,
            &NoCache,
            &mut req,
            &mut reg,
        )
        .await
        .unwrap();
    assert!(req
        .headers
        .iter()
        .any(|(k, v)| k == "Authorization" && v == "Bearer xyz"));
    assert!(reg.contains("xyz"));
}

#[tokio::test]
async fn bearer_rejects_non_secret_var() {
    let e = env(&[("tok", "xyz", false)]);
    let ctx = ResolveCtx {
        env: &e,
        collection_vars: &[],
        overrides: &[],
    };
    let mut req = empty_req();
    let mut reg = SecretRegistry::new();
    let res = DefaultResolver::new()
        .apply(
            &AuthSpec::Bearer {
                token: Template("{{tok}}".into()),
            },
            &ctx,
            &SystemClock,
            &NoCache,
            &mut req,
            &mut reg,
        )
        .await;
    assert!(matches!(
        res,
        Err(lazyfetch_core::auth::AuthError::NotSecret(_))
    ));
}

#[tokio::test]
async fn basic_encodes_credentials() {
    let e = env(&[("u", "alice", false), ("p", "secret", true)]);
    let ctx = ResolveCtx {
        env: &e,
        collection_vars: &[],
        overrides: &[],
    };
    let mut req = empty_req();
    let mut reg = SecretRegistry::new();
    DefaultResolver::new()
        .apply(
            &AuthSpec::Basic {
                user: Template("{{u}}".into()),
                pass: Template("{{p}}".into()),
            },
            &ctx,
            &SystemClock,
            &NoCache,
            &mut req,
            &mut reg,
        )
        .await
        .unwrap();
    let auth = req
        .headers
        .iter()
        .find(|(k, _)| k == "Authorization")
        .unwrap();
    assert!(auth.1.starts_with("Basic "));
    let enc = auth.1.trim_start_matches("Basic ");
    let dec = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, enc).unwrap();
    assert_eq!(String::from_utf8(dec).unwrap(), "alice:secret");
    assert!(reg.contains("secret"));
}

#[tokio::test]
async fn apikey_query_appends_param() {
    let e = env(&[("k", "k123", true)]);
    let ctx = ResolveCtx {
        env: &e,
        collection_vars: &[],
        overrides: &[],
    };
    let mut req = empty_req();
    let mut reg = SecretRegistry::new();
    DefaultResolver::new()
        .apply(
            &AuthSpec::ApiKey {
                name: "api_key".into(),
                value: Template("{{k}}".into()),
                location: ApiKeyIn::Query,
            },
            &ctx,
            &SystemClock,
            &NoCache,
            &mut req,
            &mut reg,
        )
        .await
        .unwrap();
    assert_eq!(req.url, "http://x?api_key=k123");
}
