//! Collections-pane key apply + the modals it owns.
//!
//! Collections (Coll*) and the modals triggered from them — Rename (rename a
//! collection or request), Move (relocate marked requests), SaveAs (persist
//! the current URL as a Request into a collection) — co-locate here. The
//! help-filter modal also lives here since it shares the dispatch shape.

use super::{Action, EnvDirty};
use crate::app::{AppState, Focus, Mode};
use crate::commands::{run_move, run_rename, run_save};

pub(super) fn apply_action(state: &mut AppState, action: &Action) -> Option<EnvDirty> {
    match action {
        Action::CollCursorUp => state.coll_cursor = state.coll_cursor.saturating_sub(1),
        Action::CollCursorDown => {
            let max = state.coll_rows().len();
            if max > 0 && state.coll_cursor + 1 < max {
                state.coll_cursor += 1;
            }
        }
        Action::CollToggle => {
            state.coll_toggle_expand();
        }
        Action::CollOpen => {
            if state.coll_toggle_expand() {
                return Some(EnvDirty::No);
            }
            if let Some(name) = state.coll_open_selected() {
                state.notify(format!("loaded {}", name));
                state.focus = Focus::Url;
            }
        }
        Action::CollRenameStart => {
            use crate::app::{CollRow, RenameTarget};
            let rows = state.coll_rows();
            if let Some(row) = rows.get(state.coll_cursor).copied() {
                let target = match row {
                    CollRow::Coll { idx, .. } => {
                        let name = state.collections[idx].name.clone();
                        Some(RenameTarget::Collection {
                            idx,
                            old: name.clone(),
                        })
                    }
                    CollRow::Req { coll, item } => {
                        if let lazyfetch_core::catalog::Item::Request(r) =
                            &state.collections[coll].root.items[item]
                        {
                            Some(RenameTarget::Request {
                                coll,
                                item,
                                old: r.name.clone(),
                            })
                        } else {
                            None
                        }
                    }
                };
                if let Some(t) = target {
                    state.rename_buf = match &t {
                        RenameTarget::Collection { old, .. }
                        | RenameTarget::Request { old, .. } => old.clone(),
                    };
                    state.rename_target = Some(t);
                    state.mode = Mode::Rename;
                }
            }
        }
        Action::RenameChar(c) => state.rename_buf.push(*c),
        Action::RenameBackspace => {
            state.rename_buf.pop();
        }
        Action::RenameCancel => {
            state.mode = Mode::Normal;
            state.rename_target = None;
            state.rename_buf.clear();
        }
        Action::RenameSubmit => {
            let new = std::mem::take(&mut state.rename_buf);
            let target = state.rename_target.take();
            state.mode = Mode::Normal;
            run_rename(state, target, new.trim());
        }
        Action::HelpFilterChar(c) => state.help_filter.push(*c),
        Action::HelpFilterBackspace => {
            state.help_filter.pop();
        }
        Action::CollToggleMark => {
            use crate::app::CollRow;
            if let Some(CollRow::Req { coll, item }) =
                state.coll_rows().get(state.coll_cursor).copied()
            {
                let key = (coll, item);
                if state.marked_requests.contains(&key) {
                    state.marked_requests.remove(&key);
                } else {
                    state.marked_requests.insert(key);
                }
            }
        }
        Action::CollMoveStart => {
            use crate::app::CollRow;
            if state.marked_requests.is_empty() {
                if let Some(CollRow::Req { coll, item }) =
                    state.coll_rows().get(state.coll_cursor).copied()
                {
                    state.marked_requests.insert((coll, item));
                }
            }
            if state.marked_requests.is_empty() {
                state.notify("nothing to move (use 'x' to mark requests)".to_string());
                return Some(EnvDirty::No);
            }
            state.move_buf.clear();
            state.mode = Mode::Move;
        }
        Action::MoveChar(c) => state.move_buf.push(*c),
        Action::MoveBackspace => {
            state.move_buf.pop();
        }
        Action::MoveCancel => {
            state.mode = Mode::Normal;
            state.move_buf.clear();
        }
        Action::MoveSubmit => {
            let target = std::mem::take(&mut state.move_buf);
            state.mode = Mode::Normal;
            run_move(state, target.trim());
        }
        Action::EnterSaveAs => {
            if state.url_buf.is_empty() {
                state.notify("URL is empty — nothing to save".to_string());
                return Some(EnvDirty::No);
            }
            state.mode = Mode::SaveAs;
            state.save_buf = match state.collections.first() {
                Some(c) => format!("{}/", c.name),
                None => String::new(),
            };
        }
        Action::SaveAsChar(c) => state.save_buf.push(*c),
        Action::SaveAsBackspace => {
            state.save_buf.pop();
        }
        Action::SaveAsCancel => state.mode = Mode::Normal,
        Action::SaveAsSubmit => {
            let path = std::mem::take(&mut state.save_buf);
            state.mode = Mode::Normal;
            run_save(state, path.trim());
        }
        _ => return None,
    }
    Some(EnvDirty::No)
}
