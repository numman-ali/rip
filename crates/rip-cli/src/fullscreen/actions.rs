//! Fire-and-forget status-producing actions.
//!
//! Every function here follows the same shape: ensure a continuity,
//! POST to a capability endpoint, parse the response, and push a
//! single-line status string back onto the TUI's status channel.
//! Factoring them out of the `run_fullscreen_tui_sse` match body
//! keeps that loop at event-routing size; the network payloads,
//! ripd response types, and error formatting live here.
//!
//! All functions take owned `Client` + `String` + `Sender` because
//! they immediately hand ownership to `tokio::spawn`. Callers at the
//! dispatch site write
//! `actions::spawn_compaction_status(client.clone(), server.clone(), status_tx.clone())`.

use reqwest::Client;
use tokio::sync::mpsc;

pub(super) fn spawn_compaction_cut_points(
    client: Client,
    server: String,
    tx: mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let message = match crate::ensure_thread(&client, &server).await {
            Ok(thread_id) => {
                let url = format!("{server}/threads/{thread_id}/compaction-cut-points");
                let response = client
                    .post(url)
                    .json(&serde_json::json!({ "stride_messages": null, "limit": 1 }))
                    .send()
                    .await;
                match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<ripd::CompactionCutPointsV1Response>().await {
                            Ok(out) => {
                                let latest = out.cut_points.first();
                                match latest {
                                    Some(cp) => format!(
                                        "cut_points: messages={} latest ordinal={} to_seq={} checkpointed={}",
                                        out.message_count,
                                        cp.target_message_ordinal,
                                        cp.to_seq,
                                        cp.already_checkpointed
                                    ),
                                    None => format!(
                                        "cut_points: messages={} (no eligible cut points)",
                                        out.message_count
                                    ),
                                }
                            }
                            Err(err) => format!("cut_points: parse failed: {err}"),
                        }
                    }
                    Ok(resp) => format!("cut_points: request failed: {}", resp.status()),
                    Err(err) => format!("cut_points: request failed: {err}"),
                }
            }
            Err(err) => format!("cut_points: thread ensure failed: {err}"),
        };
        let _ = tx.send(message).await;
    });
}

pub(super) fn spawn_compaction_auto(client: Client, server: String, tx: mpsc::Sender<String>) {
    tokio::spawn(async move {
        let message = match crate::ensure_thread(&client, &server).await {
            Ok(thread_id) => {
                let url = format!("{server}/threads/{thread_id}/compaction-auto");
                let response = client
                    .post(url)
                    .json(&serde_json::json!({
                        "stride_messages": null,
                        "max_new_checkpoints": null,
                        "dry_run": false,
                        "actor_id": "user",
                        "origin": "tui"
                    }))
                    .send()
                    .await;
                match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<ripd::CompactionAutoV1Response>().await {
                            Ok(out) => match out.job_id {
                                Some(job_id) => format!(
                                    "compaction auto: status={} job_id={job_id}",
                                    out.status
                                ),
                                None => format!("compaction auto: status={}", out.status),
                            },
                            Err(err) => format!("compaction auto: parse failed: {err}"),
                        }
                    }
                    Ok(resp) => format!("compaction auto: request failed: {}", resp.status()),
                    Err(err) => format!("compaction auto: request failed: {err}"),
                }
            }
            Err(err) => format!("compaction auto: thread ensure failed: {err}"),
        };
        let _ = tx.send(message).await;
    });
}

pub(super) fn spawn_compaction_auto_schedule(
    client: Client,
    server: String,
    tx: mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let message = match crate::ensure_thread(&client, &server).await {
            Ok(thread_id) => {
                let url = format!("{server}/threads/{thread_id}/compaction-auto-schedule");
                let response = client
                    .post(url)
                    .json(&serde_json::json!({
                        "stride_messages": null,
                        "max_new_checkpoints": null,
                        "block_on_inflight": true,
                        "execute": true,
                        "dry_run": false,
                        "actor_id": "user",
                        "origin": "tui"
                    }))
                    .send()
                    .await;
                match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<ripd::CompactionAutoScheduleV1Response>().await {
                            Ok(out) => match out.job_id {
                                Some(job_id) => format!(
                                    "compaction schedule: decision={} job_id={job_id}",
                                    out.decision
                                ),
                                None => {
                                    format!("compaction schedule: decision={}", out.decision)
                                }
                            },
                            Err(err) => format!("compaction schedule: parse failed: {err}"),
                        }
                    }
                    Ok(resp) => {
                        format!("compaction schedule: request failed: {}", resp.status())
                    }
                    Err(err) => format!("compaction schedule: request failed: {err}"),
                }
            }
            Err(err) => format!("compaction schedule: thread ensure failed: {err}"),
        };
        let _ = tx.send(message).await;
    });
}

pub(super) fn spawn_compaction_status(client: Client, server: String, tx: mpsc::Sender<String>) {
    tokio::spawn(async move {
        let message = match crate::ensure_thread(&client, &server).await {
            Ok(thread_id) => {
                let url = format!("{server}/threads/{thread_id}/compaction-status");
                let response = client
                    .post(url)
                    .json(&serde_json::json!({ "stride_messages": null }))
                    .send()
                    .await;
                match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<ripd::CompactionStatusV1Response>().await {
                            Ok(status) => {
                                let ckpt = status
                                    .latest_checkpoint
                                    .as_ref()
                                    .map(|c| c.to_seq.to_string())
                                    .unwrap_or_else(|| "none".to_string());
                                let next = status
                                    .next_cut_point
                                    .as_ref()
                                    .map(|c| c.to_seq.to_string())
                                    .unwrap_or_else(|| "none".to_string());
                                let sched = status
                                    .last_schedule_decision
                                    .as_ref()
                                    .map(|d| d.decision.as_str())
                                    .unwrap_or("none");
                                let job = status
                                    .last_job_outcome
                                    .as_ref()
                                    .map(|j| j.status.as_str())
                                    .unwrap_or("none");
                                let inflight = status
                                    .inflight_job_id
                                    .as_deref()
                                    .map(|id| {
                                        let short = id.chars().take(16).collect::<String>();
                                        format!(" inflight={short}")
                                    })
                                    .unwrap_or_default();
                                format!(
                                    "compaction status: messages={} ckpt_to_seq={} next_to_seq={} sched={} job={}{}",
                                    status.message_count, ckpt, next, sched, job, inflight
                                )
                            }
                            Err(err) => format!("compaction status: parse failed: {err}"),
                        }
                    }
                    Ok(resp) => format!("compaction status: request failed: {}", resp.status()),
                    Err(err) => format!("compaction status: request failed: {err}"),
                }
            }
            Err(err) => format!("compaction status: thread ensure failed: {err}"),
        };
        let _ = tx.send(message).await;
    });
}

pub(super) fn spawn_provider_cursor_status(
    client: Client,
    server: String,
    tx: mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let message = match crate::ensure_thread(&client, &server).await {
            Ok(thread_id) => {
                let url = format!("{server}/threads/{thread_id}/provider-cursor-status");
                let response = client.post(url).json(&serde_json::json!({})).send().await;
                match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<ripd::ProviderCursorStatusV1Response>().await {
                            Ok(status) => match status.active {
                                Some(active) => {
                                    let prev = active
                                        .cursor
                                        .as_ref()
                                        .and_then(|value| {
                                            value
                                                .get("previous_response_id")
                                                .and_then(|value| value.as_str())
                                        })
                                        .unwrap_or("");
                                    let prev_short = prev.chars().take(16).collect::<String>();
                                    let cursor_desc =
                                        if active.cursor.is_some() && !prev_short.is_empty() {
                                            format!("prev={prev_short}")
                                        } else if active.cursor.is_some() {
                                            "cursor=set".to_string()
                                        } else {
                                            "cursor=none".to_string()
                                        };
                                    format!(
                                        "provider cursor: action={} {}",
                                        active.action, cursor_desc
                                    )
                                }
                                None => "provider cursor: none".to_string(),
                            },
                            Err(err) => format!("provider cursor status: parse failed: {err}"),
                        }
                    }
                    Ok(resp) => {
                        format!("provider cursor status: request failed: {}", resp.status())
                    }
                    Err(err) => format!("provider cursor status: request failed: {err}"),
                }
            }
            Err(err) => format!("provider cursor status: thread ensure failed: {err}"),
        };
        let _ = tx.send(message).await;
    });
}

pub(super) fn spawn_provider_cursor_rotate(
    client: Client,
    server: String,
    tx: mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let message = match crate::ensure_thread(&client, &server).await {
            Ok(thread_id) => {
                let url = format!("{server}/threads/{thread_id}/provider-cursor-rotate");
                let response = client
                    .post(url)
                    .json(&serde_json::json!({
                        "provider": null,
                        "endpoint": null,
                        "model": null,
                        "reason": "tui",
                        "actor_id": "user",
                        "origin": "tui"
                    }))
                    .send()
                    .await;
                match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<ripd::ProviderCursorRotateV1Response>().await {
                            Ok(out) => {
                                if out.rotated {
                                    "provider cursor: rotated".to_string()
                                } else {
                                    "provider cursor: rotate noop".to_string()
                                }
                            }
                            Err(err) => format!("provider cursor rotate: parse failed: {err}"),
                        }
                    }
                    Ok(resp) => {
                        format!("provider cursor rotate: request failed: {}", resp.status())
                    }
                    Err(err) => format!("provider cursor rotate: request failed: {err}"),
                }
            }
            Err(err) => format!("provider cursor rotate: thread ensure failed: {err}"),
        };
        let _ = tx.send(message).await;
    });
}

pub(super) fn spawn_context_selection_status(
    client: Client,
    server: String,
    tx: mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let message = match crate::ensure_thread(&client, &server).await {
            Ok(thread_id) => {
                let url = format!("{server}/threads/{thread_id}/context-selection-status");
                let response = client
                    .post(url)
                    .json(&serde_json::json!({ "limit": 1 }))
                    .send()
                    .await;
                match response {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<ripd::ContextSelectionStatusV1Response>().await {
                            Ok(status) => match status.decisions.first() {
                                Some(active) => {
                                    let ckpt = active
                                        .compaction_checkpoint
                                        .as_ref()
                                        .map(|c| c.to_seq.to_string())
                                        .unwrap_or_else(|| "none".to_string());
                                    format!(
                                        "context selection: strategy={} ckpt_to_seq={} resets={}",
                                        active.compiler_strategy,
                                        ckpt,
                                        active.resets.len()
                                    )
                                }
                                None => "context selection: none".to_string(),
                            },
                            Err(err) => {
                                format!("context selection status: parse failed: {err}")
                            }
                        }
                    }
                    Ok(resp) => format!(
                        "context selection status: request failed: {}",
                        resp.status()
                    ),
                    Err(err) => format!("context selection status: request failed: {err}"),
                }
            }
            Err(err) => format!("context selection: thread ensure failed: {err}"),
        };
        let _ = tx.send(message).await;
    });
}

/// `ErrorRecoveryRotateCursor` uses a minimal POST body — unlike
/// `ProviderCursorRotate` which sends `{provider, endpoint, model, reason, actor_id, origin}`.
/// We keep the two paths distinct because the recovery path is a
/// "panic-button" rotate (no specified target), while the palette
/// command surfaces the full intent fields.
pub(super) fn spawn_error_recovery_rotate_cursor(
    client: Client,
    server: String,
    tx: mpsc::Sender<String>,
) {
    tokio::spawn(async move {
        let message = match crate::ensure_thread(&client, &server).await {
            Ok(thread_id) => {
                let url = format!("{server}/threads/{thread_id}/provider-cursor-rotate");
                match client.post(url).send().await {
                    Ok(resp) if resp.status().is_success() => "provider cursor rotated".to_string(),
                    Ok(resp) => format!("rotate cursor: {}", resp.status()),
                    Err(err) => format!("rotate cursor: {err}"),
                }
            }
            Err(err) => format!("rotate cursor: {err}"),
        };
        let _ = tx.send(message).await;
    });
}
