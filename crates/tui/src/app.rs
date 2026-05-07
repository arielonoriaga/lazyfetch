use lazyfetch_core::catalog::Collection;
use lazyfetch_core::env::{Environment, VarValue};
use secrecy::{ExposeSecret, SecretString};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Collections,
    Env,
    Request,
    Response,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Focus::Collections => Focus::Request,
            Focus::Request => Focus::Response,
            Focus::Response => Focus::Env,
            Focus::Env => Focus::Collections,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Focus::Collections => Focus::Env,
            Focus::Request => Focus::Collections,
            Focus::Response => Focus::Request,
            Focus::Env => Focus::Response,
        }
    }
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
}

impl InsertBuf {
    pub fn new(secret: bool) -> Self {
        Self {
            field: InsertField::Key,
            key: String::new(),
            value: String::new(),
            secret,
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

    pub fn env_var_at(&self, i: usize) -> Option<(&String, &str, bool)> {
        let env = self.active_env_ref()?;
        env.vars
            .get(i)
            .map(|(k, v)| (k, v.value.expose_secret().as_str(), v.secret))
    }
}
