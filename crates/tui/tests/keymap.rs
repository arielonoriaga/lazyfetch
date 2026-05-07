use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lazyfetch_tui::app::{AppState, Focus};
use lazyfetch_tui::keymap::{apply, dispatch, Action, EnvDirty};
use std::path::PathBuf;

fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

fn ev(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn step(s: &mut AppState, k: KeyEvent) -> EnvDirty {
    let a = dispatch(s, k);
    apply(s, a)
}

fn state() -> AppState {
    AppState::new(PathBuf::from("/tmp"))
}

#[test]
fn q_quits() {
    let mut s = state();
    let a = dispatch(&s, key('q'));
    assert_eq!(a, Action::Quit);
    apply(&mut s, a);
    assert!(s.should_quit);
}

#[test]
fn tab_cycles_focus() {
    let mut s = state();
    step(&mut s, ev(KeyCode::Tab));
    assert_eq!(s.focus, Focus::Request);
    step(&mut s, ev(KeyCode::Tab));
    assert_eq!(s.focus, Focus::Response);
}

#[test]
fn arrows_move_spatially() {
    // Layout: Collections | Request
    //         Env         | Response
    let mut s = state();
    assert_eq!(s.focus, Focus::Collections);
    step(&mut s, ev(KeyCode::Right));
    assert_eq!(s.focus, Focus::Request);
    step(&mut s, ev(KeyCode::Down));
    assert_eq!(s.focus, Focus::Response);
    step(&mut s, ev(KeyCode::Left));
    assert_eq!(s.focus, Focus::Env);
    step(&mut s, ev(KeyCode::Up));
    assert_eq!(s.focus, Focus::Collections);
}

#[test]
fn h_l_also_move_panes() {
    let mut s = state();
    step(&mut s, key('l'));
    assert_eq!(s.focus, Focus::Request);
    step(&mut s, key('h'));
    assert_eq!(s.focus, Focus::Collections);
}

#[test]
fn movement_off_grid_is_noop() {
    let mut s = state();
    // Collections has no left/up neighbour
    step(&mut s, ev(KeyCode::Left));
    assert_eq!(s.focus, Focus::Collections);
    step(&mut s, ev(KeyCode::Up));
    assert_eq!(s.focus, Focus::Collections);
}

fn focus_env(s: &mut AppState) {
    while s.focus != Focus::Env {
        step(s, ev(KeyCode::Tab));
    }
}

#[test]
fn env_jk_moves_row_cursor_not_panes() {
    let mut s = state();
    focus_env(&mut s);
    s.add_var("X".into(), "1".into(), false);
    s.add_var("Y".into(), "2".into(), false);
    s.env_cursor = 0;
    step(&mut s, key('j'));
    assert_eq!(s.env_cursor, 1);
    assert_eq!(s.focus, Focus::Env);
    step(&mut s, key('k'));
    assert_eq!(s.env_cursor, 0);
    assert_eq!(s.focus, Focus::Env);
}

#[test]
fn env_add_var_round_trip() {
    let mut s = state();
    focus_env(&mut s);
    step(&mut s, key('a'));
    for c in "API_URL".chars() {
        step(&mut s, key(c));
    }
    step(&mut s, ev(KeyCode::Tab));
    for c in "https://api.test".chars() {
        step(&mut s, key(c));
    }
    let dirty = step(&mut s, ev(KeyCode::Enter));
    assert_eq!(dirty, EnvDirty::Yes);
    let env = s.active_env_ref().unwrap();
    assert_eq!(env.vars.len(), 1);
    assert_eq!(env.vars[0].0, "API_URL");
}

#[test]
fn env_toggle_secret() {
    let mut s = state();
    focus_env(&mut s);
    s.add_var("TOKEN".into(), "xyz".into(), false);
    s.env_cursor = 0;
    let dirty = step(&mut s, key('m'));
    assert_eq!(dirty, EnvDirty::Yes);
    let (_, _, secret) = s.env_var_at(0).unwrap();
    assert!(secret);
}

#[test]
fn env_delete_var() {
    let mut s = state();
    focus_env(&mut s);
    s.add_var("X".into(), "1".into(), false);
    s.add_var("Y".into(), "2".into(), false);
    s.env_cursor = 0;
    let dirty = step(&mut s, key('d'));
    assert_eq!(dirty, EnvDirty::Yes);
    let env = s.active_env_ref().unwrap();
    assert_eq!(env.vars.len(), 1);
    assert_eq!(env.vars[0].0, "Y");
}

#[test]
fn help_toggles_on_question_mark() {
    let mut s = state();
    step(&mut s, key('?'));
    assert!(s.help_open);
    step(&mut s, key('x'));
    assert!(!s.help_open);
}

#[test]
fn command_env_switch() {
    let mut s = state();
    use lazyfetch_core::env::Environment;
    s.envs.push(Environment {
        id: ulid::Ulid::new(),
        name: "dev".into(),
        vars: vec![],
    });
    s.envs.push(Environment {
        id: ulid::Ulid::new(),
        name: "prod".into(),
        vars: vec![],
    });
    s.active_env = Some(0);
    step(&mut s, key(':'));
    for c in "env prod".chars() {
        step(&mut s, key(c));
    }
    step(&mut s, ev(KeyCode::Enter));
    assert_eq!(s.active_env, Some(1));
}
