# ADR-0007: Interactive process control is modeled as background tool tasks (Task entity, PTY optional)

Status
- Accepted

Context
- We need first-class support for **interactive** and **long-lived** command execution:
  - interactive CLIs that prompt (stdin required; PTY required for many tools)
  - background processes (dev servers, watchers) with log tailing and later inspection
  - cancellation/kill, signals, and terminal resizing
  - deterministic replay and surface parity (CLI/headless/server/SDK/TUI)
- Phase 1 invariant: **one session == one agent run** (`session_ended` is terminal). Background activity must not require keeping sessions alive.
- Current Phase 1 tool execution is one-shot and pipes-based; it cannot model interactive stdin/PTY without a new runtime abstraction.
- Reference implementations:
  - Codex `unified_exec`: PTY optional spawn, stable `process_id`, `write_stdin`, streaming output deltas, and an exit watcher (see `temp/codex/codex-rs/core/src/unified_exec/`).
  - OpenCode: explicit `/pty` resources, resize/write operations, and interactive connect over WebSocket (see `temp/opencode/packages/opencode/src/pty/`).
  - Pi Mono: tools stream output and persist full logs for later retrieval; wakeups are an orchestrator concern (see `temp/pi-mono/packages/coding-agent/src/core/tools/bash.ts`, `temp/pi-mono/packages/mom/src/agent.ts`).

Decision
- Represent interactive and long-lived process execution as **background tool tasks** (a first-class `task_id` entity), independent of session lifetime:
  - A task has a stable id, metadata, status, and its own event stream.
  - A task can execute in `pipes` mode (stdout/stderr) or `pty` mode (terminal byte stream + stdin).
- Expose interactive control via explicit capabilities and APIs:
  - `tool.task_write_stdin` (send input bytes)
  - `tool.task_resize` (rows/cols)
  - `tool.task_signal` (SIGINT/SIGTERM/etc; platform-mapped)
  - plus the existing `tool.task_spawn/status/cancel`
- Keep large/long-running outputs bounded via artifact-backed logs:
  - frames carry bounded previews/tails
  - full logs are stored as artifacts with range fetch
- Model “wake the agent” as orchestrated, replayable behavior:
  - watchers (SDK/CLI/extensions/server policies) observe task events and start a new session referencing `{task_id, artifact_refs}`.
  - no hidden background work; all task lifecycle and IO is logged as frames.

Consequences
- Pros:
  - Preserves Phase 1 session invariants while enabling background work.
  - Enables maximum flexibility (interactive prompts, dev servers, log tailing) across all surfaces.
  - Deterministic replay: task lifecycle + IO is a recorded event stream; status APIs are pure projections.
- Cons:
  - Requires a task/process registry and additional API surface area.
  - PTY support is platform-sensitive and must be policy-gated.
  - “Wakeups” require orchestration logic (client/server hook) rather than being implicit.
- Reversibility:
  - Future multi-turn sessions or “session-attached tasks” can be layered on top by consuming task streams, without changing task ids or replay logs.
  - Transports (SSE vs WebSocket for interactive attach) can evolve without changing the canonical task/event model.

