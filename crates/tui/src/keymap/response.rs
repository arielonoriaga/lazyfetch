//! Response-pane key apply.
//!
//! Cursor motion, visual select, yank, search, and the cURL export
//! action all dispatch off the Response pane. Pulled out of
//! `keymap/mod.rs` to keep `apply` navigable.

use super::{Action, EnvDirty};
use crate::app::{AppState, Mode};
use crate::motion::{
    copy_to_clipboard, current_line_len, current_line_text, first_non_space_col,
    matching_brace_position, next_word_pos, prev_word_pos, selection_text, sibling_target,
};

pub(super) fn apply_action(state: &mut AppState, action: &Action) -> Option<EnvDirty> {
    match action {
        Action::CursorBy(delta) => {
            state.move_cursor_by(*delta);
            state.pending_g = false;
        }
        Action::CursorPageBy(pages) => {
            let h = state.response_height.max(1) as i32;
            state.move_cursor_by(pages * h);
            state.pending_g = false;
        }
        Action::CursorTop => {
            state.move_cursor_to(0);
            state.pending_g = false;
        }
        Action::CursorBottom => {
            let last = state.response_total_lines.saturating_sub(1);
            state.move_cursor_to(last);
            state.pending_g = false;
        }
        Action::PendingG => state.pending_g = true,
        Action::CursorParagraphNext => {
            state.pending_g = false;
            if let Some(body) = state.last_response_pretty.as_deref() {
                let cur = state.response_cursor;
                let target = body
                    .lines()
                    .enumerate()
                    .skip(cur + 1)
                    .find(|(_, l)| l.trim().is_empty())
                    .map(|(i, _)| i)
                    .unwrap_or_else(|| body.lines().count().saturating_sub(1));
                state.move_cursor_to(target);
            }
        }
        Action::CursorParagraphPrev => {
            state.pending_g = false;
            if let Some(body) = state.last_response_pretty.as_deref() {
                let cur = state.response_cursor;
                let collected: Vec<(usize, &str)> = body.lines().enumerate().take(cur).collect();
                let target = collected
                    .into_iter()
                    .rev()
                    .find(|(_, l)| l.trim().is_empty())
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                state.move_cursor_to(target);
            }
        }
        Action::CursorViewportTop => {
            state.pending_g = false;
            let target = state.response_scroll as usize;
            state.move_cursor_to(target);
        }
        Action::CursorViewportMid => {
            state.pending_g = false;
            let h = state.response_height.max(1) as usize;
            let target = state.response_scroll as usize + h / 2;
            state.move_cursor_to(target);
        }
        Action::CursorViewportBot => {
            state.pending_g = false;
            let h = state.response_height.max(1) as usize;
            let target = state.response_scroll as usize + h.saturating_sub(1);
            state.move_cursor_to(target);
        }
        Action::JumpMatchingBrace => {
            state.pending_g = false;
            if let Some((line, col)) = matching_brace_position(state) {
                state.move_cursor_to(line);
                let len = current_line_len(state);
                state.move_col_to(col, len);
            } else {
                state.notify("no matching brace from cursor".to_string());
            }
        }
        Action::JumpSiblingNext => {
            state.pending_g = false;
            if let Some(target) = sibling_target(state, 1) {
                state.move_cursor_to(target);
                let col = first_non_space_col(state);
                let len = current_line_len(state);
                state.move_col_to(col, len);
            }
        }
        Action::JumpSiblingPrev => {
            state.pending_g = false;
            if let Some(target) = sibling_target(state, -1) {
                state.move_cursor_to(target);
                let col = first_non_space_col(state);
                let len = current_line_len(state);
                state.move_col_to(col, len);
            }
        }
        Action::ColBy(d) => {
            let len = current_line_len(state);
            state.move_col_by(*d, len);
        }
        Action::ColLineStart => {
            let len = current_line_len(state);
            state.move_col_to(0, len);
        }
        Action::ColLineEnd => {
            let len = current_line_len(state);
            #[allow(clippy::implicit_saturating_sub)]
            state.move_col_to(if len > 0 { len - 1 } else { 0 }, len);
        }
        Action::WordNext => {
            if let Some((line, col)) = next_word_pos(state) {
                let len = current_line_len(state);
                state.response_cursor = line;
                state.move_col_to(col, len);
            }
        }
        Action::WordPrev => {
            if let Some((line, col)) = prev_word_pos(state) {
                let len = current_line_len(state);
                state.response_cursor = line;
                state.move_col_to(col, len);
            }
        }
        Action::ToggleVisual => {
            if state.visual_anchor.is_some() {
                state.visual_anchor = None;
                state.notify("visual off".to_string());
            } else {
                state.visual_anchor = Some((state.response_cursor, state.response_col));
                state.notify("-- VISUAL --".to_string());
            }
        }
        Action::EscapeVisual => {
            state.visual_anchor = None;
            state.notify("visual off".to_string());
        }
        Action::YankSelection => {
            let text = selection_text(state).unwrap_or_else(|| current_line_text(state));
            match copy_to_clipboard(&text) {
                Ok(()) => state.toast = Some(format!("yanked {} chars", text.len())),
                Err(e) => state.toast = Some(format!("yank failed: {}", e)),
            }
            state.visual_anchor = None;
        }
        Action::EnterSearch => {
            state.mode = Mode::Search;
            state.search_buf.clear();
        }
        Action::SearchChar(c) => state.search_buf.push(*c),
        Action::SearchBackspace => {
            state.search_buf.pop();
        }
        Action::SearchCancel => {
            state.mode = Mode::Normal;
            state.search_buf.clear();
            state.search_active = None;
            state.search_match_lines.clear();
            state.search_match_idx = 0;
            state.highlighted_cache = None;
        }
        Action::SearchSubmit => {
            state.mode = Mode::Normal;
            let needle = std::mem::take(&mut state.search_buf);
            state.search_match_idx = 0;
            state.search_match_lines.clear();
            if needle.is_empty() {
                state.search_active = None;
            } else {
                let needle_lc = needle.to_lowercase();
                let matches: Vec<usize> = state
                    .last_response_pretty
                    .as_deref()
                    .map(|b| {
                        b.lines()
                            .enumerate()
                            .filter(|(_, l)| l.to_lowercase().contains(&needle_lc))
                            .map(|(i, _)| i)
                            .collect()
                    })
                    .unwrap_or_default();
                state.search_match_lines = matches;
                if let Some(&first) = state.search_match_lines.first() {
                    state.move_cursor_to(first);
                }
                if let Some(base) = state.last_response_lines.clone() {
                    let highlighted = crate::response::apply_search_highlight(base, &needle).0;
                    state.highlighted_cache = Some((state.body_gen, needle.clone(), highlighted));
                }
                state.search_active = Some(needle);
            }
        }
        Action::SearchNext => {
            if !state.search_match_lines.is_empty() {
                state.search_match_idx =
                    (state.search_match_idx + 1) % state.search_match_lines.len();
                let target = state.search_match_lines[state.search_match_idx];
                state.move_cursor_to(target);
            }
        }
        Action::SearchPrev => {
            if !state.search_match_lines.is_empty() {
                let n = state.search_match_lines.len();
                state.search_match_idx = (state.search_match_idx + n - 1) % n;
                let target = state.search_match_lines[state.search_match_idx];
                state.move_cursor_to(target);
            }
        }
        Action::CurlExport => {
            if let Some(executed) = &state.last_response {
                let curl =
                    lazyfetch_core::exec::build_curl(&executed.request_snapshot, &executed.secrets);
                let len = curl.len();
                match copy_to_clipboard(&curl) {
                    Ok(()) => state.notify(format!("cURL → clipboard ({len} chars)")),
                    Err(e) => state.notify(format!("clipboard failed: {e}")),
                }
            } else {
                state.notify("nothing sent yet".to_string());
            }
        }
        _ => return None,
    }
    Some(EnvDirty::No)
}
