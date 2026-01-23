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

use crate::handoff_context_bundle::HandoffContextBundleV1;

const INDEX_VERSION: u32 = 1;
const EVENT_CHANNEL_CAPACITY: usize = 16_384;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuityMeta {
    pub continuity_id: String,
    pub created_at_ms: u64,
    pub title: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone)]
pub struct ContinuityRunLink {
    pub continuity_id: String,
    pub message_id: String,
    pub actor_id: String,
    pub origin: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ContextCompiledPayload {
    pub(crate) run_session_id: String,
    pub(crate) bundle_artifact_id: String,
    pub(crate) compiler_id: String,
    pub(crate) compiler_strategy: String,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
}

#[derive(Debug, Clone)]
pub struct ToolSideEffects {
    pub tool_id: String,
    pub tool_name: String,
    pub affected_paths: Option<Vec<String>>,
    pub checkpoint_id: Option<String>,
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

    pub(crate) fn workspace_root(&self) -> &Path {
        &self.workspace_root
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
        self.create_continuity(workspace, Some(continuity_id), None, true)
    }

    pub fn branch(
        &self,
        parent_thread_id: &str,
        title: Option<String>,
        from_message_id: Option<String>,
        from_seq: Option<u64>,
        actor_id: String,
        origin: String,
    ) -> Result<(String, u64, Option<String>), String> {
        if from_message_id.is_some() && from_seq.is_some() {
            return Err("branch requires only one of from_message_id or from_seq".to_string());
        }

        let parent_events = self
            .replay_events(parent_thread_id)
            .map_err(|err| format!("branch parent replay failed: {err}"))?;
        if parent_events.is_empty() {
            return Err("branch parent continuity stream does not exist".to_string());
        }

        let head_seq = parent_events
            .last()
            .map(|event| event.seq)
            .unwrap_or_default();

        let (parent_seq, parent_message_id) = if let Some(from_seq) = from_seq {
            if from_seq > head_seq {
                return Err(format!(
                    "branch from_seq out of range: max_seq={head_seq}, got {from_seq}"
                ));
            }
            let last_message = parent_events
                .iter()
                .rev()
                .find(|event| {
                    event.seq <= from_seq
                        && matches!(event.kind, EventKind::ContinuityMessageAppended { .. })
                })
                .map(|event| event.id.clone());
            (from_seq, last_message)
        } else if let Some(message_id) = from_message_id.clone() {
            let mut message_seq: Option<u64> = None;
            let mut max_related_seq: Option<u64> = None;

            for event in &parent_events {
                match &event.kind {
                    EventKind::ContinuityMessageAppended { .. } if event.id == message_id => {
                        message_seq = Some(event.seq);
                        max_related_seq = Some(event.seq);
                    }
                    EventKind::ContinuityRunSpawned {
                        message_id: mid, ..
                    }
                    | EventKind::ContinuityRunEnded {
                        message_id: mid, ..
                    } if mid == &message_id => {
                        max_related_seq = Some(max_related_seq.unwrap_or(0).max(event.seq));
                    }
                    _ => {}
                }
            }

            if message_seq.is_none() {
                return Err(format!("branch from_message_id not found: {message_id}"));
            }
            (max_related_seq.unwrap_or(0), Some(message_id))
        } else {
            let last_message = parent_events
                .iter()
                .rev()
                .find(|event| matches!(event.kind, EventKind::ContinuityMessageAppended { .. }))
                .map(|event| event.id.clone());
            (head_seq, last_message)
        };

        let workspace = workspace_key(&self.workspace_root);
        let thread_id = self.create_continuity(workspace, None, title, false)?;

        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: thread_id.clone(),
            timestamp_ms: now_ms(),
            seq: 1,
            kind: EventKind::ContinuityBranched {
                parent_thread_id: parent_thread_id.to_string(),
                parent_seq,
                parent_message_id: parent_message_id.clone(),
                actor_id,
                origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity_branched: {err}"))?;
        let _ = self.sender.send(event.clone());

        self.next_seq
            .lock()
            .expect("continuity seq mutex")
            .insert(thread_id.clone(), 2);

        Ok((thread_id, parent_seq, parent_message_id))
    }

    pub fn handoff(
        &self,
        from_thread_id: &str,
        title: Option<String>,
        summary: (Option<String>, Option<String>),
        from_message_id: Option<String>,
        from_seq: Option<u64>,
        provenance: (String, String),
    ) -> Result<(String, u64, Option<String>), String> {
        let (summary_markdown, mut summary_artifact_id) = summary;
        let (actor_id, origin) = provenance;
        if summary_markdown.is_none() && summary_artifact_id.is_none() {
            return Err("handoff requires summary_markdown and/or summary_artifact_id".to_string());
        }
        if from_message_id.is_some() && from_seq.is_some() {
            return Err("handoff requires only one of from_message_id or from_seq".to_string());
        }

        let from_events = self
            .replay_events(from_thread_id)
            .map_err(|err| format!("handoff parent replay failed: {err}"))?;
        if from_events.is_empty() {
            return Err("handoff parent continuity stream does not exist".to_string());
        }

        let head_seq = from_events
            .last()
            .map(|event| event.seq)
            .unwrap_or_default();

        let (from_seq, from_message_id) = if let Some(from_seq) = from_seq {
            if from_seq > head_seq {
                return Err(format!(
                    "handoff from_seq out of range: max_seq={head_seq}, got {from_seq}"
                ));
            }
            let last_message = from_events
                .iter()
                .rev()
                .find(|event| {
                    event.seq <= from_seq
                        && matches!(event.kind, EventKind::ContinuityMessageAppended { .. })
                })
                .map(|event| event.id.clone());
            (from_seq, last_message)
        } else if let Some(message_id) = from_message_id.clone() {
            let mut message_seq: Option<u64> = None;
            let mut max_related_seq: Option<u64> = None;

            for event in &from_events {
                match &event.kind {
                    EventKind::ContinuityMessageAppended { .. } if event.id == message_id => {
                        message_seq = Some(event.seq);
                        max_related_seq = Some(event.seq);
                    }
                    EventKind::ContinuityRunSpawned {
                        message_id: mid, ..
                    }
                    | EventKind::ContinuityRunEnded {
                        message_id: mid, ..
                    } if mid == &message_id => {
                        max_related_seq = Some(max_related_seq.unwrap_or(0).max(event.seq));
                    }
                    _ => {}
                }
            }

            if message_seq.is_none() {
                return Err(format!("handoff from_message_id not found: {message_id}"));
            }
            (max_related_seq.unwrap_or(0), Some(message_id))
        } else {
            let last_message = from_events
                .iter()
                .rev()
                .find(|event| matches!(event.kind, EventKind::ContinuityMessageAppended { .. }))
                .map(|event| event.id.clone());
            (head_seq, last_message)
        };

        let workspace = workspace_key(&self.workspace_root);
        let thread_id = self.create_continuity(workspace, None, title, false)?;

        if summary_artifact_id.is_none() {
            if let Some(markdown) = summary_markdown.as_ref() {
                let bundle = HandoffContextBundleV1::new_source_cut(
                    markdown.clone(),
                    from_thread_id.to_string(),
                    from_seq,
                    from_message_id.clone(),
                );
                summary_artifact_id = Some(crate::handoff_context_bundle::write_bundle_v1(
                    &self.workspace_root,
                    &bundle,
                )?);
            }
        }

        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: thread_id.clone(),
            timestamp_ms: now_ms(),
            seq: 1,
            kind: EventKind::ContinuityHandoffCreated {
                from_thread_id: from_thread_id.to_string(),
                from_seq,
                from_message_id: from_message_id.clone(),
                summary_artifact_id,
                summary_markdown,
                actor_id,
                origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity_handoff_created: {err}"))?;
        let _ = self.sender.send(event.clone());

        self.next_seq
            .lock()
            .expect("continuity seq mutex")
            .insert(thread_id.clone(), 2);

        Ok((thread_id, from_seq, from_message_id))
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
        actor_id: String,
        origin: String,
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
                actor_id: Some(actor_id),
                origin: Some(origin),
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity run spawned: {err}"))?;
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    pub(crate) fn append_context_compiled(
        &self,
        continuity_id: &str,
        payload: ContextCompiledPayload,
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
            kind: EventKind::ContinuityContextCompiled {
                run_session_id: payload.run_session_id,
                bundle_artifact_id: payload.bundle_artifact_id,
                compiler_id: payload.compiler_id,
                compiler_strategy: payload.compiler_strategy,
                from_seq: payload.from_seq,
                from_message_id: payload.from_message_id,
                actor_id: payload.actor_id,
                origin: payload.origin,
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity context compiled: {err}"))?;
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    pub fn append_run_ended(
        &self,
        continuity_id: &str,
        message_id: &str,
        session_id: &str,
        reason: String,
        actor_id: String,
        origin: String,
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
            kind: EventKind::ContinuityRunEnded {
                run_session_id: session_id.to_string(),
                message_id: message_id.to_string(),
                reason,
                actor_id: Some(actor_id),
                origin: Some(origin),
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity run ended: {err}"))?;
        let _ = self.sender.send(event.clone());

        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(id)
    }

    pub fn append_tool_side_effects(
        &self,
        run: &ContinuityRunLink,
        run_session_id: &str,
        effects: ToolSideEffects,
    ) -> Result<String, String> {
        let continuity_id = run.continuity_id.as_str();
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
            kind: EventKind::ContinuityToolSideEffects {
                run_session_id: run_session_id.to_string(),
                tool_id: effects.tool_id,
                tool_name: effects.tool_name,
                affected_paths: effects.affected_paths,
                checkpoint_id: effects.checkpoint_id,
                actor_id: run.actor_id.clone(),
                origin: run.origin.clone(),
            },
        };
        self.event_log
            .append(&event)
            .map_err(|err| format!("append continuity tool side effects: {err}"))?;
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

    fn create_continuity(
        &self,
        workspace: String,
        continuity_id: Option<String>,
        title: Option<String>,
        set_as_default: bool,
    ) -> Result<String, String> {
        let continuity_id = continuity_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let timestamp_ms = now_ms();
        let created = Event {
            id: Uuid::new_v4().to_string(),
            session_id: continuity_id.clone(),
            timestamp_ms,
            seq: 0,
            kind: EventKind::ContinuityCreated {
                workspace: workspace.clone(),
                title: title.clone(),
            },
        };
        self.event_log
            .append(&created)
            .map_err(|err| format!("append continuity_created: {err}"))?;
        let _ = self.sender.send(created.clone());

        {
            let mut index = self.index.lock().expect("continuity index mutex");
            if set_as_default {
                index.workspaces.insert(workspace, continuity_id.clone());
            }
            index.continuities.insert(
                continuity_id.clone(),
                ContinuityMetaV1 {
                    created_at_ms: timestamp_ms,
                    title,
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
            .append_run_spawned(
                &continuity_id,
                &message_id,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
        match &events[2].kind {
            EventKind::ContinuityRunSpawned {
                run_session_id,
                actor_id,
                origin,
                ..
            } => {
                assert_eq!(run_session_id, "session-1");
                assert_eq!(actor_id.as_deref(), Some("user"));
                assert_eq!(origin.as_deref(), Some("cli"));
            }
            other => panic!("expected run_spawned, got {other:?}"),
        }
    }

    #[test]
    fn append_run_ended_advances_seq() {
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
            .append_run_spawned(
                &continuity_id,
                &message_id,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_run_ended(
                &continuity_id,
                &message_id,
                "session-1",
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
        assert_eq!(events[3].seq, 3);
        match &events[3].kind {
            EventKind::ContinuityRunEnded {
                run_session_id,
                message_id: mid,
                reason,
                actor_id,
                origin,
            } => {
                assert_eq!(run_session_id, "session-1");
                assert_eq!(mid, &message_id);
                assert_eq!(reason, "completed");
                assert_eq!(actor_id.as_deref(), Some("user"));
                assert_eq!(origin.as_deref(), Some("cli"));
            }
            other => panic!("expected run_ended, got {other:?}"),
        }
    }

    #[test]
    fn append_tool_side_effects_advances_seq() {
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
            .append_run_spawned(
                &continuity_id,
                &message_id,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_tool_side_effects(
                &ContinuityRunLink {
                    continuity_id: continuity_id.clone(),
                    message_id: message_id.clone(),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
                "session-1",
                ToolSideEffects {
                    tool_id: "tool-1".to_string(),
                    tool_name: "write".to_string(),
                    affected_paths: Some(vec!["a.txt".to_string()]),
                    checkpoint_id: Some("checkpoint-1".to_string()),
                },
            )
            .expect("tool side effects");

        let events = event_log
            .replay_stream(StreamKind::Continuity, &continuity_id)
            .expect("replay");
        assert_eq!(events.len(), 4);
        assert_eq!(events[3].seq, 3);
        match &events[3].kind {
            EventKind::ContinuityToolSideEffects {
                run_session_id,
                tool_id,
                tool_name,
                affected_paths,
                checkpoint_id,
                actor_id,
                origin,
            } => {
                assert_eq!(run_session_id, "session-1");
                assert_eq!(tool_id, "tool-1");
                assert_eq!(tool_name, "write");
                assert_eq!(affected_paths.as_deref(), Some(&["a.txt".to_string()][..]));
                assert_eq!(checkpoint_id.as_deref(), Some("checkpoint-1"));
                assert_eq!(actor_id, "user");
                assert_eq!(origin, "cli");
            }
            other => panic!("expected tool side effects, got {other:?}"),
        }
    }

    #[test]
    fn branch_creates_child_with_cutpoint_and_provenance() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let parent_thread_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &parent_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "turn1".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &parent_thread_id,
                &m1,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_run_ended(
                &parent_thread_id,
                &m1,
                "session-1",
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");
        let _m2 = store
            .append_message(
                &parent_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "turn2".to_string(),
            )
            .expect("append");

        let (child_thread_id, parent_seq, parent_message_id) = store
            .branch(
                &parent_thread_id,
                Some("child".to_string()),
                Some(m1.clone()),
                None,
                "alice".to_string(),
                "team".to_string(),
            )
            .expect("branch");

        assert_eq!(parent_seq, 3, "expected cut to include run_ended");
        assert_eq!(parent_message_id.as_deref(), Some(m1.as_str()));

        let child_events = event_log
            .replay_stream(StreamKind::Continuity, &child_thread_id)
            .expect("replay child");
        assert_eq!(child_events.len(), 2);
        assert_eq!(child_events[0].seq, 0);
        assert_eq!(child_events[1].seq, 1);
        match &child_events[1].kind {
            EventKind::ContinuityBranched {
                parent_thread_id: parent_id,
                parent_seq: cut_seq,
                parent_message_id: cut_message_id,
                actor_id,
                origin,
            } => {
                assert_eq!(parent_id, &parent_thread_id);
                assert_eq!(*cut_seq, 3);
                assert_eq!(cut_message_id.as_deref(), Some(m1.as_str()));
                assert_eq!(actor_id, "alice");
                assert_eq!(origin, "team");
            }
            other => panic!("expected continuity_branched, got {other:?}"),
        }

        store
            .append_message(
                &child_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "child turn".to_string(),
            )
            .expect("append child");
        let child_events = event_log
            .replay_stream(StreamKind::Continuity, &child_thread_id)
            .expect("replay child");
        assert_eq!(child_events.len(), 3);
        assert_eq!(
            child_events[2].seq, 2,
            "expected seq to continue after branch"
        );
    }

    #[test]
    fn branch_rejects_conflicting_cut_selectors() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let parent_thread_id = store.ensure_default().expect("ensure");
        let err = store
            .branch(
                &parent_thread_id,
                None,
                Some("m1".to_string()),
                Some(1),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect_err("expected error");
        assert!(err.contains("from_message_id") && err.contains("from_seq"));
    }

    #[test]
    fn handoff_creates_child_with_cutpoint_provenance_and_summary() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, _data_dir) = store_for(&dir);

        let from_thread_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &from_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "turn1".to_string(),
            )
            .expect("append");
        store
            .append_run_spawned(
                &from_thread_id,
                &m1,
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_run_ended(
                &from_thread_id,
                &m1,
                "session-1",
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");
        let _m2 = store
            .append_message(
                &from_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "turn2".to_string(),
            )
            .expect("append");

        let (child_thread_id, from_seq, from_message_id) = store
            .handoff(
                &from_thread_id,
                Some("handoff".to_string()),
                (Some("summary".to_string()), None),
                Some(m1.clone()),
                None,
                ("alice".to_string(), "team".to_string()),
            )
            .expect("handoff");

        assert_eq!(from_seq, 3, "expected cut to include run_ended");
        assert_eq!(from_message_id.as_deref(), Some(m1.as_str()));

        let child_events = event_log
            .replay_stream(StreamKind::Continuity, &child_thread_id)
            .expect("replay child");
        assert_eq!(child_events.len(), 2);
        assert_eq!(child_events[0].seq, 0);
        assert_eq!(child_events[1].seq, 1);
        let artifact_id = match &child_events[1].kind {
            EventKind::ContinuityHandoffCreated {
                from_thread_id: event_from_id,
                from_seq: cut_seq,
                from_message_id: cut_message_id,
                summary_artifact_id,
                summary_markdown,
                actor_id,
                origin,
            } => {
                assert_eq!(event_from_id, &from_thread_id);
                assert_eq!(*cut_seq, 3);
                assert_eq!(cut_message_id.as_deref(), Some(m1.as_str()));
                let artifact_id = summary_artifact_id.as_deref().expect("summary_artifact_id");
                assert_eq!(artifact_id.len(), 64);
                assert_eq!(summary_markdown.as_deref(), Some("summary"));
                assert_eq!(actor_id, "alice");
                assert_eq!(origin, "team");
                artifact_id.to_string()
            }
            other => panic!("expected continuity_handoff_created, got {other:?}"),
        };

        let blob_path = dir
            .path()
            .join("workspace")
            .join(".rip")
            .join("artifacts")
            .join("blobs")
            .join(&artifact_id);
        let bytes = fs::read(&blob_path).expect("read bundle artifact");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("bundle json");
        assert_eq!(
            json.get("schema").and_then(|v| v.as_str()),
            Some("rip.handoff_context_bundle.v1")
        );
        assert_eq!(
            json.get("summary_markdown").and_then(|v| v.as_str()),
            Some("summary")
        );
        let thread_refs = json
            .get("refs")
            .and_then(|v| v.get("threads"))
            .and_then(|v| v.as_array())
            .expect("thread refs");
        assert_eq!(thread_refs.len(), 1);
        assert_eq!(
            thread_refs[0].get("thread_id").and_then(|v| v.as_str()),
            Some(from_thread_id.as_str())
        );
        assert_eq!(thread_refs[0].get("seq").and_then(|v| v.as_u64()), Some(3));
        assert_eq!(
            thread_refs[0].get("message_id").and_then(|v| v.as_str()),
            Some(m1.as_str())
        );

        store
            .append_message(
                &child_thread_id,
                "user".to_string(),
                "cli".to_string(),
                "child turn".to_string(),
            )
            .expect("append child");
        let child_events = event_log
            .replay_stream(StreamKind::Continuity, &child_thread_id)
            .expect("replay child");
        assert_eq!(child_events.len(), 3);
        assert_eq!(
            child_events[2].seq, 2,
            "expected seq to continue after handoff"
        );
    }

    #[test]
    fn handoff_rejects_missing_summary() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let from_thread_id = store.ensure_default().expect("ensure");
        let err = store
            .handoff(
                &from_thread_id,
                None,
                (None, None),
                None,
                None,
                ("user".to_string(), "cli".to_string()),
            )
            .expect_err("expected error");
        assert!(err.contains("summary"), "expected summary validation");
    }

    #[test]
    fn handoff_rejects_conflicting_cut_selectors() {
        let dir = tempdir().expect("tmp");
        let (_event_log, store, _data_dir) = store_for(&dir);

        let from_thread_id = store.ensure_default().expect("ensure");
        let err = store
            .handoff(
                &from_thread_id,
                None,
                (Some("summary".to_string()), None),
                Some("m1".to_string()),
                Some(1),
                ("user".to_string(), "cli".to_string()),
            )
            .expect_err("expected error");
        assert!(err.contains("from_message_id") && err.contains("from_seq"));
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
            .append_run_spawned(
                "missing-thread-id",
                "message-1",
                "session-1",
                "user".to_string(),
                "cli".to_string(),
            )
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
