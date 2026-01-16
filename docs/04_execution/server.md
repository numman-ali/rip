# Agent Server Execution Model

Summary
- Server exposes agent sessions over HTTP/SSE.
- Not an Open Responses API.

Session lifecycle (draft)
- POST /sessions -> session id
- POST /sessions/:id/input -> send user input
- GET /sessions/:id/events -> SSE event stream
- POST /sessions/:id/cancel -> cancel session

Notes
- Server is optional; CLI can talk directly to ripd.
