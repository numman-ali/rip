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
- `--view output` prints human output: text deltas only (tool stdout/stderr emitted only if no model output)

Provider shortcuts (local runs only)
- `--provider openai|openrouter` selects the OpenResponses endpoint and API key env fallback.
- `--model <id>` overrides `RIP_OPENRESPONSES_MODEL`.
- `--stateless-history` enables stateless followups (`RIP_OPENRESPONSES_STATELESS_HISTORY=1`).
- `--followup-user-message <text>` sets `RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE`.
- Flags are ignored for `--server` runs; configure the server environment instead.
Examples:
- OpenAI: `OPENAI_API_KEY=... rip run "<task>" --provider openai --model gpt-5-nano-2025-08-07`
- OpenRouter: `OPENROUTER_API_KEY=... rip run "<task>" --provider openrouter --model mistralai/devstral-2512:free --stateless-history`

Notes
- CLI is a thin adapter over the shared session engine.
- Default: `rip run ...` executes in-process (no HTTP required).
- Remote: `rip run ... --server <url>` streams the same event frames over HTTP/SSE.
- `rip serve` (or `ripd`) starts the agent server for remote clients/SDKs.
- Default output: `rip run ...` uses `--view output` (human-readable). Use `--view raw` for newline-delimited JSON frames.
- Phase 1 is single-run sessions (no multi-turn/thread resume yet); OpenResponses tool execution is sequential and capped (`max_tool_calls=32`, `parallel_tool_calls=false`) per ADR-0005.
