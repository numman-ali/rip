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

Rules to avoid confusion
- Tools and background tasks are **runtime capabilities**. Transports (control plane endpoints, stdio protocols, etc.) are adapters.
- Prefer phrasing:
  - "local runtime" vs "remote runtime"
  - "control plane (server)" vs "runtime"
  - "remote attach" (client -> control plane) vs "local run" (client -> runtime)

Implementation note (Phase 1)
- Today, the `ripd` crate contains both runtime code and the HTTP/SSE control plane implementation. Treat them as **conceptually separate modules** even if they ship together in Phase 1.
