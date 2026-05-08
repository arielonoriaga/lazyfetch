//! Repeat-last semantics — `R` replays Executed.request_template, not the
//! user's current edits in url_buf / method / body.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use http::Method;
use lazyfetch_core::catalog::{Body, Request};
use lazyfetch_core::exec::{Executed, WireRequest, WireResponse};
use lazyfetch_core::primitives::{Template, UrlTemplate};
use lazyfetch_core::secret::SecretRegistry;
use lazyfetch_tui::app::AppState;
use lazyfetch_tui::keymap::{dispatch, Action};
use std::path::PathBuf;
use std::time::Duration;
use ulid::Ulid;

fn fake_executed(method: Method, url: &str) -> Executed {
    let template = Request {
        id: Ulid::new(),
        name: "frozen".into(),
        method: method.clone(),
        url: UrlTemplate(Template(url.into())),
        query: vec![],
        headers: vec![],
        body: Body::None,
        auth: None,
        notes: None,
        follow_redirects: true,
        max_redirects: 10,
        timeout_ms: None,
    };
    Executed {
        request_template: template.clone(),
        request_snapshot: WireRequest {
            method,
            url: url.into(),
            headers: vec![],
            body_bytes: vec![],
            multipart: None,
            timeout: Duration::from_secs(5),
            follow_redirects: true,
            max_redirects: 10,
        },
        response: WireResponse {
            status: 200,
            headers: vec![],
            body_bytes: vec![],
            elapsed: Duration::from_millis(42),
            size: 0,
        },
        at: chrono::Utc::now(),
        secrets: SecretRegistry::new(),
    }
}

#[test]
fn capital_r_emits_repeat_last_action() {
    let mut s = AppState::new(PathBuf::from("/tmp"));
    s.last_response = Some(fake_executed(Method::GET, "https://api.test/x"));
    let ev = KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT);
    let a = dispatch(&s, ev);
    assert_eq!(a, Action::RepeatLast);
}

#[test]
fn repeat_last_template_unchanged_by_url_buf_edits() {
    // Snapshot was captured with a specific url+method.
    let mut s = AppState::new(PathBuf::from("/tmp"));
    s.last_response = Some(fake_executed(Method::POST, "https://api.test/orig"));
    // Simulate the user editing the URL bar and changing method since.
    s.url_buf = "https://different.test/edited".into();
    s.method = Method::DELETE;
    // The template the event loop reads on `R` is independent.
    let template = s
        .last_response
        .as_ref()
        .map(|e| e.request_template.clone())
        .unwrap();
    assert_eq!(template.url.0 .0, "https://api.test/orig");
    assert_eq!(template.method, Method::POST);
    // And state's edits stayed put.
    assert_eq!(s.url_buf, "https://different.test/edited");
    assert_eq!(s.method, Method::DELETE);
}

#[test]
fn repeat_last_with_no_response_yet_still_dispatches_action() {
    // Action::RepeatLast fires even when last_response is None — event::run
    // is responsible for the "nothing sent yet" toast.
    let s = AppState::new(PathBuf::from("/tmp"));
    let ev = KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT);
    assert_eq!(dispatch(&s, ev), Action::RepeatLast);
}
