# ADR-0008: Continuity OS (provider state is cache)

Status
- Accepted (2026-01-21)

Context
- We want "one chat forever": users should not manage sessions/conversations explicitly.
- Long-lived state must remain fast, replayable, and provider-agnostic.
- Provider conversation state (Open Responses `previous_response_id`, vendor thread ids) is inherently provider-specific and cannot be the system of record.

Decision
- Treat RIP as a **continuity OS**:
  - The **continuity event log** is the source of truth (append-only, replayable).
  - A **session/run** is a single compute job/turn that emits frames; sessions are not user-facing.
  - Provider conversation state is a **cache** that can be rotated/rebuilt at any time without changing continuity truth.
- Context injection becomes a first-class, deterministic pipeline ("context compiler") whose decisions are logged.

Consequences
- Enables silent provider cursor rotation, compaction/summarization, memory, and background workers without breaking user continuity.
- Forces multi-stream framing (continuity/session/task) and explicit provenance (`actor_id`, `origin`) for shared continuities.
- Keeps provider adapters simple and replaceable; Open Responses stays at the boundary only.

References
- `docs/02_architecture/continuity_os.md`
- `docs/03_contracts/event_frames.md` (Phase 2: stream-scoped envelope)
