//! Env-pane key apply + the popup Insert-mode editor it owns.
//!
//! The `Insert*` action family lives here because the popup is exclusively
//! used to edit env variables (key + value, with a secret toggle). If
//! another pane ever needs an insert popup, those arms can move.

use super::{Action, EnvDirty};
use crate::app::{AppState, InsertBuf, InsertField, Mode};

pub(super) fn apply_action(state: &mut AppState, action: &Action) -> Option<EnvDirty> {
    match action {
        Action::EnvCursorUp => {
            if state.env_cursor > 0 {
                state.env_cursor -= 1;
            }
        }
        Action::EnvCursorDown => {
            let max = state.active_env_ref().map(|e| e.vars.len()).unwrap_or(0);
            if max > 0 && state.env_cursor + 1 < max {
                state.env_cursor += 1;
            }
        }
        Action::EnvAdd { secret } => {
            state.mode = Mode::Insert;
            state.insert_buf = Some(InsertBuf::new(*secret));
        }
        Action::EnvEdit => {
            let cur = state.env_cursor;
            if let Some((k, v, secret)) = state.env_var_at(cur) {
                let key = k.clone();
                let value = v.to_string();
                state.mode = Mode::Insert;
                state.insert_buf = Some(InsertBuf::editing(cur, key, value, secret));
            } else {
                state.notify("nothing to edit".to_string());
            }
        }
        Action::EnvToggleReveal => {
            state.toggle_reveal();
        }
        Action::EnvDelete => {
            if state.delete_var() {
                return Some(EnvDirty::Yes);
            }
        }
        Action::EnvToggleSecret => {
            if state.toggle_secret() {
                return Some(EnvDirty::Yes);
            }
        }
        Action::InsertChar(c) => {
            if let Some(buf) = state.insert_buf.as_mut() {
                match buf.field {
                    InsertField::Key => buf.key.push(*c),
                    InsertField::Value => buf.value.push(*c),
                }
            }
        }
        Action::InsertBackspace => {
            if let Some(buf) = state.insert_buf.as_mut() {
                match buf.field {
                    InsertField::Key => buf.key.pop(),
                    InsertField::Value => buf.value.pop(),
                };
            }
        }
        Action::InsertNextField => {
            if let Some(buf) = state.insert_buf.as_mut() {
                buf.field = match buf.field {
                    InsertField::Key => InsertField::Value,
                    InsertField::Value => InsertField::Key,
                };
            }
        }
        Action::InsertCancel => {
            state.mode = Mode::Normal;
            state.insert_buf = None;
        }
        Action::InsertSubmit => {
            if let Some(buf) = state.insert_buf.take() {
                state.mode = Mode::Normal;
                if buf.key.is_empty() {
                    return Some(EnvDirty::No);
                }
                match buf.edit_idx {
                    Some(i) => {
                        if state.replace_var(i, buf.key, buf.value, buf.secret) {
                            return Some(EnvDirty::Yes);
                        }
                    }
                    None => {
                        state.add_var(buf.key, buf.value, buf.secret);
                        return Some(EnvDirty::Yes);
                    }
                }
            } else {
                state.mode = Mode::Normal;
            }
        }
        _ => return None,
    }
    Some(EnvDirty::No)
}
