use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rip_kernel::{Event, EventKind, StreamKind};
use rip_log::EventLog;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

const INDEX_VERSION: u32 = 1;
const EVENT_CHANNEL_CAPACITY: usize = 16_384;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuityMeta {
    pub continuity_id: String,
    pub created_at_ms: u64,
    pub title: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContinuityIndexV1 {
    version: u32,
    /// workspace key -> default continuity id
    workspaces: HashMap<String, String>,
    continuities: HashMap<String, ContinuityMetaV1>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContinuityMetaV1 {
    created_at_ms: u64,
    title: Option<String>,
    archived: bool,
}

pub struct ContinuityStore {
    data_dir: PathBuf,
    workspace_root: PathBuf,
    event_log: Arc<EventLog>,
    sender: broadcast::Sender<Event>,
    index: Mutex<ContinuityIndexV1>,
    next_seq: Mutex<HashMap<String, u64>>,
}

impl ContinuityStore {
    pub fn new(
        data_dir: PathBuf,
        workspace_root: PathBuf,
        event_log: Arc<EventLog>,
    ) -> Result<Self, String> {
        let index = load_index(&index_path(&data_dir)).unwrap_or_default();
        let (sender, _receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Ok(Self {
            data_dir,
            workspace_root,
            event_log,
            sender,
            index: Mutex::new(index),
            next_seq: Mutex::new(HashMap::new()),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub fn replay_events(&self, continuity_id: &str) -> io::Result<Vec<Event>> {
        self.event_log
            .replay_stream(StreamKind::Continuity, continuity_id)
    }

    pub fn ensure_default(&self) -> Result<String, String> {
        let workspace = workspace_key(&self.workspace_root);

        if let Some(existing) = self
            .index
            .lock()
            .expect("continuity index mutex")
            .workspaces
            .get(&workspace)
            .cloned()
        {
            return Ok(existing);
        }

        if let Some(existing) = self
            .find_latest_continuity_for_workspace(&workspace)
            .map_err(|err| format!("continuity log scan failed: {err}"))?
        {
            // Backfill the cache index so future calls are O(1).
            {
                let mut index = self.index.lock().expect("continuity index mutex");
                index.workspaces.insert(workspace.clone(), existing.clone());
                let _ = save_index(&index_path(&self.data_dir), &index);
            }
            return Ok(existing);
        }

        let continuity_id = Uuid::new_v4().to_string();
        let timestamp_ms = now_ms();
        let created = Event {
            id: Uuid::new_v4().to_string(),
            session_id: continuity_id.clone(),
            timestamp_ms,
            seq: 0,
            kind: EventKind::ContinuityCreated {
                workspace: workspace.clone(),
                title: None,
            },
        };
        self.event_log
            .append(&created)
            .map_err(|err| format!("append continuity_created: {err}"))?;
        let _ = self.sender.send(created.clone());

        {
            let mut index = self.index.lock().expect("continuity index mutex");
            index.workspaces.insert(workspace, continuity_id.clone());
            index.continuities.insert(
                continuity_id.clone(),
                ContinuityMetaV1 {
                    created_at_ms: timestamp_ms,
                    title: None,
                    archived: false,
                },
            );
            save_index(&index_path(&self.data_dir), &index)
                .map_err(|err| format!("save continuity index: {err}"))?;
        }

        self.next_seq
            .lock()
            .expect("continuity seq mutex")
            .insert(continuity_id.clone(), 1);

        Ok(continuity_id)
    }

    pub fn list(&self) -> Vec<ContinuityMeta> {
        let index = self.index.lock().expect("continuity index mutex");
        index
            .continuities
            .iter()
            .map(|(id, meta)| ContinuityMeta {
                continuity_id: id.clone(),
                created_at_ms: meta.created_at_ms,
                title: meta.title.clone(),
                archived: meta.archived,
            })
            .collect()
    }

    pub fn get(&self, continuity_id: &str) -> Option<ContinuityMeta> {
        let index = self.index.lock().expect("continuity index mutex");
        let meta = index.continuities.get(continuity_id)?;
        Some(ContinuityMeta {
            continuity_id: continuity_id.to_string(),
            created_at_ms: meta.created_at_ms,
            title: meta.title.clone(),
            archived: meta.archived,
        })
    }

    pub fn append_message(
        &self,
        continuity_id: &str,
        actor_id: String,
        origin: String,
        content: String,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let message_id = Uuid::new_v4().to_string();
        let event = Event {
            id: message_id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityMessageAppended {
                actor_id,
                origin,
                content,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity message: {err}"))?;
        let _ = self.sender.send(event.clone());

        // Only advance after a successful append to avoid gaps in the truth log.
        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(message_id)
    }

    pub fn append_run_spawned(
        &self,
        continuity_id: &str,
        message_id: &str,
        session_id: &str,
    ) -> Result<String, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };

        let id = Uuid::new_v4().to_string();
        let event = Event {
            id: id.clone(),
            session_id: continuity_id.to_string(),
            timestamp_ms: now_ms(),
            seq,
            kind: EventKind::ContinuityRunSpawned {
                run_session_id: session_id.to_string(),
                message_id: message_id.to_string(),
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity run spawned: {err}"))?;
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    fn load_next_seq_for(&self, continuity_id: &str) -> Result<u64, io::Error> {
        let events = self
            .event_log
            .replay_stream(StreamKind::Continuity, continuity_id)?;
        let last = events.last().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "continuity stream does not exist")
        })?;
        Ok(last.seq.saturating_add(1))
    }

    fn find_latest_continuity_for_workspace(
        &self,
        workspace: &str,
    ) -> Result<Option<String>, io::Error> {
        let events = self.event_log.replay_validated()?;
        let mut best: Option<(u64, String)> = None;
        for event in events {
            let EventKind::ContinuityCreated { workspace: w, .. } = event.kind else {
                continue;
            };
            if w != workspace {
                continue;
            }
            let id = event.session_id;
            match best {
                Some((ts, _)) if ts >= event.timestamp_ms => {}
                _ => best = Some((event.timestamp_ms, id)),
            }
        }
        Ok(best.map(|(_, id)| id))
    }
}

fn index_path(data_dir: &Path) -> PathBuf {
    data_dir.join("continuities").join("index.json")
}

fn load_index(path: &Path) -> io::Result<ContinuityIndexV1> {
    let bytes = fs::read(path)?;
    let parsed: ContinuityIndexV1 = serde_json::from_slice(&bytes)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if parsed.version != INDEX_VERSION {
        return Ok(ContinuityIndexV1::default());
    }
    Ok(parsed)
}

fn save_index(path: &Path, index: &ContinuityIndexV1) -> io::Result<()> {
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

fn workspace_key(workspace_root: &Path) -> String {
    workspace_root.to_string_lossy().to_string()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::StreamKind;
    use tempfile::tempdir;

    fn store_for(dir: &tempfile::TempDir) -> (Arc<EventLog>, ContinuityStore, PathBuf) {
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");
        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store = ContinuityStore::new(data_dir.clone(), workspace_root, event_log.clone())
            .expect("store");
        (event_log, store, data_dir)
    }

    #[test]
    fn ensure_default_creates_and_is_idempotent() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, data_dir) = store_for(&dir);

        let first = store.ensure_default().expect("ensure");
        let second = store.ensure_default().expect("ensure");
        assert_eq!(first, second);

        let index = fs::read_to_string(index_path(&data_dir)).expect("index file");
        assert!(index.contains(&first));

        let events = event_log
            .replay_stream(StreamKind::Continuity, &first)
            .expect("replay");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, 0);
        match &events[0].kind {
            EventKind::ContinuityCreated { workspace, .. } => {
                assert!(!workspace.is_empty());
            }
            other => panic!("expected continuity_created, got {other:?}"),
        }
    }

    #[test]
    fn ensure_default_recovers_from_missing_index() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, data_dir) = store_for(&dir);

        let first = store.ensure_default().expect("ensure");
        fs::remove_file(index_path(&data_dir)).expect("remove index");

        let (_event_log2, store2, _data_dir2) = store_for(&dir);
        let second = store2.ensure_default().expect("ensure");
        assert_eq!(first, second);
    }

    #[test]
    fn append_message_increments_seq() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        let m2 = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "world".to_string(),
            )
            .expect("append");
        assert_ne!(m1, m2);

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
        match &events[2].kind {
            EventKind::ContinuityMessageAppended { content, .. } => assert_eq!(content, "world"),
            other => panic!("expected message, got {other:?}"),
        }
    }

    #[test]
    fn append_run_spawned_advances_seq() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(&continuity_id, &message_id, "session-1")
            .expect("run spawned");

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
        match &events[2].kind {
            EventKind::ContinuityRunSpawned { run_session_id, .. } => {
                assert_eq!(run_session_id, "session-1")
            }
            other => panic!("expected run_spawned, got {other:?}"),
        }
    }

    #[test]
    fn list_and_get_reflect_created_thread() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let continuity_id = store.ensure_default().expect("ensure");

        let all = store.list();
        assert!(all.iter().any(|meta| meta.continuity_id == continuity_id));

        let meta = store.get(&continuity_id).expect("meta");
        assert_eq!(meta.continuity_id, continuity_id);
        assert!(!meta.archived);
    }

    #[test]
    fn append_message_unknown_continuity_is_error() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let err = store
            .append_message(
                "missing-thread-id",
                "user".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect_err("expected error");
        assert!(err.contains("continuity stream does not exist"));
    }

    #[test]
    fn append_run_spawned_unknown_continuity_is_error() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let err = store
            .append_run_spawned("missing-thread-id", "message-1", "session-1")
            .expect_err("expected error");
        assert!(err.contains("continuity stream does not exist"));
    }

    #[test]
    fn new_ignores_invalid_index_json() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");

        let path = index_path(&data_dir);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, b"not json").expect("write");

        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store =
            ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

        let continuity_id = store.ensure_default().expect("ensure");
        assert!(!continuity_id.is_empty());
    }

    #[test]
    fn new_resets_index_on_version_mismatch() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");

        let legacy_id = "legacy-thread-id";
        let legacy = serde_json::json!({
            "version": 0,
            "workspaces": {
                workspace_key(&workspace_root): legacy_id,
            },
            "continuities": {
                legacy_id: {
                    "created_at_ms": 0,
                    "title": null,
                    "archived": false,
                }
            }
        });
        let path = index_path(&data_dir);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, legacy.to_string()).expect("write");

        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store =
            ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

        let continuity_id = store.ensure_default().expect("ensure");
        assert_ne!(continuity_id, legacy_id);
    }

    #[test]
    fn ensure_default_errors_when_index_parent_is_file() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        fs::create_dir_all(&workspace_root).expect("workspace");
        fs::create_dir_all(&data_dir).expect("data");
        fs::write(data_dir.join("continuities"), "file").expect("continuities file");

        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store =
            ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

        let err = store.ensure_default().expect_err("expected error");
        assert!(err.contains("save continuity index"));
    }
}
