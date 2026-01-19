# Contract: Tool Tasks + Artifact-backed Outputs (Phase 2)

Summary
- Enables background tool execution (task handles) while preserving determinism, replay, and surface parity.
- Prevents context explosion by storing large tool outputs as artifacts and referencing them from event frames and tool outputs.

Related capabilities
- `tool.task_spawn`, `tool.task_status`, `tool.task_cancel`
- `tool.output_store`, `tool.output_fetch`
- `context.refs.artifact`
- `ui.background_tasks`
- `execution.rpc`, `execution.resume_session` (for programmatic orchestration)

Goals
- Allow any tool invocation to run as a background task:
  - return a stable task id immediately
  - stream outputs as structured events
  - support cancellation and status queries
- Ensure large outputs do not break the system:
  - inline frames carry bounded previews
  - full data lives in an artifact store and is retrievable by id + ranges
- Keep replay deterministic:
  - task lifecycle events and artifact references are fully recorded
  - replays can reproduce the same final snapshot and equivalent event stream

Non-goals (initial)
- Unlogged side effects (background tasks must not “do hidden work”).
- Cross-host distributed task execution without a logged RPC boundary.

Architecture (Phase 2)

1) Background tool tasks
- A background task is a tool invocation with a stable `task_id` (may reuse `tool_id`).
- Tool output events (`tool_stdout`/`tool_stderr`/`tool_ended`) are emitted against the same id over time.
- Status queries are derived from event log state (and may be served via a dedicated endpoint for convenience).

2) Artifact-backed outputs
- When output exceeds configured limits:
  - emit a truncated preview in event frames
  - persist full content into an artifact store (content-addressed or session-addressed)
  - emit references (artifact id, byte/line ranges, digest, size)
- Retrieval uses range APIs/tools to avoid loading full blobs into memory/context.

3) Agent orchestration (“insert message back”)
- Orchestrators (SDK/CLI/extensions) watch task events and can send a follow-up session input that references:
  - task id + terminal status, and/or
  - artifact ids (for summarization or targeted reading)
- Optional: policies/extensions may auto-inject completion summaries, but this must be explicit and replay-logged.

Determinism & replay rules
- Task state is derivable from the event log; status APIs must be pure projections.
- Artifact ids and digests must be stable; storing must be deterministic (no random names without recording).
- Cancellation must be logged and reproducible:
  - replays may simulate cancellation by truncating the task stream at the recorded cancel boundary.

Policy defaults (Phase 2)
- Safe profile: background tasks disabled by default unless explicitly allowed per tool + args.
- Full-auto profile: background tasks allowed per policy; still logged and bounded by budgets.

Tests (required)
- Contract tests:
  - spawn/status/cancel state machine is stable
  - background stdout/stderr ordering is deterministic
  - artifact references are emitted when outputs exceed limits
- Replay tests:
  - recorded task run reproduces identical final snapshot and equivalent event stream
- Benchmarks:
  - overhead of task registry bookkeeping (no-op)
  - artifact store write/read ranges under load
