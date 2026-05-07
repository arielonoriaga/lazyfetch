use crate::atomic::write_atomic;
use lazyfetch_core::auth::AuthSpec;
use lazyfetch_core::catalog::{Collection, Folder, Item, Request};
use lazyfetch_core::primitives::KV;
use std::path::{Path, PathBuf};

pub struct FsCollectionRepo {
    root: PathBuf,
}

impl FsCollectionRepo {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn slug(s: &str) -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    }

    pub fn save(&self, c: &Collection) -> std::io::Result<()> {
        let dir = self.root.join(Self::slug(&c.name));
        std::fs::create_dir_all(dir.join("requests"))?;
        let header = serde_yaml::to_string(&CollectionHeader {
            id: c.id,
            name: c.name.clone(),
            auth: c.auth.clone(),
            vars: c.vars.clone(),
        })
        .map_err(io_err)?;
        write_atomic(&dir.join("collection.yaml"), header.as_bytes())?;
        Self::save_folder(&dir.join("requests"), &c.root)?;
        Ok(())
    }

    fn save_folder(dir: &Path, f: &Folder) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let meta = serde_yaml::to_string(&FolderHeader {
            id: f.id,
            name: f.name.clone(),
            auth: f.auth.clone(),
        })
        .map_err(io_err)?;
        write_atomic(&dir.join("_folder.yaml"), meta.as_bytes())?;
        for item in &f.items {
            match item {
                Item::Folder(sub) => Self::save_folder(&dir.join(Self::slug(&sub.name)), sub)?,
                Item::Request(r) => {
                    let y = serde_yaml::to_string(r).map_err(io_err)?;
                    write_atomic(
                        &dir.join(format!("{}.yaml", Self::slug(&r.name))),
                        y.as_bytes(),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Drop a single Request into `<root>/<coll>/requests/<name>.yaml`. Creates the
    /// collection scaffold (`collection.yaml`, `requests/_folder.yaml`) if absent.
    pub fn save_request(&self, coll_name: &str, req: &Request) -> std::io::Result<()> {
        let coll_dir = self.root.join(Self::slug(coll_name));
        let req_dir = coll_dir.join("requests");
        std::fs::create_dir_all(&req_dir)?;

        let coll_yaml = coll_dir.join("collection.yaml");
        if !coll_yaml.exists() {
            let header = serde_yaml::to_string(&CollectionHeader {
                id: ulid::Ulid::new(),
                name: coll_name.to_string(),
                auth: None,
                vars: vec![],
            })
            .map_err(io_err)?;
            crate::atomic::write_atomic(&coll_yaml, header.as_bytes())?;
        }
        let folder_yaml = req_dir.join("_folder.yaml");
        if !folder_yaml.exists() {
            let meta = serde_yaml::to_string(&FolderHeader {
                id: ulid::Ulid::new(),
                name: "root".into(),
                auth: None,
            })
            .map_err(io_err)?;
            crate::atomic::write_atomic(&folder_yaml, meta.as_bytes())?;
        }
        let path = req_dir.join(format!("{}.yaml", Self::slug(&req.name)));
        let yaml = serde_yaml::to_string(req).map_err(io_err)?;
        crate::atomic::write_atomic(&path, yaml.as_bytes())
    }

    pub fn load_by_name(&self, name: &str) -> std::io::Result<Collection> {
        let dir = self.root.join(Self::slug(name));
        let header: CollectionHeader =
            serde_yaml::from_str(&std::fs::read_to_string(dir.join("collection.yaml"))?)
                .map_err(io_err)?;
        let root = Self::load_folder(&dir.join("requests"))?;
        Ok(Collection {
            id: header.id,
            name: header.name,
            root,
            auth: header.auth,
            vars: header.vars,
        })
    }

    fn load_folder(dir: &Path) -> std::io::Result<Folder> {
        let meta_path = dir.join("_folder.yaml");
        let header: FolderHeader = if meta_path.exists() {
            serde_yaml::from_str(&std::fs::read_to_string(&meta_path)?).map_err(io_err)?
        } else {
            FolderHeader {
                id: ulid::Ulid::new(),
                name: dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                auth: None,
            }
        };
        let mut items = vec![];
        for ent in std::fs::read_dir(dir)? {
            let e = ent?;
            let p = e.path();
            if e.file_type()?.is_dir() {
                items.push(Item::Folder(Self::load_folder(&p)?));
            } else if p.file_name().unwrap_or_default() != "_folder.yaml" {
                let r: Request =
                    serde_yaml::from_str(&std::fs::read_to_string(&p)?).map_err(io_err)?;
                items.push(Item::Request(r));
            }
        }
        Ok(Folder {
            id: header.id,
            name: header.name,
            items,
            auth: header.auth,
        })
    }
}

fn io_err<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CollectionHeader {
    id: ulid::Ulid,
    name: String,
    auth: Option<AuthSpec>,
    vars: Vec<KV>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FolderHeader {
    id: ulid::Ulid,
    name: String,
    auth: Option<AuthSpec>,
}
