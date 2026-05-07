use crate::app::{AppState, Mode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    FocusNext,
    FocusPrev,
    NoOp,
}

pub fn dispatch(state: &AppState, ev: KeyEvent) -> Action {
    if state.mode != Mode::Normal {
        return Action::NoOp;
    }
    match (ev.code, ev.modifiers) {
        (KeyCode::Char('q'), KeyModifiers::NONE) => Action::Quit,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        (KeyCode::Tab, _) => Action::FocusNext,
        (KeyCode::BackTab, _) => Action::FocusPrev,
        _ => Action::NoOp,
    }
}

pub fn apply(state: &mut AppState, action: Action) {
    match action {
        Action::Quit => state.should_quit = true,
        Action::FocusNext => state.focus = state.focus.next(),
        Action::FocusPrev => state.focus = state.focus.prev(),
        Action::NoOp => {}
    }
}
