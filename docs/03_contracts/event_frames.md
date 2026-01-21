# Event Frames (Phase 1)

Summary
- Canonical internal event schema for all surfaces.
- Frames are compact structs in Rust; JSON only at the edges (SSE/logging).

Schema (v1 — session-scoped)
- `id`: string (uuid)
- `session_id`: string (uuid)
- `seq`: u64 (monotonic per session)
- `timestamp_ms`: u64 (unix epoch ms)
- `type`: string (frame type)
- `payload`: fields defined by `type` (serialized alongside `type`)

Frame types
- `session_started`
  - `input`: string
- `output_text_delta`
  - `delta`: string
- `session_ended`
  - `reason`: string
- `tool_started`
  - `tool_id`: string (uuid)
  - `name`: string
  - `args`: object
  - `timeout_ms`: u64 | null
- `tool_stdout`
  - `tool_id`: string
  - `chunk`: string
- `tool_stderr`
  - `tool_id`: string
  - `chunk`: string
- `tool_ended`
  - `tool_id`: string
  - `exit_code`: i32
  - `duration_ms`: u64
  - `artifacts`: object | null
- `tool_failed`
  - `tool_id`: string
  - `error`: string
- `provider_event`
  - `provider`: string (e.g. `openresponses`)
  - `status`: `event` | `done` | `invalid_json`
  - `event_name`: string | null (SSE `event:` value)
  - `data`: object | null (parsed Open Responses event payload)
  - `raw`: string | null (raw `data:` payload, only when needed)
  - `errors`: string[] (schema/validation errors)
  - `response_errors`: string[] (ResponseResource validation errors)
- `checkpoint_created`
  - `checkpoint_id`: string (uuid)
  - `label`: string
  - `created_at_ms`: u64
  - `files`: string[] (relative paths)
  - `auto`: bool
  - `tool_name`: string | null
- `checkpoint_rewound`
  - `checkpoint_id`: string (uuid)
  - `label`: string
  - `files`: string[] (relative paths)
- `checkpoint_failed`
  - `action`: `create` | `rewind`
  - `error`: string

Invariants
- `seq` starts at 0 and increments by 1 for each emitted frame.
- Frames are append-only and ordered within a session.
- `session_ended` is the terminal frame for a runtime-generated session.
- Background tool tasks are modeled as **task event streams** and are encoded using the v1 envelope for now:
  - `session_id` holds the `task_id` (stream id) for task streams.
  - Task streams do not emit `session_started/session_ended`; lifecycle is expressed via `tool_task_*` frames.
- Provider adapters emit `provider_event` for every SSE event (no drops).
- Automatic checkpoint events for file-edit tools are emitted before the tool starts.

Example
```
{"id":"...","session_id":"...","timestamp_ms":0,"seq":0,"type":"session_started","input":"hi"}
{"id":"...","session_id":"...","timestamp_ms":1,"seq":1,"type":"provider_event","provider":"openresponses","status":"event","event_name":"response.output_text.delta","data":{"type":"response.output_text.delta","delta":"hi"},"raw":null,"errors":[],"response_errors":[]}
{"id":"...","session_id":"...","timestamp_ms":2,"seq":2,"type":"output_text_delta","delta":"ack: hi"}
{"id":"...","session_id":"...","timestamp_ms":3,"seq":3,"type":"session_ended","reason":"completed"}
```

Phase 2 (planned additions)

Schema (v2 — stream-scoped; planned)
- Phase 2 introduces additional event streams beyond sessions (notably tool tasks). Frames remain the canonical interchange, but the envelope gains an explicit stream scope.
- `id`: string (uuid)
- `stream_kind`: `"session"` | `"task"` | `"thread"` | `"artifact"` (extensible)
- `stream_id`: string (e.g. session id, task id)
- `seq`: u64 (monotonic per `{stream_kind, stream_id}`)
- `timestamp_ms`: u64 (unix epoch ms)
- `type`: string (frame type)
- `payload`: fields defined by `type`
- Compatibility:
  - Session streams may continue to include `session_id` as an alias of `{stream_id}` while v1 is still supported.
  - Task streams never emit `session_started/session_ended`; their lifecycle is expressed via task/tool frames.

- Background tool tasks:
  - `tool_task_spawned`: `{task_id, tool_name, args, cwd?, title?, execution_mode: pipes|pty, origin_session_id?, artifacts?}`
  - `tool_task_status`: `{task_id, status, exit_code?, started_at_ms?, ended_at_ms?, artifacts?, error?}`
  - `tool_task_cancel_requested`: `{task_id, reason}`
  - `tool_task_cancelled`: `{task_id, reason, wall_time_ms?}`
  - `tool_task_output_delta`: `{task_id, stream: stdout|stderr|pty, chunk, artifacts?}`
  - `tool_task_stdin_written`: `{task_id, chunk_b64}` (PTY only)
  - `tool_task_resized`: `{task_id, rows, cols}`
  - `tool_task_signalled`: `{task_id, signal}`
- Artifact-backed outputs:
  - `artifact_written`: `{artifact_id, kind, bytes, digest, preview?}`
  - Tool frames may include bounded previews plus `artifact_refs` when full output is stored externally.
- Skills (Agent Skills/OpenSkills):
  - `skill_catalog_updated`: `{count, roots, collisions?}`
  - `skill_loaded`: `{name, path, digest, frontmatter, warnings?}`
  - `skill_invoked`: `{name, args?, mode:manual|auto, effective_allowed_tools?}`
  - `skill_warning`: `{name?, kind, detail}`
