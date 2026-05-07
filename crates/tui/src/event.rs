use crate::app::AppState;
use crate::keymap::{apply, dispatch};
use crate::layout::draw;
use crate::terminal::TerminalGuard;
use crossterm::event::{self, Event};
use std::time::Duration;

pub fn run(initial: AppState) -> anyhow::Result<()> {
    let mut guard = TerminalGuard::new()?;
    let mut state = initial;
    while !state.should_quit {
        guard.term.draw(|f| draw(f, &state))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(k) => {
                    let action = dispatch(&state, k);
                    apply(&mut state, action);
                }
                Event::Resize(_, _) => { /* layout recomputed next frame */ }
                _ => {}
            }
        }
    }
    Ok(())
}
