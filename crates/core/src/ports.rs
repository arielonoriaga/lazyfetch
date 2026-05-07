use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub trait Browser: Send + Sync {
    fn open(&self, url: &str) -> std::io::Result<()>;
}

pub enum EditHint {
    PlainText,
    Json,
    Yaml,
    Form,
}

pub trait Editor: Send + Sync {
    fn edit(&self, buf: &str, hint: EditHint) -> std::io::Result<String>;
}
