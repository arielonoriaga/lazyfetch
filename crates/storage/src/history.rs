use fd_lock::RwLock;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct FsHistoryRepo {
    path: PathBuf,
    max: usize,
    lock: Mutex<()>,
}

impl FsHistoryRepo {
    pub fn new(path: PathBuf, max: usize) -> Self {
        Self {
            path,
            max,
            lock: Mutex::new(()),
        }
    }

    pub fn append_raw(&self, line: &str) -> std::io::Result<()> {
        let _g = self.lock.lock().unwrap();
        if let Some(p) = self.path.parent() {
            if !p.as_os_str().is_empty() {
                std::fs::create_dir_all(p)?;
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.path)?;
        let mut lock = RwLock::new(file);
        let mut w = lock.write()?;
        writeln!(*w, "{}", line)?;
        w.sync_data()?;
        Ok(())
    }

    pub fn tail(&self, n: usize) -> std::io::Result<Vec<String>> {
        let s = std::fs::read_to_string(&self.path).unwrap_or_default();
        Ok(s.lines().rev().take(n).map(String::from).collect())
    }

    pub fn truncate_to_max(&self) -> std::io::Result<()> {
        let _g = self.lock.lock().unwrap();
        let s = std::fs::read_to_string(&self.path).unwrap_or_default();
        let lines: Vec<&str> = s.lines().collect();
        if lines.len() <= self.max {
            return Ok(());
        }
        let keep = &lines[lines.len() - self.max..];
        let joined = keep.join("\n") + "\n";
        crate::atomic::write_atomic(&self.path, joined.as_bytes())
    }
}
