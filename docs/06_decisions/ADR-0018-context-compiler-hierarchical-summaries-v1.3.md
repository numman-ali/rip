# ADR-0018: Context compiler hierarchical summaries + compaction checkpoint indexes (v1.3)

Status
- Accepted (2026-01-25)

Decision packet
- Decision: keep `context.compile` O(k) at 1M+ continuity events by (a) indexing compaction checkpoints as rebuildable caches and (b) introducing a hierarchical summary strategy that can reference multiple compaction summaries plus a bounded recent raw window.
- Options:
  1) Keep single-summary composition (`summaries_recent_messages_v1`) only.
     - Pros: simplest prompt shape; minimal new surface area.
     - Cons: a single bounded cumulative summary may lose older detail as the thread grows; selection still needs to locate checkpoints efficiently at scale.
  2) Add segment summaries (non-overlapping ranges) and compose them.
     - Pros: minimal redundancy; better “zoom levels”.
     - Cons: requires new summary kinds + job semantics; larger change to compaction and selection.
  3) Add hierarchical checkpoint composition (multiple cumulative summaries) + per-kind cache indexes.
     - Pros: bounded, deterministic, replay-safe; improves “one chat forever” detail retention without changing compaction jobs; keeps selection O(k) with cache indexes.
     - Cons: overlaps are redundant by construction; requires explicit truth logging of selected checkpoints.
- Recommendation: Option 3 now (v1.3), keep segment summaries as a future additive strategy/kind.
- Reversibility: additive changes only (new compiler strategy id, new optional frame fields, new cache files). Segment summaries can be introduced later via new `summary_kind` and/or a new strategy without rewriting history.

Context
- Continuity OS posture: the continuity event log is truth; caches are rebuildable; provider cursors are optional caches (AGENTS.md, ADR-0010).
- Compaction posture: checkpoint frames reference immutable summary artifacts; compiler selection must be deterministic under cache rotation (ADR-0011, ADR-0014, ADR-0016).
- Performance constraint: `context.compile` must not scan 1M+ continuity events on the hot path; selection must stay O(k) in the selected inputs (events/items), not O(n) in total history.

Decision

## 1) Add compiler strategy: `hierarchical_summaries_recent_messages_v1`
- When at least two eligible cumulative compaction checkpoints exist (`summary_kind="cumulative_v1"` and `to_seq <= from_seq`):
  - Select a bounded hierarchy of checkpoints (max N, v1.3 uses N=3):
    - include the latest eligible checkpoint (max `to_seq`),
    - then repeatedly include the latest checkpoint with `to_seq <= floor(prev_to_seq/2)` until N is reached or no such checkpoint exists.
  - Emit `summary_ref` items for all selected checkpoints (ordered by `to_seq` ascending).
  - Include a bounded recent raw window of messages after the latest selected checkpoint (`RECENT_MESSAGES_V1_LIMIT`), anchored at the run’s message id.
- If fewer than two eligible checkpoints exist:
  - Preserve v1 behavior (`summaries_recent_messages_v1` with a single summary ref, or `recent_messages_v1` with no summaries).

Determinism
- Inputs are explicit and replay-addressable:
  - checkpoints are identified by their logged `{checkpoint_id,to_seq,summary_artifact_id}`,
  - summaries are referenced by immutable artifact ids,
  - raw messages are bounded and selected from continuity truth by seq.

## 2) Extend truth logging: record all selected checkpoints
- Extend `continuity_context_selection_decided` to include:
  - `compaction_checkpoints: ContextSelectionCompactionCheckpointV1[]` (optional; may be empty)
- Maintain the existing `compaction_checkpoint` field as the “primary” checkpoint (the most recent selected checkpoint) for backwards compatibility.

## 3) Add per-kind, rebuildable cache indexes for compaction checkpoints
- Add a derived cache index per continuity:
  - `continuity_streams/<thread_id>.comp.idx.v1.jsonl`
  - Stores a compact projection of `continuity_compaction_checkpoint_created` frames (seq, to_seq, checkpoint_id, cut_rule_id, summary_kind, summary_artifact_id).
- Cache posture:
  - truth remains the continuity log (`events.jsonl`);
  - the index is rebuildable from the continuity sidecar(s) and may be deleted/rotated at any time without changing correctness.

Consequences
- `context.compile` stays O(k) for checkpoint selection and recent-window reads when caches exist.
- Prompt shape becomes “multi-tier” only when multiple checkpoints exist; limits keep it bounded.
- Surfaces can display strategy evolution and selected checkpoints via the truth-derived context selection status projection (ADR-0016).

References
- `docs/06_decisions/ADR-0010-context-compiler-truth.md`
- `docs/06_decisions/ADR-0011-compaction-cutpoints-summaries.md`
- `docs/06_decisions/ADR-0014-auto-compaction-summaries-v0.2.md`
- `docs/06_decisions/ADR-0016-context-selection-truth-logging.md`
- `docs/03_contracts/event_frames.md`
- `docs/03_contracts/context_bundle.md`
- `docs/07_tasks/roadmap.md`

