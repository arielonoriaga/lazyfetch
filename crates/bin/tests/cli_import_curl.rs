//! End-to-end smoke for `lazyfetch import-curl`. Spawns the built binary
//! against a tempdir config and verifies stdout shape + the optional
//! `--save` round-trip into FsCollectionRepo.

use std::process::Command;

fn bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("lazyfetch")
}

#[test]
fn import_curl_positional_prints_summary() {
    let out = Command::new(bin())
        .args(["import-curl", "curl https://api.test/x"])
        .output()
        .expect("spawn");
    assert!(out.status.success(), "exit: {:?}\n{}", out.status, String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("imported GET https://api.test/x"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("warnings: 0"), "stdout: {stdout}");
}

#[test]
fn import_curl_save_writes_yaml_into_collection() {
    let cfg = tempfile::tempdir().unwrap();
    let out = Command::new(bin())
        .args([
            "--config-dir",
            cfg.path().to_str().unwrap(),
            "import-curl",
            "curl -X POST https://api.test/users -H 'Content-Type: application/json' -d '{\"a\":1}'",
            "--save",
            "demo/create-user",
        ])
        .output()
        .expect("spawn");
    assert!(out.status.success(), "exit: {:?}\n{}", out.status, String::from_utf8_lossy(&out.stderr));
    let yaml_path = cfg
        .path()
        .join("collections")
        .join("demo")
        .join("requests")
        .join("create-user.yaml");
    assert!(yaml_path.exists(), "expected {:?}", yaml_path);
    let yaml = std::fs::read_to_string(&yaml_path).unwrap();
    assert!(yaml.contains("create-user"), "yaml: {yaml}");
    assert!(yaml.contains("POST"), "yaml: {yaml}");
    assert!(yaml.contains("https://api.test/users"), "yaml: {yaml}");
}

#[test]
fn import_curl_unknown_flag_emits_warning() {
    let out = Command::new(bin())
        .args(["import-curl", "curl --frobnicate https://api.test/x"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown flag"),
        "stderr: {stderr}"
    );
}
