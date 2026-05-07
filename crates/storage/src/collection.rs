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
    /// Detects slug collisions: if a different request whose name slugs to the same
    /// filename already lives there, the save is rejected so neither row is silently
    /// overwritten.
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
        if path.exists() {
            // Same path may belong to the same request (overwrite ok) or to a
            // different request whose name slugs identically (collision — refuse).
            if let Ok(existing) = serde_yaml::from_str::<Request>(&std::fs::read_to_string(&path)?)
            {
                if existing.id != req.id && existing.name != req.name {
                    return Err(std::io::Error::other(format!(
                        "name collision: '{}' and '{}' map to the same file slug '{}'",
                        existing.name,
                        req.name,
                        Self::slug(&req.name)
                    )));
                }
            }
        }
        let yaml = serde_yaml::to_string(req).map_err(io_err)?;
        crate::atomic::write_atomic(&path, yaml.as_bytes())
    }

    /// Rename a collection on disk: rewrite `collection.yaml.name` first (atomic, in-place
    /// on the old dir), then rename the dir. If the rewrite fails, the dir is untouched and
    /// the operation is a no-op. If the dir-rename fails, the YAML still has the new name
    /// but the dir name is the old slug — re-runnable, never silently corrupt.
    pub fn rename_collection(&self, old: &str, new: &str) -> std::io::Result<()> {
        if old == new {
            return Ok(());
        }
        let old_dir = self.root.join(Self::slug(old));
        let new_dir = self.root.join(Self::slug(new));
        if !old_dir.is_dir() {
            return Err(std::io::Error::other(format!(
                "collection not found: {}",
                old
            )));
        }
        if new_dir.exists() {
            return Err(std::io::Error::other(format!(
                "collection already exists: {}",
                new
            )));
        }
        let yaml_path = old_dir.join("collection.yaml");
        if yaml_path.exists() {
            let mut header: CollectionHeader =
                serde_yaml::from_str(&std::fs::read_to_string(&yaml_path)?).map_err(io_err)?;
            header.name = new.to_string();
            let s = serde_yaml::to_string(&header).map_err(io_err)?;
            crate::atomic::write_atomic(&yaml_path, s.as_bytes())?;
        }
        std::fs::rename(&old_dir, &new_dir)?;
        Ok(())
    }

    /// Move a request from one collection to another. Creates the destination collection
    /// scaffold if needed. Refuses to overwrite an existing target file. Best-effort
    /// cleanup: if target write succeeds but source remove fails, the duplicate is left
    /// in place and the error is returned so the caller can surface it.
    pub fn move_request(&self, from_coll: &str, name: &str, to_coll: &str) -> std::io::Result<()> {
        if from_coll == to_coll {
            return Ok(());
        }
        let from_dir = self.root.join(Self::slug(from_coll)).join("requests");
        let from_path = from_dir.join(format!("{}.yaml", Self::slug(name)));
        if !from_path.is_file() {
            return Err(std::io::Error::other(format!(
                "request not found: {}/{}",
                from_coll, name
            )));
        }
        let to_dir = self.root.join(Self::slug(to_coll)).join("requests");
        let to_path = to_dir.join(format!("{}.yaml", Self::slug(name)));
        if to_path.exists() {
            return Err(std::io::Error::other(format!(
                "target already exists: {}/{}",
                to_coll, name
            )));
        }
        let req: Request =
            serde_yaml::from_str(&std::fs::read_to_string(&from_path)?).map_err(io_err)?;
        self.save_request(to_coll, &req)?;
        if let Err(e) = std::fs::remove_file(&from_path) {
            tracing::error!(
                target: "lazyfetch::storage",
                error = %e,
                ?from_path,
                "move_request: target written but source remove failed; duplicate left in place"
            );
            return Err(e);
        }
        Ok(())
    }

    /// Rename a request inside a collection.
    pub fn rename_request(&self, coll_name: &str, old: &str, new: &str) -> std::io::Result<()> {
        if old == new {
            return Ok(());
        }
        let coll_dir = self.root.join(Self::slug(coll_name));
        let req_dir = coll_dir.join("requests");
        let old_path = req_dir.join(format!("{}.yaml", Self::slug(old)));
        let new_path = req_dir.join(format!("{}.yaml", Self::slug(new)));
        if !old_path.is_file() {
            return Err(std::io::Error::other(format!("request not found: {}", old)));
        }
        if new_path.exists() {
            return Err(std::io::Error::other(format!("request exists: {}", new)));
        }
        let mut req: Request =
            serde_yaml::from_str(&std::fs::read_to_string(&old_path)?).map_err(io_err)?;
        req.name = new.to_string();
        let yaml = serde_yaml::to_string(&req).map_err(io_err)?;
        crate::atomic::write_atomic(&new_path, yaml.as_bytes())?;
        std::fs::remove_file(&old_path)?;
        Ok(())
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
