# CLI Execution Model

Summary
- Interactive CLI is the primary UX.
- Headless CLI is for automation.
- Full-screen TUI is a separate surface (`rip-tui`, Phase 2) and is tracked separately.

Interactive mode (draft)
- rip run <task>
- streams event frames (no diffs/approvals in Phase 1)

Headless mode (draft)
- rip run <task> --headless --view raw
- emits newline-delimited JSON event frames
- `--view output` prints text + reasoning + tool deltas extracted from provider events

Notes
- CLI is a thin UI over the agent server (HTTP/SSE).
- Start a local server with `rip serve` (or `ripd`), then run `rip run ...`.
- No agent logic lives in the CLI.
