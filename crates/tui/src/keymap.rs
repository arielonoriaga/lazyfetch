use crate::app::{AppState, Dir, Focus, InsertBuf, InsertField, Mode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    FocusNext,
    FocusPrev,
    FocusDir(Dir),
    FocusJump(Focus),
    EnvCursorUp,
    EnvCursorDown,
    EnvAdd { secret: bool },
    EnvEdit,
    EnvDelete,
    EnvToggleSecret,
    EnvToggleReveal,
    EnterCommand,
    CommandChar(char),
    CommandBackspace,
    CommandSubmit,
    CommandCancel,
    InsertChar(char),
    InsertBackspace,
    InsertNextField,
    InsertSubmit,
    InsertCancel,
    ToggleHelp,
    CloseHelp,
    CloseMessages,
    UrlChar(char),
    UrlBackspace,
    UrlSubmit,
    UrlSuggestNext,
    UrlSuggestPrev,
    UrlSuggestAccept,
    UrlSuggestDismiss,
    MethodNext,
    MethodPrev,
    SendRequest,
    CursorBy(i32),
    CursorPageBy(i32),
    CursorTop,
    CursorBottom,
    CursorParagraphNext,
    CursorParagraphPrev,
    CursorViewportTop,
    CursorViewportMid,
    CursorViewportBot,
    PendingG,
    JumpMatchingBrace,
    JumpSiblingNext,
    JumpSiblingPrev,
    ColBy(i32),
    ColLineStart,
    ColLineEnd,
    WordNext,
    WordPrev,
    ToggleVisual,
    YankSelection,
    EscapeVisual,
    EnterSearch,
    SearchChar(char),
    SearchBackspace,
    SearchSubmit,
    SearchCancel,
    SearchNext,
    SearchPrev,
    CollCursorUp,
    CollCursorDown,
    CollToggle,
    CollOpen,
    CollRenameStart,
    CollToggleMark,
    CollMoveStart,
    MoveChar(char),
    MoveBackspace,
    MoveSubmit,
    MoveCancel,
    RenameChar(char),
    RenameBackspace,
    RenameSubmit,
    RenameCancel,
    HelpFilterChar(char),
    HelpFilterBackspace,
    EnterSaveAs,
    SaveAsChar(char),
    SaveAsBackspace,
    SaveAsSubmit,
    SaveAsCancel,
    NoOp,
}

pub fn dispatch(state: &AppState, ev: KeyEvent) -> Action {
    if state.messages_open {
        return match (ev.code, ev.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
            _ => Action::CloseMessages,
        };
    }
    if state.help_open {
        return match (ev.code, ev.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
            (KeyCode::Esc, _) | (KeyCode::Char('?'), _) => Action::CloseHelp,
            (KeyCode::Backspace, _) => Action::HelpFilterBackspace,
            (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                Action::HelpFilterChar(c)
            }
            _ => Action::NoOp,
        };
    }
    match state.mode {
        Mode::Normal => dispatch_normal(state, ev),
        Mode::Command => dispatch_command(ev),
        Mode::Insert => dispatch_insert(ev),
        Mode::Search => dispatch_search(ev),
        Mode::SaveAs => dispatch_save_as(ev),
        Mode::Rename => dispatch_rename(ev),
        Mode::Move => dispatch_move(ev),
    }
}

fn dispatch_move(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Esc, _) => Action::MoveCancel,
        (KeyCode::Enter, _) => Action::MoveSubmit,
        (KeyCode::Backspace, _) => Action::MoveBackspace,
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            Action::MoveChar(c)
        }
        _ => Action::NoOp,
    }
}

fn dispatch_rename(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Esc, _) => Action::RenameCancel,
        (KeyCode::Enter, _) => Action::RenameSubmit,
        (KeyCode::Backspace, _) => Action::RenameBackspace,
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            Action::RenameChar(c)
        }
        _ => Action::NoOp,
    }
}

fn dispatch_save_as(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Esc, _) => Action::SaveAsCancel,
        (KeyCode::Enter, _) => Action::SaveAsSubmit,
        (KeyCode::Backspace, _) => Action::SaveAsBackspace,
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            Action::SaveAsChar(c)
        }
        _ => Action::NoOp,
    }
}

fn dispatch_search(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Esc, _) => Action::SearchCancel,
        (KeyCode::Enter, _) => Action::SearchSubmit,
        (KeyCode::Backspace, _) => Action::SearchBackspace,
        (KeyCode::F(5), _) => Action::SendRequest,
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            Action::SearchChar(c)
        }
        _ => Action::NoOp,
    }
}

fn dispatch_normal(state: &AppState, ev: KeyEvent) -> Action {
    // URL bar is a text input — chars go to the buffer, not to global keymap.
    // Only navigation/control keys escape it.
    if state.focus == Focus::Url {
        // If a {{var}} suggestion is active, intercept navigation/select keys.
        let suggestions_active = !state.url_var_suggestions().is_empty();
        return match (ev.code, ev.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::SendRequest,
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => Action::EnterSaveAs,
            (KeyCode::F(5), _) => Action::SendRequest,
            (KeyCode::Enter, _) if suggestions_active => Action::UrlSuggestAccept,
            (KeyCode::Tab, _) if suggestions_active => Action::UrlSuggestAccept,
            (KeyCode::Down, _) if suggestions_active => Action::UrlSuggestNext,
            (KeyCode::Up, _) if suggestions_active => Action::UrlSuggestPrev,
            (KeyCode::Esc, _) if suggestions_active => Action::UrlSuggestDismiss,
            (KeyCode::Enter, _) => Action::SendRequest,
            (KeyCode::Tab, _) => Action::FocusNext,
            (KeyCode::BackTab, _) => Action::FocusPrev,
            (KeyCode::Up, m) if m.contains(KeyModifiers::ALT) => Action::MethodPrev,
            (KeyCode::Down, m) if m.contains(KeyModifiers::ALT) => Action::MethodNext,
            (KeyCode::Left, _) => Action::FocusDir(Dir::Left),
            (KeyCode::Right, _) => Action::FocusDir(Dir::Right),
            (KeyCode::Up, _) => Action::FocusDir(Dir::Up),
            (KeyCode::Down, _) => Action::FocusDir(Dir::Down),
            (KeyCode::Esc, _) => Action::FocusDir(Dir::Down),
            _ => dispatch_url(ev),
        };
    }
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        (KeyCode::Char('q'), KeyModifiers::NONE) => Action::Quit,
        (KeyCode::Tab, _) => Action::FocusNext,
        (KeyCode::BackTab, _) => Action::FocusPrev,
        (KeyCode::Char(':'), KeyModifiers::NONE) => Action::EnterCommand,
        (KeyCode::Char('?'), _) => Action::ToggleHelp,
        // Lazygit-style numeric jumps
        (KeyCode::Char('1'), KeyModifiers::NONE) => Action::FocusJump(Focus::Collections),
        (KeyCode::Char('2'), KeyModifiers::NONE) => Action::FocusJump(Focus::Url),
        (KeyCode::Char('3'), KeyModifiers::NONE) => Action::FocusJump(Focus::Request),
        (KeyCode::Char('4'), KeyModifiers::NONE) if state.focus != Focus::Response => {
            Action::FocusJump(Focus::Response)
        }
        (KeyCode::Char('5'), KeyModifiers::NONE) => Action::FocusJump(Focus::Env),
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::SendRequest,
        (KeyCode::F(5), _) => Action::SendRequest,
        // Response pane keys (vim navigation + search)
        (KeyCode::Char('j'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::CursorBy(1)
        }
        (KeyCode::Char('k'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::CursorBy(-1)
        }
        (KeyCode::Down, _) if state.focus == Focus::Response => Action::CursorBy(1),
        (KeyCode::Up, _) if state.focus == Focus::Response => Action::CursorBy(-1),
        (KeyCode::Char('d'), KeyModifiers::CONTROL) if state.focus == Focus::Response => {
            Action::CursorBy(10)
        }
        (KeyCode::Char('u'), KeyModifiers::CONTROL) if state.focus == Focus::Response => {
            Action::CursorBy(-10)
        }
        (KeyCode::Char('f'), KeyModifiers::CONTROL) if state.focus == Focus::Response => {
            Action::CursorPageBy(1)
        }
        (KeyCode::Char('b'), KeyModifiers::CONTROL) if state.focus == Focus::Response => {
            Action::CursorPageBy(-1)
        }
        (KeyCode::PageDown, _) if state.focus == Focus::Response => Action::CursorPageBy(1),
        (KeyCode::PageUp, _) if state.focus == Focus::Response => Action::CursorPageBy(-1),
        (KeyCode::Char('g'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            if state.pending_g {
                Action::CursorTop
            } else {
                Action::PendingG
            }
        }
        (KeyCode::Char('G'), _) if state.focus == Focus::Response => Action::CursorBottom,
        (KeyCode::Char('{'), _) if state.focus == Focus::Response => Action::CursorParagraphPrev,
        (KeyCode::Char('}'), _) if state.focus == Focus::Response => Action::CursorParagraphNext,
        (KeyCode::Char('H'), _) if state.focus == Focus::Response => Action::CursorViewportTop,
        (KeyCode::Char('M'), _) if state.focus == Focus::Response => Action::CursorViewportMid,
        (KeyCode::Char('L'), _) if state.focus == Focus::Response => Action::CursorViewportBot,
        (KeyCode::Char('%'), _) if state.focus == Focus::Response => Action::JumpMatchingBrace,
        (KeyCode::Char(']'), _) if state.focus == Focus::Response => Action::JumpSiblingNext,
        (KeyCode::Char('['), _) if state.focus == Focus::Response => Action::JumpSiblingPrev,
        // Horizontal cursor (Response only — overrides spatial h/l)
        (KeyCode::Char('h'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::ColBy(-1)
        }
        (KeyCode::Char('l'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::ColBy(1)
        }
        (KeyCode::Left, _) if state.focus == Focus::Response => Action::ColBy(-1),
        (KeyCode::Right, _) if state.focus == Focus::Response => Action::ColBy(1),
        (KeyCode::Char('0'), _) if state.focus == Focus::Response => Action::ColLineStart,
        (KeyCode::Char('$'), _) if state.focus == Focus::Response => Action::ColLineEnd,
        (KeyCode::Char('w'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::WordNext
        }
        (KeyCode::Char('b'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::WordPrev
        }
        (KeyCode::Char('v'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::ToggleVisual
        }
        (KeyCode::Char('y'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::YankSelection
        }
        (KeyCode::Esc, _) if state.focus == Focus::Response && state.visual_anchor.is_some() => {
            Action::EscapeVisual
        }
        (KeyCode::Char('/'), _) if state.focus == Focus::Response => Action::EnterSearch,
        (KeyCode::Char('n'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::SearchNext
        }
        (KeyCode::Char('N'), _) if state.focus == Focus::Response => Action::SearchPrev,
        // Send (after Response keys so 's' doesn't fire while focused there — actually allow s globally below)
        (KeyCode::Char('s'), KeyModifiers::NONE) => Action::SendRequest,
        // Spatial pane move (only fires when not handled by per-pane block above).
        (KeyCode::Char('h'), KeyModifiers::NONE) => Action::FocusDir(Dir::Left),
        (KeyCode::Char('l'), KeyModifiers::NONE) => Action::FocusDir(Dir::Right),
        (KeyCode::Left, _) => Action::FocusDir(Dir::Left),
        (KeyCode::Right, _) => Action::FocusDir(Dir::Right),
        (KeyCode::Up, _) => Action::FocusDir(Dir::Up),
        (KeyCode::Down, _) => Action::FocusDir(Dir::Down),
        _ if state.focus == Focus::Env => dispatch_env(ev),
        _ if state.focus == Focus::Collections => dispatch_collections(ev),
        _ => Action::NoOp,
    }
}

fn dispatch_url(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Backspace, _) => Action::UrlBackspace,
        (KeyCode::Enter, _) => Action::UrlSubmit,
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            Action::UrlChar(c)
        }
        _ => Action::NoOp,
    }
}

fn dispatch_collections(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('j'), _) => Action::CollCursorDown,
        (KeyCode::Char('k'), _) => Action::CollCursorUp,
        (KeyCode::Char(' '), _) => Action::CollToggle,
        (KeyCode::Enter, _) => Action::CollOpen,
        (KeyCode::Char('r'), _) => Action::CollRenameStart,
        (KeyCode::Char('x'), _) => Action::CollToggleMark,
        (KeyCode::Char('M'), _) => Action::CollMoveStart,
        _ => Action::NoOp,
    }
}

fn dispatch_env(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('j'), _) => Action::EnvCursorDown,
        (KeyCode::Char('k'), _) => Action::EnvCursorUp,
        (KeyCode::Char('a'), KeyModifiers::NONE) => Action::EnvAdd { secret: false },
        (KeyCode::Char('A'), _) => Action::EnvAdd { secret: true },
        (KeyCode::Char('e'), KeyModifiers::NONE) => Action::EnvEdit,
        (KeyCode::Char('d'), KeyModifiers::NONE) => Action::EnvDelete,
        (KeyCode::Char('m'), KeyModifiers::NONE) => Action::EnvToggleSecret,
        (KeyCode::Char('r'), KeyModifiers::NONE) => Action::EnvToggleReveal,
        _ => Action::NoOp,
    }
}

fn dispatch_command(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Esc, _) => Action::CommandCancel,
        (KeyCode::Enter, _) => Action::CommandSubmit,
        (KeyCode::Backspace, _) => Action::CommandBackspace,
        (KeyCode::F(5), _) => Action::SendRequest,
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            Action::CommandChar(c)
        }
        _ => Action::NoOp,
    }
}

fn dispatch_insert(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Esc, _) => Action::InsertCancel,
        (KeyCode::Enter, _) => Action::InsertSubmit,
        (KeyCode::Tab, _) => Action::InsertNextField,
        (KeyCode::Backspace, _) => Action::InsertBackspace,
        (KeyCode::F(5), _) => Action::SendRequest,
        (KeyCode::Char(c), KeyModifiers::NONE) | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
            Action::InsertChar(c)
        }
        _ => Action::NoOp,
    }
}

/// Side-effect-free or self-contained mutations only. I/O persistence is the caller's job —
/// `event::run` saves after `EnvAdd` / `EnvDelete` / `EnvToggleSecret` / `CommandSubmit (env switch)`.
pub fn apply(state: &mut AppState, action: Action) -> EnvDirty {
    match action {
        Action::Quit => {
            state.should_quit = true;
            EnvDirty::No
        }
        Action::FocusNext => {
            state.focus = state.focus.next();
            EnvDirty::No
        }
        Action::FocusPrev => {
            state.focus = state.focus.prev();
            EnvDirty::No
        }
        Action::FocusDir(d) => {
            state.focus = state.focus.neighbour(d);
            EnvDirty::No
        }
        Action::FocusJump(f) => {
            state.focus = f;
            EnvDirty::No
        }
        Action::EnvCursorUp => {
            if state.env_cursor > 0 {
                state.env_cursor -= 1;
            }
            EnvDirty::No
        }
        Action::EnvCursorDown => {
            let max = state.active_env_ref().map(|e| e.vars.len()).unwrap_or(0);
            if max > 0 && state.env_cursor + 1 < max {
                state.env_cursor += 1;
            }
            EnvDirty::No
        }
        Action::EnvAdd { secret } => {
            state.mode = Mode::Insert;
            state.insert_buf = Some(InsertBuf::new(secret));
            EnvDirty::No
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
            EnvDirty::No
        }
        Action::EnvToggleReveal => {
            state.toggle_reveal();
            EnvDirty::No
        }
        Action::EnvDelete => {
            if state.delete_var() {
                EnvDirty::Yes
            } else {
                EnvDirty::No
            }
        }
        Action::EnvToggleSecret => {
            if state.toggle_secret() {
                EnvDirty::Yes
            } else {
                EnvDirty::No
            }
        }
        Action::EnterCommand => {
            state.mode = Mode::Command;
            state.command_buf.clear();
            EnvDirty::No
        }
        Action::CommandChar(c) => {
            state.command_buf.push(c);
            EnvDirty::No
        }
        Action::CommandBackspace => {
            state.command_buf.pop();
            EnvDirty::No
        }
        Action::CommandCancel => {
            state.mode = Mode::Normal;
            state.command_buf.clear();
            EnvDirty::No
        }
        Action::CommandSubmit => {
            let cmd = std::mem::take(&mut state.command_buf);
            state.mode = Mode::Normal;
            run_command(state, &cmd)
        }
        Action::InsertChar(c) => {
            if let Some(buf) = state.insert_buf.as_mut() {
                match buf.field {
                    InsertField::Key => buf.key.push(c),
                    InsertField::Value => buf.value.push(c),
                }
            }
            EnvDirty::No
        }
        Action::InsertBackspace => {
            if let Some(buf) = state.insert_buf.as_mut() {
                match buf.field {
                    InsertField::Key => buf.key.pop(),
                    InsertField::Value => buf.value.pop(),
                };
            }
            EnvDirty::No
        }
        Action::InsertNextField => {
            if let Some(buf) = state.insert_buf.as_mut() {
                buf.field = match buf.field {
                    InsertField::Key => InsertField::Value,
                    InsertField::Value => InsertField::Key,
                };
            }
            EnvDirty::No
        }
        Action::InsertCancel => {
            state.mode = Mode::Normal;
            state.insert_buf = None;
            EnvDirty::No
        }
        Action::InsertSubmit => {
            if let Some(buf) = state.insert_buf.take() {
                state.mode = Mode::Normal;
                if buf.key.is_empty() {
                    return EnvDirty::No;
                }
                match buf.edit_idx {
                    Some(i) => {
                        if state.replace_var(i, buf.key, buf.value, buf.secret) {
                            return EnvDirty::Yes;
                        }
                    }
                    None => {
                        state.add_var(buf.key, buf.value, buf.secret);
                        return EnvDirty::Yes;
                    }
                }
            } else {
                state.mode = Mode::Normal;
            }
            EnvDirty::No
        }
        Action::ToggleHelp => {
            state.help_open = !state.help_open;
            EnvDirty::No
        }
        Action::CloseHelp => {
            state.help_open = false;
            state.help_filter.clear();
            EnvDirty::No
        }
        Action::CloseMessages => {
            state.messages_open = false;
            EnvDirty::No
        }
        Action::UrlChar(c) => {
            state.url_buf.push(c);
            state.url_suggest_idx = 0;
            EnvDirty::No
        }
        Action::UrlBackspace => {
            state.url_buf.pop();
            state.url_suggest_idx = 0;
            EnvDirty::No
        }
        Action::UrlSuggestNext => {
            let n = state.url_var_suggestions().len();
            if n > 0 {
                state.url_suggest_idx = (state.url_suggest_idx + 1) % n;
            }
            EnvDirty::No
        }
        Action::UrlSuggestPrev => {
            let n = state.url_var_suggestions().len();
            if n > 0 {
                state.url_suggest_idx = (state.url_suggest_idx + n - 1) % n;
            }
            EnvDirty::No
        }
        Action::UrlSuggestAccept => {
            let suggestions = state.url_var_suggestions();
            if let Some(name) = suggestions.get(state.url_suggest_idx).cloned() {
                state.url_complete_var(&name);
            }
            EnvDirty::No
        }
        Action::UrlSuggestDismiss => {
            // Insert a `}}` to close the token so the suggestion list collapses.
            state.url_buf.push_str("}}");
            state.url_suggest_idx = 0;
            EnvDirty::No
        }
        Action::UrlSubmit => {
            state.notify(format!("URL: {}", state.url_buf));
            state.focus = Focus::Request;
            EnvDirty::No
        }
        Action::MethodNext => {
            state.method = next_method(&state.method);
            state.notify(format!("method: {}", state.method));
            EnvDirty::No
        }
        Action::MethodPrev => {
            state.method = prev_method(&state.method);
            state.notify(format!("method: {}", state.method));
            EnvDirty::No
        }
        Action::SendRequest => {
            // Sentinel — the event loop owns the tokio Handle and dispatches.
            EnvDirty::No
        }
        Action::CursorBy(delta) => {
            state.move_cursor_by(delta);
            state.pending_g = false;
            EnvDirty::No
        }
        Action::CursorPageBy(pages) => {
            let h = state.response_height.max(1) as i32;
            state.move_cursor_by(pages * h);
            state.pending_g = false;
            EnvDirty::No
        }
        Action::CursorTop => {
            state.move_cursor_to(0);
            state.pending_g = false;
            EnvDirty::No
        }
        Action::CursorBottom => {
            let last = state.response_total_lines.saturating_sub(1);
            state.move_cursor_to(last);
            state.pending_g = false;
            EnvDirty::No
        }
        Action::PendingG => {
            state.pending_g = true;
            EnvDirty::No
        }
        Action::CursorParagraphNext => {
            state.pending_g = false;
            // Layout owns line content; we approximate via search index of blank lines
            // recomputed at search-submit and stored in `search_match_lines`. For paragraph
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
            EnvDirty::No
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
            EnvDirty::No
        }
        Action::CursorViewportTop => {
            state.pending_g = false;
            let target = state.response_scroll as usize;
            state.move_cursor_to(target);
            EnvDirty::No
        }
        Action::CursorViewportMid => {
            state.pending_g = false;
            let h = state.response_height.max(1) as usize;
            let target = state.response_scroll as usize + h / 2;
            state.move_cursor_to(target);
            EnvDirty::No
        }
        Action::CursorViewportBot => {
            state.pending_g = false;
            let h = state.response_height.max(1) as usize;
            let target = state.response_scroll as usize + h.saturating_sub(1);
            state.move_cursor_to(target);
            EnvDirty::No
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
            EnvDirty::No
        }
        Action::JumpSiblingNext => {
            state.pending_g = false;
            if let Some(target) = sibling_target(state, 1) {
                state.move_cursor_to(target);
                let col = first_non_space_col(state);
                let len = current_line_len(state);
                state.move_col_to(col, len);
            }
            EnvDirty::No
        }
        Action::JumpSiblingPrev => {
            state.pending_g = false;
            if let Some(target) = sibling_target(state, -1) {
                state.move_cursor_to(target);
                let col = first_non_space_col(state);
                let len = current_line_len(state);
                state.move_col_to(col, len);
            }
            EnvDirty::No
        }
        Action::ColBy(d) => {
            let len = current_line_len(state);
            state.move_col_by(d, len);
            EnvDirty::No
        }
        Action::ColLineStart => {
            let len = current_line_len(state);
            state.move_col_to(0, len);
            EnvDirty::No
        }
        Action::ColLineEnd => {
            let len = current_line_len(state);
            #[allow(clippy::implicit_saturating_sub)]
            state.move_col_to(if len > 0 { len - 1 } else { 0 }, len);
            EnvDirty::No
        }
        Action::WordNext => {
            if let Some((line, col)) = next_word_pos(state) {
                let len = current_line_len(state);
                state.response_cursor = line;
                state.move_col_to(col, len);
            }
            EnvDirty::No
        }
        Action::WordPrev => {
            if let Some((line, col)) = prev_word_pos(state) {
                let len = current_line_len(state);
                state.response_cursor = line;
                state.move_col_to(col, len);
            }
            EnvDirty::No
        }
        Action::ToggleVisual => {
            if state.visual_anchor.is_some() {
                state.visual_anchor = None;
                state.notify("visual off".to_string());
            } else {
                state.visual_anchor = Some((state.response_cursor, state.response_col));
                state.notify("-- VISUAL --".to_string());
            }
            EnvDirty::No
        }
        Action::EscapeVisual => {
            state.visual_anchor = None;
            state.notify("visual off".to_string());
            EnvDirty::No
        }
        Action::YankSelection => {
            let text = selection_text(state).unwrap_or_else(|| current_line_text(state));
            match copy_to_clipboard(&text) {
                Ok(()) => state.toast = Some(format!("yanked {} chars", text.len())),
                Err(e) => state.toast = Some(format!("yank failed: {}", e)),
            }
            state.visual_anchor = None;
            EnvDirty::No
        }
        Action::EnterSearch => {
            state.mode = Mode::Search;
            state.search_buf.clear();
            EnvDirty::No
        }
        Action::SearchChar(c) => {
            state.search_buf.push(c);
            EnvDirty::No
        }
        Action::SearchBackspace => {
            state.search_buf.pop();
            EnvDirty::No
        }
        Action::SearchCancel => {
            state.mode = Mode::Normal;
            state.search_buf.clear();
            state.search_active = None;
            state.search_match_lines.clear();
            state.search_match_idx = 0;
            EnvDirty::No
        }
        Action::SearchSubmit => {
            state.mode = Mode::Normal;
            let needle = std::mem::take(&mut state.search_buf);
            state.search_match_idx = 0;
            state.search_match_lines.clear();
            if needle.is_empty() {
                state.search_active = None;
            } else {
                // Compute match line indices against the current rendered body (json or plain).
                if let Some(body) = state.last_response_pretty.clone() {
                    let needle_lc = needle.to_lowercase();
                    for (i, line) in body.lines().enumerate() {
                        if line.to_lowercase().contains(&needle_lc) {
                            state.search_match_lines.push(i);
                        }
                    }
                    if let Some(&first) = state.search_match_lines.first() {
                        state.move_cursor_to(first);
                    }
                }
                state.search_active = Some(needle);
            }
            EnvDirty::No
        }
        Action::SearchNext => {
            if !state.search_match_lines.is_empty() {
                state.search_match_idx =
                    (state.search_match_idx + 1) % state.search_match_lines.len();
                let target = state.search_match_lines[state.search_match_idx];
                state.move_cursor_to(target);
            }
            EnvDirty::No
        }
        Action::SearchPrev => {
            if !state.search_match_lines.is_empty() {
                let n = state.search_match_lines.len();
                state.search_match_idx = (state.search_match_idx + n - 1) % n;
                let target = state.search_match_lines[state.search_match_idx];
                state.move_cursor_to(target);
            }
            EnvDirty::No
        }
        Action::CollCursorUp => {
            state.coll_cursor = state.coll_cursor.saturating_sub(1);
            EnvDirty::No
        }
        Action::CollCursorDown => {
            let max = state.coll_rows().len();
            if max > 0 && state.coll_cursor + 1 < max {
                state.coll_cursor += 1;
            }
            EnvDirty::No
        }
        Action::CollToggle => {
            state.coll_toggle_expand();
            EnvDirty::No
        }
        Action::CollOpen => {
            if state.coll_toggle_expand() {
                return EnvDirty::No;
            }
            if let Some(name) = state.coll_open_selected() {
                state.notify(format!("loaded {}", name));
                state.focus = Focus::Url;
            }
            EnvDirty::No
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
            EnvDirty::No
        }
        Action::RenameChar(c) => {
            state.rename_buf.push(c);
            EnvDirty::No
        }
        Action::RenameBackspace => {
            state.rename_buf.pop();
            EnvDirty::No
        }
        Action::RenameCancel => {
            state.mode = Mode::Normal;
            state.rename_target = None;
            state.rename_buf.clear();
            EnvDirty::No
        }
        Action::RenameSubmit => {
            let new = std::mem::take(&mut state.rename_buf);
            let target = state.rename_target.take();
            state.mode = Mode::Normal;
            run_rename(state, target, new.trim());
            EnvDirty::No
        }
        Action::HelpFilterChar(c) => {
            state.help_filter.push(c);
            EnvDirty::No
        }
        Action::HelpFilterBackspace => {
            state.help_filter.pop();
            EnvDirty::No
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
            EnvDirty::No
        }
        Action::CollMoveStart => {
            use crate::app::CollRow;
            // If nothing is marked, mark the cursor row (if a request) so move has a target.
            if state.marked_requests.is_empty() {
                if let Some(CollRow::Req { coll, item }) =
                    state.coll_rows().get(state.coll_cursor).copied()
                {
                    state.marked_requests.insert((coll, item));
                }
            }
            if state.marked_requests.is_empty() {
                state.notify("nothing to move (use 'x' to mark requests)".to_string());
                return EnvDirty::No;
            }
            state.move_buf.clear();
            state.mode = Mode::Move;
            EnvDirty::No
        }
        Action::MoveChar(c) => {
            state.move_buf.push(c);
            EnvDirty::No
        }
        Action::MoveBackspace => {
            state.move_buf.pop();
            EnvDirty::No
        }
        Action::MoveCancel => {
            state.mode = Mode::Normal;
            state.move_buf.clear();
            EnvDirty::No
        }
        Action::MoveSubmit => {
            let target = std::mem::take(&mut state.move_buf);
            state.mode = Mode::Normal;
            run_move(state, target.trim());
            EnvDirty::No
        }
        Action::EnterSaveAs => {
            if state.url_buf.is_empty() {
                state.notify("URL is empty — nothing to save".to_string());
                return EnvDirty::No;
            }
            state.mode = Mode::SaveAs;
            // Always re-prefill: stale buffer from a previous failed save shouldn't leak in.
            state.save_buf = match state.collections.first() {
                Some(c) => format!("{}/", c.name),
                None => String::new(),
            };
            EnvDirty::No
        }
        Action::SaveAsChar(c) => {
            state.save_buf.push(c);
            EnvDirty::No
        }
        Action::SaveAsBackspace => {
            state.save_buf.pop();
            EnvDirty::No
        }
        Action::SaveAsCancel => {
            state.mode = Mode::Normal;
            EnvDirty::No
        }
        Action::SaveAsSubmit => {
            let path = std::mem::take(&mut state.save_buf);
            state.mode = Mode::Normal;
            run_save(state, path.trim());
            EnvDirty::No
        }
        Action::NoOp => EnvDirty::No,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvDirty {
    Yes,
    No,
}

fn run_command(state: &mut AppState, cmd: &str) -> EnvDirty {
    let cmd = cmd.trim();
    if cmd == "messages" {
        state.messages_open = true;
        return EnvDirty::No;
    }
    if let Some(name) = cmd.strip_prefix("env ").map(str::trim) {
        if state.switch_env(name) {
            state.notify(format!("env: {}", name));
        } else {
            state.notify(format!("env not found: {}", name));
        }
        return EnvDirty::No;
    }
    if let Some(name) = cmd.strip_prefix("newenv ").map(str::trim) {
        if name.is_empty() {
            state.notify("usage: :newenv <name>".to_string());
        } else if state.create_env(name) {
            state.notify(format!("created env: {}", name));
            return EnvDirty::Yes;
        } else {
            state.notify(format!("env already exists: {}", name));
        }
        return EnvDirty::No;
    }
    if let Some(rest) = cmd.strip_prefix("save ").map(str::trim) {
        return run_save(state, rest);
    }
    if let Some(name) = cmd.strip_prefix("method ").map(str::trim) {
        if let Ok(m) = name.to_ascii_uppercase().parse::<http::Method>() {
            state.method = m;
            state.notify(format!("method: {}", state.method));
        } else {
            state.notify(format!("invalid method: {}", name));
        }
        return EnvDirty::No;
    }
    if cmd == "q" || cmd == "quit" {
        state.should_quit = true;
        return EnvDirty::No;
    }
    state.notify(format!("unknown: {}", cmd));
    EnvDirty::No
}

/// Vim-style `%`: from the cursor position, find the next brace on the current line, then
/// jump to its matching pair (line + column). Skips characters inside string literals.
pub fn matching_brace_position(state: &AppState) -> Option<(usize, usize)> {
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let cur_line = state.response_cursor.min(lines.len().saturating_sub(1));
    let cur_col = state.response_col;

    let line_chars: Vec<char> = lines.get(cur_line)?.chars().collect();
    let (start_col, start_brace) = first_brace_at_or_after(&line_chars, cur_col)?;
    let opener = matches!(start_brace, '{' | '[' | '(');

    if opener {
        forward_match(&lines, cur_line, start_col + 1, start_brace)
    } else {
        backward_match(&lines, cur_line, start_col, start_brace)
    }
}

fn first_brace_at_or_after(chars: &[char], from: usize) -> Option<(usize, char)> {
    let mut in_str = false;
    let mut esc = false;
    for (i, &c) in chars.iter().enumerate() {
        if i < from {
            // Still need to track string state from line start.
            if esc {
                esc = false;
            } else if c == '\\' && in_str {
                esc = true;
            } else if c == '"' {
                in_str = !in_str;
            }
            continue;
        }
        if esc {
            esc = false;
            continue;
        }
        if c == '\\' && in_str {
            esc = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if !in_str && matches!(c, '{' | '}' | '[' | ']' | '(' | ')') {
            return Some((i, c));
        }
    }
    None
}

fn forward_match(
    lines: &[&str],
    start_line: usize,
    start_col: usize,
    opener: char,
) -> Option<(usize, usize)> {
    let close = match opener {
        '{' => '}',
        '[' => ']',
        '(' => ')',
        _ => return None,
    };
    let mut depth: i32 = 1;
    let mut in_str = false;
    let mut esc = false;
    for (i, l) in lines.iter().enumerate().skip(start_line) {
        let chars: Vec<char> = l.chars().collect();
        let from = if i == start_line { start_col } else { 0 };
        for (off, &c) in chars[from..].iter().enumerate() {
            let abs = from + off;
            if esc {
                esc = false;
                continue;
            }
            if c == '\\' && in_str {
                esc = true;
                continue;
            }
            if c == '"' {
                in_str = !in_str;
                continue;
            }
            if in_str {
                continue;
            }
            if c == opener {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some((i, abs));
                }
            }
        }
    }
    None
}

fn backward_match(
    lines: &[&str],
    start_line: usize,
    start_col: usize,
    closer: char,
) -> Option<(usize, usize)> {
    let open = match closer {
        '}' => '{',
        ']' => '[',
        ')' => '(',
        _ => return None,
    };
    let mut depth: i32 = 1;
    let mut entries: Vec<(usize, usize, char)> = Vec::new();
    let mut in_str = false;
    let mut esc = false;
    for (li, l) in lines.iter().enumerate().take(start_line + 1) {
        let chars: Vec<char> = l.chars().collect();
        let upto = if li == start_line {
            start_col
        } else {
            chars.len()
        };
        for (col, &c) in chars[..upto].iter().enumerate() {
            if esc {
                esc = false;
                continue;
            }
            if c == '\\' && in_str {
                esc = true;
                continue;
            }
            if c == '"' {
                in_str = !in_str;
                continue;
            }
            if in_str {
                continue;
            }
            entries.push((li, col, c));
        }
    }
    for (li, col, c) in entries.into_iter().rev() {
        if c == closer {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                return Some((li, col));
            }
        }
    }
    None
}

/// Walk forward (`dir=1`) or backward (`dir=-1`) to the next non-empty line at the same
/// indent depth as the cursor line. Pretty-printed JSON sibling = same-indent line.
fn sibling_target(state: &AppState, dir: i32) -> Option<usize> {
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let cur = state.response_cursor.min(lines.len().saturating_sub(1));
    let depth = indent_of(lines.get(cur)?);
    let n = lines.len();
    let mut i = cur as i32 + dir;
    while i >= 0 && (i as usize) < n {
        let line = lines[i as usize];
        if !line.trim().is_empty() && indent_of(line) == depth {
            return Some(i as usize);
        }
        // If we hit a shallower line, the parent block ended — stop.
        if !line.trim().is_empty() && indent_of(line) < depth {
            return None;
        }
        i += dir;
    }
    None
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn current_body(state: &AppState) -> Option<String> {
    state.last_response_pretty.clone()
}

fn first_non_space_col(state: &AppState) -> usize {
    current_body(state)
        .and_then(|b| {
            b.lines()
                .nth(state.response_cursor)
                .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        })
        .unwrap_or(0)
}

fn current_line_len(state: &AppState) -> usize {
    current_body(state)
        .and_then(|b| {
            b.lines()
                .nth(state.response_cursor)
                .map(|l| l.chars().count())
        })
        .unwrap_or(0)
}

fn current_line_text(state: &AppState) -> String {
    current_body(state)
        .and_then(|b| b.lines().nth(state.response_cursor).map(String::from))
        .unwrap_or_default()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Find the next word boundary (vim-style `w`).
fn next_word_pos(state: &AppState) -> Option<(usize, usize)> {
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let cur_line = state.response_cursor;
    let cur_col = state.response_col;
    if cur_line >= lines.len() {
        return None;
    }
    let chars: Vec<char> = lines[cur_line].chars().collect();
    let mut i = cur_col;
    let was_word = chars.get(i).map(|c| is_word_char(*c)).unwrap_or(false);
    // Skip current run of same-class chars
    while i < chars.len()
        && chars.get(i).map(|c| is_word_char(*c)).unwrap_or(false) == was_word
        && !chars.get(i).map(|c| c.is_whitespace()).unwrap_or(false)
    {
        i += 1;
    }
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i < chars.len() {
        Some((cur_line, i))
    } else if cur_line + 1 < lines.len() {
        Some((cur_line + 1, 0))
    } else {
        None
    }
}

fn prev_word_pos(state: &AppState) -> Option<(usize, usize)> {
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let mut cur_line = state.response_cursor;
    if cur_line >= lines.len() {
        return None;
    }
    let mut chars: Vec<char> = lines[cur_line].chars().collect();
    let mut i = state.response_col;
    if i == 0 {
        if cur_line == 0 {
            return None;
        }
        cur_line -= 1;
        chars = lines[cur_line].chars().collect();
        i = chars.len();
    }
    i = i.saturating_sub(1);
    while i > 0 && chars.get(i).map(|c| c.is_whitespace()).unwrap_or(false) {
        i -= 1;
    }
    let was_word = chars.get(i).map(|c| is_word_char(*c)).unwrap_or(false);
    while i > 0
        && chars
            .get(i - 1)
            .map(|c| is_word_char(*c) == was_word && !c.is_whitespace())
            .unwrap_or(false)
    {
        i -= 1;
    }
    Some((cur_line, i))
}

fn selection_text(state: &AppState) -> Option<String> {
    let anchor = state.visual_anchor?;
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let (a, b) = (
        (anchor.0, anchor.1),
        (state.response_cursor, state.response_col),
    );
    let (start, end) = if (a.0, a.1) <= (b.0, b.1) {
        (a, b)
    } else {
        (b, a)
    };
    let mut out = String::new();
    for line in start.0..=end.0 {
        let chars: Vec<char> = lines.get(line)?.chars().collect();
        let from = if line == start.0 { start.1 } else { 0 };
        let to = if line == end.0 {
            (end.1 + 1).min(chars.len())
        } else {
            chars.len()
        };
        if from < chars.len() {
            out.extend(&chars[from..to.min(chars.len())]);
        }
        if line < end.0 {
            out.push('\n');
        }
    }
    Some(out)
}

/// Pipe text to a system clipboard helper. These tools daemonize and retain ownership of
/// the selection after our process exits, which `arboard` does not on X11/Wayland.
fn copy_to_clipboard(s: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let on_x11 = std::env::var_os("DISPLAY").is_some();
    let mut candidates: Vec<(&str, Vec<&str>)> = Vec::new();
    if on_wayland {
        candidates.push(("wl-copy", vec![]));
    }
    if on_x11 {
        candidates.push(("xclip", vec!["-selection", "clipboard"]));
        candidates.push(("xsel", vec!["--clipboard", "--input"]));
    }
    candidates.push(("pbcopy", vec![])); // macOS
    candidates.push(("clip.exe", vec![])); // Windows / WSL

    let mut last_err = String::from(
        "no clipboard tool found in PATH (tried: wl-copy, xclip, xsel, pbcopy, clip.exe)",
    );
    for (cmd, args) in candidates {
        let spawn = Command::new(cmd)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawn {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    if let Err(e) = stdin.write_all(s.as_bytes()) {
                        last_err = format!("{}: write failed: {}", cmd, e);
                        continue;
                    }
                }
                // Drop our handle so the helper can finish reading and detach.
                // Don't wait() — wl-copy etc. fork a daemon and the parent exits cleanly.
                let _ = child.wait();
                return Ok(());
            }
            Err(e) => {
                last_err = format!("{}: {}", cmd, e);
            }
        }
    }
    Err(last_err)
}

fn run_move(state: &mut AppState, target: &str) {
    use lazyfetch_core::catalog::Item;
    use lazyfetch_storage::collection::FsCollectionRepo;
    if target.is_empty() {
        state.notify("usage: type target collection name".to_string());
        return;
    }
    let repo = FsCollectionRepo::new(state.config_dir.join("collections"));
    let marks: Vec<(usize, usize)> = state.marked_requests.iter().copied().collect();
    let mut moved = 0usize;
    let mut errors = 0usize;
    for (coll_idx, item_idx) in &marks {
        let Some(coll) = state.collections.get(*coll_idx) else {
            continue;
        };
        if coll.name == target {
            continue; // skip same-collection moves
        }
        let from_coll = coll.name.clone();
        let req_name = match coll.root.items.get(*item_idx) {
            Some(Item::Request(r)) => r.name.clone(),
            _ => continue,
        };
        match repo.move_request(&from_coll, &req_name, target) {
            Ok(()) => moved += 1,
            Err(_) => errors += 1,
        }
    }

    // Reload affected collections + ensure target is loaded.
    let mut affected_names: std::collections::HashSet<String> = marks
        .iter()
        .filter_map(|(c, _)| state.collections.get(*c).map(|x| x.name.clone()))
        .collect();
    affected_names.insert(target.to_string());
    for name in affected_names {
        if let Ok(c) = repo.load_by_name(&name) {
            if let Some(idx) = state.collections.iter().position(|x| x.name == name) {
                state.collections[idx] = c;
            } else {
                state.collections.push(c);
            }
        }
    }

    state.marked_requests.clear();
    state.toast = Some(if errors == 0 {
        format!("moved {} → {}", moved, target)
    } else {
        format!("moved {} ({} failed) → {}", moved, errors, target)
    });
}

fn run_rename(state: &mut AppState, target: Option<crate::app::RenameTarget>, new: &str) {
    use crate::app::RenameTarget;
    use lazyfetch_storage::collection::FsCollectionRepo;
    let Some(target) = target else { return };
    if new.is_empty() {
        state.notify("name is empty".to_string());
        return;
    }
    let repo = FsCollectionRepo::new(state.config_dir.join("collections"));
    match target {
        RenameTarget::Collection { idx, old } => {
            if old == new {
                return;
            }
            match repo.rename_collection(&old, new) {
                Ok(()) => {
                    if let Some(c) = state.collections.get_mut(idx) {
                        c.name = new.to_string();
                    }
                    state.notify(format!("renamed {} → {}", old, new));
                }
                Err(e) => state.toast = Some(format!("rename failed: {}", e)),
            }
        }
        RenameTarget::Request { coll, item, old } => {
            if old == new {
                return;
            }
            let coll_name = state.collections[coll].name.clone();
            match repo.rename_request(&coll_name, &old, new) {
                Ok(()) => {
                    if let lazyfetch_core::catalog::Item::Request(r) =
                        &mut state.collections[coll].root.items[item]
                    {
                        r.name = new.to_string();
                    }
                    state.notify(format!("renamed {} → {}", old, new));
                }
                Err(e) => state.toast = Some(format!("rename failed: {}", e)),
            }
        }
    }
}

fn run_save(state: &mut AppState, arg: &str) -> EnvDirty {
    use lazyfetch_core::catalog::{Body, Request};
    use lazyfetch_core::primitives::{Template, UrlTemplate};
    use lazyfetch_storage::collection::FsCollectionRepo;

    let (coll, name) = match arg.split_once('/') {
        Some((c, n)) if !c.is_empty() && !n.is_empty() => (c, n),
        _ => {
            state.notify("usage: :save <collection>/<name>".to_string());
            return EnvDirty::No;
        }
    };
    if state.url_buf.is_empty() {
        state.notify("URL is empty — type one in the URL pane first".to_string());
        return EnvDirty::No;
    }
    let req = Request {
        id: ulid::Ulid::new(),
        name: name.to_string(),
        method: state.method.clone(),
        url: UrlTemplate(Template(state.url_buf.clone())),
        query: vec![],
        headers: vec![],
        body: Body::None,
        auth: None,
        notes: None,
        follow_redirects: true,
        max_redirects: 10,
        timeout_ms: None,
    };
    let repo = FsCollectionRepo::new(state.config_dir.join("collections"));
    match repo.save_request(coll, &req) {
        Ok(()) => {
            state.notify(format!("saved {}/{}", coll, name));
            // Reload the collection so it shows up in the Collections pane.
            if let Ok(c) = repo.load_by_name(coll) {
                if let Some(idx) = state.collections.iter().position(|x| x.name == c.name) {
                    state.collections[idx] = c;
                } else {
                    state.collections.push(c);
                }
            }
        }
        Err(e) => state.toast = Some(format!("save failed: {}", e)),
    }
    EnvDirty::No
}

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

fn next_method(current: &http::Method) -> http::Method {
    let i = METHODS
        .iter()
        .position(|m| *m == current.as_str())
        .map(|i| (i + 1) % METHODS.len())
        .unwrap_or(0);
    METHODS[i]
        .parse()
        .expect("METHODS table contains valid HTTP methods")
}

fn prev_method(current: &http::Method) -> http::Method {
    let i = METHODS
        .iter()
        .position(|m| *m == current.as_str())
        .map(|i| (i + METHODS.len() - 1) % METHODS.len())
        .unwrap_or(0);
    METHODS[i]
        .parse()
        .expect("METHODS table contains valid HTTP methods")
}
