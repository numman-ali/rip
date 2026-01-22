# ADR-0009: Branching and handoff are explicit continuity links (no history copying)

Status
- Accepted (2026-01-22)

Decision packet
- Decision: how to model `thread.branch` and `thread.handoff` in the continuity log + surfaces without breaking replay determinism or the “continuity OS” posture.
- Options:
  1) **Copy history into the new thread** (materialize parent events in the child).
     - Pros: trivial reads; no cross-thread resolution.
     - Cons: duplicates data; makes compaction/cursor-rotation harder; expensive at 1M+ events; lineage is no longer a pure function of the log.
  2) **Link-only** (child starts with an explicit relationship event that points at the parent + cut point).
     - Pros: append-only; minimal storage; lineage is explicit and replayable; compaction/cursor rotation remain orthogonal.
     - Cons: viewers/context compiler must resolve inherited history on demand.
  3) **Snapshot-at-branch** (write a compiled-context artifact/summary and link to that).
     - Pros: bounded + fast context; great for handoff.
     - Cons: requires compaction/context compiler infrastructure; adds more moving parts.
- Recommendation: Option 2 now; layer Option 3 later for handoff + compaction summaries.
- Reversibility: keep relationship frames stable; later add optional summary/context artifacts referenced from the same relationship frames (no breaking changes, replay stays valid).

Context
- Continuity OS posture: the continuity event log is truth; provider state is cache; sessions are runs.
- We need power-user flows that create new continuities from an existing one:
  - **branch**: inherit the parent history up to a chosen point
  - **handoff**: start fresh but carry curated context (summary + refs)
- These flows must be deterministic at scale (1M+ events) and surface-parity-friendly.

Decision
- `thread.branch` creates a new continuity stream in the same workspace and appends:
  - `continuity_created` (new thread id, title optional)
  - `continuity_branched` (relationship + cut point + provenance)
- Branching **does not copy** parent events into the child. Inheritance is logical:
  - The context compiler / viewers treat the branch’s “base” as the parent continuity stream up to `parent_seq` (inclusive), plus any linked run/session streams referenced by those continuity events.
- `thread.handoff` uses the same “link-only” posture but **does not inherit** full history:
  - it carries an explicit curated summary/context bundle recorded as frames and/or artifacts.
  - `thread.handoff` creates a new continuity stream in the same workspace and appends:
    - `continuity_created` (new thread id, title optional)
    - `continuity_handoff_created` (relationship + cut point + provenance + curated context)

Server API (v1)
- `POST /threads/{id}/branch`
  - request body:
    - `title?: string`
    - `from_message_id?: string` (cut selection)
    - `from_seq?: u64` (direct cut selection; power/debug)
    - `actor_id?: string` (defaults to `"user"`)
    - `origin?: string` (defaults to `"server"`)
  - response body:
    - `thread_id: string` (new continuity id)
    - `parent_thread_id: string`
    - `parent_seq: u64`
    - `parent_message_id?: string`
- `POST /threads/{id}/handoff`
  - request body:
    - `title?: string`
    - `summary_markdown?: string`
    - `summary_artifact_id?: string`
      - Invariant: at least one of `summary_markdown` or `summary_artifact_id` is provided.
    - `from_message_id?: string` (cut selection)
    - `from_seq?: u64` (direct cut selection; power/debug)
    - `actor_id?: string` (defaults to `"user"`)
    - `origin?: string` (defaults to `"server"`)
  - response body:
    - `thread_id: string` (new continuity id)
    - `from_thread_id: string`
    - `from_seq: u64`
    - `from_message_id?: string`

Determinism & replay rules
- The server/runtime must record the **chosen cut point** in relationship events:
  - Branch: `parent_seq`
  - Handoff: `from_seq`
  - Any implicit “latest” selection is made explicit by the recorded value.
- Relationship resolution is a pure function of the event log:
  - parent continuity stream + cut point + child stream
  - no hidden mutable lineage state (indexes are caches only).
- Provenance is mandatory on relationship actions:
  - `actor_id` and `origin` are stored on `continuity_branched` and `continuity_handoff_created`.

Surface parity
- Phase 1 implementation targets: headless CLI (local + `--server`), server (OpenAPI), and TypeScript SDK (via `rip` CLI per ADR-0006).
- TUI flow (`/branch`, `/handoff`) remains Phase 2; track any parity gaps explicitly in `docs/07_tasks/roadmap.md`.

References
- `docs/02_architecture/continuity_os.md`
- `docs/03_contracts/event_frames.md`
- `docs/03_contracts/capability_registry.md`
- `docs/06_decisions/ADR-0008-continuity-os.md`
- `docs/06_decisions/ADR-0006-sdk-transport.md`
