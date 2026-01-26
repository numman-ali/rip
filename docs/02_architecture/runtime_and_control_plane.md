# Runtime vs Control Plane (and "Remote Runtime")

Summary
- **Runtime** executes the agent loop, tools, workspace actions, and emits canonical event frames.
- **Control plane** is a transport/API surface that lets clients drive a runtime (create/run sessions, stream frames, etc.).
- "Server" is ambiguous; in RIP docs, prefer **control plane** (API surface) vs **runtime** (engine).

Definitions
- **Runtime**: the in-process engine that runs a session (LLM loop + tools + logging/replay).
  - Local runtime: embedded inside `rip`/`rip run`.
  - Remote runtime: running elsewhere (another host/process/container).
- **Control plane**: the session API surface (HTTP/SSE today) used by clients (TUI/SDK/remote CLI) to drive a runtime and observe frames.
  - Local control plane: `rip serve` / embedded server for remote clients.
  - Remote control plane: a network-accessible endpoint that fronts a remote runtime.
- **Store**: the persistence boundary for continuity truth + artifacts (today: `RIP_DATA_DIR`).
- **Authority**: the single sequencer/writer for truth writes for a store (ADR-0019).
  - Many clients may attach to an authority (many terminals/devices), but truth writes must be sequenced through one authority to preserve replay determinism.
- **Provider boundary**: Open Responses ingress/egress only; internal frames remain canonical.

Entity model (core data modeling)
- **Continuity** (`continuity_id`): durable, user-facing identity/state (default UX: one continuity forever). Sessions attach to a continuity.
- **Session** (`session_id`): one agent run/turn producing an ordered stream of frames; persisted via event log + snapshots.
- **Task** (`task_id`): a background tool execution entity with its own event stream (`tool_task_*` frames); may outlive a session.
- **Artifact** (`artifact_id`): stored blob referenced by frames/tools (range-readable; prevents context/log explosion).
- **Provider cursor**: ephemeral per-provider conversation handle (e.g., Open Responses `previous_response_id` chain); treated as a cache and may be rotated/rebuilt.

Surface responsibilities (no business logic in surfaces)
- **Headless CLI (local runtime)**: runs sessions/tools and emits JSONL frames; primary automation surface.
- **TUI (local runtime)**: renders frames and provides interactive IO; no capability semantics.
- **Control plane (server)**: exposes session control + streaming of the same frames; enables remote clients.
- **Remote mode** (`--server <url>`): a transport choice for clients; must not change semantics.
- **SDK**: adapter over canonical frames + control plane; no business logic.

Capabilities vs tools (important distinction)
- **Capability**: a named, versioned feature of the harness (see `docs/03_contracts/capability_registry.md`) exposed across surfaces (CLI/TUI/control plane/SDK). Capabilities are how we mutate/observe Continuity OS truth deterministically.
  - Examples: `thread.post_message`, `thread.branch`, `thread.handoff`, `thread.stream_events`, `checkpoint.*`, `tool.task_*`.
- **Tool**: a model-invoked runtime primitive executed *within a session/run* by the tool runtime (e.g., shell/file tools). Tools produce `tool_*` frames and are always subject to policy/budgets.
  - Tools are not the primary interface for manipulating Continuity OS structure (threads/compaction/lineage).
- Default posture:
  - Continuity OS “internal management” operations ship as **capabilities** first, available through every surface adapter.
  - If we later want subagents/workers to perform management actions autonomously, we add **explicit tool wrappers** that call the same capability implementation, are policy-gated, and emit replayable tool + continuity frames (no bypass paths).

Rules to avoid confusion
- Tool runtime and background tasks (task entities) are **runtime capabilities**. Transports (control plane endpoints, stdio protocols, etc.) are adapters.
- Prefer phrasing:
  - "local runtime" vs "remote runtime"
  - "control plane (server)" vs "runtime"
  - "remote attach" (client -> control plane) vs "local run" (client -> runtime)

Implementation note (Phase 1)
- Today, the `ripd` crate contains both runtime code and the HTTP/SSE control plane implementation. Treat them as **conceptually separate modules** even if they ship together in Phase 1.
- Local multi-terminal posture (implemented v0.1): “one store just works” by auto-start/auto-attach to a per-store local authority for that store (ADR-0019).
  - Discovery: `RIP_DATA_DIR/authority/meta.json` (endpoint + pid).
  - Lock: `RIP_DATA_DIR/authority/lock.json` (single-writer).
  - Spawned authority binds `RIP_SERVER_ADDR=127.0.0.1:0` (ephemeral port); clients verify liveness via `/openapi.json`.
  - Explicit `--server <url>` remains authoritative and bypasses local auto-start/attach.

## Local authority v0.2: lifecycle + stale-lock recovery

Authority files
- `RIP_DATA_DIR/authority/lock.json`: lock record `{pid, started_at_ms, workspace_root}` created by the authority process.
- `RIP_DATA_DIR/authority/meta.json`: discovery record `{endpoint, pid, started_at_ms, workspace_root}` written after bind (atomic write).
- `RIP_DATA_DIR/authority/authority.log`: stdout/stderr for the background authority process (local auto-start only).

Client attach behavior (default local CLI/TUI)
- If `meta.json` exists and `GET {endpoint}/openapi.json` succeeds: attach immediately.
- If `lock.json` exists but `meta.json` is missing: treat as **authority starting** and wait (no cleanup while `pid` is live).
- If `meta.json` exists but the endpoint is unreachable:
  - If `pid` is **live**: treat as **authority unavailable/restarting** and wait (deterministic backoff).
  - If `pid` is **dead**: treat as **stale lock** and recover by atomically reclaiming `lock.json` (rename tombstone + delete), then respawn.
- If `lock.json` is invalid JSON and `meta.json` is absent (crash/partial write): wait briefly, then recover by atomically reclaiming the lock under contention.

Safety invariants
- Never delete a lock based only on “no meta yet” or “endpoint unreachable”; cleanup requires `pid` death (or lock corruption after a grace period).
- Cleanup is contention-safe: clients atomically rename the observed `lock.json` before deleting, so they cannot remove a newly-created lock.
- Workspace-root mismatches are fatal: a store is bound to exactly one `workspace_root` per authority; clients refuse to attach/recover when roots disagree.
