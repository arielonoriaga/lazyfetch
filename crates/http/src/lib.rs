//! lazyfetch-http

use async_trait::async_trait;
use lazyfetch_core::exec::{HttpSender, MultipartKind, SendError, WireRequest, WireResponse};
use std::time::Instant;

#[derive(Default)]
pub struct ReqwestSender;

impl ReqwestSender {
    pub fn new() -> Self {
        Self
    }

    fn build_client(req: &WireRequest) -> reqwest::Client {
        let policy = if req.follow_redirects {
            reqwest::redirect::Policy::limited(req.max_redirects as usize)
        } else {
            reqwest::redirect::Policy::none()
        };
        reqwest::Client::builder()
            .redirect(policy)
            .timeout(req.timeout)
            .build()
            .expect("reqwest client build")
    }
}

#[async_trait]
impl HttpSender for ReqwestSender {
    async fn send(&self, r: WireRequest) -> Result<WireResponse, SendError> {
        let client = Self::build_client(&r);
        let mut rb = client.request(r.method.clone(), &r.url);
        for (k, v) in &r.headers {
            rb = rb.header(k, v);
        }
        if let Some(parts) = r.multipart.as_ref() {
            let mut form = reqwest::multipart::Form::new();
            for f in parts {
                form = match &f.kind {
                    MultipartKind::Text(s) => {
                        form.part(f.name.clone(), reqwest::multipart::Part::text(s.clone()))
                    }
                    MultipartKind::File(path) => {
                        let mime = mime_guess::from_path(path)
                            .first_or_octet_stream()
                            .essence_str()
                            .to_string();
                        let part = reqwest::multipart::Part::file(path)
                            .await
                            .map_err(|e| SendError::Other(anyhow::anyhow!(e)))?
                            .mime_str(&mime)
                            .map_err(|e| SendError::Other(anyhow::anyhow!(e)))?;
                        let part = match &f.filename {
                            Some(n) => part.file_name(n.clone()),
                            None => part,
                        };
                        form.part(f.name.clone(), part)
                    }
                };
            }
            rb = rb.multipart(form);
        } else if !r.body_bytes.is_empty() {
            rb = rb.body(r.body_bytes.clone());
        }
        let started = Instant::now();
        let resp = rb.send().await.map_err(map_err)?;
        let status = resp.status().as_u16();
        let headers: Vec<(String, String)> = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let bytes = resp.bytes().await.map_err(map_err)?.to_vec();
        let elapsed = started.elapsed();
        let size = bytes.len() as u64;
        Ok(WireResponse {
            status,
            headers,
            body_bytes: bytes,
            elapsed,
            size,
        })
    }
}

fn map_err(e: reqwest::Error) -> SendError {
    if e.is_timeout() {
        SendError::Timeout
    } else if e.is_connect() {
        SendError::Net(format!("{e}"))
    } else {
        SendError::Other(anyhow::anyhow!(e))
    }
}
