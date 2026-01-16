use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub session_id: String,
    pub label: String,
    pub created_at_ms: u64,
    pub files: Vec<CheckpointFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointFile {
    pub path: String,
    pub exists: bool,
    pub sha256: Option<String>,
}

pub struct Workspace {
    root: PathBuf,
    checkpoints_dir: PathBuf,
}

impl Workspace {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let checkpoints_dir = root.join(".rip").join("checkpoints");
        fs::create_dir_all(&checkpoints_dir)?;
        Ok(Self {
            root,
            checkpoints_dir,
        })
    }

    pub fn create_checkpoint(
        &self,
        session_id: &str,
        label: impl Into<String>,
        files: &[PathBuf],
    ) -> io::Result<Checkpoint> {
        let checkpoint_id = Uuid::new_v4().to_string();
        let label = label.into();
        let created_at_ms = now_ms();
        let checkpoint_root = self
            .checkpoints_dir
            .join(session_id)
            .join(&checkpoint_id);
        let files_root = checkpoint_root.join("files");
        fs::create_dir_all(&files_root)?;

        let mut entries = Vec::new();

        for path in files {
            let rel = self.to_relative(path)?;
            let dest = files_root.join(&rel);

            if path.exists() {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                let bytes = fs::read(path)?;
                let hash = hash_bytes(&bytes);
                fs::write(&dest, &bytes)?;
                entries.push(CheckpointFile {
                    path: rel.to_string_lossy().to_string(),
                    exists: true,
                    sha256: Some(hash),
                });
            } else {
                entries.push(CheckpointFile {
                    path: rel.to_string_lossy().to_string(),
                    exists: false,
                    sha256: None,
                });
            }
        }

        let checkpoint = Checkpoint {
            id: checkpoint_id,
            session_id: session_id.to_string(),
            label,
            created_at_ms,
            files: entries,
        };

        let metadata_path = checkpoint_root.join("checkpoint.json");
        let payload = serde_json::to_vec_pretty(&checkpoint)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        fs::write(metadata_path, payload)?;

        Ok(checkpoint)
    }

    pub fn list_checkpoints(&self, session_id: &str) -> io::Result<Vec<Checkpoint>> {
        let session_dir = self.checkpoints_dir.join(session_id);
        if !session_dir.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        for entry in fs::read_dir(session_dir)? {
            let entry = entry?;
            let path = entry.path().join("checkpoint.json");
            if path.exists() {
                let payload = fs::read(&path)?;
                let checkpoint: Checkpoint = serde_json::from_slice(&payload)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                checkpoints.push(checkpoint);
            }
        }

        checkpoints.sort_by_key(|checkpoint| checkpoint.created_at_ms);
        Ok(checkpoints)
    }

    pub fn rewind_to_checkpoint(&self, session_id: &str, checkpoint_id: &str) -> io::Result<()> {
        let checkpoint_root = self
            .checkpoints_dir
            .join(session_id)
            .join(checkpoint_id);
        let metadata_path = checkpoint_root.join("checkpoint.json");
        let payload = fs::read(&metadata_path)?;
        let checkpoint: Checkpoint = serde_json::from_slice(&payload)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

        let mut undo = BTreeMap::new();

        for file in &checkpoint.files {
            let target_path = self.root.join(&file.path);
            if target_path.exists() {
                let bytes = fs::read(&target_path)?;
                undo.insert(file.path.clone(), Some(bytes));
            } else {
                undo.insert(file.path.clone(), None);
            }
        }

        let apply_result = (|| -> io::Result<()> {
            for file in &checkpoint.files {
                let target_path = self.root.join(&file.path);
                if file.exists {
                    let source_path = checkpoint_root.join("files").join(&file.path);
                    let bytes = fs::read(&source_path)?;
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&target_path, &bytes)?;
                } else if target_path.exists() {
                    fs::remove_file(&target_path)?;
                }
            }
            Ok(())
        })();

        if let Err(err) = apply_result {
            for (rel, previous) in undo {
                let path = self.root.join(rel);
                match previous {
                    Some(bytes) => {
                        if let Some(parent) = path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::write(&path, bytes);
                    }
                    None => {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
            return Err(err);
        }

        Ok(())
    }

    fn to_relative(&self, path: &Path) -> io::Result<PathBuf> {
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        abs.strip_prefix(&self.root)
            .map(|p| p.to_path_buf())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path outside workspace"))
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_and_rewind_checkpoint() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");

        let file_a = root.join("a.txt");
        fs::write(&file_a, b"one").expect("write");

        let checkpoint = workspace
            .create_checkpoint("s1", "initial", &[file_a.clone(), root.join("b.txt")])
            .expect("checkpoint");

        fs::write(&file_a, b"two").expect("write");
        let file_b = root.join("b.txt");
        fs::write(&file_b, b"new").expect("write");

        workspace
            .rewind_to_checkpoint("s1", &checkpoint.id)
            .expect("rewind");

        assert_eq!(fs::read_to_string(&file_a).unwrap(), "one");
        assert!(!file_b.exists());
    }

    #[test]
    fn list_checkpoints_sorted() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let file_a = root.join("a.txt");
        fs::write(&file_a, b"one").expect("write");

        let cp1 = workspace
            .create_checkpoint("s1", "first", &[file_a.clone()])
            .expect("checkpoint");
        let cp2 = workspace
            .create_checkpoint("s1", "second", &[file_a.clone()])
            .expect("checkpoint");

        let list = workspace.list_checkpoints("s1").expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, cp1.id);
        assert_eq!(list[1].id, cp2.id);
    }
}
