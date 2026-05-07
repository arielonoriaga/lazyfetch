use crate::app::AppState;
use crate::keymap::{apply, dispatch, EnvDirty};
use crate::layout::draw;
use crate::terminal::TerminalGuard;
use crossterm::event::{self, Event};
use lazyfetch_storage::collection::FsCollectionRepo;
use lazyfetch_storage::env::FsEnvRepo;
use std::time::Duration;

pub fn run(mut state: AppState) -> anyhow::Result<()> {
    load_from_disk(&mut state)?;
    let mut guard = TerminalGuard::new()?;
    while !state.should_quit {
        guard.term.draw(|f| draw(f, &state))?;
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(k) => {
                    let action = dispatch(&state, k);
                    let dirty = apply(&mut state, action);
                    if dirty == EnvDirty::Yes {
                        if let Some(env) = state.active_env_ref() {
                            let repo = FsEnvRepo::new(state.config_dir.join("environments"));
                            if let Err(e) = repo.save(env) {
                                state.toast = Some(format!("save failed: {}", e));
                            } else {
                                state.toast = Some(format!("saved {}", env.name));
                            }
                        }
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}

fn load_from_disk(state: &mut AppState) -> anyhow::Result<()> {
    let env_dir = state.config_dir.join("environments");
    if env_dir.is_dir() {
        let repo = FsEnvRepo::new(&env_dir);
        let mut names: Vec<String> = std::fs::read_dir(&env_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("yaml") {
                    p.file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        for n in names {
            if let Ok(env) = repo.load_by_name(&n) {
                state.envs.push(env);
            }
        }
        if !state.envs.is_empty() {
            state.active_env = Some(0);
        }
    }

    let coll_dir = state.config_dir.join("collections");
    if coll_dir.is_dir() {
        let repo = FsCollectionRepo::new(&coll_dir);
        let mut names: Vec<String> = std::fs::read_dir(&coll_dir)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let p = e.path();
                if p.is_dir() {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        names.sort();
        for n in names {
            if let Ok(coll) = repo.load_by_name(&n) {
                state.collections.push(coll);
            }
        }
    }
    Ok(())
}
