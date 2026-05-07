use http::Method;
use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(m: &Method, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(m.as_str())
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Method, D::Error> {
    let s = String::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}
