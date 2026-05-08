//! Pure dyn-var resolver. No IO. Time read through `Clock` port.

use crate::ports::Clock;
use base64::Engine;
use rand::Rng;
use thiserror::Error;
use tracing::instrument;

const RAND_STRING_MAX: usize = 1024;
const RAND_STRING_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

#[derive(Debug, Error)]
pub enum DynError {
    #[error("unknown dyn var: ${0}")]
    Unknown(String),
    #[error("syntax error parsing args of ${name}: {msg}")]
    ParseSyntax { name: String, msg: String },
    #[error("arg parse failed for ${name}: {msg}")]
    ArgParse { name: String, msg: String },
    #[error("recursion limit hit for ${0}")]
    TooDeep(String),
    #[error("bounds invalid for ${name}: min={min} max={max}")]
    Bounds { name: String, min: i64, max: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg(pub String);
impl Arg {
    pub fn str(s: &str) -> Self {
        Self(s.to_string())
    }
}

pub struct DynCtx<'a> {
    pub clock: &'a dyn Clock,
}

#[instrument(target = "lazyfetch::dynvars", skip(ctx), fields(name = %name))]
pub fn resolve(name: &str, args: &[Arg], ctx: &DynCtx) -> Result<String, DynError> {
    match name {
        "now" => now(args, ctx),
        "timestamp" => Ok(ctx.clock.now().timestamp().to_string()),
        "uuid" => Ok(uuid::Uuid::new_v4().to_string()),
        "ulid" => Ok(ulid::Ulid::new().to_string()),
        "randomInt" => random_int(args),
        "randomString" => random_string(args),
        "base64" => base64_arg(args),
        other => Err(DynError::Unknown(other.into())),
    }
}

fn now(args: &[Arg], ctx: &DynCtx) -> Result<String, DynError> {
    let dt = ctx.clock.now();
    if args.is_empty() {
        return Ok(dt.to_rfc3339());
    }
    Ok(match args[0].0.as_str() {
        "rfc3339" => dt.to_rfc3339(),
        "rfc2822" => dt.to_rfc2822(),
        "iso8601" => dt.to_rfc3339(),
        fmt => dt.format(fmt).to_string(),
    })
}

fn random_int(args: &[Arg]) -> Result<String, DynError> {
    if args.is_empty() {
        return Ok(rand::thread_rng().gen::<u32>().to_string());
    }
    if args.len() != 2 {
        return Err(DynError::ArgParse {
            name: "randomInt".into(),
            msg: format!("expected 0 or 2 args, got {}", args.len()),
        });
    }
    let min: i64 = args[0].0.parse().map_err(|e| DynError::ArgParse {
        name: "randomInt".into(),
        msg: format!("min: {e}"),
    })?;
    let max: i64 = args[1].0.parse().map_err(|e| DynError::ArgParse {
        name: "randomInt".into(),
        msg: format!("max: {e}"),
    })?;
    if min > max {
        return Err(DynError::Bounds {
            name: "randomInt".into(),
            min,
            max,
        });
    }
    Ok(rand::thread_rng().gen_range(min..=max).to_string())
}

fn random_string(args: &[Arg]) -> Result<String, DynError> {
    if args.len() != 1 {
        return Err(DynError::ArgParse {
            name: "randomString".into(),
            msg: format!("expected 1 arg (length), got {}", args.len()),
        });
    }
    let n: usize = args[0].0.parse().map_err(|e| DynError::ArgParse {
        name: "randomString".into(),
        msg: format!("length: {e}"),
    })?;
    if n > RAND_STRING_MAX {
        return Err(DynError::Bounds {
            name: "randomString".into(),
            min: 0,
            max: RAND_STRING_MAX as i64,
        });
    }
    let mut rng = rand::thread_rng();
    Ok((0..n)
        .map(|_| RAND_STRING_ALPHABET[rng.gen_range(0..RAND_STRING_ALPHABET.len())] as char)
        .collect())
}

fn base64_arg(args: &[Arg]) -> Result<String, DynError> {
    if args.len() != 1 {
        return Err(DynError::ArgParse {
            name: "base64".into(),
            msg: format!("expected 1 arg, got {}", args.len()),
        });
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(args[0].0.as_bytes()))
}
