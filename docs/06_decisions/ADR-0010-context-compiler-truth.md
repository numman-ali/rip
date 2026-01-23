# ADR-0010: Context compiler is truth-derived; providers are replaceable compute (cursors are caches)

Status
- Accepted (2026-01-23)

Decision packet
- Decision: where “model memory” lives across runs, without violating Continuity OS determinism/replay at 1M+ events.
- Options:
  1) **Stateless runs** (no cross-run provider cursor; each run starts a fresh provider conversation).
     - Pros: simplest concurrency; easiest replay; no provider coupling.
     - Cons: “one chat forever” is weak without a context compiler; token/latency costs shift into ad-hoc prompt stuffing.
  2) **Provider cursor as primary continuity** (persist `previous_response_id`/vendor thread ids per continuity and rely on providers for memory).
     - Pros: great TTFT and minimal tokens; immediate “continue” UX.
     - Cons: makes RIP provider-coupled; cursor invalidation/rotation becomes correctness-risk; parallel runs per continuity are hard; replay depends on external state.
  3) **RIP-owned deterministic context compiler** (compile context from continuity truth + artifacts; provider cursor is an optional cache only).
     - Pros: matches Continuity OS first principles; provider-agnostic; cursor rotation is safe; enables background workers/memory/indexing.
     - Cons: requires new contracts (bundle artifact + compile frames) and careful performance budgets.
- Recommendation: Option 3, implemented in versioned slices (compiler kernel first), with optional provider cursor caching layered later as a replay-rebuildable optimization.
- Reversibility: the bundle schema and compile frames are versioned; provider cursor caching is additive and can be disabled/rotated without changing continuity truth.

Context
- Continuity OS posture (ADR-0008): the continuity event log is truth; provider state is cache; sessions are runs.
- “One chat forever” requires that long histories do not depend on a provider’s opaque, expiring conversation handle.
- We need deterministic replay across:
  - parallel runs/jobs,
  - cursor rotation,
  - compaction/summarization checkpoints,
  - multi-actor provenance (`actor_id`, `origin`),
  - remote control plane vs local runtime.

Decision
- Introduce a first-class capability: `context.compile` (Phase 2 capability, pulled forward as a foundation slice).
- `context.compile` deterministically produces a **context bundle artifact** (`rip.context_bundle.v1`) from:
  - the continuity stream up to an explicit cut point, and
  - referenced artifacts (handoff bundles, summaries, indexes), and
  - an explicit compilation strategy/budget.
- Every compilation that influences a run must be recorded in the continuity stream via a new frame:
  - `continuity_context_compiled` (links `{run_session_id, cut point, compiler_id}` -> `bundle_artifact_id`, with provenance).
- Provider adapters consume the context bundle and render provider requests (Open Responses today; other providers later).
  - Open Responses remains boundary-only; the bundle format is **internal and provider-agnostic**.
- Provider cursors (`previous_response_id`, vendor thread ids) remain **optional caches**:
  - may be rotated/invalidated at any time,
  - must never be required to reconstruct continuity truth,
  - if persisted, must be rebuildable from logged continuity events (future: explicit cursor frames).

Determinism & replay rules
- The compiler input set is explicit:
  - cut point is always recorded (seq/message id).
  - referenced artifacts are recorded by id (no hidden reads).
  - strategy/budgets are recorded (versioned).
- The compiler output is immutable:
  - bundles are stored as artifacts and referenced by id from `continuity_context_compiled`.
  - replay uses the recorded bundle artifact id (no re-generation required for replay).
- Concurrency is cut-point based:
  - multiple runs may compile in parallel as long as they reference explicit cut points.
  - workspace mutations remain serialized (already a Phase 1 invariant).

Implementation slices (recommended)
1) Contracts (this ADR + frame + bundle schema) and roadmap updates.
2) Compiler kernel v1:
   - compile recent continuity messages into `rip.context_bundle.v1` (messages-only) with full provenance.
   - append `continuity_context_compiled` when a run starts.
3) Wire session provider requests to use compiled bundles as input (fresh provider convo per run).
4) Add compaction summary artifacts + compile strategies that mix summaries + recent raw events.
5) Optional: provider cursor cache + rotation frames as a performance optimization layer.

Surface parity posture
- `context.compile` is a capability (not a model tool). If invoked by autonomous workers later, provide a policy-gated tool wrapper that calls the same capability.
- Delivery order remains: headless CLI local runtime -> TUI -> server -> remote -> SDK (track any gaps explicitly in the roadmap).

References
- `AGENTS.md`
- `docs/02_architecture/continuity_os.md`
- `docs/02_architecture/capability_matrix.md`
- `docs/03_contracts/event_frames.md`
- `docs/03_contracts/handoff_context_bundle.md`
- `docs/03_contracts/capability_registry.md`
- `docs/06_decisions/ADR-0008-continuity-os.md`
- `docs/06_decisions/ADR-0005-openresponses-tool-loop.md`
