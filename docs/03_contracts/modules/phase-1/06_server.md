# Contract: agent server

Summary
- Exposes the coding agent via HTTP/SSE sessions.
- Not an Open Responses API.
- Canonical control plane for active capabilities; API surface must match the capability registry.

Inputs
- Session lifecycle requests (start, send input, cancel).
- Thread ("continuity") requests (`thread.*`, `compaction.*`, provider cursor status/rotate, context selection status).
- Background tool task requests (`tool.task_*`, including optional PTY controls when policy-enabled).
- Tool/checkpoint command envelopes via session input (for deterministic testing).

Outputs
- Structured event stream over SSE (event frames: `docs/03_contracts/event_frames.md`).
- Session status and artifacts.
- Thread ("continuity") event streams over SSE (messages, run links, summaries).
- Task event streams over SSE (spawn/status/output deltas; terminal status).
- OpenAPI spec generated from server code and exposed at a canonical endpoint.

Config
- Bind address, auth, session limits.

Invariants
- One session maps to one agent run.
- Event stream is ordered and replayable.
- Workspace-mutating operations are serialized across sessions and background tasks; read-only tools may run concurrently.

Tests
- Session lifecycle integration tests.
- SSE stream compliance tests.
- OpenAPI schema generation/validation tests.

Phase 2 planned extensions
- Auth/ACL policy (multi-actor) for remote control planes.
- Advanced continuity semantics:
  - tags/search/archive/share/reference/map capabilities
  - cross-thread/global memory refs (Phase 3 posture)
- Artifact fetch/read surfaces beyond task logs (generic artifact range reads for large outputs).
- RPC/multiplexed execution modes and additional SDK transports.
