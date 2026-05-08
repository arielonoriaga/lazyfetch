//! Port-trait bundle injected by the composition root (bin) at startup.
//!
//! `Adapters` lives on `AppState` so `sender::dispatch_request` can spawn an
//! `execute()` call without `lazyfetch-tui` reaching back to concrete crates
//! (lazyfetch-http, lazyfetch-auth). Tests construct an `Adapters` with a
//! mock `HttpSender` to exercise the send → `last_response` path without a
//! reqwest stack.

use async_trait::async_trait;
use lazyfetch_core::auth::{
    AuthCache, AuthError, AuthResolver, AuthSpec, Token, TokenKey,
};
use lazyfetch_core::env::ResolveCtx;
use lazyfetch_core::exec::{HttpSender, SendError, WireRequest, WireResponse};
use lazyfetch_core::ports::{Clock, SystemClock};
use lazyfetch_core::secret::SecretRegistry;
use std::sync::Arc;

#[derive(Clone)]
pub struct Adapters {
    pub http: Arc<dyn HttpSender>,
    pub auth_resolver: Arc<dyn AuthResolver>,
    pub auth_cache: Arc<dyn AuthCache>,
    pub clock: Arc<dyn Clock>,
}

impl Adapters {
    /// Test/fallback bundle. Uses `NullHttpSender` (errors on every send),
    /// `NullAuthResolver`, `NullAuthCache`, and `SystemClock`. Suitable for
    /// unit tests that don't exercise the network — never wire this in bin.
    pub fn testing() -> Self {
        Self {
            http: Arc::new(NullHttpSender),
            auth_resolver: Arc::new(NullAuthResolver),
            auth_cache: Arc::new(NullAuthCache),
            clock: Arc::new(SystemClock),
        }
    }

    /// Builder for the production bundle. Caller (bin) injects the concrete
    /// HTTP / auth adapters; clock defaults to `SystemClock`.
    pub fn new(
        http: Arc<dyn HttpSender>,
        auth_resolver: Arc<dyn AuthResolver>,
        auth_cache: Arc<dyn AuthCache>,
    ) -> Self {
        Self {
            http,
            auth_resolver,
            auth_cache,
            clock: Arc::new(SystemClock),
        }
    }
}

/// Send always returns `SendError::Net("no http adapter wired")`. Used by
/// tests and as a fallback so AppState::new can produce a complete state
/// even when no real sender has been injected.
pub struct NullHttpSender;

#[async_trait]
impl HttpSender for NullHttpSender {
    async fn send(&self, _r: WireRequest) -> Result<WireResponse, SendError> {
        Err(SendError::Net("no http adapter wired".into()))
    }
}

pub struct NullAuthResolver;

#[async_trait]
impl AuthResolver for NullAuthResolver {
    async fn apply(
        &self,
        _spec: &AuthSpec,
        _ctx: &ResolveCtx<'_>,
        _clock: &dyn Clock,
        _cache: &dyn AuthCache,
        _req: &mut WireRequest,
        _reg: &mut SecretRegistry,
    ) -> Result<(), AuthError> {
        Ok(())
    }
}

pub struct NullAuthCache;

impl AuthCache for NullAuthCache {
    fn get(&self, _key: &TokenKey) -> Option<Token> {
        None
    }
    fn put(&self, _key: &TokenKey, _token: Token) {}
    fn evict(&self, _key: &TokenKey) {}
}
