# Contract: workspace engine

Summary
- Applies patches, manages snapshots, and provides diffs.

Inputs
- Patch operations (apply, rollback, preview).
- File read/write requests.

Outputs
- Diffs, snapshots, and status events.

Config
- Workspace root.
- Snapshot retention policy.

Invariants
- Patch apply is atomic (success or revert).
- Snapshot IDs are stable and replayable.

Tests
- Patch apply/rollback fixtures.
- Diff correctness on sample repos.

Benchmarks
- Patch apply throughput on real repos.
