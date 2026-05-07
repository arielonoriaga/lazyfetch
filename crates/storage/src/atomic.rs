use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub fn write_atomic(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent dir"))?;
    if !parent.as_os_str().is_empty() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = tempfile::Builder::new()
        .prefix(".lazyfetch-")
        .tempfile_in(if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        })?;
    {
        let mut f = tmp.as_file();
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    tmp.persist(target).map_err(|e| e.error)?;
    #[cfg(unix)]
    {
        if let Ok(dir) = OpenOptions::new().read(true).open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}
