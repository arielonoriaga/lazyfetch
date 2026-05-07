use serde::{Deserialize, Serialize};

pub type Id = ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KV {
    pub key: String,
    pub value: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub secret: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Template(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UrlTemplate(pub Template);
