use lazyfetch_storage::history::FsHistoryRepo;
use std::sync::Arc;
use std::thread;

#[test]
fn concurrent_appends_no_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hist.jsonl");
    let repo = Arc::new(FsHistoryRepo::new(path.clone(), 1000));

    let mut handles = vec![];
    for i in 0..50 {
        let r = repo.clone();
        handles.push(thread::spawn(move || {
            r.append_raw(&format!(r#"{{"i":{}}}"#, i)).unwrap();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    let lines: Vec<_> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    assert_eq!(lines.len(), 50);
    for l in lines {
        let _: serde_json::Value = serde_json::from_str(&l).unwrap();
    }
}
