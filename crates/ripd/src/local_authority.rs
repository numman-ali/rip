use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const AUTHORITY_DIR: &str = "authority";
const LOCK_FILE: &str = "lock.json";
const META_FILE: &str = "meta.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityLockRecord {
    pub pid: u32,
    pub started_at_ms: u64,
    pub workspace_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityMeta {
    pub endpoint: String,
    pub pid: u32,
    pub started_at_ms: u64,
    pub workspace_root: String,
}

pub fn authority_dir(data_dir: impl AsRef<Path>) -> PathBuf {
    data_dir.as_ref().join(AUTHORITY_DIR)
}

pub fn authority_lock_path(data_dir: impl AsRef<Path>) -> PathBuf {
    authority_dir(data_dir).join(LOCK_FILE)
}

pub fn authority_meta_path(data_dir: impl AsRef<Path>) -> PathBuf {
    authority_dir(data_dir).join(META_FILE)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub struct AuthorityLockGuard {
    lock_path: PathBuf,
    meta_path: PathBuf,
}

impl AuthorityLockGuard {
    pub fn try_acquire(
        data_dir: impl AsRef<Path>,
        workspace_root: impl AsRef<Path>,
    ) -> Result<Self, String> {
        let dir = authority_dir(&data_dir);
        fs::create_dir_all(&dir)
            .map_err(|err| format!("create authority dir {} failed: {err}", dir.display()))?;

        let lock_path = authority_lock_path(&data_dir);
        let meta_path = authority_meta_path(&data_dir);

        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path)
            .map_err(|err| {
                format!(
                    "store already has an authority (lock at {}): {err}",
                    lock_path.display()
                )
            })?;

        let record = AuthorityLockRecord {
            pid: std::process::id(),
            started_at_ms: now_ms(),
            workspace_root: workspace_root.as_ref().to_string_lossy().to_string(),
        };
        let json =
            serde_json::to_vec(&record).map_err(|err| format!("lock record json failed: {err}"))?;
        file.write_all(&json)
            .and_then(|()| file.write_all(b"\n"))
            .map_err(|err| format!("write lock record failed: {err}"))?;
        let _ = file.flush();

        Ok(Self {
            lock_path,
            meta_path,
        })
    }

    pub fn write_meta(&self, meta: &AuthorityMeta) -> Result<(), String> {
        let payload = serde_json::to_vec(meta).map_err(|err| format!("meta json failed: {err}"))?;
        atomic_write_file(&self.meta_path, &payload)
            .map_err(|err| format!("write meta failed: {err}"))
    }
}

impl Drop for AuthorityLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.meta_path);
        let _ = fs::remove_file(&self.lock_path);
    }
}

pub fn read_authority_meta(data_dir: impl AsRef<Path>) -> Result<Option<AuthorityMeta>, String> {
    let path = authority_meta_path(data_dir);
    let Ok(contents) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let meta: AuthorityMeta = serde_json::from_str(&contents)
        .map_err(|err| format!("meta json invalid at {}: {err}", path.display()))?;
    Ok(Some(meta))
}

pub fn read_authority_lock_record(
    data_dir: impl AsRef<Path>,
) -> Result<Option<AuthorityLockRecord>, String> {
    let path = authority_lock_path(data_dir);
    let Ok(contents) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    let record: AuthorityLockRecord = serde_json::from_str(&contents)
        .map_err(|err| format!("lock json invalid at {}: {err}", path.display()))?;
    Ok(Some(record))
}

fn atomic_write_file(path: &Path, payload: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, payload)?;
    let _ = fs::remove_file(path);
    fs::rename(tmp, path)?;
    Ok(())
}
