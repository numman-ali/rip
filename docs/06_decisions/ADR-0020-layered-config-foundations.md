# ADR-0020: Layered Config Foundations (Providers, Models, Roles)

Status
- Accepted (2026-01-30)

Context
- Phase 1 provider configuration relied on environment variables (e.g. `RIP_OPENRESPONSES_*`, `OPENAI_API_KEY`, `OPENROUTER_API_KEY`).
- Local-first operation uses a long-lived per-store authority process; environment-based configuration is brittle because the authority inherits env at startup and does not see changes.
- Operator requirement: configure providers/models once, make the active route visible, and avoid doc drift and “mystery model” behavior.

Decision
- Introduce a file-backed, layered configuration system loaded by the authority at run boundaries (no restart required for changes):
  - Format: JSON and JSONC (comments + trailing commas supported).
  - Merge semantics: deep merge for objects; scalars/arrays overwrite.
  - Sources (lowest → highest precedence):
    - Global: `$RIP_CONFIG_HOME/config.jsonc` or `$HOME/.rip/config.jsonc` (and `.json`)
    - Custom path: `RIP_CONFIG=/path/to/config.jsonc`
    - Project: `rip.jsonc` / `rip.json` (searched upward from `RIP_WORKSPACE_ROOT` until the nearest `.git/` root; deeper files override shallower).
- Config schema v1 (minimal slice to unblock operator) supports:
  - `provider.<provider_id>.endpoint` (OpenResponses endpoint URL)
  - `provider.<provider_id>.api_key` (inline string or `{ "env": "ENV_VAR" }`)
  - `provider.<provider_id>.headers` (static request headers)
  - `model` and `roles.<name>` routes in `provider_id/model_id` form (supports `#variant` suffix; variants are reserved for Phase 2 request tuning)
  - `openresponses` defaults (`stateless_history`, `parallel_tool_calls`, `followup_user_message`)
- Observability:
  - Server exposes `GET /config/doctor` returning a sanitized resolution summary (no secrets).
  - CLI exposes `rip config doctor` (local-first; uses the authority) for zero-ambiguity diagnostics.
- Compatibility:
  - Environment variables remain supported as overrides and as auth fallbacks (compat), but config is the recommended source of truth moving forward.
  - Secrets are never emitted into event frames or artifacts; doctor output reports presence/source only.

Consequences
- Provider/model selection becomes explicit, inspectable, and stable across surfaces (CLI/TUI/server) without requiring authority restarts.
- Project repos can commit non-secret defaults (e.g., model/roles) while keeping credentials in user scope.
- Follow-on work (Phase 2) can extend:
  - model catalogs + variant option application,
  - per-turn switching (`session.set_model`) and routing policies,
  - schema validation + editor integration.

