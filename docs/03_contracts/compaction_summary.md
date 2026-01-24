# Compaction Summary (Artifact)

Summary
- Compaction summaries are immutable, replay-addressable artifacts produced by background workers or manual workflows.
- They are referenced from continuity truth via `continuity_compaction_checkpoint_created` frames and consumed by `context.compile` strategies.
- Providers remain replaceable: summaries are internal artifacts; provider cursors are caches only (ADR-0010).
- This artifact schema is internal to RIP and is not the Open Responses `type="compaction"` item (which uses provider-specific `encrypted_content`).

Location / storage
- Summary artifacts are stored in the workspace artifact store: `.rip/artifacts/blobs/<artifact_id>`.

Schema (`rip.compaction_summary.v1`)
- Content is UTF-8 JSON.
- Top-level fields:
  - `schema`: `"rip.compaction_summary.v1"` (required)
  - `kind`: string (required; example: `"cumulative_v1"`)
  - `coverage`: object (required)
    - `thread_id`: string (required; continuity id)
    - `from_seq`: u64 (required; inclusive coverage start)
    - `from_message_id`: string | null (optional; when coverage start is a message boundary)
    - `to_seq`: u64 (required; inclusive coverage end; must be a message boundary)
    - `to_message_id`: string | null (optional; when coverage end is a message boundary)
  - `provenance`: object (required)
    - `actor_id`: string (required)
    - `origin`: string (required)
    - `produced_by`: object | null (optional; for audits)
      - `type`: `"task" | "session" | "manual"` (required when present)
      - `id`: string (required when present; task/session id or a stable manual label)
  - `basis`: object | null (optional; for incremental/cumulative summarizers)
    - `base_summary_artifact_id`: string | null (optional; previous summary used as input)
    - `note`: string | null (optional)
  - `summary_markdown`: string (required; markdown payload rendered into provider context)

Determinism & replay
- The artifact is immutable once written; it must be referenced from continuity truth by id.
- A `continuity_compaction_checkpoint_created` frame records the cut point and `summary_artifact_id` used for compilation; replay uses the recorded artifact ids.
- Summaries must not depend on hidden mutable inputs: any upstream artifacts used to create a new summary should be recorded explicitly (future: structured `inputs[]` list).

Context bundle integration
- Context bundles may reference compaction summaries using `items[]` entry `type="summary_ref"` (see `docs/03_contracts/context_bundle.md`).
- Provider adapters expand `summary_ref` by loading the referenced summary artifact and rendering `summary_markdown` into provider input.
