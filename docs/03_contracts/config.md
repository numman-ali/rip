# Contract: Configuration (v1)

Summary
- RIP loads layered configuration files (JSON/JSONC) and deep-merges them.
- Config is the preferred mechanism for provider/model defaults; env vars remain as compat overrides.
- Configuration is resolved by the store authority at run boundaries (no restart required for changes).

Locations + precedence (lowest → highest)
- Global:
  - `$RIP_CONFIG_HOME/config.jsonc` or `$HOME/.rip/config.jsonc`
  - `$RIP_CONFIG_HOME/config.json` or `$HOME/.rip/config.json`
- Custom path:
  - `RIP_CONFIG=/path/to/config.jsonc`
- Project:
  - `rip.jsonc` and/or `rip.json` found by searching upward from `RIP_WORKSPACE_ROOT` until the nearest `.git/` root (deeper files override shallower).

Merge semantics
- Objects: deep-merged recursively.
- Scalars/arrays: overwrite.

Config shape (v1)
```jsonc
{
  "$schema": "rip://config/v1",
  "provider": {
    "openrouter": {
      "endpoint": "https://openrouter.ai/api/v1/responses",
      "api_key": { "env": "OPENROUTER_API_KEY" }, // or "sk-..."
      "headers": {
        "HTTP-Referer": "https://example.com",
        "X-Title": "rip"
      },
      // Provider-scoped OpenResponses defaults (optional; overlays the global defaults).
      "openresponses": {
        "stateless_history": true,
        "reasoning": {
          "effort": "medium",
          "summary": "concise"
        }
      }
    },
    "openai": {
      "endpoint": "https://api.openai.com/v1/responses",
      "api_key": { "env": "OPENAI_API_KEY" },
      "openresponses": {
        "stateless_history": false
      }
    }
  },

  // Default model route (provider_id/model_id). Provider id is the key under "provider".
  "model": "openrouter/openai/gpt-oss-20b",

  // Optional explicit roles (Phase 2 will expand role usage).
  "roles": {
    "primary": "openrouter/openai/gpt-oss-20b"
  },

  // Default OpenResponses behavior (optional).
  "openresponses": {
    "parallel_tool_calls": false,
    "reasoning": {
      "summary": "concise"
    }
  }
}
```

Route strings
- Route format: `provider_id/model_id`
  - Example: `openrouter/openai/gpt-oss-20b`
  - `model_id` may include `/`; parsing splits on the first `/`.
- Optional variant suffix (reserved for Phase 2 request tuning): `provider_id/model_id#variant`

Secrets posture
- `api_key` may be specified inline or via `{ "env": "ENV_VAR" }`.
- Secrets MUST NOT be emitted into event frames, artifacts, or logs.
- Diagnostic surfaces report only presence + source (never the secret value).

Diagnostics
- Server: `GET /config/doctor` returns a sanitized config resolution summary.
- CLI: `rip config doctor` prints the same summary (local-first by default).
- Doctor reports both the configured route and the effective route:
  - `route`: the default route chosen from config (`roles.primary` or `model`)
  - `effective_route`: the provider/model actually used after endpoint/model overrides are applied
- Doctor also reports per-field provenance where relevant (`*_source`), so it is obvious whether endpoint/model/OpenResponses defaults came from config, env compat, or per-run overrides.
- The first typed reasoning controls now live in that same OpenResponses surface:
  - `reasoning.effort`: `none|minimal|low|medium|high|xhigh`
  - `reasoning.summary`: `concise|detailed|auto`
- They resolve through the same layered path as the other OpenResponses fields:
  - global config
  - provider-scoped overlay
  - env compat overrides (`RIP_OPENRESPONSES_REASONING_EFFORT`, `RIP_OPENRESPONSES_REASONING_SUMMARY`)
  - per-run overrides
- Doctor now also surfaces the resolved OpenResponses compatibility profile for the active route:
  - provider profile health (`native` / `compat` / `unsupported` / `unknown`)
  - active vs recommended conversation strategy
  - effective validation normalizations
  - any curated model overlay attached to the resolved route
- Doctor also surfaces the resolved `reasoning` object and per-field provenance, so operators can see both what RIP is asking for and what the compatibility matrix says that route supports.
- Compatibility resolution prefers the resolved `provider_id` from route/config selection and falls back to endpoint heuristics only when RIP has no canonical provider id for the route. This keeps custom proxies and loopback/provider-fixture endpoints aligned with the intended provider profile.
