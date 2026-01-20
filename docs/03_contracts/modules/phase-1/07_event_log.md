# Contract: event log + snapshots

Summary
- Append-only event log is the source of truth.
- Snapshots provide fast reads and deterministic replay.

Inputs
- Structured event frames from ripd (`docs/03_contracts/event_frames.md`).

Outputs
- Replay streams and snapshots.

Config
- Log retention and snapshot cadence.

Invariants
- Log is append-only and ordered.
- Snapshots are derived; never the source of truth.

Phase 2 planned extensions
- The log stores multiple stream kinds (sessions, tasks, threads) while preserving per-stream ordering.
- See `docs/03_contracts/event_frames.md` (Phase 2 planned stream-scoped envelope) and `docs/03_contracts/modules/phase-2/03_tool_tasks.md`.

Tests
- Replay -> snapshot equivalence.
- Corruption detection tests.

Benchmarks
- Append throughput.
- Replay speed to last snapshot.
