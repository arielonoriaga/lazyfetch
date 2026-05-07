use crate::app::{AppState, Dir, Focus, InsertBuf, InsertField, Mode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    FocusNext,
    FocusPrev,
    FocusDir(Dir),
    EnvCursorUp,
    EnvCursorDown,
    EnvAdd { secret: bool },
    EnvDelete,
    EnvToggleSecret,
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
    UrlChar(char),
    UrlBackspace,
    UrlSubmit,
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
    EnterSearch,
    SearchChar(char),
    SearchBackspace,
    SearchSubmit,
    SearchCancel,
    SearchNext,
    SearchPrev,
    NoOp,
}

pub fn dispatch(state: &AppState, ev: KeyEvent) -> Action {
    if state.help_open {
        return match (ev.code, ev.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
            _ => Action::CloseHelp,
        };
    }
    match state.mode {
        Mode::Normal => dispatch_normal(state, ev),
        Mode::Command => dispatch_command(ev),
        Mode::Insert => dispatch_insert(ev),
        Mode::Search => dispatch_search(ev),
    }
}

fn dispatch_search(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Esc, _) => Action::SearchCancel,
        (KeyCode::Enter, _) => Action::SearchSubmit,
        (KeyCode::Backspace, _) => Action::SearchBackspace,
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
        return match (ev.code, ev.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::SendRequest,
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
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::SendRequest,
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
        (KeyCode::Char('/'), _) if state.focus == Focus::Response => Action::EnterSearch,
        (KeyCode::Char('n'), KeyModifiers::NONE) if state.focus == Focus::Response => {
            Action::SearchNext
        }
        (KeyCode::Char('N'), _) if state.focus == Focus::Response => Action::SearchPrev,
        // Send (after Response keys so 's' doesn't fire while focused there — actually allow s globally below)
        (KeyCode::Char('s'), KeyModifiers::NONE) => Action::SendRequest,
        (KeyCode::Left, _) | (KeyCode::Char('h'), KeyModifiers::NONE) => {
            Action::FocusDir(Dir::Left)
        }
        (KeyCode::Right, _) | (KeyCode::Char('l'), KeyModifiers::NONE) => {
            Action::FocusDir(Dir::Right)
        }
        (KeyCode::Up, _) => Action::FocusDir(Dir::Up),
        (KeyCode::Down, _) => Action::FocusDir(Dir::Down),
        _ if state.focus == Focus::Env => dispatch_env(ev),
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

fn dispatch_env(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('j'), _) => Action::EnvCursorDown,
        (KeyCode::Char('k'), _) => Action::EnvCursorUp,
        (KeyCode::Char('a'), KeyModifiers::NONE) => Action::EnvAdd { secret: false },
        (KeyCode::Char('A'), _) => Action::EnvAdd { secret: true },
        (KeyCode::Char('d'), KeyModifiers::NONE) => Action::EnvDelete,
        (KeyCode::Char('m'), KeyModifiers::NONE) => Action::EnvToggleSecret,
        _ => Action::NoOp,
    }
}

fn dispatch_command(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Esc, _) => Action::CommandCancel,
        (KeyCode::Enter, _) => Action::CommandSubmit,
        (KeyCode::Backspace, _) => Action::CommandBackspace,
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
                if !buf.key.is_empty() {
                    state.add_var(buf.key, buf.value, buf.secret);
                    return EnvDirty::Yes;
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
            EnvDirty::No
        }
        Action::UrlChar(c) => {
            state.url_buf.push(c);
            EnvDirty::No
        }
        Action::UrlBackspace => {
            state.url_buf.pop();
            EnvDirty::No
        }
        Action::UrlSubmit => {
            state.toast = Some(format!("URL: {}", state.url_buf));
            state.focus = Focus::Request;
            EnvDirty::No
        }
        Action::MethodNext => {
            state.method = next_method(&state.method);
            state.toast = Some(format!("method: {}", state.method));
            EnvDirty::No
        }
        Action::MethodPrev => {
            state.method = prev_method(&state.method);
            state.toast = Some(format!("method: {}", state.method));
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
            // motion we re-scan body lines lazily via the cached pretty body in last_response.
            if let Some(executed) = &state.last_response {
                let ct = executed
                    .response
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                let body = pretty_body_for_search(ct, &executed.response.body_bytes);
                let body = executed.secrets.redact(&body);
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
            if let Some(executed) = &state.last_response {
                let ct = executed
                    .response
                    .headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                let body = pretty_body_for_search(ct, &executed.response.body_bytes);
                let body = executed.secrets.redact(&body);
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
                if let Some(executed) = &state.last_response {
                    let ct = executed
                        .response
                        .headers
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("");
                    let body = pretty_body_for_search(ct, &executed.response.body_bytes);
                    let body = executed.secrets.redact(&body);
                    let needle_lc = needle.to_lowercase();
                    for (i, line) in body.lines().enumerate() {
                        if line.to_lowercase().contains(&needle_lc) {
                            // Body lines start at row 2 of the rendered Paragraph (status + blank).
                            // The scroll counter is over the *body* so use raw `i`.
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
    if let Some(name) = cmd.strip_prefix("env ").map(str::trim) {
        if state.switch_env(name) {
            state.toast = Some(format!("env: {}", name));
        } else {
            state.toast = Some(format!("env not found: {}", name));
        }
        return EnvDirty::No;
    }
    if let Some(name) = cmd.strip_prefix("method ").map(str::trim) {
        if let Ok(m) = name.to_ascii_uppercase().parse::<http::Method>() {
            state.method = m;
            state.toast = Some(format!("method: {}", state.method));
        } else {
            state.toast = Some(format!("invalid method: {}", name));
        }
        return EnvDirty::No;
    }
    if cmd == "q" || cmd == "quit" {
        state.should_quit = true;
        return EnvDirty::No;
    }
    state.toast = Some(format!("unknown: {}", cmd));
    EnvDirty::No
}

/// Pretty-print body identically to layout::pretty_body so search line indices line up.
fn pretty_body_for_search(content_type: &str, body: &[u8]) -> String {
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("json") || looks_like_json(body) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
            if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                return pretty;
            }
        }
    }
    String::from_utf8_lossy(body).into_owned()
}

fn looks_like_json(body: &[u8]) -> bool {
    let s = std::str::from_utf8(body).unwrap_or("").trim_start();
    s.starts_with('{') || s.starts_with('[')
}

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

fn next_method(current: &http::Method) -> http::Method {
    let i = METHODS
        .iter()
        .position(|m| *m == current.as_str())
        .map(|i| (i + 1) % METHODS.len())
        .unwrap_or(0);
    METHODS[i].parse().unwrap()
}

fn prev_method(current: &http::Method) -> http::Method {
    let i = METHODS
        .iter()
        .position(|m| *m == current.as_str())
        .map(|i| (i + METHODS.len() - 1) % METHODS.len())
        .unwrap_or(0);
    METHODS[i].parse().unwrap()
}
