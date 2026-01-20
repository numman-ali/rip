use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

mod patch;

pub use patch::{Patch, PatchHunk, PatchOp, PatchParseError};

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

    pub fn apply_patch(&self, patch: &str) -> io::Result<PatchApplyResult> {
        let patch = Patch::parse(patch)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;

        let mut seen = BTreeSet::new();
        let mut undo: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
        let mut changed_files: Vec<String> = Vec::new();

        let mut record_undo = |path: &PathBuf| -> io::Result<()> {
            if !seen.insert(path.clone()) {
                return Ok(());
            }
            let previous = if path.exists() {
                Some(fs::read(path)?)
            } else {
                None
            };
            undo.push((path.clone(), previous));
            Ok(())
        };

        let apply_result = (|| -> io::Result<()> {
            for op in patch.ops() {
                match op {
                    PatchOp::AddFile { path, content } => {
                        let dest = self.safe_join(path)?;
                        if dest.exists() {
                            return Err(io::Error::new(
                                io::ErrorKind::AlreadyExists,
                                format!("file already exists: {}", path.display()),
                            ));
                        }
                        record_undo(&dest)?;
                        if let Some(parent) = dest.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::write(&dest, content.as_bytes())?;
                        changed_files.push(normalize_rel(path));
                    }
                    PatchOp::DeleteFile { path } => {
                        let dest = self.safe_join(path)?;
                        if !dest.exists() {
                            return Err(io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("file not found: {}", path.display()),
                            ));
                        }
                        record_undo(&dest)?;
                        fs::remove_file(&dest)?;
                        changed_files.push(normalize_rel(path));
                    }
                    PatchOp::UpdateFile {
                        path,
                        moved_to,
                        hunks,
                    } => {
                        let dest = self.safe_join(path)?;
                        if !dest.exists() {
                            return Err(io::Error::new(
                                io::ErrorKind::NotFound,
                                format!("file not found: {}", path.display()),
                            ));
                        }
                        record_undo(&dest)?;
                        let bytes = fs::read(&dest)?;
                        let original_text = String::from_utf8(bytes).map_err(|err| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("file is not valid UTF-8: {err}"),
                            )
                        })?;
                        let updated = patch::apply_hunks_to_text(&original_text, hunks, path)?;
                        fs::write(&dest, updated.as_bytes())?;
                        changed_files.push(normalize_rel(path));

                        if let Some(moved_to) = moved_to {
                            let target = self.safe_join(moved_to)?;
                            if target.exists() {
                                return Err(io::Error::new(
                                    io::ErrorKind::AlreadyExists,
                                    format!("move target already exists: {}", moved_to.display()),
                                ));
                            }
                            record_undo(&target)?;
                            if let Some(parent) = target.parent() {
                                fs::create_dir_all(parent)?;
                            }
                            fs::rename(&dest, &target)?;
                            changed_files.push(normalize_rel(moved_to));
                        }
                    }
                }
            }
            Ok(())
        })();

        if let Err(err) = apply_result {
            let _ = self.revert_paths(undo);
            return Err(err);
        }

        changed_files.sort();
        changed_files.dedup();
        Ok(PatchApplyResult { changed_files })
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
        let checkpoint_root = self.checkpoints_dir.join(session_id).join(&checkpoint_id);
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
        let checkpoint_root = self.checkpoints_dir.join(session_id).join(checkpoint_id);
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

    fn safe_join(&self, rel: &Path) -> io::Result<PathBuf> {
        if rel.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "absolute paths are not allowed",
            ));
        }
        if rel
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path escapes workspace root",
            ));
        }
        Ok(self.root.join(rel))
    }

    fn revert_paths(&self, undo: Vec<(PathBuf, Option<Vec<u8>>)>) -> io::Result<()> {
        for (path, previous) in undo.into_iter().rev() {
            match previous {
                Some(bytes) => {
                    if let Some(parent) = path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    let _ = fs::write(path, bytes);
                }
                None => {
                    let _ = fs::remove_file(path);
                }
            }
        }
        Ok(())
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

fn normalize_rel(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchApplyResult {
    pub changed_files: Vec<String>,
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
    fn apply_patch_creates_updates_and_deletes() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");

        let patch = r#"*** Begin Patch
*** Add File: a.txt
+one
+two
*** End Patch"#;
        let result = workspace.apply_patch(patch).expect("apply");
        assert!(result.changed_files.contains(&"a.txt".to_string()));
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "one\ntwo\n"
        );

        let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-one
+ONE
 two
*** End Patch"#;
        let _ = workspace.apply_patch(patch).expect("apply");
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).unwrap(),
            "ONE\ntwo\n"
        );

        let patch = r#"*** Begin Patch
*** Delete File: a.txt
*** End Patch"#;
        let _ = workspace.apply_patch(patch).expect("apply");
        assert!(!root.join("a.txt").exists());
    }

    #[test]
    fn apply_patch_is_atomic_on_error() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");

        let patch = r#"*** Begin Patch
*** Add File: a.txt
+one
*** Update File: missing.txt
@@
-nope
+ok
*** End Patch"#;
        let _ = workspace.apply_patch(patch).expect_err("error");
        assert!(!root.join("a.txt").exists());
    }

    #[test]
    fn create_checkpoint_accepts_string_label() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let file_a = root.join("a.txt");
        fs::write(&file_a, b"one").expect("write");

        let checkpoint = workspace
            .create_checkpoint("s1", "label".to_string(), std::slice::from_ref(&file_a))
            .expect("checkpoint");
        assert_eq!(checkpoint.label, "label");
    }

    #[test]
    fn list_checkpoints_sorted() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let file_a = root.join("a.txt");
        fs::write(&file_a, b"one").expect("write");

        let cp1 = workspace
            .create_checkpoint("s1", "first", std::slice::from_ref(&file_a))
            .expect("checkpoint");
        let cp2 = workspace
            .create_checkpoint("s1", "second", std::slice::from_ref(&file_a))
            .expect("checkpoint");

        let list = workspace.list_checkpoints("s1").expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, cp1.id);
        assert_eq!(list[1].id, cp2.id);
    }

    #[test]
    fn list_checkpoints_empty_session() {
        let dir = tempdir().expect("tmp");
        let workspace = Workspace::new(dir.path()).expect("workspace");
        let list = workspace.list_checkpoints("missing").expect("list");
        assert!(list.is_empty());
    }

    #[test]
    fn create_checkpoint_records_missing_file() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let missing = root.join("missing.txt");

        let checkpoint = workspace
            .create_checkpoint("s1", "missing", std::slice::from_ref(&missing))
            .expect("checkpoint");

        assert_eq!(checkpoint.files.len(), 1);
        assert!(!checkpoint.files[0].exists);
    }

    #[test]
    fn create_checkpoint_rejects_outside_paths() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let outside = root.parent().unwrap().join("outside.txt");
        let err = workspace
            .create_checkpoint("s1", "outside", std::slice::from_ref(&outside))
            .expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn rewind_missing_checkpoint_errors() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let err = workspace
            .rewind_to_checkpoint("s1", "missing")
            .expect_err("err");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn rewind_failure_rolls_back() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let file_a = root.join("a.txt");
        fs::write(&file_a, b"one").expect("write");

        let checkpoint = workspace
            .create_checkpoint("s1", "initial", std::slice::from_ref(&file_a))
            .expect("checkpoint");

        fs::write(&file_a, b"two").expect("write");

        let checkpoint_file = root
            .join(".rip")
            .join("checkpoints")
            .join("s1")
            .join(&checkpoint.id)
            .join("files")
            .join("a.txt");
        fs::remove_file(&checkpoint_file).expect("remove");

        let err = workspace
            .rewind_to_checkpoint("s1", &checkpoint.id)
            .expect_err("rewind");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert_eq!(fs::read_to_string(&file_a).unwrap(), "two");
    }

    #[test]
    fn list_checkpoints_invalid_metadata_errors() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let session_dir = root.join(".rip").join("checkpoints").join("s1");
        fs::create_dir_all(&session_dir).expect("dir");
        let bad = session_dir.join("bad.json");
        fs::write(&bad, "{not json}").expect("write");
        let entry_dir = session_dir.join("bad-checkpoint");
        fs::create_dir_all(&entry_dir).expect("dir");
        fs::rename(&bad, entry_dir.join("checkpoint.json")).expect("move");

        let err = workspace.list_checkpoints("s1").expect_err("err");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn apply_patch_rejects_existing_add() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let file = root.join("a.txt");
        fs::write(&file, b"one").expect("write");

        let patch = r#"*** Begin Patch
*** Add File: a.txt
+two
*** End Patch"#;
        let err = workspace.apply_patch(patch).expect_err("err");
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&file).unwrap(), "one");
    }

    #[test]
    fn apply_patch_rejects_delete_missing_file() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let patch = r#"*** Begin Patch
*** Delete File: missing.txt
*** End Patch"#;
        let err = workspace.apply_patch(patch).expect_err("err");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn apply_patch_rejects_invalid_utf8() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let file = root.join("a.txt");
        fs::write(&file, vec![0xff, 0xfe]).expect("write");

        let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-bad
+good
*** End Patch"#;
        let err = workspace.apply_patch(patch).expect_err("err");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn apply_patch_rejects_move_target_exists() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let workspace = Workspace::new(root).expect("workspace");
        let file = root.join("a.txt");
        let target = root.join("b.txt");
        fs::write(&file, b"one").expect("write");
        fs::write(&target, b"two").expect("write");

        let patch = r#"*** Begin Patch
*** Update File: a.txt
*** Move to: b.txt
@@
-one
+one
*** End Patch"#;
        let err = workspace.apply_patch(patch).expect_err("err");
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn safe_join_rejects_absolute_and_parent_paths() {
        let dir = tempdir().expect("tmp");
        let workspace = Workspace::new(dir.path()).expect("workspace");
        let err = workspace.safe_join(Path::new("/abs.txt")).expect_err("err");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

        let err = workspace
            .safe_join(Path::new("../escape.txt"))
            .expect_err("err");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn normalize_rel_converts_backslashes() {
        let path = Path::new("a\\b");
        assert_eq!(normalize_rel(path), "a/b");
    }
}
