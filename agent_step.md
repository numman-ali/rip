# Agent Step

Current focus
- Phase 1: shared session runner across server + CLI (frames are canonical).
- Continuity OS posture: continuity log is truth; provider state is cache (cursor rotation allowed); sessions are runs/turns (not user-facing by default).
- Default local execution: `rip` launches fullscreen TUI (in-process); `rip run` stays headless; `--server <url>` enables remote runs; `rip serve` stays the remote control plane.
- OpenResponses provider compatibility: stateless history mode + tool schema strict=false; fix provider_errors without dropping raw fidelity.
- Output view: human-friendly aggregation (no tool arg deltas), aligned with Codex exec expectations.
- Now: background tool tasks **as task entities** are implemented in `pipes` mode (spawn/status/stream/cancel + artifact-backed log tailing).
- Now: PTY mode + interactive control ops (stdin/resize/signal) are implemented (policy-gated); deterministic task replay fixtures added.
- Now: CLI task watch UI exists (`rip tasks --server <url> watch`) for list/select/tail/cancel (minimal keys; no PTY attach yet).
- Operator gate: capability delivery order is `cli_h(local)` → `tui` → `server` → `remote` → `sdk`.
- Terminology: see `docs/02_architecture/runtime_and_control_plane.md` (runtime vs control plane vs remote runtime).
- Next up: get `scripts/check` green again (llvm-cov thresholds), then continuities (threads): resume/branch + cursor rotation design.
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
- SDK TS checks (local): `scripts/check-sdk-ts`.
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
