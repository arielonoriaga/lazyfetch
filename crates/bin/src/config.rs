use std::path::{Path, PathBuf};

const PROJECT_DIR: &str = ".lazyfetch";

/// Resolve config dir with this precedence:
/// 1. explicit override (CLI flag) — caller passes `Some(p)`
/// 2. nearest ancestor of `start` containing a `.lazyfetch/` directory
/// 3. global XDG dir (`~/.config/lazyfetch`)
pub fn resolve(explicit: Option<PathBuf>, start: &Path) -> PathBuf {
    if let Some(p) = explicit {
        return p;
    }
    if let Some(p) = walk_up(start) {
        return p;
    }
    global_default()
}

fn walk_up(start: &Path) -> Option<PathBuf> {
    let mut cur = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    loop {
        let candidate = cur.join(PROJECT_DIR);
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !cur.pop() {
            return None;
        }
    }
}

pub fn global_default() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lazyfetch")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().to_path_buf();
        assert_eq!(resolve(Some(p.clone()), Path::new("/")), p);
    }

    #[test]
    fn finds_project_dir_in_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".lazyfetch")).unwrap();
        let got = resolve(None, tmp.path());
        assert_eq!(got, tmp.path().join(".lazyfetch"));
    }

    #[test]
    fn finds_project_dir_in_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".lazyfetch")).unwrap();
        let nested = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).unwrap();
        let got = resolve(None, &nested);
        assert_eq!(got, tmp.path().join(".lazyfetch"));
    }

    #[test]
    fn falls_back_to_global_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let got = resolve(None, tmp.path());
        assert_eq!(got, global_default());
    }
}
