use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{fs, io};

const INDEX_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ContinuityIndexV1 {
    version: u32,
    pub(super) workspaces: HashMap<String, String>,
    pub(super) continuities: HashMap<String, ContinuityMetaV1>,
}

impl Default for ContinuityIndexV1 {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            workspaces: HashMap::new(),
            continuities: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ContinuityMetaV1 {
    pub(super) created_at_ms: u64,
    pub(super) title: Option<String>,
    pub(super) archived: bool,
}

pub(super) fn index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("continuities").join("index.json")
}

pub(super) fn load_index(path: &Path) -> io::Result<ContinuityIndexV1> {
    let bytes = fs::read(path)?;
    let parsed: ContinuityIndexV1 = serde_json::from_slice(&bytes)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if parsed.version != INDEX_VERSION {
        return Ok(ContinuityIndexV1::default());
    }
    Ok(parsed)
}

pub(super) fn save_index(path: &Path, index: &ContinuityIndexV1) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(index)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, payload)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub(super) fn workspace_key(workspace_root: &Path) -> String {
    workspace_root.to_string_lossy().to_string()
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
