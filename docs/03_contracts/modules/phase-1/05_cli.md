# Contract: CLI (interactive + headless)

Summary
- Interactive: lightweight streaming UI (no diffs/approvals in Phase 1).
- Headless: JSON events for automation.
- Full-screen TUI is a separate surface (`rip-tui`) with the same capability ids (Phase 2).

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
