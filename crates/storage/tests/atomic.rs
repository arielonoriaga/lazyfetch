use lazyfetch_storage::atomic::write_atomic;
use std::fs;

#[test]
fn writes_and_replaces() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("f.yaml");
    fs::write(&p, "old").unwrap();
    write_atomic(&p, b"new").unwrap();
    assert_eq!(fs::read_to_string(&p).unwrap(), "new");
    let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(entries.len(), 1, "no leftover tempfile");
}
