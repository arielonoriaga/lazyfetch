//! Hybrid KV editor — Normal mode nav + Insert mode inline edit.
//! Used by Headers, Query, Form (urlencoded body), and Multipart (with kind toggle).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvRowKind {
    Text,
    File,
}

#[derive(Debug, Clone)]
pub struct KvRow {
    pub kind: KvRowKind,
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub secret: bool,
}

impl KvRow {
    pub fn text(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            kind: KvRowKind::Text,
            key: key.into(),
            value: value.into(),
            enabled: true,
            secret: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KvMode {
    Normal,
    InsertKey { row: usize },
    InsertValue { row: usize },
}

pub struct KvEditor {
    pub rows: Vec<KvRow>,
    pub cursor: usize,
    pub mode: KvMode,
    pub buf: String,
    pub cursor_col: usize,
    fresh_row: bool,
}

impl Default for KvEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl KvEditor {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            cursor: 0,
            mode: KvMode::Normal,
            buf: String::new(),
            cursor_col: 0,
            fresh_row: false,
        }
    }

    pub fn push_row(&mut self, key: &str, value: &str) {
        self.rows.push(KvRow::text(key, value));
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }
    pub fn move_down(&mut self) {
        if self.cursor + 1 < self.rows.len() {
            self.cursor += 1;
        }
    }

    pub fn start_add(&mut self) {
        self.rows.push(KvRow::text("", ""));
        let row = self.rows.len() - 1;
        self.cursor = row;
        self.mode = KvMode::InsertKey { row };
        self.buf.clear();
        self.cursor_col = 0;
        self.fresh_row = true;
    }

    pub fn start_edit_value(&mut self) {
        if self.cursor < self.rows.len() {
            self.buf = self.rows[self.cursor].value.clone();
            self.cursor_col = self.buf.len();
            self.mode = KvMode::InsertValue { row: self.cursor };
        }
    }

    pub fn start_edit_key(&mut self) {
        if self.cursor < self.rows.len() {
            self.buf = self.rows[self.cursor].key.clone();
            self.cursor_col = self.buf.len();
            self.mode = KvMode::InsertKey { row: self.cursor };
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.buf.push(c);
        self.cursor_col += 1;
        self.write_buf_into_row();
    }

    pub fn backspace(&mut self) {
        if self.buf.pop().is_some() {
            self.cursor_col = self.cursor_col.saturating_sub(1);
            self.write_buf_into_row();
        }
    }

    pub fn tab(&mut self) {
        let row = match self.mode {
            KvMode::InsertKey { row } => {
                self.buf = self.rows[row].value.clone();
                self.cursor_col = self.buf.len();
                self.mode = KvMode::InsertValue { row };
                return;
            }
            KvMode::InsertValue { row } => row,
            KvMode::Normal => return,
        };
        self.buf = self.rows[row].key.clone();
        self.cursor_col = self.buf.len();
        self.mode = KvMode::InsertKey { row };
    }

    pub fn commit(&mut self) {
        let row = match self.mode {
            KvMode::InsertKey { row } | KvMode::InsertValue { row } => row,
            KvMode::Normal => return,
        };
        self.write_buf_into_row();
        if self.rows[row].key.is_empty() {
            self.mode = KvMode::InsertKey { row };
            return;
        }
        self.mode = KvMode::Normal;
        self.buf.clear();
        self.cursor_col = 0;
        self.fresh_row = false;
    }

    pub fn cancel(&mut self) {
        if let KvMode::InsertKey { row } | KvMode::InsertValue { row } = self.mode {
            if self.fresh_row {
                self.rows.remove(row);
                if self.cursor >= self.rows.len() {
                    self.cursor = self.rows.len().saturating_sub(1);
                }
            }
        }
        self.mode = KvMode::Normal;
        self.buf.clear();
        self.cursor_col = 0;
        self.fresh_row = false;
    }

    pub fn toggle_enabled(&mut self) {
        if self.cursor < self.rows.len() {
            self.rows[self.cursor].enabled = !self.rows[self.cursor].enabled;
        }
    }

    pub fn toggle_secret(&mut self) {
        if self.cursor < self.rows.len() {
            self.rows[self.cursor].secret = !self.rows[self.cursor].secret;
        }
    }

    pub fn toggle_kind(&mut self) {
        if self.cursor < self.rows.len() {
            self.rows[self.cursor].kind = match self.rows[self.cursor].kind {
                KvRowKind::Text => KvRowKind::File,
                KvRowKind::File => KvRowKind::Text,
            };
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.rows.len() {
            self.rows.remove(self.cursor);
            if self.cursor >= self.rows.len() && !self.rows.is_empty() {
                self.cursor = self.rows.len() - 1;
            } else if self.rows.is_empty() {
                self.cursor = 0;
            }
        }
    }

    pub fn enabled_text_rows(&self) -> Vec<lazyfetch_core::primitives::KV> {
        self.rows
            .iter()
            .filter(|r| r.enabled && r.kind == KvRowKind::Text)
            .map(|r| lazyfetch_core::primitives::KV {
                key: r.key.clone(),
                value: r.value.clone(),
                enabled: true,
                secret: r.secret,
            })
            .collect()
    }

    fn write_buf_into_row(&mut self) {
        match self.mode {
            KvMode::InsertKey { row } => self.rows[row].key = self.buf.clone(),
            KvMode::InsertValue { row } => self.rows[row].value = self.buf.clone(),
            KvMode::Normal => {}
        }
    }
}
