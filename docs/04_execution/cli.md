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
- `--view output` prints human output: text deltas + tool stdout/stderr + reasoning/tool-call deltas

Notes
- CLI is a thin adapter over the shared session engine.
- Default: `rip run ...` executes in-process (no HTTP required).
- Remote: `rip run ... --server <url>` streams the same event frames over HTTP/SSE.
- `rip serve` (or `ripd`) starts the agent server for remote clients/SDKs.
- Phase 1 is single-run sessions (no multi-turn/thread resume yet); OpenResponses tool execution is sequential and capped (`max_tool_calls=32`, `parallel_tool_calls=false`) per ADR-0005.
