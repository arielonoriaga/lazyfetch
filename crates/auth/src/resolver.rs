use async_trait::async_trait;
use base64::Engine;
use lazyfetch_core::auth::{ApiKeyIn, AuthCache, AuthError, AuthResolver, AuthSpec};
use lazyfetch_core::env::{interpolate, Interpolated, ResolveCtx};
use lazyfetch_core::exec::WireRequest;
use lazyfetch_core::ports::Clock;
use lazyfetch_core::secret::SecretRegistry;

#[derive(Default)]
pub struct DefaultResolver;

impl DefaultResolver {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AuthResolver for DefaultResolver {
    async fn apply(
        &self,
        spec: &AuthSpec,
        ctx: &ResolveCtx<'_>,
        _clock: &dyn Clock,
        _cache: &dyn AuthCache,
        req: &mut WireRequest,
        reg: &mut SecretRegistry,
    ) -> Result<(), AuthError> {
        match spec {
            AuthSpec::None | AuthSpec::Inherit => Ok(()),
            AuthSpec::Bearer { token } => {
                let i = interpolate(&token.0, ctx)?;
                require_secret(&token.0, &i)?;
                req.headers
                    .push(("Authorization".into(), format!("Bearer {}", i.value)));
                reg.extend(&i.used_secrets);
                Ok(())
            }
            AuthSpec::Basic { user, pass } => {
                let u = interpolate(&user.0, ctx)?;
                let p = interpolate(&pass.0, ctx)?;
                require_secret(&pass.0, &p)?;
                let raw = format!("{}:{}", u.value, p.value);
                let enc = base64::engine::general_purpose::STANDARD.encode(raw);
                req.headers
                    .push(("Authorization".into(), format!("Basic {}", enc)));
                reg.extend(&u.used_secrets);
                reg.extend(&p.used_secrets);
                Ok(())
            }
            AuthSpec::ApiKey {
                name,
                value,
                location,
            } => {
                let v = interpolate(&value.0, ctx)?;
                require_secret(&value.0, &v)?;
                match location {
                    ApiKeyIn::Header => req.headers.push((name.clone(), v.value.clone())),
                    ApiKeyIn::Query => {
                        let sep = if req.url.contains('?') { '&' } else { '?' };
                        req.url.push(sep);
                        req.url.push_str(name);
                        req.url.push('=');
                        req.url.push_str(&v.value);
                    }
                }
                reg.extend(&v.used_secrets);
                Ok(())
            }
            AuthSpec::OAuth2(_) => Err(AuthError::OAuth(
                "OAuth2 not yet wired (see Task 11)".into(),
            )),
        }
    }
}

fn require_secret(tpl: &str, i: &Interpolated) -> Result<(), AuthError> {
    if tpl.contains("{{") && i.used_secrets.is_empty() && i.value != *tpl {
        Err(AuthError::NotSecret(tpl.to_string()))
    } else {
        Ok(())
    }
}
