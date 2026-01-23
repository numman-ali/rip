# Contract: Context Compiler (`context.compile`) + Context Bundles (Phase 2)

Summary
- `context.compile` is the deterministic pipeline that turns **continuity truth** (events + artifacts) into a **compiled context bundle** used to start runs.
- Providers are replaceable compute substrates; provider conversation state (cursors) is an optional cache only (ADR-0010).
- The compiler is the foundation for: compaction checkpoints, cursor rotation, memory/index retrieval, skills, and subagents.

Decision
- See `docs/06_decisions/ADR-0010-context-compiler-truth.md`.

Related capabilities
- `context.compile` (core)
- `thread.*` (continuities)
- `compaction.*` (summary checkpoints; planned)
- `context.refs.*` (artifact/file/thread refs; planned)
- `tool.task_*` (background work that produces artifacts; optional inputs to compilation)

Inputs
- Continuity truth:
  - continuity event stream up to an explicit cut point (`from_seq` and optionally `from_message_id`)
  - referenced artifacts (handoff bundles, summaries, indexes) by id
- Compiler settings (versioned):
  - strategy id (which selection algorithm)
  - budgets (max tokens/items/bytes, reserved response budget)
  - policy knobs (e.g., include tool side-effect hints)
- Provenance:
  - `actor_id`, `origin` (who/what requested the compile)
  - `run_session_id` (if compilation is for a run)

Outputs
- Context bundle artifact:
  - schema: `rip.context_bundle.v1` (`docs/03_contracts/context_bundle.md`)
  - stored in `.rip/artifacts/blobs/<artifact_id>`
- Continuity event:
  - `continuity_context_compiled` links `{run_session_id, cut point, compiler_id/strategy}` -> `bundle_artifact_id` with provenance.

Non-goals (initial)
- Hidden mutable “memory” state (anything that changes context must be logged as events + artifacts).
- Provider-specific compiled bundles (Open Responses is boundary-only; bundles are internal format).
- Automatic cross-thread resolution beyond explicit refs (handoff/branch links define ancestry; the compiler resolves only what is referenced).

Architecture (target posture)

1) Compilation is a pure function (plus recorded artifacts)
- The compiler’s behavior is defined by:
  - the continuity stream (truth),
  - explicitly referenced artifact ids,
  - and the compiler version/strategy/budgets recorded in frames.
- The compiler may write derived artifacts (bundles, summaries, indexes), but those artifacts are always referenced from the continuity stream.

2) Cut points make concurrency replayable
- Every compile uses an explicit cut point (seq/message id) so:
  - parallel runs can compile deterministically without racing “latest head”.
  - replay can reproduce the same compilation target even if the continuity grows later.

3) Handoff and compaction feed the compiler
- Handoff (`thread.handoff`) writes a curated bundle artifact (`rip.handoff_context_bundle.v1`) referenced by `continuity_handoff_created`.
- Compaction (planned) writes summary artifacts at deterministic cut points and emits checkpoint frames.
- `context.compile` can include:
  - a handoff summary bundle as “base context” for a new thread, and
  - compaction summaries + recent raw events for long threads.

4) Provider adapters render bundles (no OpenResponses in core)
- Provider adapters accept a bundle and produce provider requests.
- Provider cursors (Open Responses `previous_response_id`, vendor thread ids) may be used as an optimization but must be rotatable without changing compilation correctness.

Determinism & replay rules
- All compilation decisions that affect a run are logged:
  - cut point,
  - compiler id/strategy,
  - bundle artifact id,
  - provenance.
- Bundles are immutable and replay-addressable by artifact id.
- Any non-message inclusion must be by reference:
  - artifact ids, file refs with checkpoint ids, thread refs with cut points.
- Replays do not re-run compilation unless explicitly requested:
  - correctness is “bundle referenced by event is the context used”.
  - optional audit mode can recompute and compare bundles (future).

Performance budgets (early)
- Compilation must be bounded:
  - O(k) in selected events/items, never O(n) over the full continuity stream for normal runs.
  - selection should prefer recent windows + summary artifacts.
- Context bundle render must be cheap:
  - streaming parse and tool dispatch remain on the hot path; compilation must not dominate TTFT.

Tests (required)
- Contract tests:
  - bundle schema round-trips and rejects invalid shapes
  - compilation frame ordering invariants hold (`continuity_run_spawned` -> `continuity_context_compiled` -> `continuity_run_ended`)
- Replay tests:
  - parallel runs compile at explicit cut points without nondeterministic ordering
  - late subscribers see identical compiled bundles via artifact ids
- Benchmarks (planned):
  - compilation time vs bundle size
  - render-to-provider request overhead

