use crate::app::{AppState, Dir, Focus, InsertBuf, InsertField, Mode};
use crate::commands::{run_command, run_move, run_rename, run_save};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

mod request;
mod response;
mod url;

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
    CurlExport,
    RepeatLast,
    ReqTabSwitch(crate::app::ReqTab),
    ReqTabCycle,
    BodyKindCycle,
    KvCursorUp,
    KvCursorDown,
    KvAdd,
    KvEditValue,
    KvToggleEnabled,
    KvDelete,
    KvToggleSecret,
    KvToggleKind,
    KvInsertChar(char),
    KvInsertBackspace,
    KvInsertTab,
    KvCommit,
    KvCancel,
    BodyEnterEdit,
    BodyExitEdit,
    BodyInputChar(char),
    BodyInputBackspace,
    BodyInputNewline,
    BodyShellOut,
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
        Mode::ImportCurl => Action::NoOp,
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
    if state.focus == Focus::Request {
        if let Some(a) = request::dispatch(state, ev) {
            return a;
        }
    }
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
            _ => url::dispatch(ev),
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
        (KeyCode::Char('Y'), _) if state.focus == Focus::Response => Action::CurlExport,
        (KeyCode::Char('R'), _) => Action::RepeatLast,
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
    if let Some(dirty) = request::apply_action(state, &action) {
        return dirty;
    }
    if let Some(dirty) = url::apply_action(state, &action) {
        return dirty;
    }
    if let Some(dirty) = response::apply_action(state, &action) {
        return dirty;
    }
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
        // Url + Method actions handled by `url::apply_action` early-exit above.
        Action::UrlChar(_)
        | Action::UrlBackspace
        | Action::UrlSuggestNext
        | Action::UrlSuggestPrev
        | Action::UrlSuggestAccept
        | Action::UrlSuggestDismiss
        | Action::UrlSubmit
        | Action::MethodNext
        | Action::MethodPrev => EnvDirty::No,
        Action::SendRequest => {
            // Sentinel — the event loop owns the tokio Handle and dispatches.
            EnvDirty::No
        }
        // Response-pane actions handled by `response::apply_action` early-exit above.
        Action::CursorBy(_)
        | Action::CursorPageBy(_)
        | Action::CursorTop
        | Action::CursorBottom
        | Action::PendingG
        | Action::CursorParagraphNext
        | Action::CursorParagraphPrev
        | Action::CursorViewportTop
        | Action::CursorViewportMid
        | Action::CursorViewportBot
        | Action::JumpMatchingBrace
        | Action::JumpSiblingNext
        | Action::JumpSiblingPrev
        | Action::ColBy(_)
        | Action::ColLineStart
        | Action::ColLineEnd
        | Action::WordNext
        | Action::WordPrev
        | Action::ToggleVisual
        | Action::EscapeVisual
        | Action::YankSelection
        | Action::EnterSearch
        | Action::SearchChar(_)
        | Action::SearchBackspace
        | Action::SearchCancel
        | Action::SearchSubmit
        | Action::SearchNext
        | Action::SearchPrev => EnvDirty::No,
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
        Action::CurlExport => EnvDirty::No, // handled by response::apply_action above
        Action::RepeatLast => EnvDirty::No,
        // Request pane actions handled by `request::apply_action` early-exit
        // above (kept here for exhaustiveness on the unified Action enum).
        Action::ReqTabSwitch(_)
        | Action::ReqTabCycle
        | Action::BodyKindCycle
        | Action::KvCursorUp
        | Action::KvCursorDown
        | Action::KvAdd
        | Action::KvEditValue
        | Action::KvToggleEnabled
        | Action::KvDelete
        | Action::KvToggleSecret
        | Action::KvToggleKind
        | Action::KvInsertChar(_)
        | Action::KvInsertBackspace
        | Action::KvInsertTab
        | Action::KvCommit
        | Action::KvCancel
        | Action::BodyEnterEdit
        | Action::BodyExitEdit
        | Action::BodyInputChar(_)
        | Action::BodyInputNewline
        | Action::BodyInputBackspace
        | Action::BodyShellOut => EnvDirty::No,
        Action::NoOp => EnvDirty::No,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvDirty {
    Yes,
    No,
}

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

pub(super) fn next_method(current: &http::Method) -> http::Method {
    let i = METHODS
        .iter()
        .position(|m| *m == current.as_str())
        .map(|i| (i + 1) % METHODS.len())
        .unwrap_or(0);
    METHODS[i]
        .parse()
        .expect("METHODS table contains valid HTTP methods")
}

pub(super) fn prev_method(current: &http::Method) -> http::Method {
    let i = METHODS
        .iter()
        .position(|m| *m == current.as_str())
        .map(|i| (i + METHODS.len() - 1) % METHODS.len())
        .unwrap_or(0);
    METHODS[i]
        .parse()
        .expect("METHODS table contains valid HTTP methods")
}
