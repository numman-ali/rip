# Context Bundle (Artifact)

Summary
- `context.compile` produces a deterministic, **provider-agnostic** compiled context bundle used to start a run (session).
- The bundle is stored as an artifact and referenced from a continuity event (`continuity_context_compiled`).
- Goal: provider cursors become optional caches; the continuity log + artifacts remain sufficient to rebuild/rotate provider state.

Location / storage
- Bundle artifacts are stored in the workspace artifact store: `.rip/artifacts/blobs/<artifact_id>`.
- Fetch is currently via the in-session `artifact_fetch` tool (range reads); a dedicated server artifact API is planned.

Bundle schema (`rip.context_bundle.v1`)
- Content is UTF-8 JSON.
- Top-level fields:
  - `schema`: `"rip.context_bundle.v1"` (required)
  - `compiler`: object (required)
    - `id`: string (required; example: `"rip.context_compiler.v1"`)
    - `strategy`: string (required; example: `"recent_messages_v1"`)
  - `source`: object (required)
    - `thread_id`: string (required; continuity id)
    - `from_seq`: u64 (required; inclusive cut point in the continuity stream)
    - `from_message_id`: string | null (optional; the message id that anchored the cut when known)
  - `provenance`: object (required)
    - `run_session_id`: string (required; the run that will consume this bundle)
    - `actor_id`: string (required)
    - `origin`: string (required)
  - `items`: array (required; may be empty)

Item union (`items[]`)
- Each item is an object with:
  - `type`: string (required)
- v1 required support:
  - `type="message"`
    - `role`: `"system" | "developer" | "user" | "assistant"` (required)
    - `content`: string (required; UTF-8)
    - `actor_id`: string | null (optional; set when sourced from `continuity_message_appended`)
    - `origin`: string | null (optional; set when sourced from `continuity_message_appended`)
    - `thread_seq`: u64 | null (optional; continuity seq of the source event when applicable)
    - `thread_event_id`: string | null (optional; continuity event id of the source when applicable)
  - `type="summary_ref"`
    - Purpose: reference a compaction summary artifact (do not inline large summary text into the bundle).
    - `artifact_id`: string (required; schema `rip.compaction_summary.v1`)
    - `note`: string | null (optional)

Planned item types (future versions)
- `handoff_bundle_ref`: `{artifact_id}` (handoff curated context bundle)
- `artifact_ref`: `{artifact_id, note?, mime?}` (tool outputs, indexes)
- `file_ref`: `{path, checkpoint_id?, note?}` (workspace state references)
- `tool_side_effect_ref`: `{run_session_id, tool_id, affected_paths?, checkpoint_id?}` (provenance-first mutation hints)

Determinism & replay
- The bundle is immutable once written; the truth link is the continuity event `continuity_context_compiled`.
- Replay consumes the recorded `bundle_artifact_id` (no re-generation required).
- The bundle must not include hidden mutable state; all non-message context must be referenced by artifact ids / continuity event ids.

Provider adapter mapping (intent)
- Provider adapters render bundles to provider request formats:
  - Open Responses:
    - `type="message"` -> OpenResponses `input[]` message items (role + content)
    - `type="summary_ref"` -> load the referenced compaction summary artifact (`docs/03_contracts/compaction_summary.md`) and render its `summary_markdown` as a provider message (implementation-defined role/prefix, but deterministic)
      - Spec note: Open Responses `message` input items accept `content` as a single string across message roles (including `system`), so the adapter may emit `{type:"message", role:"system", content:"..."}` without constructing content-part arrays.
      - Note: Open Responses also defines a `type="compaction"` input item shape with `encrypted_content`, but that format is provider-specific (e.g. produced by `/responses/compact`) and is not suitable for RIP’s provider-agnostic summary artifacts.
    - plus tool declarations as configured
  - Anthropic (future): `items[]` -> `messages[]` (role + content), with tool calls mapped at the provider boundary.
- Providers remain replaceable compute substrates; provider state is optional cache only.

Example (messages-only bundle)
```json
{
  "schema": "rip.context_bundle.v1",
  "compiler": { "id": "rip.context_compiler.v1", "strategy": "recent_messages_v1" },
  "source": {
    "thread_id": "11111111-1111-1111-1111-111111111111",
    "from_seq": 42,
    "from_message_id": "22222222-2222-2222-2222-222222222222"
  },
  "provenance": { "run_session_id": "33333333-3333-3333-3333-333333333333", "actor_id": "user", "origin": "cli" },
  "items": [
    { "type": "message", "role": "user", "content": "Ship it.", "actor_id": "user", "origin": "cli", "thread_seq": 41, "thread_event_id": "22222222-2222-2222-2222-222222222222" }
  ]
}
```
