//! URL bar key dispatch + apply.
//!
//! Owns:
//! - `dispatch` — fall-through key map for the URL pane (handles plain
//!   character input + the `{{var}}` autocomplete intercepts).
//! - `apply_action` — UrlChar / UrlBackspace / UrlSuggest* / UrlSubmit /
//!   Method*.

use super::{Action, EnvDirty};
use crate::app::{AppState, Focus};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Final-resort dispatcher for keys that landed in the URL bar without
/// matching a navigation/control binding. Plain characters become
/// `UrlChar`; Backspace becomes `UrlBackspace`; everything else is no-op.
pub(super) fn dispatch(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Backspace, _) => Action::UrlBackspace,
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            Action::UrlChar(c)
        }
        _ => Action::NoOp,
    }
}

/// Apply a URL-bar / method action. Returns `Some(EnvDirty)` when the
/// action belongs to this pane, `None` to fall through.
pub(super) fn apply_action(state: &mut AppState, action: &Action) -> Option<EnvDirty> {
    match action {
        Action::UrlChar(c) => {
            state.url_buf.push(*c);
            state.url_suggest_idx = 0;
        }
        Action::UrlBackspace => {
            state.url_buf.pop();
            state.url_suggest_idx = 0;
        }
        Action::UrlSuggestNext => {
            let n = state.url_var_suggestions().len();
            if n > 0 {
                state.url_suggest_idx = (state.url_suggest_idx + 1) % n;
            }
        }
        Action::UrlSuggestPrev => {
            let n = state.url_var_suggestions().len();
            if n > 0 {
                state.url_suggest_idx = (state.url_suggest_idx + n - 1) % n;
            }
        }
        Action::UrlSuggestAccept => {
            let suggestions = state.url_var_suggestions();
            if let Some(name) = suggestions.get(state.url_suggest_idx).cloned() {
                state.url_complete_var(&name);
            }
        }
        Action::UrlSuggestDismiss => {
            state.url_buf.push_str("}}");
            state.url_suggest_idx = 0;
        }
        Action::UrlSubmit => {
            state.notify(format!("URL: {}", state.url_buf));
            state.focus = Focus::Request;
        }
        Action::MethodNext => {
            state.method = next_method(&state.method);
            state.notify(format!("method: {}", state.method));
        }
        Action::MethodPrev => {
            state.method = prev_method(&state.method);
            state.notify(format!("method: {}", state.method));
        }
        _ => return None,
    }
    Some(EnvDirty::No)
}

fn next_method(m: &http::Method) -> http::Method {
    super::next_method(m)
}
fn prev_method(m: &http::Method) -> http::Method {
    super::prev_method(m)
}
