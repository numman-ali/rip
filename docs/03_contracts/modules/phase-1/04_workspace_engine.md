# Contract: workspace engine

Summary
- Applies patches, manages snapshots, and provides diffs.

Inputs
- Patch operations (apply, rollback, preview).
- File read/write requests.

Outputs
- Diffs, snapshots, and status events.
- Status events map to `checkpoint_created`, `checkpoint_rewound`, and `checkpoint_failed` frames.

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
