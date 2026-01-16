# Contract: event log + snapshots

Summary
- Append-only event log is the source of truth.
- Snapshots provide fast reads and deterministic replay.

Inputs
- Structured events from ripd.

Outputs
- Replay streams and snapshots.

Config
- Log retention and snapshot cadence.

Invariants
- Log is append-only and ordered.
- Snapshots are derived; never the source of truth.

Tests
- Replay -> snapshot equivalence.
- Corruption detection tests.

Benchmarks
- Append throughput.
- Replay speed to last snapshot.
