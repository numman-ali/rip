# Contract: ripd core runtime

Summary
- Central runtime that executes agent sessions and routes events.
- Owns session lifecycle, provider streaming integration, and tool dispatch.

Inputs
- Client requests from CLI/server (start session, send input, cancel).
- Provider stream events (via adapter).
- Tool outputs (via tool runtime).

Outputs
- Structured event stream for UI/SDK (event frames: `docs/03_contracts/event_frames.md`).
- Updates to event log + snapshots.
- Tool invocations to tool runtime.

Config
- Max concurrency (sessions, tools, tasks).
- Tool budgets and timeouts.
- Workspace root and artifact store paths.

Invariants
- Deterministic processing order given the same event stream.
- No blocking on background workers.
- All outputs are structured events.
- Workspace-mutating operations (tool calls + background tasks) are serialized through a single workspace lock; read-only tools may run concurrently.

Tests
- Replay a golden stream and compare final snapshot.
- Concurrency tests for workspace mutation serialization across sessions + tasks.

Benchmarks
- Event routing latency (per event).
