# Handoff Context Bundle (Artifact)

Summary
- `thread.handoff` creates a new continuity (“thread”) that starts fresh but carries curated context.
- Curated context is stored as an **artifact-backed bundle** referenced from the child thread’s `continuity_handoff_created.summary_artifact_id`.
- The bundle is designed for deterministic replay and future `context.compile` use (Phase 2).

Location / storage
- Bundle artifacts are stored in the workspace artifact store: `.rip/artifacts/blobs/<artifact_id>`.
- Fetch is currently via the in-session `artifact_fetch` tool (range reads); a dedicated server artifact API is planned.

Bundle schema (`rip.handoff_context_bundle.v1`)
- Content is UTF-8 JSON.
- Top-level fields:
  - `schema`: `"rip.handoff_context_bundle.v1"` (required)
  - `summary_markdown`: string (required)
  - `refs`: object (required)
    - `threads`: array of thread refs (required; may be empty, but handoff creation should include at least the source cut ref)
    - `artifacts`: array of artifact refs (required)
    - `files`: array of file refs (required)

Thread ref (`refs.threads[]`)
- `thread_id`: string (required)
- `seq`: u64 (required; inclusive cut point in the referenced continuity stream)
- `message_id`: string | null (optional; the message id that anchored the cut when known)
- `note`: string | null (optional; human hint)

Artifact ref (`refs.artifacts[]`)
- `artifact_id`: string (required)
- `note`: string | null (optional)

File ref (`refs.files[]`)
- `path`: string (required; workspace-relative, forward slashes)
- `note`: string | null (optional)

Example
```json
{
  "schema": "rip.handoff_context_bundle.v1",
  "summary_markdown": "### Summary\n- …\n",
  "refs": {
    "threads": [
      {
        "thread_id": "11111111-1111-1111-1111-111111111111",
        "seq": 42,
        "message_id": "22222222-2222-2222-2222-222222222222",
        "note": "source cut"
      }
    ],
    "artifacts": [],
    "files": []
  }
}
```

Determinism & replay
- The chosen cut point (`from_seq`) is recorded in `continuity_handoff_created` and duplicated in the bundle’s source ref.
- The artifact id is recorded in `continuity_handoff_created.summary_artifact_id`; replay uses the recorded id (no re-generation).

Mapping to runs (Phase 2 target)
- A run spawned on a handoff-created thread links normally via `continuity_run_spawned` / `continuity_run_ended`.
- `context.compile` treats the handoff bundle as the thread’s “base context”:
  - include `summary_markdown`
  - optionally resolve `refs.*` into additional compiled context artifacts
- Handoff does **not** inherit full parent history (ADR-0009); only what is referenced via the bundle is included.

