use crate::app::{AppState, Focus};
use crate::keymap::{apply, dispatch, Action, EnvDirty};
use crate::layout::draw;
use crate::sender;
use crate::terminal::TerminalGuard;
use crossterm::event::{self, Event, MouseButton, MouseEvent, MouseEventKind};
use lazyfetch_storage::collection::FsCollectionRepo;
use lazyfetch_storage::env::FsEnvRepo;
use ratatui::layout::Rect;
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
        state.last_layout = info;

        // Poll inflight result.
        if let Some(rx) = state.inflight.as_ref() {
            match rx.try_recv() {
                Ok(Ok(executed)) => {
                    state.toast = Some(format!(
                        "{} {}ms",
                        executed.response.status,
                        executed.response.elapsed.as_millis()
                    ));
                    // Cache the pretty-printed body once on receipt so layout / keymap /
                    // mouse handlers don't re-parse the JSON on every event.
                    let ct = executed
                        .response
                        .headers
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("");
                    let pretty = crate::response::pretty_body(ct, &executed.response.body_bytes);
                    let pretty = executed.secrets.redact(&pretty);
                    // Colorize once on receipt; render reads the cached spans every frame.
                    let kind = crate::response::render_kind(ct, &executed.response.body_bytes);
                    let lines = if matches!(kind, Some("json")) {
                        crate::response::colorize_json(&pretty)
                    } else {
                        crate::response::plain_lines(&pretty)
                    };
                    state.last_response_pretty = Some(pretty);
                    state.last_response_lines = Some(lines);
                    state.last_response = Some(executed);
                    state.last_error = None;
                    state.inflight = None;
                }
                Ok(Err(e)) => {
                    state.last_error = Some(format!("{e}"));
                    state.notify("error".to_string());
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
                                state.notify(format!("save failed: {}", e));
                            } else {
                                state.notify(format!("saved {}", env.name));
                            }
                        }
                    }
                    if send_now {
                        if state.inflight.is_some() {
                            state.notify("send already in flight".to_string());
                        } else if state.url_buf.is_empty() {
                            state.notify("URL is empty".to_string());
                        } else {
                            state.toast =
                                Some(format!("sending {} {}…", state.method, state.url_buf));
                            state.inflight = Some(sender::dispatch(&state, rt.clone()));
                        }
                    }
                }
                Event::Mouse(m) => handle_mouse(&mut state, m),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}

fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
    x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height
}

fn handle_mouse(state: &mut AppState, m: MouseEvent) {
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let info = state.last_layout;
            let (x, y) = (m.column, m.row);
            let new_focus = if rect_contains(info.collections_rect, x, y) {
                Some(Focus::Collections)
            } else if rect_contains(info.env_rect, x, y) {
                Some(Focus::Env)
            } else if rect_contains(info.url_rect, x, y) {
                Some(Focus::Url)
            } else if rect_contains(info.request_rect, x, y) {
                Some(Focus::Request)
            } else if rect_contains(info.response_rect, x, y) {
                Some(Focus::Response)
            } else {
                None
            };
            if let Some(f) = new_focus {
                state.focus = f;
            }
            // If clicking inside the response body, set cursor line + col.
            if rect_contains(info.response_body_rect, x, y) {
                let body = info.response_body_rect;
                let row_in_body = (y - body.y) as usize;
                let col_in_body = (x.saturating_sub(body.x)) as usize;
                let target_line = (state.response_scroll as usize + row_in_body)
                    .min(state.response_total_lines.saturating_sub(1));
                state.move_cursor_to(target_line);
                let len = current_line_len(state);
                let target_col = state.response_hscroll as usize + col_in_body;
                state.move_col_to(target_col, len);
            }
        }
        MouseEventKind::ScrollDown if state.focus == Focus::Response => {
            state.move_cursor_by(3);
        }
        MouseEventKind::ScrollUp if state.focus == Focus::Response => {
            state.move_cursor_by(-3);
        }
        _ => {}
    }
}

fn current_line_len(state: &AppState) -> usize {
    state
        .last_response_pretty
        .as_deref()
        .and_then(|b| {
            b.lines()
                .nth(state.response_cursor)
                .map(|l| l.chars().count())
        })
        .unwrap_or(0)
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
