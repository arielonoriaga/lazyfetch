use lazyfetch_core::catalog::Collection;
use lazyfetch_core::env::Environment;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command,
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

pub struct AppState {
    pub mode: Mode,
    pub focus: Focus,
    pub collections: Vec<Collection>,
    pub envs: Vec<Environment>,
    pub active_env: Option<usize>,
    pub should_quit: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            focus: Focus::Collections,
            collections: vec![],
            envs: vec![],
            active_env: None,
            should_quit: false,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
