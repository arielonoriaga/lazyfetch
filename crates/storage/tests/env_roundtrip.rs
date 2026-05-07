use lazyfetch_core::env::{Environment, VarValue};
use lazyfetch_storage::env::FsEnvRepo;
use secrecy::{ExposeSecret, SecretString};
use ulid::Ulid;

#[test]
fn save_load_secret_flag() {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsEnvRepo::new(dir.path());
    let env = Environment {
        id: Ulid::new(),
        name: "dev".into(),
        vars: vec![
            (
                "base".into(),
                VarValue {
                    value: SecretString::new("https://api".into()),
                    secret: false,
                },
            ),
            (
                "tok".into(),
                VarValue {
                    value: SecretString::new("xyz".into()),
                    secret: true,
                },
            ),
        ],
    };
    repo.save(&env).unwrap();
    let loaded = repo.load_by_name("dev").unwrap();
    assert!(!loaded.vars[0].1.secret);
    assert!(loaded.vars[1].1.secret);
    assert_eq!(loaded.vars[1].1.value.expose_secret(), "xyz");
}
