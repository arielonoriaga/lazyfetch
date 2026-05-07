use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lazyfetch_tui::app::{AppState, Focus};
use lazyfetch_tui::keymap::{apply, dispatch, Action};

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

#[test]
fn q_quits() {
    let mut s = AppState::new();
    let a = dispatch(&s, key('q'));
    assert_eq!(a, Action::Quit);
    apply(&mut s, a);
    assert!(s.should_quit);
}

#[test]
fn tab_cycles_focus() {
    let mut s = AppState::new();
    let a = dispatch(&s, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    apply(&mut s, a);
    assert_eq!(s.focus, Focus::Request);
    let a = dispatch(&s, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    apply(&mut s, a);
    assert_eq!(s.focus, Focus::Response);
}
