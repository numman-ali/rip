# CLI Execution Model

Summary
- Interactive CLI is the primary UX.
- Headless CLI is for automation.

Interactive mode (draft)
- rip run <task>
- streams events, diffs, and approvals

Headless mode (draft)
- rip run <task> --headless --json
- emits newline-delimited JSON events

Notes
- CLI is a thin UI over ripd.
- No agent logic lives in the CLI.
