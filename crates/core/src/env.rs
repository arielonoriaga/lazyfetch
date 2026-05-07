use crate::error::CoreError;
use crate::primitives::Id;
use crate::secret::SecretRegistry;
use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct VarValue {
    pub value: SecretString,
    pub secret: bool,
}

pub type VarSet = Vec<(String, VarValue)>;

#[derive(Debug, Clone)]
pub struct Environment {
    pub id: Id,
    pub name: String,
    pub vars: VarSet,
}

pub struct ResolveCtx<'a> {
    pub env: &'a Environment,
    pub collection_vars: &'a [(String, VarValue)],
    pub overrides: &'a [(String, VarValue)],
}

#[derive(Debug, Clone)]
pub struct Interpolated {
    pub value: String,
    pub used_secrets: SecretRegistry,
}

fn lookup<'a>(name: &str, ctx: &'a ResolveCtx<'a>) -> Option<&'a VarValue> {
    ctx.overrides
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
        .or_else(|| ctx.env.vars.iter().find(|(k, _)| k == name).map(|(_, v)| v))
        .or_else(|| {
            ctx.collection_vars
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v)
        })
}

pub fn interpolate(s: &str, ctx: &ResolveCtx) -> Result<Interpolated, CoreError> {
    let mut out = String::with_capacity(s.len());
    let mut reg = SecretRegistry::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| CoreError::InvalidTemplate(s.into()))?;
        let name = after[..end].trim();
        let v = lookup(name, ctx).ok_or_else(|| CoreError::MissingVar(name.into()))?;
        let val = v.value.expose_secret();
        out.push_str(val);
        if v.secret {
            reg.insert(val.clone());
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(Interpolated {
        value: out,
        used_secrets: reg,
    })
}
