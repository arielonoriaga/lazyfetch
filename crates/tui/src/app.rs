use crate::adapters::Adapters;
use crate::editor::BodyEditorState;
use crate::kv_editor::KvEditor;
use lazyfetch_core::catalog::{BodyKind, Collection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollRow {
    Coll { idx: usize, expanded: bool },
    Req { coll: usize, item: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameTarget {
    Collection {
        idx: usize,
        old: String,
    },
    Request {
        coll: usize,
        item: usize,
        old: String,
    },
}
use lazyfetch_core::env::{Environment, VarValue};
use lazyfetch_core::exec::{ExecError, Executed};
use secrecy::{ExposeSecret, SecretString};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Insert,
    Search,
    SaveAs,
    Rename,
    Move,
    ImportCurl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReqTab {
    Body,
    Headers,
    Query,
}

impl ReqTab {
    pub fn cycle(self) -> Self {
        match self {
            Self::Body => Self::Headers,
            Self::Headers => Self::Query,
            Self::Query => Self::Body,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Collections,
    Env,
    Url,
    Request,
    Response,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Focus::Collections => Focus::Url,
            Focus::Url => Focus::Request,
            Focus::Request => Focus::Response,
            Focus::Response => Focus::Env,
            Focus::Env => Focus::Collections,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Focus::Collections => Focus::Env,
            Focus::Url => Focus::Collections,
            Focus::Request => Focus::Url,
            Focus::Response => Focus::Request,
            Focus::Env => Focus::Response,
        }
    }

    /// Spatial neighbour.
    /// Layout:  Collections | URL
    ///          Collections | Request
    ///          Env         | Response
    pub fn neighbour(self, dir: Dir) -> Self {
        match (self, dir) {
            (Focus::Collections, Dir::Right) => Focus::Request,
            (Focus::Collections, Dir::Down) => Focus::Env,
            (Focus::Url, Dir::Left) => Focus::Collections,
            (Focus::Url, Dir::Down) => Focus::Request,
            (Focus::Request, Dir::Left) => Focus::Collections,
            (Focus::Request, Dir::Up) => Focus::Url,
            (Focus::Request, Dir::Down) => Focus::Response,
            (Focus::Response, Dir::Left) => Focus::Env,
            (Focus::Response, Dir::Up) => Focus::Request,
            (Focus::Env, Dir::Right) => Focus::Response,
            (Focus::Env, Dir::Up) => Focus::Collections,
            (s, _) => s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertField {
    Key,
    Value,
}

#[derive(Debug, Clone)]
pub struct InsertBuf {
    pub field: InsertField,
    pub key: String,
    pub value: String,
    pub secret: bool,
    /// `Some(i)` when editing the row at index `i`; `None` for adding a new row.
    pub edit_idx: Option<usize>,
}

impl InsertBuf {
    pub fn new(secret: bool) -> Self {
        Self {
            field: InsertField::Key,
            key: String::new(),
            value: String::new(),
            secret,
            edit_idx: None,
        }
    }

    pub fn editing(idx: usize, key: String, value: String, secret: bool) -> Self {
        Self {
            field: InsertField::Value,
            key,
            value,
            secret,
            edit_idx: Some(idx),
        }
    }
}

pub struct AppState {
    pub mode: Mode,
    pub focus: Focus,
    pub config_dir: PathBuf,
    pub collections: Vec<Collection>,
    pub envs: Vec<Environment>,
    pub active_env: Option<usize>,
    pub env_cursor: usize,
    pub insert_buf: Option<InsertBuf>,
    pub command_buf: String,
    pub toast: Option<String>,
    pub help_open: bool,
    pub url_buf: String,
    pub method: http::Method,
    pub last_response: Option<Executed>,
    pub last_response_pretty: Option<String>,
    /// Colorized body lines, computed once per response (or never if non-JSON).
    pub last_response_lines: Option<Vec<Vec<ratatui::text::Span<'static>>>>,
    /// Memoized output of `apply_search_highlight` for the current `(body_gen, needle)`.
    /// Cleared when a new response arrives or the search is cancelled. Kept here
    /// instead of `RefCell` because both update sites already hold `&mut AppState`.
    pub highlighted_cache: Option<(u64, String, Vec<Vec<ratatui::text::Span<'static>>>)>,
    /// Generation counter — bumps on every new response receipt. Used as the cache key
    /// for `highlighted_cache` so a stale highlight from a previous response can never
    /// be displayed against fresh body lines.
    pub body_gen: u64,
    pub last_error: Option<String>,
    pub inflight: Option<Receiver<Result<Executed, ExecError>>>,
    pub response_scroll: u16,
    pub response_hscroll: u16,
    pub response_cursor: usize,
    pub response_col: usize,
    pub response_height: u16,
    pub response_width: u16,
    pub response_total_lines: usize,
    pub last_layout: crate::layout::DrawInfo,
    pub revealed_secrets: std::collections::HashSet<(ulid::Ulid, usize)>,
    pub save_buf: String,
    pub url_suggest_idx: usize,
    /// Cache for `url_var_suggestions()` keyed on the partial prefix. Recomputes only
    /// when the user types a new char or the active env changes.
    url_suggestions_cache: std::cell::RefCell<Option<(String, Vec<String>)>>,
    pub coll_cursor: usize,
    pub expanded_colls: std::collections::HashSet<ulid::Ulid>,
    coll_rows_cache: std::cell::RefCell<Option<Vec<CollRow>>>,
    pub marked_requests: std::collections::HashSet<(usize, usize)>,
    pub move_buf: String,
    pub rename_target: Option<RenameTarget>,
    pub rename_buf: String,
    pub help_filter: String,
    pub messages: std::collections::VecDeque<String>,
    pub messages_open: bool,
    pub pending_g: bool,
    pub visual_anchor: Option<(usize, usize)>,
    pub search_buf: String,
    pub search_active: Option<String>,
    pub search_match_lines: Vec<usize>,
    pub search_match_idx: usize,
    pub should_quit: bool,
    pub req_tab: ReqTab,
    pub req_body_kind: BodyKind,
    pub body_editor: BodyEditorState,
    /// Last non-empty body text. Survives switches to KV-backed kinds (Form/Multipart)
    /// so the user can cycle Json→Form→Json without losing what they typed.
    pub body_scratch: String,
    pub headers_kv: KvEditor,
    pub query_kv: KvEditor,
    pub form_kv: KvEditor,
    pub import_curl_buf: String,
    pub body_editing: bool,
    pub adapters: Adapters,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            mode: Mode::Normal,
            focus: Focus::Collections,
            config_dir,
            collections: vec![],
            envs: vec![],
            active_env: None,
            env_cursor: 0,
            insert_buf: None,
            command_buf: String::new(),
            toast: None,
            help_open: false,
            url_buf: String::new(),
            method: http::Method::GET,
            last_response: None,
            last_response_pretty: None,
            last_response_lines: None,
            highlighted_cache: None,
            body_gen: 0,
            last_error: None,
            inflight: None,
            response_scroll: 0,
            response_hscroll: 0,
            response_cursor: 0,
            response_col: 0,
            response_height: 1,
            response_width: 1,
            response_total_lines: 0,
            last_layout: crate::layout::DrawInfo::default(),
            revealed_secrets: std::collections::HashSet::new(),
            save_buf: String::new(),
            url_suggest_idx: 0,
            url_suggestions_cache: std::cell::RefCell::new(None),
            coll_cursor: 0,
            expanded_colls: std::collections::HashSet::new(),
            coll_rows_cache: std::cell::RefCell::new(None),
            marked_requests: std::collections::HashSet::new(),
            move_buf: String::new(),
            rename_target: None,
            rename_buf: String::new(),
            help_filter: String::new(),
            messages: std::collections::VecDeque::with_capacity(64),
            messages_open: false,
            pending_g: false,
            visual_anchor: None,
            search_buf: String::new(),
            search_active: None,
            search_match_lines: vec![],
            search_match_idx: 0,
            should_quit: false,
            req_tab: ReqTab::Body,
            req_body_kind: BodyKind::None,
            body_editor: BodyEditorState::None,
            body_scratch: String::new(),
            headers_kv: KvEditor::new(),
            query_kv: KvEditor::new(),
            form_kv: KvEditor::new(),
            import_curl_buf: String::new(),
            body_editing: false,
            adapters: Adapters::testing(),
        }
    }

    /// Replace the test-default adapter bundle. Composition root (bin)
    /// calls this to inject the production HTTP / auth implementations.
    pub fn with_adapters(mut self, adapters: Adapters) -> Self {
        self.adapters = adapters;
        self
    }

    pub fn active_env_mut(&mut self) -> Option<&mut Environment> {
        self.active_env.and_then(|i| self.envs.get_mut(i))
    }

    pub fn active_env_ref(&self) -> Option<&Environment> {
        self.active_env.and_then(|i| self.envs.get(i))
    }

    pub fn switch_env(&mut self, name: &str) -> bool {
        if let Some(i) = self.envs.iter().position(|e| e.name == name) {
            self.active_env = Some(i);
            self.env_cursor = 0;
            self.invalidate_url_suggestions();
            true
        } else {
            false
        }
    }

    pub fn add_var(&mut self, key: String, value: String, secret: bool) {
        let id = self.env_or_create();
        let env = &mut self.envs[id];
        env.vars.push((
            key,
            VarValue {
                value: SecretString::new(value),
                secret,
            },
        ));
        self.env_cursor = env.vars.len().saturating_sub(1);
        self.active_env = Some(id);
        self.invalidate_url_suggestions();
    }

    pub fn replace_var(&mut self, idx: usize, key: String, value: String, secret: bool) -> bool {
        if let Some(env) = self.active_env_mut() {
            if let Some(slot) = env.vars.get_mut(idx) {
                *slot = (
                    key,
                    VarValue {
                        value: SecretString::new(value),
                        secret,
                    },
                );
                self.invalidate_url_suggestions();
                return true;
            }
        }
        false
    }

    pub fn toggle_reveal(&mut self) -> bool {
        let cur = self.env_cursor;
        let Some(env_id) = self.active_env_ref().map(|e| e.id) else {
            return false;
        };
        let key = (env_id, cur);
        if self.revealed_secrets.contains(&key) {
            self.revealed_secrets.remove(&key);
        } else {
            self.revealed_secrets.insert(key);
        }
        true
    }

    pub fn is_revealed(&self, idx: usize) -> bool {
        self.active_env_ref()
            .map(|e| self.revealed_secrets.contains(&(e.id, idx)))
            .unwrap_or(false)
    }

    /// Detect if the URL buffer is currently inside a `{{...` token.
    /// Returns the partial prefix typed after `{{` (could be empty).
    pub fn url_var_prefix(&self) -> Option<String> {
        let s = &self.url_buf;
        // Find the rightmost `{{` that has not yet been closed.
        let open = s.rfind("{{")?;
        let after = &s[open + 2..];
        if after.contains("}}") {
            return None;
        }
        // Bail out if the partial contains whitespace — likely not a var name anymore.
        if after.chars().any(|c| c.is_whitespace()) {
            return None;
        }
        Some(after.to_string())
    }

    /// Variable names matching the current URL prefix, sorted alphabetically.
    /// Memoized on the prefix — recomputes only when the user changes the partial.
    /// The cache is cleared explicitly via `invalidate_url_suggestions` when the
    /// active env or env vars mutate (rare relative to keystrokes).
    pub fn url_var_suggestions(&self) -> Vec<String> {
        let Some(prefix) = self.url_var_prefix() else {
            return vec![];
        };
        if let Some((cached_prefix, cached)) = self.url_suggestions_cache.borrow().as_ref() {
            if *cached_prefix == prefix {
                return cached.clone();
            }
        }
        let lower = prefix.to_lowercase();
        let mut names: Vec<String> = self
            .active_env_ref()
            .into_iter()
            .flat_map(|e| e.vars.iter().map(|(k, _)| k.clone()))
            .chain(
                self.collections
                    .iter()
                    .flat_map(|c| c.vars.iter().map(|kv| kv.key.clone())),
            )
            .filter(|n| n.to_lowercase().starts_with(&lower))
            .collect();
        names.sort();
        names.dedup();
        *self.url_suggestions_cache.borrow_mut() = Some((prefix, names.clone()));
        names
    }

    pub fn invalidate_url_suggestions(&self) {
        *self.url_suggestions_cache.borrow_mut() = None;
    }

    /// Replace the active `{{<prefix>` with `{{<chosen>}}` and reset the suggestion cursor.
    pub fn url_complete_var(&mut self, chosen: &str) {
        let Some(prefix) = self.url_var_prefix() else {
            return;
        };
        let trim_len = prefix.len();
        let new_len = self.url_buf.len() - trim_len;
        self.url_buf.truncate(new_len);
        self.url_buf.push_str(chosen);
        self.url_buf.push_str("}}");
        self.url_suggest_idx = 0;
    }

    /// Flatten the Collections list into displayable rows. Cached: invalidate via
    /// `invalidate_coll_rows()` whenever `collections` or `expanded_colls` mutates.
    /// Top-level requests only — nested folders are TODO.
    pub fn coll_rows(&self) -> Vec<CollRow> {
        if let Some(cached) = self.coll_rows_cache.borrow().as_ref() {
            return cached.clone();
        }
        let rows = self.compute_coll_rows();
        *self.coll_rows_cache.borrow_mut() = Some(rows.clone());
        rows
    }

    fn compute_coll_rows(&self) -> Vec<CollRow> {
        use lazyfetch_core::catalog::Item;
        let mut rows = vec![];
        for (ci, c) in self.collections.iter().enumerate() {
            rows.push(CollRow::Coll {
                idx: ci,
                expanded: self.expanded_colls.contains(&c.id),
            });
            if !self.expanded_colls.contains(&c.id) {
                continue;
            }
            for (ri, item) in c.root.items.iter().enumerate() {
                if let Item::Request(_) = item {
                    rows.push(CollRow::Req { coll: ci, item: ri });
                }
            }
        }
        rows
    }

    /// Invalidate the coll_rows cache. Call after every collections / expanded_colls write.
    pub fn invalidate_coll_rows(&self) {
        *self.coll_rows_cache.borrow_mut() = None;
    }

    /// Toggle expansion for the collection currently under the cursor (no-op on a request row).
    pub fn coll_toggle_expand(&mut self) -> bool {
        let rows = self.coll_rows();
        let Some(row) = rows.get(self.coll_cursor) else {
            return false;
        };
        match *row {
            CollRow::Coll { idx, .. } => {
                let id = self.collections[idx].id;
                if self.expanded_colls.contains(&id) {
                    self.expanded_colls.remove(&id);
                } else {
                    self.expanded_colls.insert(id);
                }
                self.invalidate_coll_rows();
                true
            }
            CollRow::Req { .. } => false,
        }
    }

    /// Load the request under the cursor into URL+method (returns true on success).
    pub fn coll_open_selected(&mut self) -> Option<String> {
        use lazyfetch_core::catalog::Item;
        let rows = self.coll_rows();
        let row = rows.get(self.coll_cursor).copied()?;
        match row {
            CollRow::Req { coll, item } => {
                let r = match self.collections.get(coll)?.root.items.get(item)? {
                    Item::Request(r) => r,
                    _ => return None,
                };
                self.url_buf = r.url.0 .0.clone();
                self.method = r.method.clone();
                self.url_suggest_idx = 0;
                Some(r.name.clone())
            }
            CollRow::Coll { .. } => None,
        }
    }

    /// Push a transient message both to the visible toast slot and into the rolling
    /// `:messages` history (capped at 64 entries — oldest dropped). Use this instead of
    /// setting `state.toast = Some(...)` directly so nothing slips out of the audit trail.
    pub fn notify(&mut self, msg: String) {
        if self.messages.len() >= 64 {
            self.messages.pop_front();
        }
        self.messages.push_back(msg.clone());
        self.toast = Some(msg);
    }

    pub fn create_env(&mut self, name: &str) -> bool {
        if self.envs.iter().any(|e| e.name == name) {
            return false;
        }
        self.envs.push(Environment {
            id: Ulid::new(),
            name: name.into(),
            vars: vec![],
        });
        self.active_env = Some(self.envs.len() - 1);
        self.env_cursor = 0;
        self.invalidate_url_suggestions();
        true
    }

    fn env_or_create(&mut self) -> usize {
        if let Some(i) = self.active_env {
            i
        } else {
            self.envs.push(Environment {
                id: Ulid::new(),
                name: "default".into(),
                vars: vec![],
            });
            let i = self.envs.len() - 1;
            self.active_env = Some(i);
            i
        }
    }

    pub fn delete_var(&mut self) -> bool {
        let cur = self.env_cursor;
        if let Some(env) = self.active_env_mut() {
            if cur < env.vars.len() {
                env.vars.remove(cur);
                if cur >= env.vars.len() && !env.vars.is_empty() {
                    self.env_cursor = env.vars.len() - 1;
                } else if env.vars.is_empty() {
                    self.env_cursor = 0;
                }
                self.invalidate_url_suggestions();
                return true;
            }
        }
        false
    }

    pub fn toggle_secret(&mut self) -> bool {
        let cur = self.env_cursor;
        if let Some(env) = self.active_env_mut() {
            if let Some((_, v)) = env.vars.get_mut(cur) {
                v.secret = !v.secret;
                return true;
            }
        }
        false
    }

    /// Move cursor to `target`, then adjust scroll so the cursor stays in the visible window.
    pub fn move_cursor_to(&mut self, target: usize) {
        let last = self.response_total_lines.saturating_sub(1);
        self.response_cursor = target.min(last);
        let h = self.response_height.max(1) as usize;
        let scroll = self.response_scroll as usize;
        if self.response_cursor < scroll {
            self.response_scroll = self.response_cursor as u16;
        } else if self.response_cursor >= scroll + h {
            self.response_scroll = (self.response_cursor + 1 - h) as u16;
        }
        self.response_col = 0;
        self.response_hscroll = 0;
    }

    pub fn move_cursor_by(&mut self, delta: i32) {
        let cur = self.response_cursor as i32 + delta;
        let cur = cur.max(0) as usize;
        self.move_cursor_to(cur);
    }

    /// Move cursor column on the current line; horizontal scroll follows.
    pub fn move_col_to(&mut self, col: usize, line_len: usize) {
        self.response_col = col.min(line_len.saturating_sub(1));
        let w = self.response_width.max(1) as usize;
        let hs = self.response_hscroll as usize;
        if self.response_col < hs {
            self.response_hscroll = self.response_col as u16;
        } else if self.response_col >= hs + w {
            self.response_hscroll = (self.response_col + 1 - w) as u16;
        }
    }

    pub fn move_col_by(&mut self, delta: i32, line_len: usize) {
        let c = self.response_col as i32 + delta;
        let c = c.max(0) as usize;
        self.move_col_to(c, line_len);
    }

    pub fn env_var_at(&self, i: usize) -> Option<(&String, &str, bool)> {
        let env = self.active_env_ref()?;
        env.vars
            .get(i)
            .map(|(k, v)| (k, v.value.expose_secret().as_str(), v.secret))
    }
}
