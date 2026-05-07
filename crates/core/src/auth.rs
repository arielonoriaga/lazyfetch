use crate::env::ResolveCtx;
use crate::exec::WireRequest;
use crate::ports::Clock;
use crate::primitives::{Id, Template};
use crate::secret::SecretRegistry;
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthSpec {
    None,
    Inherit,
    Bearer {
        token: Template,
    },
    Basic {
        user: Template,
        pass: Template,
    },
    ApiKey {
        name: String,
        value: Template,
        location: ApiKeyIn,
    },
    OAuth2(OAuth2Spec),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyIn {
    Header,
    Query,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "flow", rename_all = "snake_case")]
pub enum OAuth2Spec {
    ClientCredentials {
        token_url: Template,
        client_id: Template,
        client_secret: Template,
        #[serde(default)]
        scopes: Vec<String>,
        audience: Option<Template>,
    },
    AuthCode {
        auth_url: Template,
        token_url: Template,
        client_id: Template,
        client_secret: Option<Template>,
        redirect_uri: String,
        #[serde(default)]
        scopes: Vec<String>,
        #[serde(default = "yes")]
        pkce: bool,
    },
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct Token {
    pub access: SecretString,
    pub refresh: Option<SecretString>,
    pub expires_at: DateTime<Utc>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TokenKey {
    pub collection_id: Id,
    pub auth_id: Id,
    pub env_id: Id,
    pub scopes: Vec<String>,
}

pub fn token_key_hash(k: &TokenKey) -> String {
    let canon = format!(
        "{}|{}|{}|{}",
        k.collection_id,
        k.auth_id,
        k.env_id,
        k.scopes.join(",")
    );
    blake3::hash(canon.as_bytes()).to_hex().to_string()
}

pub trait AuthCache: Send + Sync {
    fn get(&self, key: &TokenKey) -> Option<Token>;
    fn put(&self, key: &TokenKey, token: Token);
    fn evict(&self, key: &TokenKey);
}

#[async_trait::async_trait]
pub trait AuthResolver: Send + Sync {
    async fn apply(
        &self,
        spec: &AuthSpec,
        ctx: &ResolveCtx<'_>,
        clock: &dyn Clock,
        cache: &dyn AuthCache,
        req: &mut WireRequest,
        reg: &mut SecretRegistry,
    ) -> Result<(), AuthError>;
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("missing var: {0}")]
    MissingVar(String),
    #[error("non-secret var used for secret field: {0}")]
    NotSecret(String),
    #[error("oauth: {0}")]
    OAuth(String),
    #[error(transparent)]
    Core(#[from] crate::error::CoreError),
}

/// Walk request → folder chain → collection. Returns first non-`Inherit` spec, or None.
pub fn effective_auth<'a>(
    req_auth: Option<&'a AuthSpec>,
    folder_chain: &[&'a AuthSpec],
    coll_auth: Option<&'a AuthSpec>,
) -> Option<&'a AuthSpec> {
    let collapse = |a: &'a AuthSpec| match a {
        AuthSpec::None => None,
        _ => Some(a),
    };
    if let Some(a) = req_auth {
        if !matches!(a, AuthSpec::Inherit) {
            return collapse(a);
        }
    }
    for a in folder_chain {
        if !matches!(a, AuthSpec::Inherit) {
            return collapse(*a);
        }
    }
    coll_auth.and_then(collapse)
}
