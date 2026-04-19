//! Unit tests for the `format_*` success-path helpers in `actions.rs`.
//!
//! Each response type is constructed via `serde_json::from_value` so
//! tests exercise the real `Deserialize` surface without depending on
//! whether every nested struct is re-exported from `ripd`'s crate root.
//! The `Deserialize` impls are part of the capability contract, so
//! this is the right abstraction layer for test fixtures.

use super::*;
use serde_json::json;

fn parse<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("response json should deserialize")
}

#[test]
fn cut_points_latest_entry_renders_fields() {
    let resp = parse::<ripd::CompactionCutPointsV1Response>(json!({
        "thread_id": "t1",
        "stride_messages": 10,
        "message_count": 25,
        "cut_rule_id": "rule-a",
        "cut_points": [{
            "target_message_ordinal": 3,
            "to_seq": 42,
            "to_message_id": "m3",
            "already_checkpointed": false,
            "latest_checkpoint_id": null,
        }],
    }));
    assert_eq!(
        format_compaction_cut_points(&resp),
        "cut_points: messages=25 latest ordinal=3 to_seq=42 checkpointed=false"
    );
}

#[test]
fn cut_points_empty_renders_no_eligible() {
    let resp = parse::<ripd::CompactionCutPointsV1Response>(json!({
        "thread_id": "t1",
        "stride_messages": 10,
        "message_count": 4,
        "cut_rule_id": "rule-a",
        "cut_points": [],
    }));
    assert_eq!(
        format_compaction_cut_points(&resp),
        "cut_points: messages=4 (no eligible cut points)"
    );
}

fn compaction_auto(status: &str, job_id: Option<&str>) -> ripd::CompactionAutoV1Response {
    parse(json!({
        "thread_id": "t1",
        "job_id": job_id,
        "job_kind": null,
        "status": status,
        "stride_messages": 10,
        "message_count": 20,
        "cut_rule_id": "rule-a",
        "planned": [],
        "result": [],
        "error": null,
    }))
}

#[test]
fn compaction_auto_with_job_id_includes_it() {
    let resp = compaction_auto("running", Some("job-abc"));
    assert_eq!(
        format_compaction_auto(&resp),
        "compaction auto: status=running job_id=job-abc"
    );
}

#[test]
fn compaction_auto_without_job_id_omits_it() {
    let resp = compaction_auto("planned", None);
    assert_eq!(
        format_compaction_auto(&resp),
        "compaction auto: status=planned"
    );
}

fn schedule(decision: &str, job_id: Option<&str>) -> ripd::CompactionAutoScheduleV1Response {
    parse(json!({
        "thread_id": "t1",
        "decision_id": null,
        "policy_id": "policy-default",
        "decision": decision,
        "execute": false,
        "stride_messages": 10,
        "max_new_checkpoints": 1,
        "block_on_inflight": true,
        "message_count": 4,
        "cut_rule_id": "rule-a",
        "planned": [],
        "job_id": job_id,
        "job_kind": null,
        "result": [],
        "error": null,
    }))
}

#[test]
fn schedule_without_job_id_reports_decision_only() {
    let resp = schedule("skip", None);
    assert_eq!(
        format_compaction_auto_schedule(&resp),
        "compaction schedule: decision=skip"
    );
}

#[test]
fn schedule_with_job_id_includes_it() {
    let resp = schedule("run", Some("job-xyz"));
    assert_eq!(
        format_compaction_auto_schedule(&resp),
        "compaction schedule: decision=run job_id=job-xyz"
    );
}

#[test]
fn compaction_status_all_none_renders_nones() {
    let resp = parse::<ripd::CompactionStatusV1Response>(json!({
        "thread_id": "t1",
        "stride_messages": 10,
        "message_count": 0,
        "latest_checkpoint": null,
        "next_cut_point": null,
        "inflight_job_id": null,
        "last_schedule_decision": null,
        "last_job_outcome": null,
    }));
    assert_eq!(
        format_compaction_status(&resp),
        "compaction status: messages=0 ckpt_to_seq=none next_to_seq=none sched=none job=none"
    );
}

#[test]
fn compaction_status_populated_renders_all_fields_and_truncates_inflight() {
    let resp = parse::<ripd::CompactionStatusV1Response>(json!({
        "thread_id": "t1",
        "stride_messages": 10,
        "message_count": 50,
        "latest_checkpoint": {
            "checkpoint_id": "ckpt-1",
            "cut_rule_id": "rule-a",
            "summary_kind": "summary",
            "summary_artifact_id": "art-1",
            "to_seq": 100,
            "to_message_id": "m99",
        },
        "next_cut_point": {
            "target_message_ordinal": 55,
            "to_seq": 150,
            "to_message_id": "m149",
        },
        "inflight_job_id": "job-longer-than-sixteen-chars-total",
        "last_schedule_decision": {
            "decision_id": "d1",
            "policy_id": "p1",
            "decision": "run",
            "execute": true,
            "stride_messages": 10,
            "max_new_checkpoints": 1,
            "block_on_inflight": true,
            "message_count": 50,
            "cut_rule_id": "rule-a",
            "planned": [],
            "job_id": null,
            "job_kind": null,
            "actor_id": "user",
            "origin": "tui",
            "seq": 1,
            "timestamp_ms": 0,
        },
        "last_job_outcome": {
            "job_id": "job-prev",
            "job_kind": "compaction",
            "status": "succeeded",
            "error": null,
            "created": [],
            "actor_id": "user",
            "origin": "tui",
            "seq": 2,
            "timestamp_ms": 0,
        },
    }));
    assert_eq!(
        format_compaction_status(&resp),
        "compaction status: messages=50 ckpt_to_seq=100 next_to_seq=150 sched=run job=succeeded inflight=job-longer-than-"
    );
}

fn cursor_status(active: serde_json::Value) -> ripd::ProviderCursorStatusV1Response {
    parse(json!({
        "thread_id": "t1",
        "active": active,
        "cursors": [],
    }))
}

#[test]
fn provider_cursor_status_none_when_no_active() {
    let resp = cursor_status(json!(null));
    assert_eq!(
        format_provider_cursor_status(&resp),
        "provider cursor: none"
    );
}

#[test]
fn provider_cursor_status_with_previous_response_id_is_truncated() {
    let resp = cursor_status(json!({
        "cursor_event_id": "evt-1",
        "provider": "openai",
        "endpoint": null,
        "model": null,
        "cursor": { "previous_response_id": "resp_0123456789abcdef_extra" },
        "action": "bind",
        "reason": null,
        "run_session_id": null,
        "actor_id": "user",
        "origin": "tui",
        "seq": 1,
        "timestamp_ms": 0,
    }));
    assert_eq!(
        format_provider_cursor_status(&resp),
        "provider cursor: action=bind prev=resp_0123456789a"
    );
}

#[test]
fn provider_cursor_status_with_cursor_but_no_prev_says_cursor_set() {
    let resp = cursor_status(json!({
        "cursor_event_id": "evt-1",
        "provider": "openai",
        "endpoint": null,
        "model": null,
        "cursor": { "other": "field" },
        "action": "bind",
        "reason": null,
        "run_session_id": null,
        "actor_id": "user",
        "origin": "tui",
        "seq": 1,
        "timestamp_ms": 0,
    }));
    assert_eq!(
        format_provider_cursor_status(&resp),
        "provider cursor: action=bind cursor=set"
    );
}

#[test]
fn provider_cursor_status_without_cursor_says_cursor_none() {
    let resp = cursor_status(json!({
        "cursor_event_id": "evt-1",
        "provider": "openai",
        "endpoint": null,
        "model": null,
        "cursor": null,
        "action": "reset",
        "reason": null,
        "run_session_id": null,
        "actor_id": "user",
        "origin": "tui",
        "seq": 1,
        "timestamp_ms": 0,
    }));
    assert_eq!(
        format_provider_cursor_status(&resp),
        "provider cursor: action=reset cursor=none"
    );
}

fn rotate(rotated: bool) -> ripd::ProviderCursorRotateV1Response {
    parse(json!({
        "thread_id": "t1",
        "rotated": rotated,
        "provider": null,
        "endpoint": null,
        "model": null,
        "cursor_event_id": null,
    }))
}

#[test]
fn provider_cursor_rotate_reports_rotated() {
    assert_eq!(
        format_provider_cursor_rotate(&rotate(true)),
        "provider cursor: rotated"
    );
}

#[test]
fn provider_cursor_rotate_reports_noop() {
    assert_eq!(
        format_provider_cursor_rotate(&rotate(false)),
        "provider cursor: rotate noop"
    );
}

#[test]
fn context_selection_empty_returns_none() {
    let resp = parse::<ripd::ContextSelectionStatusV1Response>(json!({
        "thread_id": "t1",
        "decisions": [],
    }));
    assert_eq!(
        format_context_selection_status(&resp),
        "context selection: none"
    );
}

#[test]
fn context_selection_populated_renders_strategy_ckpt_resets() {
    let resp = parse::<ripd::ContextSelectionStatusV1Response>(json!({
        "thread_id": "t1",
        "decisions": [{
            "decision_event_id": "d1",
            "run_session_id": "r1",
            "message_id": "m1",
            "compiler_id": "default",
            "compiler_strategy": "default",
            "limits": {},
            "compaction_checkpoint": {
                "checkpoint_id": "ckpt-1",
                "summary_kind": "summary",
                "summary_artifact_id": "art-1",
                "to_seq": 80,
            },
            "compaction_checkpoints": [],
            "resets": [
                {"input": "user", "action": "skip", "reason": "fence", "ref": null},
                {"input": "tool", "action": "skip", "reason": "fence", "ref": null},
            ],
            "reason": null,
            "actor_id": "user",
            "origin": "tui",
            "seq": 1,
            "timestamp_ms": 0,
        }],
    }));
    assert_eq!(
        format_context_selection_status(&resp),
        "context selection: strategy=default ckpt_to_seq=80 resets=2"
    );
}

#[test]
fn context_selection_checkpoint_none_renders_none() {
    let resp = parse::<ripd::ContextSelectionStatusV1Response>(json!({
        "thread_id": "t1",
        "decisions": [{
            "decision_event_id": "d1",
            "run_session_id": "r1",
            "message_id": "m1",
            "compiler_id": "default",
            "compiler_strategy": "retrieval",
            "limits": {},
            "compaction_checkpoint": null,
            "compaction_checkpoints": [],
            "resets": [],
            "reason": null,
            "actor_id": "user",
            "origin": "tui",
            "seq": 1,
            "timestamp_ms": 0,
        }],
    }));
    assert_eq!(
        format_context_selection_status(&resp),
        "context selection: strategy=retrieval ckpt_to_seq=none resets=0"
    );
}
