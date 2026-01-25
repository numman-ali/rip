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

Remote interactive mode
- `rip --server <url> [<prompt>]`
- Continuity-first: posts to the default thread (continuity) and streams the resulting session frames over SSE (sessions remain hidden by default).

Attach mode (remote)
- `rip --server <url> --session <id>`
- Attaches the fullscreen UI to an existing server session and streams canonical event frames over SSE (debug/power mode).

Task attach mode (remote)
- `rip --server <url> --task <id>`
- Attaches the fullscreen UI to an existing background task and streams task frames over SSE.

Task commands (remote)
- `rip tasks --server <url> spawn --tool bash --args '{"command":"<cmd>"}'` (default `--execution-mode pipes`)
- `rip tasks --server <url> spawn --tool bash --args '{"command":"<cmd>"}' --execution-mode pty` (requires server `RIP_TASKS_ALLOW_PTY=1`)
- `rip tasks --server <url> list`
- `rip tasks --server <url> status <task_id>`
- `rip tasks --server <url> cancel <task_id> --reason "<why>"`
- `rip tasks --server <url> stdin <task_id> --text "<line>"` (PTY only; sends `<line>\n`)
- `rip tasks --server <url> resize <task_id> --rows 24 --cols 80` (PTY only)
- `rip tasks --server <url> signal <task_id> SIGINT` (PTY only today)
- `rip tasks --server <url> output <task_id> --stream stdout --offset-bytes 0 --max-bytes 4096` (`--stream stderr|pty`)
- `rip tasks --server <url> events <task_id>` (prints JSON frames until terminal `tool_task_status`)
- `rip tasks --server <url> watch` (interactive list + tail + cancel; `--interval-ms` controls refresh; keys: `q`/`Esc`/`Ctrl+C` quit, `↑/↓` or `j/k` select, `c` cancel, `s` toggle stdout/stderr)

Thread commands (local or remote)
- `rip threads ensure` (ensure default continuity)
- `rip threads list` / `rip threads get <thread_id>`
- `rip threads post-message <thread_id> --content "<text>" [--actor-id <id>] [--origin <origin>]`
- `rip threads branch <parent_thread_id> [--title <title>] [--from-message-id <id>] [--from-seq <n>] [--actor-id <id>] [--origin <origin>]`
- `rip threads handoff <from_thread_id> [--title <title>] (--summary-markdown "<md>" | --summary-artifact-id <id>) [--from-message-id <id>] [--from-seq <n>] [--actor-id <id>] [--origin <origin>]`
- Add `--server <url>` after `threads` to target a remote server: `rip threads --server <url> ...`
- Note: when using `--summary-markdown`, RIP also writes an artifact-backed handoff bundle and records it in `continuity_handoff_created.summary_artifact_id` (`docs/03_contracts/handoff_context_bundle.md`).

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
- Remote: `rip run ... --server <url>` posts to the default thread (continuity) and streams the resulting session frames over HTTP/SSE.
- `rip serve` (or `ripd`) starts the agent server for remote clients (SDKs can target it via `--server <url>`).
- Multi-terminal posture: one store needs a single authority for truth writes (ADR-0019).
  - Today: use `rip serve` and point all terminals at `--server <url>` for the same store.
  - Planned: auto-start/auto-attach to a local authority so “one store just works” without manual `--server`.
- Default output: `rip run ...` uses `--view output` (human-readable). Use `--view raw` for newline-delimited JSON frames.
- Phase 1 is single-run sessions (no multi-turn/thread resume yet); OpenResponses tool execution is sequential and capped (`max_tool_calls=32`, `parallel_tool_calls=false`) per ADR-0005.
- Workspace-mutating operations are serialized across sessions and background tasks; read-only tools may run concurrently.
