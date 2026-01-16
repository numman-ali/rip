# Contract: CLI (interactive + headless)

Summary
- Interactive: rich streaming UI with approvals/diffs.
- Headless: JSON events for automation.

Inputs
- User prompts and commands.
- Agent event stream from ripd.

Outputs
- Rendered UI (interactive) or JSON stream of event frames (headless).
- Control commands to ripd (cancel, approve, resume).

Config
- Mode: interactive or headless.
- Output format and verbosity.
- View mode: raw frames or derived output (text deltas).

Invariants
- No business logic; UI only.
- Never blocks agent runtime.

Tests
- Golden event stream rendering.
- Headless JSON schema validation.
