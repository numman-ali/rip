# Context compiler fixture: hierarchical summaries v1

Purpose
- Deterministic replay fixture for `context.compile` perf v1.3:
  - per-kind compaction checkpoint index/sidecars
  - hierarchical summary selection (multi-level `summary_ref`s) + bounded recent raw window

Contents
- `events.jsonl`: one continuity stream with 60 messages and 3 cumulative compaction checkpoints.
- `workspace/.rip/artifacts/blobs/*`: referenced `rip.compaction_summary.v1` artifacts.

