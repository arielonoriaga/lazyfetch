//! Request-pane key dispatch + apply.
//!
//! Splits out the v0.2 Request-pane keymap (Body / Headers / Query tabs,
//! Hybrid KV editor, body insert mode, $EDITOR shell-out sentinel) so the
//! main `keymap/mod.rs` doesn't drown in pane-specific match arms.
//!
//! The unified `Action` enum lives in `keymap/mod.rs` to keep the public
//! API (`dispatch` / `apply` / `Action`) stable across this split.

use super::{Action, EnvDirty};
use crate::app::{AppState, ReqTab};
use crate::editor::{BodyEditorState, GraphQlFocus};
use crate::kv_editor::{KvEditor, KvMode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lazyfetch_core::catalog::BodyKind;

/// Returns `Some(action)` when the Request pane consumes the key, `None` to
/// fall through to global navigation (so `Tab`/`h`/`l` etc. still switch
/// panes when the pane isn't actively editing).
pub(super) fn dispatch(state: &AppState, ev: KeyEvent) -> Option<Action> {
    if state.body_editing {
        return Some(match (ev.code, ev.modifiers) {
            (KeyCode::Esc, _) => Action::BodyExitEdit,
            (KeyCode::Enter, _) => Action::BodyInputNewline,
            (KeyCode::Backspace, _) => Action::BodyInputBackspace,
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                Action::BodyInputChar(c)
            }
            _ => Action::NoOp,
        });
    }
    if !matches!(active_kv_mode(state), KvMode::Normal) {
        return Some(match (ev.code, ev.modifiers) {
            (KeyCode::Esc, _) => Action::KvCancel,
            (KeyCode::Enter, _) => Action::KvCommit,
            (KeyCode::Tab, _) => Action::KvInsertTab,
            (KeyCode::Backspace, _) => Action::KvInsertBackspace,
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                Action::KvInsertChar(c)
            }
            _ => Action::NoOp,
        });
    }
    let on_body = state.req_tab == ReqTab::Body;
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('1'), KeyModifiers::NONE) => Some(Action::ReqTabSwitch(ReqTab::Body)),
        (KeyCode::Char('2'), KeyModifiers::NONE) => Some(Action::ReqTabSwitch(ReqTab::Headers)),
        (KeyCode::Char('3'), KeyModifiers::NONE) => Some(Action::ReqTabSwitch(ReqTab::Query)),
        (KeyCode::Char(' '), KeyModifiers::NONE) => Some(Action::ReqTabCycle),
        (KeyCode::Char('t'), KeyModifiers::NONE) if on_body => Some(Action::BodyKindCycle),
        (KeyCode::Char('i'), KeyModifiers::NONE) if on_body => Some(Action::BodyEnterEdit),
        (KeyCode::Char('a'), KeyModifiers::NONE) if on_body => Some(Action::BodyEnterEdit),
        (KeyCode::Char('e'), KeyModifiers::NONE) if on_body => Some(Action::BodyShellOut),
        (KeyCode::Char('f'), KeyModifiers::NONE)
            if on_body && state.req_body_kind == BodyKind::Multipart =>
        {
            Some(Action::KvToggleKind)
        }
        // KV navigation/edit — only on KV tabs.
        (KeyCode::Char('j'), KeyModifiers::NONE) if !on_body => Some(Action::KvCursorDown),
        (KeyCode::Char('k'), KeyModifiers::NONE) if !on_body => Some(Action::KvCursorUp),
        (KeyCode::Down, _) if !on_body => Some(Action::KvCursorDown),
        (KeyCode::Up, _) if !on_body => Some(Action::KvCursorUp),
        (KeyCode::Char('a'), KeyModifiers::NONE) if !on_body => Some(Action::KvAdd),
        (KeyCode::Char('i'), KeyModifiers::NONE) if !on_body => Some(Action::KvEditValue),
        (KeyCode::Char('x'), KeyModifiers::NONE) if !on_body => Some(Action::KvToggleEnabled),
        (KeyCode::Char('d'), KeyModifiers::NONE) if !on_body => Some(Action::KvDelete),
        (KeyCode::Char('m'), KeyModifiers::NONE) if !on_body => Some(Action::KvToggleSecret),
        _ => None,
    }
}

/// Apply a Request-pane action. Returns `Some(dirty)` when the action was
/// handled here; `None` lets `keymap::apply` fall through to its other arms.
pub(super) fn apply_action(state: &mut AppState, action: &Action) -> Option<EnvDirty> {
    match action {
        Action::ReqTabSwitch(t) => state.req_tab = *t,
        Action::ReqTabCycle => state.req_tab = state.req_tab.cycle(),
        Action::BodyKindCycle => cycle_body_kind(state),
        Action::KvCursorUp => active_kv_mut(state).move_up(),
        Action::KvCursorDown => active_kv_mut(state).move_down(),
        Action::KvAdd => active_kv_mut(state).start_add(),
        Action::KvEditValue => active_kv_mut(state).start_edit_value(),
        Action::KvToggleEnabled => active_kv_mut(state).toggle_enabled(),
        Action::KvDelete => active_kv_mut(state).delete(),
        Action::KvToggleSecret => active_kv_mut(state).toggle_secret(),
        Action::KvToggleKind => active_kv_mut(state).toggle_kind(),
        Action::KvInsertChar(c) => active_kv_mut(state).insert_char(*c),
        Action::KvInsertBackspace => active_kv_mut(state).backspace(),
        Action::KvInsertTab => active_kv_mut(state).tab(),
        Action::KvCommit => active_kv_mut(state).commit(),
        Action::KvCancel => active_kv_mut(state).cancel(),
        Action::BodyEnterEdit => enter_body_edit(state),
        Action::BodyExitEdit => state.body_editing = false,
        Action::BodyInputChar(c) => with_body(state, BodyOp::InsertChar(*c)),
        Action::BodyInputNewline => with_body(state, BodyOp::Newline),
        Action::BodyInputBackspace => with_body(state, BodyOp::Backspace),
        // BodyShellOut is a sentinel — event::run owns it (needs &mut TerminalGuard).
        Action::BodyShellOut => {}
        _ => return None,
    }
    Some(EnvDirty::No)
}

fn active_kv_mode(state: &AppState) -> KvMode {
    match state.req_tab {
        ReqTab::Headers => state.headers_kv.mode,
        ReqTab::Query => state.query_kv.mode,
        ReqTab::Body => state.form_kv.mode,
    }
}

fn active_kv_mut(state: &mut AppState) -> &mut KvEditor {
    match state.req_tab {
        ReqTab::Headers => &mut state.headers_kv,
        ReqTab::Query => &mut state.query_kv,
        ReqTab::Body => &mut state.form_kv,
    }
}

/// Cycle through body kinds. Stashes the current editor's text in
/// `body_scratch` so a switch through a KV-backed kind (Form/Multipart/None
/// /File) doesn't drop typed body text on the way back.
fn cycle_body_kind(state: &mut AppState) {
    let prev = state.body_editor.text();
    if !prev.is_empty() {
        state.body_scratch = prev.clone();
    }
    state.req_body_kind = match state.req_body_kind {
        BodyKind::None => BodyKind::Raw,
        BodyKind::Raw => BodyKind::Json,
        BodyKind::Json => BodyKind::Form,
        BodyKind::Form => BodyKind::Multipart,
        BodyKind::Multipart => BodyKind::GraphQL,
        BodyKind::GraphQL => BodyKind::File,
        BodyKind::File => BodyKind::None,
    };
    let restore = if prev.is_empty() {
        state.body_scratch.clone()
    } else {
        prev
    };
    state.body_editor = BodyEditorState::for_kind(state.req_body_kind, &restore);
}

fn enter_body_edit(state: &mut AppState) {
    if matches!(state.req_body_kind, BodyKind::None | BodyKind::File) {
        state.req_body_kind = BodyKind::Raw;
    }
    if matches!(state.body_editor, BodyEditorState::None) {
        let prev = state.body_editor.text();
        state.body_editor = BodyEditorState::for_kind(state.req_body_kind, &prev);
    }
    state.body_editing = true;
    state.notify("body insert mode — Esc to exit · :e to shell out".into());
}

enum BodyOp {
    InsertChar(char),
    Newline,
    Backspace,
}

fn with_body(state: &mut AppState, op: BodyOp) {
    let target: Option<&mut tui_textarea::TextArea<'static>> = match &mut state.body_editor {
        BodyEditorState::Single(ta) => Some(ta.as_mut()),
        BodyEditorState::Split {
            query,
            focus: GraphQlFocus::Query,
            ..
        } => Some(query.as_mut()),
        BodyEditorState::Split { variables, .. } => Some(variables.as_mut()),
        BodyEditorState::None => None,
    };
    let Some(ta) = target else { return };
    match op {
        BodyOp::InsertChar(c) => {
            ta.insert_char(c);
        }
        BodyOp::Newline => {
            ta.insert_newline();
        }
        BodyOp::Backspace => {
            ta.delete_char();
        }
    }
}
