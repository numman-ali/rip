# Contract: ripd core runtime

Summary
- Central runtime that executes agent sessions and routes events.
- Owns scheduling, sub-agent orchestration, and tool dispatch.

Inputs
- Client requests from CLI/server (start session, send input, cancel).
- Provider stream events (via adapter).
- Tool outputs (via tool runtime).

Outputs
- Structured event stream for UI/SDK.
- Updates to event log + snapshots.
- Tool invocations to tool runtime.

Config
- Max concurrency (agents, tools).
- Tool budgets and timeouts.
- Policy profile (fast vs deep).

Invariants
- Deterministic processing order given the same event stream.
- No blocking on background workers.
- All outputs are structured events.

Tests
- Replay a golden stream and compare final snapshot.
- Concurrency tests for sub-agent scheduling.

Benchmarks
- Event routing latency (per event).
- Sub-agent spawn latency.
