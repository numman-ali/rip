# Contract: CLI (interactive + headless)

Summary
- Interactive: fullscreen, frame-driven terminal UI (`rip`) for observing/running sessions and managing continuities/tasks (no business logic in the UI).
- Headless: `rip run --headless` emits newline-delimited JSON event frames for automation.
- Local-first: by default, CLI auto-starts/auto-attaches to the local authority for the store; `--server <url>` targets a remote control plane.

Inputs
- User prompts and commands.
- Agent event stream from the shared session engine (in-process) or from `ripd` over HTTP/SSE.

Outputs
- Rendered UI (interactive) or JSON stream of event frames (headless).
- Control commands to ripd (cancel).

Config
- Mode: interactive or headless.
- Output format and verbosity.
- View mode (headless): raw frames or derived output (text/reasoning/tool deltas).

Invariants
- No business logic; UI only.
- Never blocks agent runtime.

Tests
- Golden event stream rendering.
- Headless JSON schema validation.
