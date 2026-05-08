use lazyfetch_core::dynvars::{resolve, Arg, DynCtx, DynError};
use lazyfetch_core::ports::SystemClock;

fn ctx() -> DynCtx<'static> {
    static CLOCK: SystemClock = SystemClock;
    DynCtx { clock: &CLOCK }
}

#[test]
fn now_is_rfc3339() {
    let s = resolve("now", &[], &ctx()).unwrap();
    assert!(chrono::DateTime::parse_from_rfc3339(&s).is_ok(), "got {s}");
}

#[test]
fn timestamp_is_unix_secs() {
    let s = resolve("timestamp", &[], &ctx()).unwrap();
    let n: u64 = s.parse().expect("digits");
    assert!(n > 1_700_000_000);
}

#[test]
fn uuid_is_v4_hex() {
    let s = resolve("uuid", &[], &ctx()).unwrap();
    let u = uuid::Uuid::parse_str(&s).unwrap();
    assert_eq!(u.get_version_num(), 4);
}

#[test]
fn ulid_is_crockford() {
    let s = resolve("ulid", &[], &ctx()).unwrap();
    assert_eq!(s.len(), 26);
    assert!(ulid::Ulid::from_string(&s).is_ok());
}

#[test]
fn random_int_in_bounds() {
    for _ in 0..200 {
        let s = resolve("randomInt", &[Arg::str("5"), Arg::str("10")], &ctx()).unwrap();
        let n: i64 = s.parse().unwrap();
        assert!((5..=10).contains(&n), "got {n}");
    }
}

#[test]
fn random_int_bounds_invalid() {
    let r = resolve("randomInt", &[Arg::str("10"), Arg::str("5")], &ctx());
    assert!(matches!(r, Err(DynError::Bounds { .. })));
}

#[test]
fn base64_literal() {
    let s = resolve("base64", &[Arg::str("foo")], &ctx()).unwrap();
    assert_eq!(s, "Zm9v");
}

#[test]
fn random_string_alphabet_and_length() {
    let s = resolve("randomString", &[Arg::str("16")], &ctx()).unwrap();
    assert_eq!(s.chars().count(), 16);
    assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn random_string_length_cap() {
    let r = resolve("randomString", &[Arg::str("5000")], &ctx());
    assert!(matches!(r, Err(DynError::Bounds { .. })));
}

#[test]
fn unknown_returns_unknown_err() {
    let r = resolve("nope", &[], &ctx());
    assert!(matches!(r, Err(DynError::Unknown(_))));
}

#[test]
fn now_format_alias() {
    let s = resolve("now", &[Arg::str("rfc2822")], &ctx()).unwrap();
    assert!(s.ends_with("+0000") || s.ends_with("UT"));
}

#[test]
fn now_chrono_strftime() {
    let s = resolve("now", &[Arg::str("%Y-%m-%d")], &ctx()).unwrap();
    assert_eq!(s.len(), 10);
    assert_eq!(&s[4..5], "-");
}
