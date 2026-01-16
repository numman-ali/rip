# Contract: CLI (interactive + headless)

Summary
- Interactive: rich streaming UI with approvals/diffs.
- Headless: JSON events for automation.

Inputs
- User prompts and commands.
- Agent event stream from ripd.

Outputs
- Rendered UI (interactive) or JSON stream (headless).
- Control commands to ripd (cancel, approve, resume).

Config
- Mode: interactive or headless.
- Output format and verbosity.

Invariants
- No business logic; UI only.
- Never blocks agent runtime.

Tests
- Golden event stream rendering.
- Headless JSON schema validation.
