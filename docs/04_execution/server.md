# Agent Server Execution Model

Summary
- Server exposes agent sessions over HTTP/SSE.
- Not an Open Responses API.
- OpenAPI spec is generated from code and served by the server.
- Server API is the canonical control plane for active capabilities.

Session lifecycle (draft)
- POST /sessions -> session id
- POST /sessions/:id/input -> send user input
- GET /sessions/:id/events -> SSE event stream
- POST /sessions/:id/cancel -> cancel session

Notes
- Today: `rip serve` (or `ripd`) exposes the session API for remote clients/SDKs over HTTP/SSE.
- Today: `rip run` defaults to in-process execution; use `--server <url>` to target a remote server.
- SSE stream emits JSON event frames (`docs/03_contracts/event_frames.md`).
- OpenAPI spec is exposed at `/openapi.json` (canonical) and may be mirrored in `schemas/`.
- JSON input envelopes can trigger tool execution and checkpoint actions (used for deterministic tests):
  - Tool: `{"tool":"write","args":{"path":"a.txt","content":"hi"},"timeout_ms":1000}`
  - Checkpoint create: `{"checkpoint":{"action":"create","label":"manual","files":["a.txt"]}}`
  - Checkpoint rewind: `{"checkpoint":{"action":"rewind","id":"<checkpoint_id>"}}`

Provider config (OpenResponses, Phase 1)
- If `RIP_OPENRESPONSES_ENDPOINT` is set, prompt inputs stream OpenResponses SSE and emit `provider_event` frames (plus derived `output_text_delta`).
- Env vars:
  - `RIP_OPENRESPONSES_ENDPOINT` (example: `https://api.openai.com/v1/responses`)
  - `RIP_OPENRESPONSES_API_KEY` (optional; sent as `Authorization: Bearer ...`)
  - `RIP_OPENRESPONSES_MODEL` (optional; if unset, the request omits `model`)
  - `RIP_OPENRESPONSES_TOOL_CHOICE` (optional; default `auto`)
    - `auto` | `none` | `required`
    - `function:<tool_name>` (request a specific function tool)
    - `json:<tool_choice_json>` (pass a full OpenResponses `tool_choice` value)
  - `RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE` (optional; if set, append this user message after tool outputs in follow-up requests for provider compatibility)
  - `RIP_OPENRESPONSES_STATELESS_HISTORY` (optional; if set, follow-ups resend full input history instead of using `previous_response_id`)
  - `RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS` (optional; if set, request parallel tool calls; execution remains sequential in Phase 1)
- If `RIP_OPENRESPONSES_ENDPOINT` is not set, ripd runs in stub mode (`output_text_delta: "ack: <input>"`).

Other env vars
- `RIP_DATA_DIR`: overrides the default `data/` directory.
- `RIP_WORKSPACE_ROOT`: overrides the workspace root used for tool IO and checkpoints.
