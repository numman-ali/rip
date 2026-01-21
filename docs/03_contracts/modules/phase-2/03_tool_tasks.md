# Contract: Tool Tasks + Artifact-backed Outputs (Phase 2)

Summary
- Enables background, long-lived, and interactive tool execution **as first-class tasks** while preserving determinism, replay, and surface parity.
- Prevents context explosion by storing large tool outputs as artifacts and referencing them from frames and tool outputs.
- Adds PTY-backed interactive process control (stdin/resize/signal) without breaking Phase 1 session invariants.

Decision
- See `docs/06_decisions/ADR-0007-tool-tasks-pty.md`.

Current status (implemented)
- Pipes-mode background tasks are implemented and exposed via server + CLI:
  - Task lifecycle/events: `POST /tasks`, `GET /tasks`, `GET /tasks/{id}`, `GET /tasks/{id}/events`, `POST /tasks/{id}/cancel`.
  - Artifact-backed log tailing: `GET /tasks/{id}/output?stream=stdout|stderr|pty&offset_bytes=...&max_bytes=...` (stream depends on task mode).
  - Frames: `tool_task_*` (see `docs/03_contracts/event_frames.md`).
- PTY mode and interactive control operations (`stdin/resize/signal`) are implemented but policy-gated:
  - Spawn PTY tasks: `POST /tasks` with `execution_mode=pty`.
  - Control ops (PTY only): `POST /tasks/{id}/stdin`, `POST /tasks/{id}/resize`, `POST /tasks/{id}/signal`.

Related capabilities
- `tool.task_spawn`, `tool.task_status`, `tool.task_cancel`
- `tool.task_stream_events`, `tool.task_write_stdin`, `tool.task_resize`, `tool.task_signal`
- `tool.output_store`, `tool.output_fetch`
- `context.refs.artifact`
- `ui.background_tasks`
- `execution.rpc`, `execution.resume_session` (for programmatic orchestration)

Goals
- Allow any tool invocation to run as a background task:
  - return a stable `task_id` immediately
  - stream output as structured events over time
  - support cancellation, signals, and status queries
- Support interactive CLI programs and long-lived servers:
  - PTY-backed tasks that accept stdin and terminal resize
  - safe, replayable “attach” semantics across CLI/SDK/TUI
- Ensure large outputs do not break the system:
  - inline frames carry bounded previews
  - full data lives in an artifact store and is retrievable by id + ranges
- Keep replay deterministic:
  - task lifecycle events and artifact references are fully recorded
  - replays can reproduce the same final snapshot and equivalent event stream

Non-goals (initial)
- Unlogged side effects (background tasks must not “do hidden work”).
- Cross-host distributed task execution without a logged RPC boundary.
- A full “terminal multiplexer” UI as a core capability (surfaces may render/attach, but the core contract is task IO + events).

Architecture (Phase 2)

1) Background tool tasks (task entity)
- A background task is a tool invocation with a stable `task_id`.
- A task has:
  - metadata: `{tool_name, args, cwd?, title?, execution_mode}`
  - status: `{queued|running|exited|cancelled|failed}`
  - an event stream addressable by `task_id` (`tool.task_stream_events`)
- Phase 1 session invariant remains: **one session == one agent run**.
  - tasks are independent entities; they may outlive the session that spawned them.
  - “continue later / insert message back” is orchestration (see §3).

2) Task execution modes
- `pipes` (default): stdout/stderr streams; no stdin control.
  - best for non-interactive tools; lowest overhead.
- `pty` (opt-in): pseudo-terminal process session; supports stdin, terminal resize, and better compatibility with interactive CLIs.
  - output is a terminal byte stream (may contain ANSI control sequences).
  - status/cancellation semantics are the same; only IO differs.

3) Agent orchestration (“wake the agent”)
- Background tasks do not implicitly keep an agent session alive.
- Orchestrators (SDK/CLI/extensions/server policies) may:
  - watch task events (or poll task status) and,
  - start a new session input that references `{task_id, status, artifact_refs}`.
- Optional policy: “auto-wakeup” (server-side) is allowed only when explicit and fully logged (no hidden work).

4) Artifact-backed outputs
- When output exceeds configured limits:
  - emit a truncated preview in event frames
  - persist full content into an artifact store (content-addressed or session-addressed)
  - emit references (artifact id, byte/line ranges, digest, size)
- Retrieval uses range APIs/tools to avoid loading full blobs into memory/context.

5) Task control (interactive IO)
- Interactive control is modeled as explicit task operations that produce frames (replayable):
  - `tool.task_write_stdin`: send stdin bytes (PTY only)
  - `tool.task_resize`: resize rows/cols (PTY only)
  - `tool.task_signal`: SIGINT/SIGTERM/etc (platform-mapped)
- Each operation must be:
  - logged as an event frame (including payload size bounds)
  - applied in-order relative to the task’s output stream

Determinism & replay rules
- Task state is derivable from the event log; status APIs must be pure projections.
- Every externally-observable transition is logged:
  - spawn accepted/rejected
  - process started
  - stdout/stderr/pty output deltas (bounded payloads)
  - stdin writes / resizes / signals
  - exit / cancel / failure
- Artifact ids and digests must be stable; storing must be deterministic (no random names without recording).
- Replay of tasks:
  - must not re-execute real processes; it replays the recorded task event stream.
  - “attach” during replay is equivalent to attaching to the recorded stream.

Policy defaults (Phase 2)
- Safe profile: background tasks disabled by default unless explicitly allowed per tool + args.
- Safe profile: PTY mode disabled by default (requires explicit allow + justification, because PTYs enable interactive programs and can hide prompts).
- Full-auto profile: background tasks allowed per policy; still logged and bounded by budgets.
- Full-auto profile: PTY mode allowed per policy, still bounded (output/event caps, timeouts, kill escalation).

Tests (required)
- Contract tests:
  - spawn/status/cancel state machine is stable
  - task event ordering is deterministic
  - PTY stdin/resize/signal are applied in-order and produce frames
  - artifact references are emitted when outputs exceed limits
- Replay tests:
  - recorded task run reproduces identical final snapshot and equivalent event stream
  - attach/reconnect is deterministic (late subscribers see the same tail + artifact refs)
- Benchmarks:
  - overhead of task registry bookkeeping (no-op)
  - artifact store write/read ranges under load
  - per-event overhead for task output deltas (pipes vs PTY)

Frames (planned)
- See `docs/03_contracts/event_frames.md` (Phase 2 planned additions).
- Minimum lifecycle coverage:
  - `tool_task_spawned` (task metadata)
  - output deltas (`tool_stdout`/`tool_stderr` for pipes; `tool_task_output_delta(stream=pty)` for PTY)
  - control operations (`tool_task_stdin_written`, `tool_task_resized`, `tool_task_signalled`)
  - termination (`tool_ended` and/or `tool_task_status(status=exited|cancelled|failed)` with `exit_code`)
