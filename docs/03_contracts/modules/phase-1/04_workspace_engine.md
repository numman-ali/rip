# Contract: workspace engine

Summary
- Applies patches, manages snapshots, and provides diffs.

Inputs
- Patch operations (apply, rollback, preview).
- File read/write requests.

Outputs
- Diffs, snapshots, and status events.
- Status events map to `checkpoint_created`, `checkpoint_rewound`, and `checkpoint_failed` frames.

Patch format (Phase 1)
- The patch apply input is the Codex-style patch envelope:
  - `*** Begin Patch` / `*** End Patch`
  - `*** Add File: <path>` with `+`-prefixed content lines
  - `*** Update File: <path>` with optional `*** Move to: <path>` and `+/-/ ` hunk lines
  - `*** Delete File: <path>`
- Paths MUST be workspace-relative; absolute paths and `..` segments are rejected.
- Apply is atomic: either all operations apply or the workspace is reverted.

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
