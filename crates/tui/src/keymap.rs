use crate::app::{AppState, Focus, InsertBuf, InsertField, Mode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    FocusNext,
    FocusPrev,
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
    }
}

fn dispatch_normal(state: &AppState, ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        (KeyCode::Char('q'), KeyModifiers::NONE) => Action::Quit,
        (KeyCode::Tab, _) => Action::FocusNext,
        (KeyCode::BackTab, _) => Action::FocusPrev,
        (KeyCode::Char(':'), KeyModifiers::NONE) => Action::EnterCommand,
        (KeyCode::Char('?'), _) => Action::ToggleHelp,
        _ if state.focus == Focus::Env => dispatch_env(ev),
        _ => Action::NoOp,
    }
}

fn dispatch_env(ev: KeyEvent) -> Action {
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Action::EnvCursorDown,
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Action::EnvCursorUp,
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
    if cmd == "q" || cmd == "quit" {
        state.should_quit = true;
        return EnvDirty::No;
    }
    state.toast = Some(format!("unknown: {}", cmd));
    EnvDirty::No
}
