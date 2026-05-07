use std::collections::HashSet;

#[derive(Debug, Default, Clone)]
pub struct SecretRegistry {
    values: HashSet<String>,
}

impl SecretRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, v: impl Into<String>) {
        let s = v.into();
        if !s.is_empty() {
            self.values.insert(s);
        }
    }

    pub fn extend(&mut self, other: &SecretRegistry) {
        self.values.extend(other.values.iter().cloned());
    }

    pub fn contains(&self, v: &str) -> bool {
        self.values.contains(v)
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn redact(&self, s: &str) -> String {
        let mut out = s.to_string();
        for v in &self.values {
            out = out.replace(v, "***");
        }
        out
    }
}
