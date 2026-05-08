//! Demonstrates the HTTP-port inversion: TUI's send pipeline runs against
//! an injected `HttpSender` mock, no reqwest stack required.

use async_trait::async_trait;
use http::Method;
use lazyfetch_core::exec::{HttpSender, SendError, WireRequest, WireResponse};
use lazyfetch_tui::adapters::{Adapters, NullAuthCache, NullAuthResolver};
use lazyfetch_tui::app::AppState;
use lazyfetch_tui::sender;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct RecordingSender {
    seen: Mutex<Vec<WireRequest>>,
}

#[async_trait]
impl HttpSender for RecordingSender {
    async fn send(&self, r: WireRequest) -> Result<WireResponse, SendError> {
        self.seen.lock().unwrap().push(r);
        Ok(WireResponse {
            status: 204,
            headers: vec![("X-Stub".into(), "yes".into())],
            body_bytes: vec![],
            elapsed: Duration::from_millis(7),
            size: 0,
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn injected_sender_handles_dispatch() {
    let recorder: Arc<RecordingSender> = Arc::new(RecordingSender::default());
    let adapters = Adapters::new(
        recorder.clone(),
        Arc::new(NullAuthResolver),
        Arc::new(NullAuthCache),
    );
    let mut state = AppState::new(PathBuf::from("/tmp")).with_adapters(adapters);
    state.method = Method::GET;
    state.url_buf = "https://api.test/probe".into();

    let rt = tokio::runtime::Handle::current();
    let rx = sender::dispatch(&state, rt);

    // Block until the spawned task completes — recv blocks max 2s in case of regression.
    let result = tokio::task::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(2))
    })
    .await
    .unwrap()
    .expect("send result");
    let executed = result.expect("execute Ok");

    assert_eq!(executed.response.status, 204);
    let seen = recorder.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].url, "https://api.test/probe");
    assert_eq!(seen[0].method, Method::GET);
}
