use super::*;
use uuid::Uuid;

impl ContinuityStore {
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
        self.stream_cache.append_best_effort(&event);
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
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event.clone());

        self.next_seq
            .lock()
            .expect("continuity seq mutex")
            .insert(thread_id.clone(), 2);

        Ok((thread_id, from_seq, from_message_id))
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
        self.stream_cache.append_best_effort(&created);
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
