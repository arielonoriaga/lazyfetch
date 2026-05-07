use lazyfetch_core::env::{interpolate, Environment, ResolveCtx, VarValue};
use secrecy::SecretString;

fn ev(pairs: &[(&str, &str, bool)]) -> Environment {
    Environment {
        id: ulid::Ulid::new(),
        name: "test".into(),
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

#[test]
fn substitutes_simple() {
    let env = ev(&[("base", "https://api.test", false)]);
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    let out = interpolate("{{base}}/x", &ctx).unwrap();
    assert_eq!(out.value, "https://api.test/x");
    assert!(out.used_secrets.is_empty());
}

#[test]
fn override_beats_env() {
    let env = ev(&[("k", "env", false)]);
    let ov: Vec<_> = vec![(
        "k".into(),
        VarValue {
            value: SecretString::new("ov".into()),
            secret: false,
        },
    )];
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &ov,
    };
    assert_eq!(interpolate("{{k}}", &ctx).unwrap().value, "ov");
}

#[test]
fn missing_var_errors() {
    let env = ev(&[]);
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    assert!(interpolate("{{nope}}", &ctx).is_err());
}

#[test]
fn secret_tracked_in_registry() {
    let env = ev(&[("tok", "s3cret", true)]);
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    let out = interpolate("Bearer {{tok}}", &ctx).unwrap();
    assert_eq!(out.value, "Bearer s3cret");
    assert!(out.used_secrets.contains("s3cret"));
}

use proptest::prelude::*;

proptest! {
    #[test]
    fn no_var_no_change(s in "[^{}]{0,100}") {
        let env = ev(&[]);
        let ctx = ResolveCtx { env: &env, collection_vars: &[], overrides: &[] };
        let out = interpolate(&s, &ctx).unwrap();
        prop_assert_eq!(out.value, s);
    }
}
