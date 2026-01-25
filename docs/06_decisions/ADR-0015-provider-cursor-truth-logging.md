# ADR-0015: Provider cursor cache state is continuity truth (rotation logs v0.1)

Status
- Accepted (2026-01-25)

Decision packet
- Decision: represent provider conversation cache state changes (cursors) as continuity-truth frames and expose a parity-gated capability surface to rotate/reset and inspect them.
- Options:
  1) Keep provider cursor state as hidden server/runtime state (DB/in-memory).
     - Pros: simplest initially.
     - Cons: violates Continuity OS posture (hidden mutable state); non-auditable; breaks replay/cursor-rotation assumptions.
  2) Store cursor state only in provider adapters (opaque per-provider files/state).
     - Pros: keeps server minimal.
     - Cons: still hidden state; hard to rebuild/rotate deterministically; weak provenance.
  3) Log cursor state changes as continuity frames (truth), with a small derived projection for “current cursor”.
     - Pros: fully auditable + rebuildable; deterministic replay; supports multi-actor provenance; works for local + remote surfaces.
     - Cons: adds a new frame type + capability surface; requires projection logic.
- Recommendation: Option 3.
- Reversibility: additive, versioned frame + capability ids; future providers/cursor schemas extend via additional cursor payload fields or a v2 frame without rewriting history.

Context
- Continuity OS posture: the continuity event log is truth (append-only + replayable). Provider conversation state (Open Responses `previous_response_id`, vendor thread ids) is a cache only (ADR-0010).
- Cursor rotation/reset is expected and must not compromise auditability or determinism (1M+ events, parallel jobs, multi-actor/shared continuities).
- `thread.post_message` hot path must remain unchanged: background policy/maintenance decisions are expressed as continuity-truth frames + artifact refs only.

Decision

## 1) Add a new continuity-truth frame: `continuity_provider_cursor_updated`
- Purpose: record a **provider conversation cursor cache** change for a thread so caches can be rebuilt/rotated without changing truth.
- Fields (v0.1):
  - `provider`: string (example: `"openresponses"`)
  - `endpoint`: string | null (example: Open Responses base endpoint; no secrets)
  - `model`: string | null
  - `cursor`: object | null (provider-specific cursor payload; v0.1 uses an object with stable keys)
    - Open Responses v0.1 cursor payload: `{ "previous_response_id": "<response_id>" }`
  - `action`: string (example: `set` | `cleared` | `rotated`)
  - `reason`: string | null (stable, user-supplied or system-defined)
  - `run_session_id`: string | null (links the update to a run when produced by a run)
  - provenance: `actor_id`, `origin`

## 2) Cursor state is a derived projection (no hidden mutable state)
- “Current cursor” for a `{thread_id, provider, endpoint, model}` key is defined as the latest `continuity_provider_cursor_updated` event for that key.
- Rotation/reset is expressed by emitting an update with `cursor=null` and `action=rotated|cleared`.
- The continuity truth stream is sufficient to:
  - audit cursor changes, and
  - rebuild any cached projection/index at any time (cache loss is non-fatal).

## 3) Expose capability ids with surface parity
- New capabilities (v0.1):
  - `thread.provider_cursor.status` (v1): return a truth-derived status projection (latest cursor event per key).
  - `thread.provider_cursor.rotate` (v1): append a rotation/reset decision to the continuity stream (clears the cursor for the active provider key).
- Surface order gate: implement and validate in `cli_h(local)` → `tui` → `server` → `remote` → `sdk`.

Non-goals (v0.1)
- Using provider cursors as primary continuity memory (providers remain replaceable).
- Token-aware or provider-aware cursor policies (future logged policies).
- Multi-provider routing and catalogs (Phase 2).

References
- `AGENTS.md`
- `docs/06_decisions/ADR-0010-context-compiler-truth.md`
- `docs/03_contracts/event_frames.md`
- `docs/03_contracts/capability_registry.md`
- `docs/02_architecture/continuity_os.md`

