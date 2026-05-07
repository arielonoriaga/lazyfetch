use lazyfetch_core::catalog::Collection;
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
    pub pending_g: bool,
    pub visual_anchor: Option<(usize, usize)>,
    pub search_buf: String,
    pub search_active: Option<String>,
    pub search_match_lines: Vec<usize>,
    pub search_match_idx: usize,
    pub should_quit: bool,
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
            pending_g: false,
            visual_anchor: None,
            search_buf: String::new(),
            search_active: None,
            search_match_lines: vec![],
            search_match_idx: 0,
            should_quit: false,
        }
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
