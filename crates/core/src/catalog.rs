use crate::auth::AuthSpec;
use crate::primitives::{Id, UrlTemplate, KV};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: Id,
    pub name: String,
    pub root: Folder,
    pub auth: Option<AuthSpec>,
    #[serde(default)]
    pub vars: Vec<KV>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub items: Vec<Item>,
    pub auth: Option<AuthSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Item {
    Folder(Folder),
    Request(Request),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: Id,
    pub name: String,
    #[serde(with = "crate::method_serde")]
    pub method: http::Method,
    pub url: UrlTemplate,
    #[serde(default)]
    pub query: Vec<KV>,
    #[serde(default)]
    pub headers: Vec<KV>,
    #[serde(default)]
    pub body: Body,
    pub auth: Option<AuthSpec>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default = "yes")]
    pub follow_redirects: bool,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: u8,
    #[serde(default)]
    pub timeout_ms: Option<u32>,
}

fn yes() -> bool {
    true
}

fn default_max_redirects() -> u8 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Body {
    #[default]
    None,
    Raw {
        mime: String,
        text: String,
    },
    Json(String),
    Form(Vec<KV>),
    Multipart(Vec<Part>),
    File(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub name: String,
    pub content: PartContent,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PartContent {
    Text(String),
    File(PathBuf),
}
