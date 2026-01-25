# ADR-0019: Single authority per store; indexes are caches (multi-client + retrieval posture)

Status
- Accepted (2026-01-25)

Decision packet
- Decision: make “one store just works across many terminals/devices” compatible with Continuity OS determinism, while leaving room for fast hybrid retrieval (text + vector + rerank) on modest hardware.
- Options:
  1) **Multi-writer local files** (many processes write the same log/indexes directly).
     - Pros: zero daemon; superficially simple.
     - Cons: violates the continuity-truth invariants (`seq` contiguity + total order); races corrupt logs; workspace mutation ordering becomes nondeterministic.
  2) **Single authority per store** (many clients; one sequencer/writer).
     - Pros: preserves total order + replay; makes multi-terminal and multi-device safe; matches “runtime vs control plane” model.
     - Cons: requires an “authority” concept (daemon or embedded server) even for local-only workflows.
  3) **Distributed multi-writer with sync/replication** (offline-first, conflict semantics).
     - Pros: strongest “anything anywhere” story.
     - Cons: complex, ADR-worthy conflict semantics; easy to break replay determinism; Phase 3 work.
- Recommendation: Option 2 now; explicitly road-map Option 3.
- Reversibility: introducing an authority is additive and preserves on-disk truth formats; replication can be added later as an explicit capability layer without rewriting history.

Context
- Continuity OS posture (ADR-0008): the continuity event log is truth (append-only + replayable); provider state is cache; sessions are runs.
- A continuity stream has strict invariants (`docs/03_contracts/event_frames.md`):
  - per-stream `seq` is contiguous and totally ordered,
  - behavior-changing decisions are logged as continuity truth (e.g., context selection, cursor rotation).
- Users expect “one store” to support:
  - many concurrent terminals (same repo/workspace),
  - multiple devices (laptop/phone/24x7 worker),
  - background workers that continuously improve memory/retrieval/compaction.

Decision

## 1) Define store + authority
- **Store**: the persistence boundary containing continuity truth + artifacts (today: `RIP_DATA_DIR`).
- **Authority**: the single sequencer responsible for all writes that affect truth:
  - appends continuity/session/task frames,
  - assigns ordered `seq` per `{stream_kind, stream_id}`,
  - serializes workspace mutations (already a runtime invariant),
  - owns/refreshes rebuildable caches (sidecars/indexes) as an optimization.
- Surfaces (CLI/TUI/SDK) are **clients** when targeting an authority, even if it is local.

## 2) Multi-terminal / multi-device posture
- A single store supports many concurrent clients by targeting the same authority:
  - Local multi-terminal: connect to a **local authority** (daemon or embedded server).
  - Multi-device: connect to a **remote authority** (network control plane).
- Writing to the same store from multiple independent local processes without an authority is **unsupported** (it breaks stream ordering and can corrupt truth).

## 3) “Infinity” / cross-continuity memory posture (initial)
- Cross-project/global memory is represented as continuity-truth + artifacts, not hidden mutable state:
  - Introduce a “global” continuity (conceptually: the user/org home continuity) as the first substrate for “Infinity”.
  - Background jobs can append global memory artifacts to this continuity (summaries/indexes/notes), with provenance (`actor_id`, `origin`).
  - Workspace continuities can reference global continuity context explicitly (planned: `thread.reference`, `context.refs.thread`) so context compilation remains replayable.
- UI may choose to keep global continuity “silent” by default, but it remains fully auditable via frames.

## 4) Retrieval posture (hybrid search + rerank, deterministic by reference)
- Retrieval is a compiler-stage over truth + derived indexes:
  - first-pass lexical filtering (fast text search),
  - semantic recall (vector search over embeddings),
  - reranking (optional model-based scoring over a bounded candidate set).
- Determinism rule:
  - if retrieval/rerank influences a run, record the inputs/outputs as stable ids/refs:
    - write retrieval results (ranked refs + scores + query metadata) as an artifact,
    - append a continuity truth frame recording the decision and the artifact id,
    - compile context by referencing those artifacts/refs (no silent injection).
- Reranking models and embedding APIs are allowed as implementation details of background jobs and/or compilation stages, but replay correctness must not depend on re-running them.

## 5) Index-store posture (DBs are caches unless explicitly promoted)
- Truth remains the continuity event streams + artifact store (ADR-0010).
- Text/vector indexes are **rebuildable caches** derived from truth:
  - they may live in a DB (e.g., SQLite FTS for lexical, a vector DB for embeddings), but are not canonical truth by default.
  - caches must be safe to delete and rebuild from continuity streams + artifacts.
- Promoting a DB to be the **truth store** (replacing the continuity log) is a separate, hard-to-reverse decision requiring a dedicated ADR, migration tooling, and replay/fixture equivalence gates.

Implementation slices (recommended)
1) Make local multi-terminal safe by default:
   - auto-start/auto-attach a local authority for a store,
   - enforce “single writer” with a store lock,
   - keep local UX “one store just works” without requiring `--server`.
2) Define retrieval contracts:
   - artifact formats for indexes and ranked results,
   - truth frames + compiler selection logging for retrieval.
3) Add DB-backed caches for retrieval (optional per deployment/profile) without changing truth.
4) Phase 3: explicit replication/sync capability contracts (authority, provenance, conflict semantics).

References
- `docs/02_architecture/continuity_os.md`
- `docs/02_architecture/runtime_and_control_plane.md`
- `docs/03_contracts/event_frames.md`
- `docs/03_contracts/modules/phase-2/05_context_compiler.md`
- `docs/06_decisions/ADR-0008-continuity-os.md`
- `docs/06_decisions/ADR-0010-context-compiler-truth.md`
- `docs/06_decisions/ADR-0012-background-jobs.md`
