# OpenResponses Provider Compatibility Profiles

Summary
- Defines the versioned provider/model compatibility profile shape for the OpenResponses boundary.
- Profiles own validation-only normalizations, conversation-state expectations, request-surface health, tool-surface health, and curated provider/model capability notes.
- Raw provider payload fidelity is still canonical; profiles may only normalize a copy used for validation and health decisions.

Sources
- `temp/openresponses/src/pages/specification.mdx`
- `temp/openresponses/src/pages/compliance.mdx`
- `temp/docs/openrouter/responses/overview.md`
- `temp/docs/openrouter/responses/basic-usage.md`
- `temp/docs/openrouter/responses/reasoning.md`
- `temp/docs/openrouter/responses/tool-calling.md`
- `temp/docs/openrouter/nemotron_3_nano_30b_a3b_free_2026-04-19.md`
- `temp/docs/openai/codex/unrolling-the-codex-agent-loop_2026-01-23.md`

Why this exists
- OpenResponses is shared across providers, but provider and model behavior still diverges in real life.
- Those divergences must not leak upward as random UI suppressions, session hacks, or scattered `if provider == ...` branches.
- RIP therefore owns a declarative provider-boundary compatibility layer:
  - provider profiles describe endpoint-level behavior
  - model overlays describe model-specific health/capability observations
  - runtime selection uses those profiles to choose validation and compatibility behavior
  - docs mirror the same truth so operators can inspect the current state

Rules
- Provider/model quirks are handled at the provider boundary, not in UI/surface code.
- Validation-only normalization is allowed only through a versioned compatibility profile.
- Validation-only normalization must preserve raw provider payloads in `provider_event` frames.
- Known provider/model differences must be captured in both code and docs.
- Real downstream failures must still surface as recoverable provider errors; compat-normalized success paths must not.

Compatibility levels
- `native`: behavior matches the OpenResponses spec shape RIP expects.
- `compat`: behavior is supported through a declared normalization or adapter-side workaround.
- `unsupported`: provider/model does not currently support the behavior.
- `unknown`: not yet proven or not yet curated.

Conversation strategies
- `previous_response_id`: use provider-side conversation state for follow-ups.
- `stateless_history`: resend full prior item history for follow-ups.
- `config_driven`: the provider is generic enough that RIP should not assume a preferred mode yet.

Config composition posture
- Compatibility profiles are not a replacement for config; they are the compatibility truth that config resolves against.
- Provider-scoped config should continue to own:
  - endpoint
  - auth
  - static request headers
  - timeouts / transport knobs
  - provider-level OpenResponses defaults
- Model-scoped overlays should own:
  - capability health
  - modality health
  - reasoning/tool/structured-output expectations
  - curated limits or quirks when proven
- Run/session-scoped overrides should own:
  - temporary route choice
  - per-run request flags
  - explicit operator overrides
- The resolved runtime view is therefore:
  - config chooses the route and operator defaults
  - compat profiles describe what that route can really do
  - the runtime uses the resolved profile to validate, adapt, diagnose, and explain behavior

Profile shape (v1)
- Provider profile:
  - `version`
  - `provider_id`
  - `label`
  - `stream_shape`
  - `conversation.previous_response_id`
  - `conversation.stateless_history`
  - `conversation.recommended`
  - request health:
    - `background`
    - `store`
    - `service_tier`
    - `response_include`
    - `reasoning_parameter`
  - tool health:
    - `function_calling`
    - `tool_choice`
    - `allowed_tools`
    - `hosted_tools`
    - `mcp_servers`
    - `mcp_headers`
  - input modality health:
    - `input_text`
    - `input_image`
    - `input_file`
    - `input_video`
  - validation flags:
    - `missing_item_ids`
    - `missing_response_user`
    - `reasoning_text_events`
    - `missing_reasoning_summary`
- Model overlay:
  - `version`
  - `provider_id`
  - `model_id`
  - `label`
  - health:
    - `reasoning_parameter`
    - `tool_calling`
    - `structured_outputs`
    - input modality health (`input_text`, `input_image`, `input_file`, `input_video`)

Runtime ownership
- Code: `crates/ripd/src/openresponses_compat.rs`
- Runtime use today:
  - `crates/ripd/src/session/openresponses.rs` resolves the compatibility profile from `{provider_id, endpoint, model}` and now derives both validation behavior and the effective follow-up strategy from that profile plus the explicit `stateless_history` request preference.
  - The resolved `provider_id` is the primary selector when RIP knows it from config/route resolution; endpoint heuristics are only a fallback for generic direct-provider wiring and env-only setups.
  - `GET /config/doctor` and `rip config doctor` now surface the resolved provider profile, a structured conversation object (`requested`, `effective`, `support`, `warnings`), the effective validation posture, any curated model overlay for the active route, and a route-specific reasoning support object with requested vs effective values plus downgrade/unverified warnings.
  - `crates/ripd/src/provider_openresponses.rs` now consumes the typed reasoning config surface (`reasoning.effort`, `reasoning.summary`) through the compatibility layer, so the emitted OpenResponses `reasoning` request object is the effective route-safe value rather than the raw requested value.
  - When a run starts and RIP had to degrade a requested conversation or reasoning setting, the session stream now emits a structured compat warning frame (`rip.compat.warning`) before the first provider request. Surfaces may render that as a runtime notice, but the warning source remains the provider-boundary compatibility layer.
- This slice is intentionally modest in runtime effect:
  - current runtime selection is used for boundary validation behavior first
  - the wider request/tool/modality matrix is now versioned and inspectable, even where the runtime does not act on every field yet
  - future slices can extend the same profile to drive request defaults, model-picker capability chips, and provider health reporting

## Provider profiles (seed set)

### `openai`
- Endpoint: `https://api.openai.com/v1/responses`
- Stream shape: `native`
- Conversation:
  - `previous_response_id`: `native`
  - `stateless_history`: `native`
  - recommended: `previous_response_id`
- Request health:
  - `background`: `native`
  - `store`: `native`
  - `service_tier`: `native`
  - `response_include`: `native`
  - `reasoning_parameter`: `native`
- Tool health:
  - `function_calling`: `native`
  - `tool_choice`: `native`
  - `allowed_tools`: `native`
  - `hosted_tools`: `unknown`
  - `mcp_servers`: `unknown`
  - `mcp_headers`: `unknown`
- Input modalities:
  - `input_text`: `native`
  - `input_image`: `native`
  - `input_file`: `native`
  - `input_video`: `unknown`
- Validation posture: `strict`
- Notes:
  - OpenAI is the canonical Responses endpoint, so the base request surface is treated as the reference implementation.
  - Advanced hosted-tool and MCP-specific rows stay `unknown` until they are proven in RIP’s own integration coverage.

### `openrouter`
- Endpoint: `https://openrouter.ai/api/v1/responses`
- Stream shape: `compat`
- Conversation:
  - `previous_response_id`: `unsupported`
  - `stateless_history`: `native`
  - recommended: `stateless_history`
- Request health:
  - `background`: `unknown`
  - `store`: `unsupported`
  - `service_tier`: `unknown`
  - `response_include`: `unknown`
  - `reasoning_parameter`: `native`
- Tool health:
  - `function_calling`: `native`
  - `tool_choice`: `native`
  - `allowed_tools`: `unknown`
  - `hosted_tools`: `compat`
  - `mcp_servers`: `unknown`
  - `mcp_headers`: `unknown`
- Input modalities:
  - `input_text`: `native`
  - `input_image`: `unknown`
  - `input_file`: `unknown`
  - `input_video`: `unknown`
- Validation posture: `compat`
- Notes:
  - OpenRouter Responses Beta is stateless-only. RIP normalizes known downstream deltas for validation: missing response `user`, `response.reasoning_text.{delta,done}`, and missing item ids on paths that still rely on stateless history.
  - RIP now treats `previous_response_id` as an unsupported request on this provider profile and automatically coerces follow-ups to `stateless_history`, with the downgrade surfaced in `config doctor` rather than discovered only after a failed turn.
  - The current OpenRouter docs explicitly advertise reasoning support, tool calling, parallel tool execution, web search, and a stateless transformation layer. RIP records those facts here, but only treats the fields as `native` or `compat` where they are actually curated.
  - `store` is recorded as `unsupported` because the provider posture is stateless and does not preserve provider-side conversation state between requests.
  - Images/files/video stay `unknown` here until they are proven on the Responses endpoint in RIP’s own integration coverage and, where needed, narrowed by model overlays.

### `generic`
- Endpoint: any other OpenResponses-compatible endpoint
- Stream shape: `unknown`
- Conversation:
  - `previous_response_id`: `unknown`
  - `stateless_history`: `unknown`
  - recommended: `config_driven`
- Request health: all `unknown`
- Tool health: all `unknown`
- Input modalities:
  - `input_text`: `native`
  - `input_image`: `unknown`
  - `input_file`: `unknown`
  - `input_video`: `unknown`
- Validation posture: `strict`
- Notes:
  - Safe fallback for OpenResponses-compatible endpoints we have not curated yet.
  - `input_text` is treated as `native`; everything else must be proven before RIP relies on it.

## Model overlays (seed set)

### `openrouter / nvidia/nemotron-3-nano-30b-a3b:free`
- Reasoning parameter: `native`
- Tool calling: `unknown`
- Structured outputs: `unknown`
- Input modalities:
  - `input_text`: `native`
  - `input_image`: `unknown`
  - `input_file`: `unknown`
  - `input_video`: `unknown`
- Notes:
  - Current OpenRouter model page advertises reasoning support for this model.
  - RIP still treats stream-shape quirks as provider-boundary concerns until model-specific differences are observed and curated.

### `openrouter / google/gemma-4-26b-a4b-it`
- Reasoning parameter: `native`
- Tool calling: `unknown`
- Structured outputs: `unknown`
- Input modalities:
  - `input_text`: `native`
  - `input_image`: `unknown`
  - `input_file`: `unknown`
  - `input_video`: `unknown`
- Notes:
  - This is the current default OpenRouter reasoning route in the local operator config.
  - RIP has live proof that detailed reasoning summaries render on this route in the fullscreen TUI; the provider stream still arrives through the OpenRouter `reasoning_text` compat event family, so summary visibility is tracked as `compat` at the boundary rather than a UI special case.

## Route-specific reasoning support (v0.1)

- The global typed request grammar is still broader than any single provider/model route.
- RIP therefore treats reasoning in three layers:
  - request grammar: what the OpenResponses/OpenAI-style surface can express
  - compatibility support: what a resolved provider/model route is known to accept
  - effective request: what RIP will actually send after applying route support/degradation rules
- Current curated route rules:
  - OpenAI `gpt-5.4-mini` / `gpt-5.4-nano`
    - `reasoning.effort`: `none|low|medium|high|xhigh`
    - `reasoning.summary`: `auto|concise|detailed`
  - OpenRouter generic Responses Beta
    - `reasoning.effort`: `minimal|low|medium|high`
    - `reasoning.summary`: unverified unless a curated model overlay says otherwise
  - OpenRouter `google/gemma-4-26b-a4b-it`
    - `reasoning.summary`: `compat` with current live proof for `auto|concise|detailed`
- Happy-path handling:
  - when a requested value is in the curated route set, RIP forwards it unchanged
  - when the route is uncurated for a field, RIP forwards it but marks the field `unknown` / unverified
- Unhappy-path handling:
  - when a requested value is outside a curated supported set, RIP omits that field from the actual request and records a downgrade warning in `config.doctor`
  - this keeps the OpenResponses spec canonical while preventing known-bad provider/model values from leaking through as avoidable downstream errors

## Immediate implications

- OpenRouter false-positive provider errors in the TUI are a provider-boundary compatibility problem, not a UI problem.
- The fix for that class of bug is:
  - add/update the provider profile
  - add tests that prove the real payload is accepted without losing raw fidelity
  - document the difference in this matrix
- Static request headers belong in config, but whether a provider/model route needs or tolerates them should be captured here as compatibility knowledge rather than rediscovered ad hoc.
- Provider/model capability curation should cover request composition too: custom/static headers, hosted tools, MCP headers, reasoning flags, and multimodal/image/file/video support belong in this matrix even when config remains the place that injects the actual values.
- Current proof coverage now includes:
  - OpenAI and OpenRouter route resolution in `config.doctor`
  - provider-id-over-endpoint selection for noncanonical/loopback OpenRouter endpoints
  - a loopback OpenRouter session smoke proving an OpenRouter-shaped stream can complete without false provider errors when the OpenRouter profile is explicitly selected
  - outbound request composition proof for auth + custom headers + tool-choice/max-tool-call controls at the HTTP boundary
- The next expansion should cover:
  - request-field quirks (`store`, `background`, `service_tier`, `include`, `parallel_tool_calls`)
  - tool/runtime quirks (`tool_choice`, `allowed_tools`, hosted-tool behavior, tool schema strictness, MCP headers)
  - model capability overlays for reasoning, tool calling, structured outputs, and multimodal input
  - operator-facing surfacing in `config.doctor`, the TUI model selector, and SDK diagnostics
- Known current gap:
  - typed reasoning controls are now first-class for `effort` and `summary`, but multimodal/image/file/video request controls, hosted-tool/MCP request composition, and `include`-level details such as `reasoning.encrypted_content` are still curated in the compatibility matrix before they are lifted into the same runtime/config surface.
