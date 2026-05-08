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
                check_dyn_only(&token.0)?;
                let i = interpolate(&token.0, ctx)?;
                require_secret(&token.0, &i)?;
                req.headers
                    .push(("Authorization".into(), format!("Bearer {}", i.value)));
                reg.extend(&i.used_secrets);
                Ok(())
            }
            AuthSpec::Basic { user, pass } => {
                check_dyn_only(&pass.0)?;
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
                check_dyn_only(&value.0)?;
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

/// Template-only pre-check: refuse a secret field whose template references *no* env vars,
/// only dyn-vars. Such a token re-rolls per request → broken auth.
fn check_dyn_only(tpl: &str) -> Result<(), AuthError> {
    if !tpl.contains("{{") {
        return Ok(());
    }
    let has_var_ref = tpl.match_indices("{{").any(|(idx, _)| {
        let after = &tpl[idx + 2..];
        !after.trim_start().starts_with('$')
    });
    if !has_var_ref {
        return Err(AuthError::DynVarOnlyInSecretField {
            template: tpl.into(),
        });
    }
    Ok(())
}

/// Reject in two cases for a secret-only auth field:
/// 1. Template references *only* dyn-vars (`{{$...}}`) and no env var — value re-rolls
///    per request → broken auth.
/// 2. Template references env var(s) but none flagged secret → secret leaked through
///    SecretRegistry-less interpolation.
fn require_secret(tpl: &str, i: &Interpolated) -> Result<(), AuthError> {
    if !tpl.contains("{{") {
        return Ok(()); // plain literal — caller's responsibility
    }
    let has_var_ref = tpl.match_indices("{{").any(|(idx, _)| {
        let after = &tpl[idx + 2..];
        !after.trim_start().starts_with('$')
    });
    if !has_var_ref {
        return Err(AuthError::DynVarOnlyInSecretField {
            template: tpl.into(),
        });
    }
    if i.used_secrets.is_empty() && i.value != *tpl {
        return Err(AuthError::NotSecret(tpl.into()));
    }
    Ok(())
}
