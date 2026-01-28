use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const AUTHORITY_DIR: &str = "authority";
const LOCK_FILE: &str = "lock.json";
const META_FILE: &str = "meta.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidLiveness {
    Alive,
    Dead,
    Unknown,
}

pub fn pid_liveness(pid: u32) -> PidLiveness {
    #[cfg(unix)]
    {
        use std::os::raw::c_int;

        extern "C" {
            fn kill(pid: i32, sig: c_int) -> c_int;
        }

        const SIGNAL_0: c_int = 0;
        const EPERM: i32 = 1;
        const ESRCH: i32 = 3;

        let result = unsafe { kill(pid as i32, SIGNAL_0) };
        if result == 0 {
            return PidLiveness::Alive;
        }

        match std::io::Error::last_os_error().raw_os_error() {
            Some(ESRCH) => PidLiveness::Dead,
            Some(EPERM) => PidLiveness::Alive,
            _ => PidLiveness::Unknown,
        }
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        PidLiveness::Unknown
    }
}

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
    record: AuthorityLockRecord,
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
            record,
        })
    }

    pub fn record(&self) -> &AuthorityLockRecord {
        &self.record
    }

    pub fn write_meta(&self, endpoint: impl Into<String>) -> Result<(), String> {
        let meta = AuthorityMeta {
            endpoint: endpoint.into(),
            pid: self.record.pid,
            started_at_ms: self.record.started_at_ms,
            workspace_root: self.record.workspace_root.clone(),
        };
        let payload =
            serde_json::to_vec(&meta).map_err(|err| format!("meta json failed: {err}"))?;
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

pub fn try_cleanup_stale_authority_files(
    data_dir: impl AsRef<Path>,
    expected_pid: u32,
    expected_started_at_ms: u64,
) -> Result<bool, String> {
    let lock_path = authority_lock_path(&data_dir);
    let meta_path = authority_meta_path(&data_dir);

    if !lock_path.exists() {
        return Ok(false);
    }

    let lock = match read_authority_lock_record(&data_dir) {
        Ok(Some(lock)) => lock,
        Ok(None) => return Ok(false),
        Err(_) => return Ok(false),
    };
    // We intentionally only key cleanup on PID. Older RIP versions wrote different started_at_ms
    // values into lock.json vs meta.json (same PID), which can wedge local-first startup forever.
    //
    // Safety: callers only invoke cleanup after verifying the PID is dead and the endpoint is
    // unreachable, so accepting started_at_ms drift does not risk deleting a live authority.
    if lock.pid != expected_pid {
        return Ok(false);
    }

    let lock_tombstone = lock_path.with_file_name(format!(
        "{}.stale-{}-{}-{}",
        lock_path.file_name().unwrap_or_default().to_string_lossy(),
        expected_pid,
        // Keep started_at in the tombstone name for debugging even when it doesn't match.
        expected_started_at_ms,
        now_ms()
    ));
    match fs::rename(&lock_path, &lock_tombstone) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(format!("rename stale lock failed: {err}")),
    }

    if let Ok(Some(meta)) = read_authority_meta(&data_dir) {
        if meta.pid == expected_pid {
            let meta_tombstone = meta_path.with_file_name(format!(
                "{}.stale-{}-{}-{}",
                meta_path.file_name().unwrap_or_default().to_string_lossy(),
                expected_pid,
                expected_started_at_ms,
                now_ms()
            ));
            if fs::rename(&meta_path, &meta_tombstone).is_ok() {
                let _ = fs::remove_file(meta_tombstone);
            }
        }
    }

    let _ = fs::remove_file(lock_tombstone);
    Ok(true)
}

pub fn try_cleanup_corrupt_lock_file(data_dir: impl AsRef<Path>) -> Result<bool, String> {
    let lock_path = authority_lock_path(&data_dir);
    if !lock_path.exists() {
        return Ok(false);
    }

    if authority_meta_path(&data_dir).exists() {
        return Ok(false);
    }

    let tombstone = lock_path.with_file_name(format!(
        "{}.corrupt-{}-{}",
        lock_path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        now_ms()
    ));
    match fs::rename(&lock_path, &tombstone) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(format!("rename corrupt lock failed: {err}")),
    }

    let _ = fs::remove_file(tombstone);
    Ok(true)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cleanup_stale_authority_files_tolerates_started_at_mismatch() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(authority_dir(&data_dir)).expect("authority dir");

        let pid = 1234u32;
        let lock_started_at_ms = 1_000u64;
        let meta_started_at_ms = 1_021u64;

        let lock = AuthorityLockRecord {
            pid,
            started_at_ms: lock_started_at_ms,
            workspace_root: "/workspace".to_string(),
        };
        let meta = AuthorityMeta {
            endpoint: "http://127.0.0.1:12345".to_string(),
            pid,
            started_at_ms: meta_started_at_ms,
            workspace_root: "/workspace".to_string(),
        };

        std::fs::write(
            authority_lock_path(&data_dir),
            format!("{}\n", serde_json::to_string(&lock).unwrap()),
        )
        .expect("write lock");
        std::fs::write(
            authority_meta_path(&data_dir),
            format!("{}\n", serde_json::to_string(&meta).unwrap()),
        )
        .expect("write meta");

        let cleaned =
            try_cleanup_stale_authority_files(&data_dir, pid, meta_started_at_ms).expect("cleanup");
        assert!(cleaned, "expected stale cleanup to succeed");
        assert!(
            !authority_lock_path(&data_dir).exists(),
            "lock should be removed"
        );
        assert!(
            !authority_meta_path(&data_dir).exists(),
            "meta should be removed"
        );
    }

    #[test]
    fn cleanup_stale_authority_files_requires_pid_match() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(authority_dir(&data_dir)).expect("authority dir");

        let lock = AuthorityLockRecord {
            pid: 1234,
            started_at_ms: 1_000,
            workspace_root: "/workspace".to_string(),
        };
        std::fs::write(
            authority_lock_path(&data_dir),
            format!("{}\n", serde_json::to_string(&lock).unwrap()),
        )
        .expect("write lock");

        let cleaned = try_cleanup_stale_authority_files(&data_dir, 9999, 1_000).expect("cleanup");
        assert!(!cleaned, "cleanup should not run for different pid");
        assert!(
            authority_lock_path(&data_dir).exists(),
            "lock should remain"
        );
    }
}
