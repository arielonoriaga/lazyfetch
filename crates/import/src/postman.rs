use lazyfetch_core::auth::{ApiKeyIn, AuthSpec};
use lazyfetch_core::catalog::{Body, Collection, Folder, Item, Request};
use lazyfetch_core::primitives::{Template, UrlTemplate, KV};
use serde::Deserialize;

#[derive(Debug, Default)]
pub struct ImportReport {
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PostmanError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("input too large ({0} bytes)")]
    TooLarge(usize),
}

pub const DEFAULT_MAX_INPUT: usize = 16 * 1024 * 1024;

pub fn parse(json: &str) -> Result<(Collection, ImportReport), PostmanError> {
    parse_with_limit(json, DEFAULT_MAX_INPUT)
}

pub fn parse_with_limit(
    json: &str,
    max_bytes: usize,
) -> Result<(Collection, ImportReport), PostmanError> {
    if json.len() > max_bytes {
        return Err(PostmanError::TooLarge(json.len()));
    }
    let pm: PmCollection = serde_json::from_str(json)?;
    let mut report = ImportReport::default();
    let root = walk_items(&pm.item, &mut report);
    let coll_auth = pm.auth.as_ref().map(|a| convert_auth(a, &mut report));
    let vars = pm
        .variable
        .iter()
        .map(|v| KV {
            key: v.key.clone(),
            value: v.value.clone().unwrap_or_default(),
            enabled: true,
            secret: false,
        })
        .collect();
    let coll = Collection {
        id: ulid::Ulid::new(),
        name: pm.info.name,
        root: Folder {
            id: ulid::Ulid::new(),
            name: "root".into(),
            items: root,
            auth: None,
        },
        auth: coll_auth,
        vars,
    };
    Ok((coll, report))
}

fn walk_items(items: &[PmItem], report: &mut ImportReport) -> Vec<Item> {
    items
        .iter()
        .map(|item| match item {
            PmItem::Folder(f) => Item::Folder(Folder {
                id: ulid::Ulid::new(),
                name: f.name.clone(),
                items: walk_items(&f.item, report),
                auth: f.auth.as_ref().map(|a| convert_auth(a, report)),
            }),
            PmItem::Request(r) => Item::Request(convert_request(r, report)),
        })
        .collect()
}

fn convert_request(r: &PmRequestItem, report: &mut ImportReport) -> Request {
    let raw_url = match &r.request.url {
        PmUrl::String(s) => s.clone(),
        PmUrl::Object { raw, .. } => raw.clone(),
    };
    let query = match &r.request.url {
        PmUrl::Object { query, .. } => query
            .iter()
            .map(|q| KV {
                key: q.key.clone(),
                value: q.value.clone().unwrap_or_default(),
                enabled: !q.disabled.unwrap_or(false),
                secret: false,
            })
            .collect(),
        _ => vec![],
    };
    let headers = r
        .request
        .header
        .iter()
        .map(|h| KV {
            key: h.key.clone(),
            value: h.value.clone(),
            enabled: !h.disabled.unwrap_or(false),
            secret: false,
        })
        .collect();
    let body = r
        .request
        .body
        .as_ref()
        .map(|b| convert_body(b, report))
        .unwrap_or(Body::None);
    let auth = r.request.auth.as_ref().map(|a| convert_auth(a, report));
    let method = r.request.method.parse().unwrap_or(http::Method::GET);
    let notes = if r.event.is_empty() {
        None
    } else {
        Some(
            r.event
                .iter()
                .map(|e| format!("// {} script:\n{}", e.listen, e.script.exec.join("\n")))
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    };
    Request {
        id: ulid::Ulid::new(),
        name: r.name.clone(),
        method,
        url: UrlTemplate(Template(raw_url)),
        query,
        headers,
        body,
        auth,
        notes,
        follow_redirects: true,
        max_redirects: 10,
        timeout_ms: None,
    }
}

fn convert_body(b: &PmBody, report: &mut ImportReport) -> Body {
    match b.mode.as_str() {
        "raw" => {
            let mime = b
                .options
                .as_ref()
                .and_then(|o| o.raw.as_ref())
                .and_then(|r| r.language.clone())
                .unwrap_or_else(|| "text/plain".into());
            let text = b.raw.clone().unwrap_or_default();
            if mime == "json" || mime == "application/json" {
                Body::Json { text }
            } else {
                Body::Raw { mime, text }
            }
        }
        "urlencoded" => Body::Form(
            b.urlencoded
                .iter()
                .map(|kv| KV {
                    key: kv.key.clone(),
                    value: kv.value.clone().unwrap_or_default(),
                    enabled: !kv.disabled.unwrap_or(false),
                    secret: false,
                })
                .collect(),
        ),
        "graphql" => {
            let body = b
                .graphql
                .as_ref()
                .map(|g| {
                    serde_json::json!({
                        "query": g.query,
                        "variables": g.variables,
                    })
                    .to_string()
                })
                .unwrap_or_default();
            Body::Json { text: body }
        }
        other => {
            report
                .warnings
                .push(format!("unsupported Postman body mode: {}", other));
            Body::None
        }
    }
}

fn convert_auth(a: &PmAuth, report: &mut ImportReport) -> AuthSpec {
    match a.kind.as_str() {
        "bearer" => {
            let token = first_value(&a.bearer, "token").unwrap_or_default();
            AuthSpec::Bearer {
                token: Template(token),
            }
        }
        "basic" => {
            let user = first_value(&a.basic, "username").unwrap_or_default();
            let pass = first_value(&a.basic, "password").unwrap_or_default();
            AuthSpec::Basic {
                user: Template(user),
                pass: Template(pass),
            }
        }
        "apikey" => {
            let name = first_value(&a.apikey, "key").unwrap_or_else(|| "X-API-Key".into());
            let value = first_value(&a.apikey, "value").unwrap_or_default();
            let in_q = first_value(&a.apikey, "in")
                .map(|s| s == "query")
                .unwrap_or(false);
            AuthSpec::ApiKey {
                name,
                value: Template(value),
                location: if in_q {
                    ApiKeyIn::Query
                } else {
                    ApiKeyIn::Header
                },
            }
        }
        other => {
            report
                .warnings
                .push(format!("unsupported Postman auth kind: {}", other));
            AuthSpec::None
        }
    }
}

fn first_value(rows: &[PmKvAuth], key: &str) -> Option<String> {
    rows.iter().find(|r| r.key == key).map(|r| r.value.clone())
}

#[derive(Debug, Deserialize)]
struct PmCollection {
    info: PmInfo,
    #[serde(default)]
    item: Vec<PmItem>,
    #[serde(default)]
    auth: Option<PmAuth>,
    #[serde(default)]
    variable: Vec<PmVar>,
}

#[derive(Debug, Deserialize)]
struct PmInfo {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PmItem {
    Request(Box<PmRequestItem>),
    Folder(Box<PmFolder>),
}

#[derive(Debug, Deserialize)]
struct PmFolder {
    name: String,
    #[serde(default)]
    item: Vec<PmItem>,
    #[serde(default)]
    auth: Option<PmAuth>,
}

#[derive(Debug, Deserialize)]
struct PmRequestItem {
    name: String,
    request: PmRequestBody,
    #[serde(default)]
    event: Vec<PmEvent>,
}

#[derive(Debug, Deserialize)]
struct PmRequestBody {
    method: String,
    url: PmUrl,
    #[serde(default)]
    header: Vec<PmHeader>,
    #[serde(default)]
    body: Option<PmBody>,
    #[serde(default)]
    auth: Option<PmAuth>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PmUrl {
    String(String),
    Object {
        raw: String,
        #[serde(default)]
        query: Vec<PmQuery>,
    },
}

#[derive(Debug, Deserialize)]
struct PmQuery {
    key: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    disabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PmHeader {
    key: String,
    value: String,
    #[serde(default)]
    disabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PmBody {
    mode: String,
    #[serde(default)]
    raw: Option<String>,
    #[serde(default)]
    urlencoded: Vec<PmKv>,
    #[serde(default)]
    graphql: Option<PmGraphql>,
    #[serde(default)]
    options: Option<PmBodyOpts>,
}

#[derive(Debug, Deserialize)]
struct PmBodyOpts {
    raw: Option<PmBodyOptsRaw>,
}

#[derive(Debug, Deserialize)]
struct PmBodyOptsRaw {
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PmKv {
    key: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    disabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct PmGraphql {
    #[serde(default)]
    query: String,
    #[serde(default)]
    variables: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct PmAuth {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    bearer: Vec<PmKvAuth>,
    #[serde(default)]
    basic: Vec<PmKvAuth>,
    #[serde(default)]
    apikey: Vec<PmKvAuth>,
}

#[derive(Debug, Deserialize)]
struct PmKvAuth {
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct PmVar {
    key: String,
    #[serde(default)]
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PmEvent {
    listen: String,
    script: PmScript,
}

#[derive(Debug, Deserialize)]
struct PmScript {
    #[serde(default)]
    exec: Vec<String>,
}
