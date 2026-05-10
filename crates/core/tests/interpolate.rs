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

use lazyfetch_core::dynvars;
use lazyfetch_core::env::interpolate_with_dyn;
use lazyfetch_core::ports::SystemClock;

#[test]
fn interpolate_resolves_now() {
    let env = ev(&[]);
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    let dyn_ctx = dynvars::DynCtx {
        clock: &SystemClock,
    };
    let out = interpolate_with_dyn("Sent at {{$now}}", &ctx, &dyn_ctx).unwrap();
    assert!(out.value.starts_with("Sent at "));
    assert!(chrono::DateTime::parse_from_rfc3339(&out.value["Sent at ".len()..]).is_ok());
    assert!(out.used_secrets.is_empty());
}

#[test]
fn interpolate_taints_base64_of_secret() {
    let env = ev(&[("TOKEN", "hunter2", true)]);
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    let dyn_ctx = dynvars::DynCtx {
        clock: &SystemClock,
    };
    let out = interpolate_with_dyn("Auth: {{$base64({{TOKEN}})}}", &ctx, &dyn_ctx).unwrap();
    let expected_b64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"hunter2");
    assert!(out.value.contains(&expected_b64));
    assert!(out.used_secrets.contains("hunter2"));
    assert!(out.used_secrets.contains(&expected_b64));
}

#[test]
fn unknown_dyn_var_falls_through_to_missing() {
    let env = ev(&[]);
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    let dyn_ctx = dynvars::DynCtx {
        clock: &SystemClock,
    };
    let r = interpolate_with_dyn("{{$nope}}", &ctx, &dyn_ctx);
    assert!(r.is_err());
}

#[test]
fn nested_dyn_var_recursion_capped_at_8() {
    let env = ev(&[]);
    let ctx = ResolveCtx {
        env: &env,
        collection_vars: &[],
        overrides: &[],
    };
    let dyn_ctx = dynvars::DynCtx {
        clock: &SystemClock,
    };
    let mut s = String::from("'x'");
    for _ in 0..10 {
        s = format!("{{{{$base64({s})}}}}");
    }
    let r = interpolate_with_dyn(&s, &ctx, &dyn_ctx);
    assert!(r.is_err(), "10-deep should exceed depth 8 → TooDeep");
}
