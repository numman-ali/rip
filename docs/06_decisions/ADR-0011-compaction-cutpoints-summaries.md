# ADR-0011: Deterministic compaction cut points + summary artifacts

Status
- Accepted (2026-01-24)

Decision packet
- Decision: define deterministic compaction checkpoints (cut points + summary artifacts) and how `context.compile` composes summaries + recent raw messages without breaking replay determinism at 1M+ events.
- Options:
  1) **Token-budget cut points** (compute token counts over history, cut when budget exceeded).
     - Pros: cuts align to actual prompt pressure.
     - Cons: expensive to compute deterministically (tokenizer/version drift); hard to keep compilation O(k) without additional indexes; brittle across model/provider changes.
  2) **Message-count stride cut points** (cut every N `continuity_message_appended` events; anchor to that message’s `{seq,id}`).
     - Pros: cheap, deterministic, provider-agnostic; supports seek/index caches; stable under dense non-message event streams.
     - Cons: doesn’t directly model token pressure; may cut “too early/late” for very long/short messages.
  3) **Time-based cut points** (daily/weekly summaries).
     - Pros: intuitive human cadence.
     - Cons: time is a weak proxy for prompt pressure; nondeterministic when clocks/timezones differ; hard to test/replay cleanly.
- Recommendation: Option 2 (message-count stride, anchored to message boundaries), with versioned rules recorded in continuity events; keep token-aware policies as future additions layered on top.
- Reversibility: cut-point rules are versioned and recorded; summary artifacts are immutable and versioned; future work can add hierarchical/segment summaries or token-aware policies without rewriting history (new frame types / new artifact schemas / new compiler strategies).

Context
- Continuity OS posture (ADR-0008): continuity event log is truth (append-only, replayable); providers are replaceable; provider cursors are caches.
- `context.compile` is the canonical “memory” mechanism (ADR-0010): every behavior-changing compilation decision is logged as continuity events + artifact refs.
- We need compaction to keep `context.compile` bounded (O(k)) on 1M+ event continuities while remaining deterministic under:
  - dense non-message continuity events (e.g., tool side-effect logs),
  - parallel foreground runs and background workers,
  - multi-actor/shared continuities (`actor_id`, `origin`),
  - cache rotation/rebuild (sidecars are best-effort, truth log remains canonical).

Decision

1) Compaction checkpoints are continuity events with explicit cut points
- Introduce a new continuity frame:
  - `continuity_compaction_checkpoint_created`
- It records:
  - the **cut point** in the continuity stream being summarized (`to_seq`, `to_message_id`),
  - the **coverage** of the summary (`from_seq`, `from_message_id`),
  - the produced **summary artifact id** (`summary_artifact_id`, schema `rip.compaction_summary.v1`),
  - the rule/strategy identifiers needed for replay audits (`cut_rule_id`, `summary_kind`),
  - provenance (`actor_id`, `origin`).
- Invariants:
  - The cut point **must be a message boundary**: `to_seq` / `to_message_id` must identify a `continuity_message_appended` event.
  - `from_seq <= to_seq` and both are inclusive.
  - Multiple checkpoints may exist for the same cut point; later checkpoints supersede earlier ones by stream order (they are not destructive edits).

2) Summary artifacts are immutable, replay-addressable blobs
- Introduce a versioned artifact schema:
  - `rip.compaction_summary.v1`
- The artifact includes:
  - coverage `{thread_id, from_seq/from_message_id, to_seq/to_message_id}`,
  - provenance `{actor_id, origin, produced_by?}`,
  - summary payload (markdown text; structured extensions versioned later).
- Summary artifacts are stored in the artifact store (`.rip/artifacts/blobs/<artifact_id>`). A checkpoint frame references the artifact id; nothing in history is overwritten.

3) Context bundles reference summaries by artifact id (no hidden reads)
- Extend compiled context bundles to support an item type:
  - `summary_ref` (references a compaction summary artifact id)
- Provider adapters may expand `summary_ref` by loading the referenced artifact (deterministic; blob is immutable) and rendering it as provider input.

4) Compilation strategy: summaries + recent raw messages
- Add a compiler strategy (initial):
  - `summaries_recent_messages_v1`
- Selection rule (deterministic):
  - Find the latest valid compaction checkpoint whose `to_seq <= from_seq` (where `from_seq` is the compile cut point).
  - Include exactly one `summary_ref` item referencing that checkpoint’s `summary_artifact_id`.
  - Then include up to `RECENT_MESSAGES_V1_LIMIT` raw messages after the checkpoint cut point (bounded window), anchored at the run’s message id.
  - If no valid checkpoint exists, fall back to `recent_messages_v1` behavior.
- Rationale: single-summary keeps prompt shape stable and bounded; future hierarchical summaries can add additional summary tiers without changing this baseline behavior.

Determinism & concurrency rules
- Cut points are deterministic when created:
  - The “stride” (e.g., every 10k messages) is defined by `cut_rule_id` and must anchor to the Nth `continuity_message_appended` event (not total continuity events).
  - The checkpoint frame records the resolved `{from_seq,to_seq}` so replays do not depend on recomputing “latest head”.
- Concurrency:
  - Multiple background jobs may attempt to summarize the same cut point; the continuity stream order is the arbiter.
  - Compiler selection is deterministic given the stream: choose the checkpoint with the greatest `to_seq <= from_seq`; break ties by the checkpoint frame’s `seq` (latest wins).
- Cache posture:
  - Sidecar indexes may omit compaction frames; compilation must fall back to truth replay when caches are missing/invalid.
  - Compaction never changes continuity truth; it only adds checkpoint frames and artifact refs.

Implementation slice (v0.1)
- Contracts:
  - Add `continuity_compaction_checkpoint_created` to event frames.
  - Document `rip.compaction_summary.v1` artifact schema.
  - Extend `rip.context_bundle.v1` items with `summary_ref`.
- Runtime:
  - Minimal write/read helpers for compaction summary artifacts.
  - Minimal “manual checkpoint” entrypoint (CLI local-first) to append a checkpoint + artifact id to a continuity.
  - Add compiler strategy `summaries_recent_messages_v1` (summary + recent raw messages; fallback-safe).

References
- `docs/02_architecture/continuity_os.md`
- `docs/06_decisions/ADR-0010-context-compiler-truth.md`
- `docs/03_contracts/event_frames.md`
- `docs/03_contracts/context_bundle.md`
- `docs/03_contracts/modules/phase-2/05_context_compiler.md`
- `docs/07_tasks/roadmap.md`

