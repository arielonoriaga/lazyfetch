use crate::atomic::write_atomic;
use lazyfetch_core::env::{Environment, VarValue};
use secrecy::{ExposeSecret, SecretString};
use std::path::{Path, PathBuf};

#[derive(serde::Serialize, serde::Deserialize)]
struct EnvFile {
    id: ulid::Ulid,
    name: String,
    vars: Vec<VarRow>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct VarRow {
    name: String,
    value: String,
    #[serde(default)]
    secret: bool,
}

pub struct FsEnvRepo {
    root: PathBuf,
}

impl FsEnvRepo {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn save(&self, e: &Environment) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        let f = EnvFile {
            id: e.id,
            name: e.name.clone(),
            vars: e
                .vars
                .iter()
                .map(|(k, v)| VarRow {
                    name: k.clone(),
                    value: v.value.expose_secret().clone(),
                    secret: v.secret,
                })
                .collect(),
        };
        let y = serde_yaml::to_string(&f).map_err(|e| std::io::Error::other(e.to_string()))?;
        write_atomic(&self.root.join(format!("{}.yaml", e.name)), y.as_bytes())
    }

    pub fn load_by_name(&self, name: &str) -> std::io::Result<Environment> {
        let s = std::fs::read_to_string(self.root.join(format!("{}.yaml", name)))?;
        let f: EnvFile =
            serde_yaml::from_str(&s).map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(Environment {
            id: f.id,
            name: f.name,
            vars: f
                .vars
                .into_iter()
                .map(|r| {
                    (
                        r.name,
                        VarValue {
                            value: SecretString::new(r.value),
                            secret: r.secret,
                        },
                    )
                })
                .collect(),
        })
    }
}
