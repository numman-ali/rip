# Task 05: Event log + snapshots

Goal
- Implement append-only event log and derived snapshots.

Inputs
- Module contract: docs/03_contracts/modules/phase-1/07_event_log.md

Outputs
- Log writer/reader and snapshot generator.

Acceptance criteria
- Replay reconstructs final snapshot deterministically.
- Corruption detection signals failures.

Tests
- Replay equivalence tests.
- Corruption detection tests.

Benchmarks
- Append throughput.
- Replay speed to latest snapshot.
