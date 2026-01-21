# CLI Execution Model

Summary
- `rip` (no subcommand) launches the fullscreen interactive terminal UI (TUI).
- Headless CLI is for automation.
- `rip run` remains the headless/automation entrypoint (JSONL frames or rendered text).

Interactive mode (default)
- `rip [<prompt>]`
- Starts an interactive terminal UI driven by canonical event frames.
- Local-first: runs the session engine in-process (no HTTP required).
- Phase 1 posture: single-run sessions; enter a prompt to start a run, then submit another prompt for a new run.

Attach mode (remote)
- `rip --server <url> --session <id>`
- Attaches the fullscreen UI to an existing server session and streams canonical event frames over SSE.

Task attach mode (remote)
- `rip --server <url> --task <id>`
- Attaches the fullscreen UI to an existing background task and streams task frames over SSE.

Task commands (remote)
- `rip tasks --server <url> spawn --tool bash --args '{"command":"<cmd>"}'`
- `rip tasks --server <url> list`
- `rip tasks --server <url> status <task_id>`
- `rip tasks --server <url> cancel <task_id> --reason "<why>"`
- `rip tasks --server <url> output <task_id> --stream stdout --offset-bytes 0 --max-bytes 4096`
- `rip tasks --server <url> events <task_id>` (prints JSON frames until terminal `tool_task_status`)

Headless mode (draft)
- rip run <task> --headless --view raw
- emits newline-delimited JSON event frames
- `--view output` prints human output: text deltas only (tool stdout/stderr emitted only if no model output)

Provider shortcuts (local runs only)
- `--provider openai|openrouter` selects the OpenResponses endpoint and API key env fallback.
- `--model <id>` overrides `RIP_OPENRESPONSES_MODEL`.
- `--stateless-history` enables stateless followups (`RIP_OPENRESPONSES_STATELESS_HISTORY=1`).
- `--parallel-tool-calls` sets `RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS=1` (request-only; execution remains sequential).
- `--followup-user-message <text>` sets `RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE`.
- Flags are ignored for `--server` runs; configure the server environment instead.
Examples:
- OpenAI: `OPENAI_API_KEY=... rip run "<task>" --provider openai --model gpt-5-nano-2025-08-07`
- OpenRouter: `OPENROUTER_API_KEY=... rip run "<task>" --provider openrouter --model mistralai/devstral-2512:free --stateless-history`
- Live sweep: `scripts/live-openresponses-sweep` (real APIs; runs a tool-sweep against OpenAI/OpenRouter).

Notes
- CLI is a thin adapter over the shared session engine.
- Default: interactive `rip` and headless `rip run` execute in-process (no HTTP required).
- Remote: `rip run ... --server <url>` streams the same event frames over HTTP/SSE.
- `rip serve` (or `ripd`) starts the agent server for remote clients (SDKs can target it via `--server <url>`).
- Default output: `rip run ...` uses `--view output` (human-readable). Use `--view raw` for newline-delimited JSON frames.
- Phase 1 is single-run sessions (no multi-turn/thread resume yet); OpenResponses tool execution is sequential and capped (`max_tool_calls=32`, `parallel_tool_calls=false`) per ADR-0005.
