# Compaction (Cut Points + Auto Summarization)

Summary
- Compaction keeps long continuities replayable and fast by inserting deterministic checkpoints (ADR-0011) that reference immutable summary artifacts (`rip.compaction_summary.v1`).
- Auto compaction is executed as a background job over the continuity stream (ADR-0012) and emits only continuity truth + artifact refs (no hidden mutable state).
- Providers remain replaceable compute; compaction outputs are internal artifacts referenced by id and consumed by `context.compile` strategies.

Related docs
- Decisions:
  - `docs/06_decisions/ADR-0011-compaction-cutpoints-summaries.md`
  - `docs/06_decisions/ADR-0012-background-jobs.md`
  - `docs/06_decisions/ADR-0010-context-compiler-truth.md`
- Contracts:
  - `docs/03_contracts/event_frames.md`
  - `docs/03_contracts/compaction_summary.md`
  - `docs/03_contracts/context_bundle.md`

## Capability: `compaction.cut_points` (v1)

Intent
- Deterministically compute stride-based cut points from continuity truth (message-count based; message boundaries only).
- Used by:
  - `compaction.auto` (to pick the next cut point),
  - diagnostics (operators can inspect “what would be summarized next”).

Inputs
- `thread_id`: string (required)
- `stride_messages`: u64 (optional; default: `10_000`)
- `limit`: u32 (optional; default: `1`)
  - Maximum number of cut points returned (latest-first). This keeps the response bounded.

Outputs
- `thread_id`: string
- `stride_messages`: u64
- `message_count`: u64
  - Count of `continuity_message_appended` events in the thread (not total continuity events).
- `cut_rule_id`: string
  - Format: `stride_messages_v1/<stride_messages>`.
- `cut_points`: array (length ≤ `limit`)
  - Each entry:
    - `target_message_ordinal`: u64 (1-based ordinal among `continuity_message_appended` events)
    - `to_seq`: u64 (continuity seq; must be a message boundary)
    - `to_message_id`: string (continuity event id; must identify a `continuity_message_appended`)
    - `already_checkpointed`: bool
    - `latest_checkpoint_id`: string | null (when `already_checkpointed=true`)

Errors (non-exhaustive)
- `thread_not_found`
- `invalid_stride` (stride == 0)
- `limit_too_large` (implementation-defined cap; must be bounded)

Determinism invariants
- `message_count` is derived only from continuity truth (`continuity_message_appended` count).
- Cut points are **message-count stride** based:
  - eligible ordinals are multiples of `stride_messages` (`stride, 2*stride, ...`) up to `message_count`.
  - each cut point’s `{to_seq,to_message_id}` must identify the **Nth** message event by ordinal.
- The response is stable given the same continuity stream, independent of caches:
  - caches may accelerate lookups but must not change the computed cut points.

## Capability: `compaction.auto` (v1)

Intent
- Create new deterministic compaction checkpoints when eligible cut points exist.
- Represent execution as a background job (ADR-0012) that writes summary artifacts + appends checkpoint frames.

Inputs
- `thread_id`: string (required)
- `stride_messages`: u64 (optional; default: `10_000`)
- `max_new_checkpoints`: u32 (optional; default: `1`)
  - Bounds work per invocation.
- `dry_run`: bool (optional; default: `false`)
  - When true, compute and report the planned cut point(s) but do not write artifacts or append continuity frames.
- `actor_id`: string (required)
- `origin`: string (required)

Outputs
- `thread_id`: string
- `job_id`: string | null
  - Present when a job was spawned (non-dry-run and there is eligible work).
- `job_kind`: string | null
  - Example: `compaction_summarizer_v1`.
- `status`: `"noop" | "spawned" | "completed" | "failed"`
- `planned`: array
  - Each entry:
    - `target_message_ordinal`, `to_seq`, `to_message_id`
- `result`: array
  - Populated when `status="completed"`:
    - `checkpoint_id`, `summary_artifact_id`, `to_seq`, `to_message_id`, `cut_rule_id`
- `error`: string | null (only when `status="failed"`)

Job representation (continuity truth)
- When `dry_run=false` and there is eligible work:
  - append `continuity_job_spawned` (records the resolved cut point and any recorded inputs)
  - write one or more `rip.compaction_summary.v1` artifacts (immutable)
  - append `continuity_compaction_checkpoint_created` referencing each summary artifact id
  - append `continuity_job_ended` with status + result ids
- When there is no eligible work (`status="noop"`), the capability must not emit continuity truth frames.

Determinism invariants
- Cut point selection uses `compaction.cut_points` rules (message-count stride; message boundaries only).
- Any behavior-changing decision is captured by appended continuity frames + artifact ids:
  - the resolved `{to_seq,to_message_id,target_message_ordinal,cut_rule_id}` appears in job/checkpoint frames.
- Concurrency is replay-safe:
  - multiple jobs may target the same cut point; later checkpoint frames supersede earlier ones by stream order (ADR-0011).

## Capability: `compaction.auto.schedule` (v1)

Intent
- Deterministically decide **when/why** to run `compaction.auto` under an explicit scheduling policy (ADR-0013).
- The scheduling decision is logged as continuity truth (no hidden mutable state).
- This capability is designed to be invoked out-of-band (server worker / cron / operator), so `thread.post_message` stays fast.

Inputs
- `thread_id`: string (required)
- `stride_messages`: u64 (optional; default: `10_000`)
- `max_new_checkpoints`: u32 (optional; default: `1`)
- `block_on_inflight`: bool (optional; default: `true`)
  - When true, do not start new compaction work if a compaction summarizer job is already in-flight for the thread (best-effort, derived from continuity truth).
- `execute`: bool (optional; default: `true`)
  - When true, the scheduler starts the job immediately (server may execute asynchronously).
  - When false, the scheduler records the decision and spawns the job, but does not execute it (future external job runners can execute pending jobs).
- `dry_run`: bool (optional; default: `false`)
  - When true, compute the planned cut point(s) but do not emit continuity truth frames or start jobs.
- `actor_id`: string (required)
- `origin`: string (required)

Outputs
- `thread_id`: string
- `decision_id`: string | null
  - Present when the scheduler emitted a `continuity_compaction_auto_schedule_decided` frame (non-dry-run, eligible work).
- `policy_id`: string
  - A versioned identifier that captures resolved policy parameters.
- `decision`: `"noop" | "skipped_inflight" | "scheduled" | "completed" | "failed"`
- `execute`: bool
- `job_id`: string | null
- `job_kind`: string | null
- `planned`: array
  - Each entry: `target_message_ordinal`, `to_seq`, `to_message_id`
- `result`: array
  - When `decision="completed"`: same shape as `compaction.auto` results (`checkpoint_id`, `summary_artifact_id`, `to_seq`, `to_message_id`, `cut_rule_id`)
- `error`: string | null

Continuity-truth decision frame
- When the scheduler is invoked and there is eligible work, it appends:
  - `continuity_compaction_auto_schedule_decided`
    - records policy parameters, planned cut point(s), and the decision outcome
    - links to the spawned job id when scheduled

Determinism invariants
- The scheduler decision must be derived only from:
  - continuity truth (messages/checkpoints/jobs),
  - and the explicit request parameters.
- Cache posture:
  - caches may accelerate evaluation but must not change the computed plan/decision.
