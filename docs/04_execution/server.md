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

Task lifecycle (pipes + pty; implemented)
- POST /tasks -> task id (background task spawn)
- GET /tasks -> list tasks
- GET /tasks/:id -> task status
- GET /tasks/:id/events -> SSE task event stream
- POST /tasks/:id/cancel -> cancel task (best-effort)
- GET /tasks/:id/output -> range fetch task logs (`stream=stdout|stderr|pty` depending on task mode)
- POST /tasks/:id/stdin -> write stdin bytes (`chunk_b64`, PTY only)
- POST /tasks/:id/resize -> resize terminal (`rows`,`cols`, PTY only)
- POST /tasks/:id/signal -> send a signal (`signal`, PTY only today)

Thread lifecycle (continuities; implemented)
- POST /threads/ensure -> default thread id
- GET /threads -> list threads (power/debug)
- GET /threads/:id -> thread metadata
- POST /threads/:id/messages -> append a message and spawn a run (returns `{thread_id, message_id, session_id}`)
- POST /threads/:id/branch -> create a child thread linked to a parent cut point
- POST /threads/:id/handoff -> create a new thread with curated context (`summary_markdown` and/or `summary_artifact_id`)
- GET /threads/:id/events -> SSE continuity event stream (past + live)

Notes
- Today: `rip serve` (or `ripd`) exposes the session API for remote clients (SDKs can attach via `--server <url>`).
- Default local: `rip run` (and `rip`) auto-start/auto-attach to a local authority for the store; use `--server <url>` to target a remote authority.
- Today: `rip tasks ...` is the CLI adapter over the task API (local-first; use `--server <url>` for remote).
- SSE stream emits JSON event frames (`docs/03_contracts/event_frames.md`).
- OpenAPI spec is exposed at `/openapi.json` (canonical) and may be mirrored in `schemas/`.
- Authority posture (ADR-0019): the server/control plane is the single sequencer for truth writes for a store; many clients may attach concurrently.
- Server bind addr: `RIP_SERVER_ADDR` (default: `127.0.0.1:7341`).
- Local authority discovery: the authority writes `RIP_DATA_DIR/authority/meta.json` and holds a store lock at `RIP_DATA_DIR/authority/lock.json`.
- Workspace-mutating operations are serialized across sessions and background tasks; read-only tools may run concurrently.
- JSON input envelopes can trigger tool execution and checkpoint actions (used for deterministic tests):
  - Tool: `{"tool":"write","args":{"path":"a.txt","content":"hi"},"timeout_ms":1000}`
  - Checkpoint create: `{"checkpoint":{"action":"create","label":"manual","files":["a.txt"]}}`
  - Checkpoint rewind: `{"checkpoint":{"action":"rewind","id":"<checkpoint_id>"}}`

Provider config (OpenResponses, Phase 1)
- If `RIP_OPENRESPONSES_ENDPOINT` is set, prompt inputs stream OpenResponses SSE and emit `provider_event` frames (plus derived `output_text_delta`).
- For latency debugging, ripd also emits:
  - `openresponses_request_started` (immediately before sending the request)
  - `openresponses_response_headers` (after receiving HTTP headers)
  - `openresponses_response_first_byte` (after receiving the first body bytes)
- Env vars:
  - `RIP_OPENRESPONSES_ENDPOINT` (example: `https://api.openai.com/v1/responses`)
  - `RIP_OPENRESPONSES_API_KEY` (optional; sent as `Authorization: Bearer ...`)
  - `RIP_OPENRESPONSES_MODEL` (optional)
    - If unset and the endpoint is OpenRouter Responses (`https://openrouter.ai/api/v1/responses`), RIP defaults to `openai/gpt-oss-20b`.
    - Otherwise, the request omits `model` (provider may reject it).
  - `RIP_OPENRESPONSES_TOOL_CHOICE` (optional; default `auto`)
    - `auto` | `none` | `required`
    - `function:<tool_name>` (request a specific function tool)
    - `json:<tool_choice_json>` (pass a full OpenResponses `tool_choice` value)
  - `RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE` (optional; if set, append this user message after tool outputs in follow-up requests for provider compatibility)
  - `RIP_OPENRESPONSES_STATELESS_HISTORY` (optional; if set, follow-ups resend full input history instead of using `previous_response_id`)
  - `RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS` (optional; if set, request parallel tool calls; execution remains sequential in Phase 1)
  - Observability (opt-in; writes prompt/tool definitions into artifact blobs):
    - `RIP_OPENRESPONSES_DUMP_REQUEST=1` emits `openresponses_request` frames and writes each request body to `.rip/artifacts/blobs/<artifact_id>`.
    - `RIP_OPENRESPONSES_DUMP_REQUEST_MAX_BYTES` caps per-request bytes stored (default: 1,000,000).
- If `RIP_OPENRESPONSES_ENDPOINT` is not set, ripd runs in stub mode (`output_text_delta: "ack: <input>"`).

Other env vars
- `RIP_DATA_DIR`: overrides the default `data/` directory.
- `RIP_WORKSPACE_ROOT`: overrides the workspace root used for tool IO and checkpoints.
- `RIP_TASKS_ALLOW_PTY`: if set (`1|true|yes|on`), allow `execution_mode=pty` for background tasks and enable PTY control ops.
