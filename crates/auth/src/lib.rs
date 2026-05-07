//! lazyfetch-auth

pub mod resolver;

pub struct NoCache;

impl lazyfetch_core::auth::AuthCache for NoCache {
    fn get(&self, _: &lazyfetch_core::auth::TokenKey) -> Option<lazyfetch_core::auth::Token> {
        None
    }
    fn put(&self, _: &lazyfetch_core::auth::TokenKey, _: lazyfetch_core::auth::Token) {}
    fn evict(&self, _: &lazyfetch_core::auth::TokenKey) {}
}
