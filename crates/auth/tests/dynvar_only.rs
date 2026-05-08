use lazyfetch_auth::resolver::DefaultResolver;
use lazyfetch_auth::NoCache;
use lazyfetch_core::auth::{AuthError, AuthResolver, AuthSpec};
use lazyfetch_core::env::{Environment, ResolveCtx};
use lazyfetch_core::exec::WireRequest;
use lazyfetch_core::ports::SystemClock;
use lazyfetch_core::primitives::Template;
use lazyfetch_core::secret::SecretRegistry;

fn empty_req() -> WireRequest {
    WireRequest {
        method: http::Method::GET,
        url: "http://x".into(),
        headers: vec![],
        body_bytes: vec![],
        timeout: std::time::Duration::from_secs(5),
        follow_redirects: true,
        max_redirects: 10,
        multipart: None,
    }
}

#[tokio::test]
async fn bearer_dynvar_only_is_rejected() {
    let env = Environment {
        id: ulid::Ulid::new(),
        name: "t".into(),
        vars: vec![],
    };
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    let mut req = empty_req();
    let mut reg = SecretRegistry::new();
    let res = DefaultResolver::new()
        .apply(
            &AuthSpec::Bearer {
                token: Template("{{$randomString(32)}}".into()),
            },
            &ctx,
            &SystemClock,
            &NoCache,
            &mut req,
            &mut reg,
        )
        .await;
    assert!(matches!(res, Err(AuthError::DynVarOnlyInSecretField { .. })));
}

#[tokio::test]
async fn bearer_with_env_var_still_works() {
    use lazyfetch_core::env::VarValue;
    use secrecy::SecretString;
    let env = Environment {
        id: ulid::Ulid::new(),
        name: "t".into(),
        vars: vec![(
            "TOK".into(),
            VarValue {
                value: SecretString::new("xyz".into()),
                secret: true,
            },
        )],
    };
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    let mut req = empty_req();
    let mut reg = SecretRegistry::new();
    let res = DefaultResolver::new()
        .apply(
            &AuthSpec::Bearer {
                token: Template("{{TOK}}".into()),
            },
            &ctx,
            &SystemClock,
            &NoCache,
            &mut req,
            &mut reg,
        )
        .await;
    assert!(res.is_ok());
}
