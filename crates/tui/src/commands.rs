//! Filesystem-mutating command handlers: `:env`, `:newenv`, `:save`, `:method`, `:messages`,
//! `:q`. Plus `run_save` / `run_rename` / `run_move` for the popup submit paths.
//!
//! Extracted from `keymap.rs` so the keymap doesn't directly reach into storage.

use crate::app::{AppState, RenameTarget};
use crate::keymap::EnvDirty;
use lazyfetch_core::catalog::{Body, Item, Request};
use lazyfetch_core::primitives::{Template, UrlTemplate};
use lazyfetch_storage::collection::FsCollectionRepo;

pub fn run_command(state: &mut AppState, cmd: &str) -> EnvDirty {
    let cmd = cmd.trim();
    if cmd == "messages" {
        state.messages_open = true;
        return EnvDirty::No;
    }
    if let Some(name) = cmd.strip_prefix("env ").map(str::trim) {
        if state.switch_env(name) {
            state.notify(format!("env: {}", name));
        } else {
            state.notify(format!("env not found: {}", name));
        }
        return EnvDirty::No;
    }
    if let Some(name) = cmd.strip_prefix("newenv ").map(str::trim) {
        if name.is_empty() {
            state.notify("usage: :newenv <name>".to_string());
        } else if state.create_env(name) {
            state.notify(format!("created env: {}", name));
            return EnvDirty::Yes;
        } else {
            state.notify(format!("env already exists: {}", name));
        }
        return EnvDirty::No;
    }
    if let Some(rest) = cmd.strip_prefix("save ").map(str::trim) {
        return run_save(state, rest);
    }
    if let Some(name) = cmd.strip_prefix("method ").map(str::trim) {
        if let Ok(m) = name.to_ascii_uppercase().parse::<http::Method>() {
            state.method = m;
            state.notify(format!("method: {}", state.method));
        } else {
            state.notify(format!("invalid method: {}", name));
        }
        return EnvDirty::No;
    }
    if cmd == "q" || cmd == "quit" {
        state.should_quit = true;
        return EnvDirty::No;
    }
    state.notify(format!("unknown: {}", cmd));
    EnvDirty::No
}

pub fn run_save(state: &mut AppState, arg: &str) -> EnvDirty {
    let (coll, name) = match arg.split_once('/') {
        Some((c, n)) if !c.is_empty() && !n.is_empty() => (c, n),
        _ => {
            state.notify("usage: :save <collection>/<name>".to_string());
            return EnvDirty::No;
        }
    };
    if state.url_buf.is_empty() {
        state.notify("URL is empty — type one in the URL pane first".to_string());
        return EnvDirty::No;
    }
    let req = Request {
        id: ulid::Ulid::new(),
        name: name.to_string(),
        method: state.method.clone(),
        url: UrlTemplate(Template(state.url_buf.clone())),
        query: vec![],
        headers: vec![],
        body: Body::None,
        auth: None,
        notes: None,
        follow_redirects: true,
        max_redirects: 10,
        timeout_ms: None,
    };
    let repo = FsCollectionRepo::new(state.config_dir.join("collections"));
    match repo.save_request(coll, &req) {
        Ok(()) => {
            state.notify(format!("saved {}/{}", coll, name));
            if let Ok(c) = repo.load_by_name(coll) {
                if let Some(idx) = state.collections.iter().position(|x| x.name == c.name) {
                    state.collections[idx] = c;
                } else {
                    state.collections.push(c);
                }
                state.invalidate_coll_rows();
            }
        }
        Err(e) => state.notify(format!("save failed: {}", e)),
    }
    EnvDirty::No
}

pub fn run_rename(state: &mut AppState, target: Option<RenameTarget>, new: &str) {
    let Some(target) = target else { return };
    if new.is_empty() {
        state.notify("name is empty".to_string());
        return;
    }
    let repo = FsCollectionRepo::new(state.config_dir.join("collections"));
    match target {
        RenameTarget::Collection { idx, old } => {
            if old == new {
                return;
            }
            match repo.rename_collection(&old, new) {
                Ok(()) => {
                    if let Some(c) = state.collections.get_mut(idx) {
                        c.name = new.to_string();
                    }
                    state.invalidate_coll_rows();
                    state.notify(format!("renamed {} → {}", old, new));
                }
                Err(e) => state.notify(format!("rename failed: {}", e)),
            }
        }
        RenameTarget::Request { coll, item, old } => {
            if old == new {
                return;
            }
            let coll_name = state.collections[coll].name.clone();
            match repo.rename_request(&coll_name, &old, new) {
                Ok(()) => {
                    if let Item::Request(r) = &mut state.collections[coll].root.items[item] {
                        r.name = new.to_string();
                    }
                    state.invalidate_coll_rows();
                    state.notify(format!("renamed {} → {}", old, new));
                }
                Err(e) => state.notify(format!("rename failed: {}", e)),
            }
        }
    }
}

pub fn run_move(state: &mut AppState, target: &str) {
    if target.is_empty() {
        state.notify("usage: type target collection name".to_string());
        return;
    }
    let repo = FsCollectionRepo::new(state.config_dir.join("collections"));
    let marks: Vec<(usize, usize)> = state.marked_requests.iter().copied().collect();
    let mut moved = 0usize;
    let mut errors = 0usize;
    for (coll_idx, item_idx) in &marks {
        let Some(coll) = state.collections.get(*coll_idx) else {
            continue;
        };
        if coll.name == target {
            continue;
        }
        let from_coll = coll.name.clone();
        let req_name = match coll.root.items.get(*item_idx) {
            Some(Item::Request(r)) => r.name.clone(),
            _ => continue,
        };
        match repo.move_request(&from_coll, &req_name, target) {
            Ok(()) => moved += 1,
            Err(_) => errors += 1,
        }
    }

    let mut affected_names: std::collections::HashSet<String> = marks
        .iter()
        .filter_map(|(c, _)| state.collections.get(*c).map(|x| x.name.clone()))
        .collect();
    affected_names.insert(target.to_string());
    for name in affected_names {
        if let Ok(c) = repo.load_by_name(&name) {
            if let Some(idx) = state.collections.iter().position(|x| x.name == name) {
                state.collections[idx] = c;
            } else {
                state.collections.push(c);
            }
        }
    }
    state.invalidate_coll_rows();

    state.marked_requests.clear();
    let msg = if errors == 0 {
        format!("moved {} → {}", moved, target)
    } else {
        format!("moved {} ({} failed) → {}", moved, errors, target)
    };
    state.notify(msg);
}
