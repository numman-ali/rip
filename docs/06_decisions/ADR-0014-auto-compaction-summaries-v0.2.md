# ADR-0014: Auto compaction summaries are cumulative artifacts (content contract v0.2)

Status
- Accepted (2026-01-25)

Decision packet
- Decision: define the **content contract** and **invariants** for auto-generated compaction summaries so they are cumulative, replay-safe, and scalable at 1M+ events.
- Options:
  1) Keep placeholder “metadata-only” summaries.
     - Pros: trivial; deterministic.
     - Cons: not useful context; defeats compaction’s purpose.
  2) Re-summarize from scratch for each new checkpoint (read all messages `0..to_seq`).
     - Pros: higher quality summary possible.
     - Cons: expensive at 1M+ events; requires large reads; hard to keep job inputs bounded.
  3) **Cumulative chain**: each checkpoint summary is produced from (a) a base summary artifact + (b) the new delta window since that base.
     - Pros: bounded input per checkpoint (≈ `stride_messages`); works with cache rotation; supports parallel jobs; keeps `context.compile` fast.
     - Cons: requires an explicit chaining contract and “bounded output” rules.
- Recommendation: Option 3.
- Reversibility: the artifact schema is already versioned (`rip.compaction_summary.v1`). Content rules can evolve behind `kind` (e.g., `cumulative_v2`) without rewriting history; compiler selection is based on checkpoint frames + artifact ids (ADR-0011).

Context
- Continuity OS posture: continuity event log is truth; caches are rebuildable; background workers emit structured events + immutable artifacts (AGENTS.md, ADR-0008).
- Compaction posture: deterministic cut points anchored to message boundaries; checkpoint frames reference summary artifacts (ADR-0011).
- Background jobs posture: compaction work is represented as continuity job frames with artifact refs; no hidden mutable state (ADR-0012).

Decision

## 1) Summary artifacts remain the unit of truth for compaction
- The produced summary is stored as an immutable artifact (`rip.compaction_summary.v1`) and referenced from:
  - `continuity_compaction_checkpoint_created.summary_artifact_id`.
- Replays must never “regenerate” summaries; they only replay the recorded artifact ids and frames.

## 2) Cumulative chaining is first-class and explicit
- For `kind="cumulative_v1"` summaries:
  - `basis.base_summary_artifact_id` MUST be set when a prior cumulative checkpoint exists for the thread (`to_seq` strictly less than the new checkpoint’s `to_seq`).
  - The summarizer MUST treat the base summary artifact as an immutable input and only read new delta inputs from continuity truth after the base coverage end.
- When no prior checkpoint exists, `basis.base_summary_artifact_id` MUST be null/absent.
- Upgrade/compatibility:
  - If the referenced base summary artifact exists but does not satisfy the v0.2 content contract (e.g., compat metadata-only placeholders), the summarizer MAY “bootstrap” by regenerating a real cumulative summary directly from continuity truth for the full `0..to_seq` range.
  - In bootstrap mode, `basis.base_summary_artifact_id` SHOULD still be recorded for auditability, but the produced `summary_markdown` must not embed compat placeholder-only content.

## 3) Inputs and provenance are deterministic and auditable
- Job inputs that affect the produced artifact MUST be derivable from:
  - continuity truth (messages + linked run outputs where available), and
  - explicitly referenced artifact ids (the base summary artifact, when present), and
  - the job’s recorded parameters (`cut_rule_id`, `stride_messages`, and resolved cut point).
- Provenance MUST be recorded:
  - `provenance.actor_id` and `provenance.origin` are required.
  - `provenance.produced_by` SHOULD identify `{type="job", id=<job_id>}` for auto compaction jobs.

## 4) Content contract (v0.2) for auto-generated summaries
- Auto-generated `summary_markdown` MUST contain human-usable cumulative content derived from messages, not just metadata.
- `summary_markdown` MUST be bounded to a stable maximum size (implementation-defined), to avoid compaction artifacts becoming unbounded “new history”.
- Recommended structure (markdown):
  - Header identifying it as an auto compaction summary and the covered cut point.
  - A **Cumulative Summary** section that reflects the base summary updated with the new delta.
  - A **Recent Delta Highlights** section with a bounded, deterministic selection of the most salient new items since the base checkpoint.

## 5) Hot path invariant
- `thread.post_message` remains unchanged and MUST NOT invoke summarization or scheduling work inline (ADR-0013).

References
- `docs/06_decisions/ADR-0011-compaction-cutpoints-summaries.md`
- `docs/06_decisions/ADR-0012-background-jobs.md`
- `docs/06_decisions/ADR-0013-compaction-auto-scheduling-policy.md`
- `docs/03_contracts/compaction_summary.md`
- `docs/03_contracts/event_frames.md`
