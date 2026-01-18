# Contract: model routing (OpenResponses endpoints)

Summary
- Supports multiple OpenResponses-compatible inference endpoints (“providers”) and multiple model configs per endpoint.
- Allows switching `{provider_id, model_id}` at any time during an active session (per-turn), with optional per-subagent overrides.
- Supports routing policies that choose `{provider_id, model_id}` dynamically (advisory or authoritative), while preserving deterministic replay.

Scope (Phase 2)
- Multi-endpoint configuration and model catalogs.
- Per-session and per-turn model selection and switching.
- Router policy engine with recorded decisions.

Non-goals (Phase 2)
- Provider-specific protocols (OpenResponses is the only inference protocol).
- Undocumented “magic” routing: every decision must be observable and replayable.

Inputs
- Session config:
  - `provider_id` (OpenResponses base URL + auth profile)
  - `model_id` (string)
  - per-run defaults (sampling, truncation, tool policy)
- Turn config overrides (optional):
  - change `provider_id`/`model_id` for the next turn only
  - enable/disable routing policy for a turn
- Router policy config:
  - mode: `advisory` or `authoritative`
  - policy id + params (e.g., “fast”, “deep”, “coding”, “router_v1”)

Outputs
- Session event frames that record:
  - selected `{provider_id, model_id}` per turn
  - router decisions (policy id, mode, inputs/summary, chosen route)
  - any overrides applied (explicit vs policy-derived)
- Provider-boundary request/response frames:
  - OpenResponses payloads preserved (see `openresponses.*` capabilities)

Invariants
- Switching models/providers MUST NOT mutate prior turns; changes apply forward only.
- Router decisions MUST be recorded so replay does not depend on current policy logic.
- Model catalogs MUST be versioned (hash/version string) and recorded when used.

Capabilities (registry ownership)
- `model.select`: select an upstream OpenResponses model id/string.
- `model.multi_provider`: multiple OpenResponses endpoints supported.
- `model.routing`: routing policy engine (advisory/authoritative).
- `model.catalog`: local catalog load + optional refresh hooks.
- `session.set_model`: change `{provider_id, model_id}` for an active session.
- `subagent.spawn`: subagents can specify model/provider overrides (params).
- OpenResponses boundary: `openresponses.*` capabilities for fidelity.

Storage posture
- Default: file-backed (JSON/JSONC) catalogs and provider configs for portability.
- Future: pluggable storage backend (e.g., SQLite) behind a storage trait; export/import must remain available.

Refs
- `docs/03_contracts/capability_registry.md`
- `docs/02_architecture/capability_matrix.md`
- `docs/07_tasks/roadmap.md`
- `docs/03_contracts/openresponses_coverage.md`
