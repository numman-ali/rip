# ADR-0016: Context selection strategy decisions are continuity truth (v0.1)

Status
- Accepted (2026-01-25)

Decision packet
- Decision: represent context selection/strategy decisions (compiler strategy, budgets/limits, reasons, and any input reset/rotation decisions) as continuity-truth frames and expose a parity-gated read API for surfaces.
- Options:
  1) Keep context selection decisions implicit (derived from code + current caches).
     - Pros: fewer frames; simplest.
     - Cons: violates Continuity OS posture (hidden behavior changes); hard to audit; cache loss can change observed behavior without any logged cause.
  2) Log only the compiled bundle output (`continuity_context_compiled`) and treat everything else as derivable.
     - Pros: minimal additional truth writes.
     - Cons: does not capture *why* a strategy changed or which inputs were considered/reset; strategy evolution remains opaque.
  3) Log explicit “context selection decided” frames (truth) and provide a truth-derived status projection.
     - Pros: fully auditable; replay-safe under cache rotation; supports multi-actor provenance; surfaces can explain strategy evolution.
     - Cons: adds a new frame type + a new capability surface.
- Recommendation: Option 3.
- Reversibility: additive, versioned frame + capability id; future token-aware budgets and additional strategies extend via new optional fields or a v2 frame without rewriting history.

Context
- Continuity OS posture: the continuity event log is truth (append-only + replayable). Provider conversation state is a cache only (ADR-0010).
- Determinism rule: every behavior-changing decision must be logged (context selection, routing, tool dispatch, worker outputs) (`docs/02_architecture/continuity_os.md`).
- The context compiler already logs `continuity_context_compiled`, but that frame does not capture:
  - strategy selection reasoning,
  - resolved budgets/limits,
  - which compilation inputs (compaction checkpoints, etc.) were considered/used,
  - and any “input reset/rotation” decisions (e.g., skipping an unavailable artifact).

Decision

## 1) Add a new continuity-truth frame: `continuity_context_selection_decided`
- Purpose: record context selection strategy decisions for a run so the continuity stream alone is sufficient to audit strategy evolution under cache rotation.
- Scope (v0.1):
  - compiler strategy selection (`recent_messages_v1` vs `summaries_recent_messages_v1`)
  - budgets/limits (initially: `recent_messages_v1_limit`)
  - selection inputs used (initially: latest compaction checkpoint when applicable)
  - selection reasons and any input reset decisions (stable reason codes only)
  - provenance (`actor_id`, `origin`)

## 2) Expose a read-only capability with surface parity: `thread.context_selection.status` (v1)
- Purpose: provide a compact, truth-derived projection of recent context selection decisions for UX and debugging.
- Determinism: derived only from continuity truth (tail scan via sidecars is an optimization only).
- Non-goal (v0.1): no write capability to force/override strategies; strategy changes are a function of truth + recorded decisions.

Non-goals (v0.1)
- Token-aware packing and model-specific budget computation (future: logged policy ids + resolved budgets).
- Multi-provider routing decisions (future: separate routing decision frames).
- Changing `thread.post_message` behavior or adding work on its hot path.

References
- `docs/06_decisions/ADR-0010-context-compiler-truth.md`
- `docs/06_decisions/ADR-0015-provider-cursor-truth-logging.md`
- `docs/03_contracts/event_frames.md`
- `docs/03_contracts/capability_registry.md`
- `docs/02_architecture/continuity_os.md`
