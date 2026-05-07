use crate::app::AppState;
use crate::keymap::{apply, dispatch, Action, EnvDirty};
use crate::layout::draw;
use crate::sender;
use crate::terminal::TerminalGuard;
use crossterm::event::{self, Event};
use lazyfetch_storage::collection::FsCollectionRepo;
use lazyfetch_storage::env::FsEnvRepo;
use std::time::Duration;
use tokio::runtime::Handle;

pub fn run(mut state: AppState, rt: Handle) -> anyhow::Result<()> {
    load_from_disk(&mut state)?;
    let mut guard = TerminalGuard::new()?;
    while !state.should_quit {
        let mut info = crate::layout::DrawInfo::default();
        guard.term.draw(|f| {
            info = draw(f, &state);
        })?;
        state.response_height = info.response_height;
        state.response_width = info.response_width;
        state.response_total_lines = info.response_total_lines;

        // Poll inflight result.
        if let Some(rx) = state.inflight.as_ref() {
            match rx.try_recv() {
                Ok(Ok(executed)) => {
                    state.toast = Some(format!(
                        "{} {}ms",
                        executed.response.status,
                        executed.response.elapsed.as_millis()
                    ));
                    state.last_response = Some(executed);
                    state.last_error = None;
                    state.inflight = None;
                }
                Ok(Err(e)) => {
                    state.last_error = Some(format!("{e}"));
                    state.toast = Some("error".into());
                    state.inflight = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    state.inflight = None;
                }
            }
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(k) => {
                    let action = dispatch(&state, k);
                    let send_now = matches!(action, Action::SendRequest);
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
                    if send_now {
                        if state.inflight.is_some() {
                            state.toast = Some("send already in flight".into());
                        } else if state.url_buf.is_empty() {
                            state.toast = Some("URL is empty".into());
                        } else {
                            state.toast =
                                Some(format!("sending {} {}…", state.method, state.url_buf));
                            state.inflight = Some(sender::dispatch(&state, rt.clone()));
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
