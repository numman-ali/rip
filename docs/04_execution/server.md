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
- Server is optional; CLI can talk directly to ripd (in-process) or via HTTP.
- SSE stream emits JSON event frames (`docs/03_contracts/event_frames.md`).
- OpenAPI spec is exposed at `/openapi.json` (canonical) and may be mirrored in `schemas/`.
- JSON input envelopes can trigger tool execution and checkpoint actions (used for deterministic tests):
  - Tool: `{"tool":"write","args":{"path":"a.txt","content":"hi"},"timeout_ms":1000}`
  - Checkpoint create: `{"checkpoint":{"action":"create","label":"manual","files":["a.txt"]}}`
  - Checkpoint rewind: `{"checkpoint":{"action":"rewind","id":"<checkpoint_id>"}}`
