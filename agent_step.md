# Agent Step

Current focus
- Phase 1: shared session runner across server + CLI (frames are canonical).
- Default local execution: `rip run` in-process; `--server <url>` for remote; `rip serve` stays the remote control plane for remote clients.
- OpenResponses provider compatibility: stateless history mode + tool schema strict=false; fix provider_errors without dropping raw fidelity.
- Output view: human-friendly aggregation (no tool arg deltas), aligned with Codex exec expectations.
- Next up: TypeScript SDK baseline (wrap `rip` + JSONL frames; optional remote via `--server`).
- Keep CI/bench gates green; ratchet budgets only with replay coverage.

Reorientation (read in order after compaction)
- `AGENTS.md`
- `agent_step.md`
- `docs/00_index.md`
- `docs/00_doc_map.md`
- `docs/01_north_star.md`
- `docs/07_tasks/roadmap.md`
- `docs/03_contracts/openresponses_coverage.md`
- `docs/03_contracts/openresponses_capability_map.md`
- `docs/07_tasks/openresponses_coverage.md`

Active priorities
- Keep roadmap Now/Next aligned with the implementation work.
- Keep OpenResponses boundary full-fidelity while wiring new surfaces/adapters.
- Keep OpenResponses follow-ups spec-canonical; any compatibility user message is opt-in.
- Keep stateless history compatibility opt-in; default remains `previous_response_id`.
- Prefer deterministic, replayable fixtures over ad-hoc behavior changes.
- Validate OpenAI + OpenRouter runs side-by-side; remove provider_errors in normal operation.

Next checkpoints
- CI runs `scripts/check-fast` on push/PR.
- Bench harness includes TTFT + end-to-end loop and is CI-gated (`scripts/bench`).
- `rip run <prompt>` works without a separate `ripd` process (in-process session engine).
- `rip run <prompt> --server <url>` targets a remote server and streams identical event frames.
- `rip serve` exposes the session API for remote clients; SDK can target it via `--server` (but defaults to local `rip` exec).
- Manual smoke: `cargo test -p ripd live_openresponses_smoke -- --ignored` observes real provider SSE + at least one tool call.
- Manual provider smoke (Option 1 flags, local CLI).
- OpenAI: `RIP_DATA_DIR="$(mktemp -d)" RIP_WORKSPACE_ROOT="$PWD/fixtures/repo_small" OPENAI_API_KEY=... cargo run -p rip-cli -- run "List what's in this directory. Use the ls tool, then answer with just the filenames." --provider openai --model gpt-5-nano-2025-08-07 --view output`
- OpenRouter: `RIP_DATA_DIR="$(mktemp -d)" RIP_WORKSPACE_ROOT="$PWD/fixtures/repo_small" OPENROUTER_API_KEY=... cargo run -p rip-cli -- run "List what's in this directory. Use the ls tool, then answer with just the filenames." --provider openrouter --model mistralai/devstral-2512:free --stateless-history --view output`
- Parallel tool calls (request-only): append `--parallel-tool-calls` to the command above (execution remains sequential in Phase 1).
- Live API sweep script (real APIs): `scripts/live-openresponses-sweep` (supports `--provider` and `--skip-parallel-case`)
