use std::io::{self, Write};

use futures_util::StreamExt;
use reqwest::Client;
use reqwest_eventsource::{Error as EventSourceError, Event, RequestBuilderExt};
use rip_kernel::Event as FrameEvent;

use super::*;

pub(super) async fn run_threads_remote(
    server: String,
    command: ThreadsCommand,
) -> anyhow::Result<()> {
    let client = Client::new();
    match command {
        ThreadsCommand::Ensure => {
            let url = format!("{server}/threads/ensure");
            let response = client.post(url).send().await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread ensure failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::List => {
            let url = format!("{server}/threads");
            let response = client.get(url).send().await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread list failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::Get { id } => {
            let url = format!("{server}/threads/{id}");
            let response = client.get(url).send().await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread get failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::Branch {
            id,
            title,
            from_message_id,
            from_seq,
            actor_id,
            origin,
        } => {
            let url = format!("{server}/threads/{id}/branch");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "title": title,
                    "from_message_id": from_message_id,
                    "from_seq": from_seq,
                    "actor_id": actor_id,
                    "origin": origin
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread branch failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::Handoff {
            id,
            title,
            summary_markdown,
            summary_artifact_id,
            from_message_id,
            from_seq,
            actor_id,
            origin,
        } => {
            let url = format!("{server}/threads/{id}/handoff");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "title": title,
                    "summary_markdown": summary_markdown,
                    "summary_artifact_id": summary_artifact_id,
                    "from_message_id": from_message_id,
                    "from_seq": from_seq,
                    "actor_id": actor_id,
                    "origin": origin
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread handoff failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::CompactionCheckpoint {
            id,
            summary_markdown,
            summary_artifact_id,
            to_message_id,
            to_seq,
            stride_messages,
            actor_id,
            origin,
        } => {
            let url = format!("{server}/threads/{id}/compaction-checkpoint");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "summary_markdown": summary_markdown,
                    "summary_artifact_id": summary_artifact_id,
                    "to_message_id": to_message_id,
                    "to_seq": to_seq,
                    "stride_messages": stride_messages,
                    "actor_id": actor_id,
                    "origin": origin
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread compaction-checkpoint failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::CompactionCutPoints {
            id,
            stride_messages,
            limit,
        } => {
            let url = format!("{server}/threads/{id}/compaction-cut-points");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "stride_messages": stride_messages,
                    "limit": limit,
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread compaction-cut-points failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::CompactionStatus {
            id,
            stride_messages,
        } => {
            let url = format!("{server}/threads/{id}/compaction-status");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "stride_messages": stride_messages,
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread compaction-status failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::ProviderCursorStatus { id } => {
            let url = format!("{server}/threads/{id}/provider-cursor-status");
            let response = client.post(url).json(&serde_json::json!({})).send().await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread provider-cursor-status failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::ProviderCursorRotate {
            id,
            reason,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let url = format!("{server}/threads/{id}/provider-cursor-rotate");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "provider": null,
                    "endpoint": null,
                    "model": null,
                    "reason": reason,
                    "actor_id": actor_id,
                    "origin": origin,
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread provider-cursor-rotate failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::ContextSelectionStatus { id, limit } => {
            let url = format!("{server}/threads/{id}/context-selection-status");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "limit": limit,
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread context-selection-status failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::CompactionAuto {
            id,
            stride_messages,
            max_new_checkpoints,
            dry_run,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let url = format!("{server}/threads/{id}/compaction-auto");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "stride_messages": stride_messages,
                    "max_new_checkpoints": max_new_checkpoints,
                    "dry_run": dry_run,
                    "actor_id": actor_id,
                    "origin": origin,
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread compaction-auto failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::CompactionAutoSchedule {
            id,
            stride_messages,
            max_new_checkpoints,
            allow_inflight,
            no_execute,
            dry_run,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let url = format!("{server}/threads/{id}/compaction-auto-schedule");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "stride_messages": stride_messages,
                    "max_new_checkpoints": max_new_checkpoints,
                    "block_on_inflight": !allow_inflight,
                    "execute": !no_execute,
                    "dry_run": dry_run,
                    "actor_id": actor_id,
                    "origin": origin,
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread compaction-auto-schedule failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::PostMessage {
            id,
            content,
            actor_id,
            origin,
        } => {
            let url = format!("{server}/threads/{id}/messages");
            let response = client
                .post(url)
                .json(&serde_json::json!({
                    "content": content,
                    "actor_id": actor_id,
                    "origin": origin
                }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                anyhow::bail!("thread post_message failed: {status}");
            }
            let body = response.text().await?;
            println!("{body}");
        }
        ThreadsCommand::Events { id, max_events } => {
            let url = format!("{server}/threads/{id}/events");
            let mut stream = client.get(url).eventsource()?;
            let stdout = io::stdout();
            let mut out = stdout.lock();
            stream_frames_sse(&mut stream, max_events, &mut out).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
pub(super) async fn run_threads_local_with_engine(
    engine: &ripd::SessionEngine,
    command: ThreadsCommand,
) -> anyhow::Result<()> {
    let store = engine.continuities();
    match command {
        ThreadsCommand::Ensure => {
            let thread_id = store
                .ensure_default()
                .map_err(|err| anyhow::anyhow!("thread ensure: {err}"))?;
            let payload = ThreadEnsureResponse { thread_id };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::List => {
            let mut out = Vec::new();
            for meta in store.list() {
                out.push(ThreadMeta {
                    thread_id: meta.continuity_id,
                    created_at_ms: meta.created_at_ms,
                    title: meta.title,
                    archived: meta.archived,
                });
            }
            println!("{}", serde_json::to_string(&out)?);
        }
        ThreadsCommand::Get { id } => match store.get(&id) {
            Some(meta) => {
                let payload = ThreadMeta {
                    thread_id: id,
                    created_at_ms: meta.created_at_ms,
                    title: meta.title,
                    archived: meta.archived,
                };
                println!("{}", serde_json::to_string(&payload)?);
            }
            None => anyhow::bail!("thread get failed: not found"),
        },
        ThreadsCommand::Branch {
            id,
            title,
            from_message_id,
            from_seq,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let (thread_id, parent_seq, parent_message_id) = store
                .branch(&id, title, from_message_id, from_seq, actor_id, origin)
                .map_err(|err| anyhow::anyhow!("thread branch failed: {err}"))?;
            let payload = ThreadBranchResponse {
                thread_id,
                parent_thread_id: id,
                parent_seq,
                parent_message_id,
            };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::Handoff {
            id,
            title,
            summary_markdown,
            summary_artifact_id,
            from_message_id,
            from_seq,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let (thread_id, from_seq, from_message_id) = store
                .handoff(
                    &id,
                    title,
                    (summary_markdown, summary_artifact_id),
                    from_message_id,
                    from_seq,
                    (actor_id, origin),
                )
                .map_err(|err| anyhow::anyhow!("thread handoff failed: {err}"))?;
            let payload = ThreadHandoffResponse {
                thread_id,
                from_thread_id: id,
                from_seq,
                from_message_id,
            };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::CompactionCheckpoint {
            id,
            summary_markdown,
            summary_artifact_id,
            to_message_id,
            to_seq,
            stride_messages,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let (checkpoint_id, summary_artifact_id, to_seq, to_message_id, cut_rule_id) = store
                .compaction_checkpoint_cumulative_v1(
                    &id,
                    ripd::CompactionCheckpointCumulativeV1Request {
                        summary_markdown,
                        summary_artifact_id,
                        to_message_id,
                        to_seq,
                        stride_messages,
                        actor_id,
                        origin,
                    },
                )
                .map_err(|err| anyhow::anyhow!("thread compaction-checkpoint failed: {err}"))?;
            let payload = ThreadCompactionCheckpointResponse {
                thread_id: id,
                checkpoint_id,
                cut_rule_id,
                summary_artifact_id,
                to_seq,
                to_message_id,
            };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::CompactionCutPoints {
            id,
            stride_messages,
            limit,
        } => {
            let resp = store
                .compaction_cut_points_v1(
                    &id,
                    ripd::CompactionCutPointsV1Request {
                        stride_messages,
                        limit,
                    },
                )
                .map_err(|err| anyhow::anyhow!("thread compaction-cut-points failed: {err}"))?;
            println!("{}", serde_json::to_string(&resp)?);
        }
        ThreadsCommand::CompactionStatus {
            id,
            stride_messages,
        } => {
            let resp = store
                .compaction_status_v1(&id, ripd::CompactionStatusV1Request { stride_messages })
                .map_err(|err| anyhow::anyhow!("thread compaction-status failed: {err}"))?;
            println!("{}", serde_json::to_string(&resp)?);
        }
        ThreadsCommand::ProviderCursorStatus { id } => {
            let resp = store
                .provider_cursor_status_v1(&id, ripd::ProviderCursorStatusV1Request {})
                .map_err(|err| anyhow::anyhow!("thread provider-cursor-status failed: {err}"))?;
            println!("{}", serde_json::to_string(&resp)?);
        }
        ThreadsCommand::ProviderCursorRotate {
            id,
            reason,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let resp = store
                .provider_cursor_rotate_v1(
                    &id,
                    ripd::ProviderCursorRotateV1Request {
                        provider: None,
                        endpoint: None,
                        model: None,
                        reason,
                        actor_id,
                        origin,
                    },
                )
                .map_err(|err| anyhow::anyhow!("thread provider-cursor-rotate failed: {err}"))?;
            println!("{}", serde_json::to_string(&resp)?);
        }
        ThreadsCommand::ContextSelectionStatus { id, limit } => {
            let resp = store
                .context_selection_status_v1(&id, ripd::ContextSelectionStatusV1Request { limit })
                .map_err(|err| anyhow::anyhow!("thread context-selection-status failed: {err}"))?;
            println!("{}", serde_json::to_string(&resp)?);
        }
        ThreadsCommand::CompactionAuto {
            id,
            stride_messages,
            max_new_checkpoints,
            dry_run,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let resp = store
                .compaction_auto_v1(
                    &id,
                    ripd::CompactionAutoV1Request {
                        stride_messages,
                        max_new_checkpoints,
                        dry_run: Some(dry_run),
                        actor_id,
                        origin,
                    },
                )
                .map_err(|err| anyhow::anyhow!("thread compaction-auto failed: {err}"))?;
            println!("{}", serde_json::to_string(&resp)?);
        }
        ThreadsCommand::CompactionAutoSchedule {
            id,
            stride_messages,
            max_new_checkpoints,
            allow_inflight,
            no_execute,
            dry_run,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let resp = store
                .compaction_auto_schedule_v1(
                    &id,
                    ripd::CompactionAutoScheduleV1Request {
                        stride_messages,
                        max_new_checkpoints,
                        block_on_inflight: Some(!allow_inflight),
                        execute: Some(!no_execute),
                        dry_run: Some(dry_run),
                        actor_id,
                        origin,
                    },
                )
                .map_err(|err| anyhow::anyhow!("thread compaction-auto-schedule failed: {err}"))?;
            println!("{}", serde_json::to_string(&resp)?);
        }
        ThreadsCommand::PostMessage {
            id,
            content,
            actor_id,
            origin,
        } => {
            let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
            let origin = origin.unwrap_or_else(|| "cli".to_string());
            let message_id = store
                .append_message(&id, actor_id.clone(), origin.clone(), content.clone())
                .map_err(|err| anyhow::anyhow!("thread post_message failed: {err}"))?;

            let handle = engine.create_session();
            let session_id = handle.session_id.clone();

            let run_link = ripd::ContinuityRunLink {
                continuity_id: id.clone(),
                message_id: message_id.clone(),
                actor_id: actor_id.clone(),
                origin: origin.clone(),
            };
            store
                .append_run_spawned(&id, &message_id, &session_id, actor_id, origin)
                .map_err(|err| anyhow::anyhow!("thread post_message run link failed: {err}"))?;

            engine.spawn_session(handle, content, Some(run_link), None);

            let payload = ThreadPostMessageResponse {
                thread_id: id,
                message_id,
                session_id,
            };
            println!("{}", serde_json::to_string(&payload)?);
        }
        ThreadsCommand::Events { id, max_events } => {
            let past = store
                .replay_events(&id)
                .map_err(|err| anyhow::anyhow!("thread events replay failed: {err}"))?;
            if past.is_empty() {
                anyhow::bail!("thread events failed: not found");
            }

            let mut receiver = store.subscribe();
            let stdout = io::stdout();
            let mut out = stdout.lock();
            stream_frames_local(&id, past, &mut receiver, max_events, &mut out).await?;
        }
    }

    Ok(())
}

async fn stream_frames_sse(
    stream: &mut (impl futures_util::Stream<Item = Result<Event, EventSourceError>> + Unpin),
    max_events: Option<usize>,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let mut seen = 0usize;
    while let Some(next) = stream.next().await {
        match next {
            Ok(Event::Open) => {}
            Ok(Event::Message(msg)) => {
                serde_json::from_str::<FrameEvent>(&msg.data)
                    .map_err(|err| anyhow::anyhow!("invalid event frame: {err}"))?;
                writeln!(out, "{}", msg.data)?;
                out.flush()?;
                seen = seen.saturating_add(1);
                if max_events.map(|limit| seen >= limit).unwrap_or(false) {
                    break;
                }
            }
            Err(EventSourceError::StreamEnded) => break,
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) async fn stream_frames_local(
    thread_id: &str,
    past: Vec<FrameEvent>,
    receiver: &mut tokio::sync::broadcast::Receiver<FrameEvent>,
    max_events: Option<usize>,
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let mut seen = 0usize;
    let mut last_seq = past.last().map(|event| event.seq);

    for event in past {
        let json =
            serde_json::to_string(&event).map_err(|err| anyhow::anyhow!("event json: {err}"))?;
        writeln!(out, "{json}")?;
        out.flush()?;
        seen = seen.saturating_add(1);
        if max_events.map(|limit| seen >= limit).unwrap_or(false) {
            return Ok(());
        }
    }

    loop {
        match receiver.recv().await {
            Ok(event) => {
                if event.session_id != thread_id {
                    continue;
                }
                if last_seq.map(|seq| event.seq <= seq).unwrap_or(false) {
                    continue;
                }
                last_seq = Some(event.seq);
                let json = serde_json::to_string(&event)
                    .map_err(|err| anyhow::anyhow!("event json: {err}"))?;
                writeln!(out, "{json}")?;
                out.flush()?;
                seen = seen.saturating_add(1);
                if max_events.map(|limit| seen >= limit).unwrap_or(false) {
                    break;
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }

    Ok(())
}
