# OpenResponses Traceability

Purpose
- Record the upstream OpenResponses snapshot used to generate vendored schemas and fixtures.
- Keep a committed audit trail independent of the temp mirror.

Current snapshot
- Source repo: https://github.com/openresponses/openresponses
- Local mirror: `temp/openresponses`
- Branch: `main`
- Commit: `d0f23437b27845d5c3d0abaf5cb5c4a702f26b05`
- Commit date: 2026-01-15
- Last synced: 2026-01-17

Vendored artifacts
- `schemas/openresponses/openapi.json`
- `schemas/openresponses/streaming_event_types.json` (58 events)
- `schemas/openresponses/split_components.json` (412 components)
- `schemas/openresponses/paths_responses.json`
- `crates/rip-provider-openresponses/fixtures/openresponses/stream_all.jsonl`
- `crates/rip-provider-openresponses/fixtures/openresponses/stream_all.sse`

Sync procedure (standard)
1. Update `temp/openresponses` to the desired commit (record the hash above).
2. Run `scripts/update-openresponses-types` (refreshes OpenAPI + baseline event types).
3. Regenerate `schemas/openresponses/split_components.json` from
   `temp/openresponses/schema/components/schemas/*.json`.
4. Copy `temp/openresponses/schema/paths/responses.json` to
   `schemas/openresponses/paths_responses.json`, then regenerate
   `schemas/openresponses/streaming_event_types.json` from the split paths.
5. Run `scripts/generate-openresponses-fixtures.py` to refresh stream fixtures.
6. Commit with the updated snapshot metadata.

Diff procedure (quick)
- `git diff schemas/openresponses/`
- `git diff crates/rip-provider-openresponses/fixtures/openresponses/`
- Compare counts in `temp/docs/openresponses/schema_inventory.json` and
  `temp/docs/openresponses/event_types.json` after refresh (optional).
