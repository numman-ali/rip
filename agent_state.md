# Agent State (Working Log)

Last updated: 2026-04-19

How to use
- Update this file whenever focus shifts, before ending a work session, and when blocked.
- Keep it short and decision-focused; link to docs/roadmap/ADRs instead of duplicating.

Current focus
- Continuity OS posture is locked: continuity log is truth; provider state is cache; sessions are runs/turns (not user-facing by default).
- Now: Continuities (threads) are the primary user-facing entity ("one chat forever") and must be implemented end-to-end across surfaces.
- Phase 1 baseline remains: shared session runner across server + CLI (frames are canonical).
- Default local execution: `rip` (TUI), `rip run`, `rip threads`, and `rip tasks` auto-start/auto-attach to a local authority for the store; `--server <url>` preserves explicit remote runs; `rip serve` stays the control plane.
- Decision locked (ADR-0019): one store requires a single authority for truth writes; indexes are rebuildable caches; hybrid retrieval is a compiler stage and must be truth-logged by reference.
- TUI design language revamp landed (Phases A + B + C + D shipped) and the polish follow-up slice is now in too. Canonical execution plan at `docs/07_tasks/tui_revamp.md`; shipped design at `docs/02_architecture/tui/00_design.md`. Canvas walks structured `CanvasMessage`s, ambient state persists across turns, palette exposes Command/GoTo/Threads/Models/Options with Tab cycling, `?` opens the Help overlay, Ctrl-R opens X-ray on focused item, and the ErrorRecovery overlay auto-opens on provider errors routing `r/c/m/x/⎋` through supported capabilities (`thread.post_message`, `thread.provider_cursor.rotate`, Models palette, `ErrorDetail`). The follow-up slice swapped the input editor to `ratatui-textarea`, added anchored palette origins, shipped the idle/thinking/streaming motion primitives, introduced a richer ThreadPicker overlay for next-run continuity targeting, gave subagent/reviewer/extension messages distinct gutter accents, added hero/activity/canvas mouse polish, and landed D.5 vim input mode (Normal/Insert state machine over `ratatui-textarea` with `i/a/I/A/o/O`, `h j k l / w b e / 0 $`, `x / dd / yy / p / u / G / gg`, and two-key operators via `vim_pending`). Workspace coverage gate is back at ≥ 90/90/90 lines/functions/regions. Remaining polish items (B.9 90-snapshot matrix, D.1 Ink QA, D.2 ArtifactViewer, D.6 virtualization) stay tracked as "TUI: polish tranche" in the roadmap.
- Engineering hygiene (2026-04-19): large-file refactor program is underway with a shared convention now codified in `AGENTS.md`: keep production Rust modules around the 600-700 line target, and when inline `mod tests` is what pushes a file over, move those tests to sibling module files before extracting more production modules. Kernel/CLI ownership is split from TUI ownership; `crates/ripd/src/continuities.rs` has been reduced to a thin module root with focused submodules under `crates/ripd/src/continuities/`, `crates/ripd/src/server_tests.rs` is now a 453-line harness with concern modules under `crates/ripd/src/server_tests/`, `crates/ripd/src/session.rs` is now a 369-line root with focused OpenResponses/context submodules under `crates/ripd/src/session/` (`context_compile.rs`, `openresponses.rs`, `streaming.rs`, plus `tests.rs`), `crates/ripd/src/continuity_seek_index.rs` is now a 631-line root with its sidecar builder and module-private tests moved under `crates/ripd/src/continuity_seek_index/`, and `crates/ripd/src/runner.rs` is now a 188-line production module with its runner tests moved under `crates/ripd/src/runner/tests.rs`. On the CLI side, `crates/rip-cli/src/tasks_watch.rs` is now a 611-line production file with its module-private watcher tests moved to `crates/rip-cli/src/tasks_watch/tests.rs`. The fullscreen TUI driver is now under the target: `crates/rip-cli/src/fullscreen.rs` is 602 lines (SSE run loop + remote bootstrap + init + model-catalog loader + focused_detail_overlay), with focused submodules under `crates/rip-cli/src/fullscreen/` — `actions.rs` (tokio-spawned status-producing capability calls for compaction/cursor/context/error-recovery), `events/` (sub-split into `mouse`/`keyboard`/`vim` plus a shared `UiAction` + `handle_term_event` router in `events/mod.rs`), `palette.rs` (mode openers + apply dispatch + command-action routing), `keymap.rs`, `thread_picker.rs`, `copy.rs`, `theme.rs`, `terminal.rs`, and `tests.rs`. Canvas ingest is sub-split into `canvas/ingest/{mod,turns,cards,notices}.rs`. Remaining non-TUI refactor targets are `rip-cli/src/main.rs`, `rip-cli/src/threads.rs`, `continuity_stream_cache.rs`, and `server.rs`.
- Docs clarified (2026-04-18): future cognition features should layer as core capabilities for durable truth/control, background jobs + compiler stages for memory/retrieval/subagent outputs, policy profiles for autonomy levels, and extensions/skills for modular strategies and workflows.
- Implemented (2026-04-18): TUI Confidence v1 initial pass. Fullscreen TUI no longer resets the visible transcript on each submit; it preserves the conversational Canvas across turns, shows an optimistic pending-turn state before the network round-trip completes, surfaces OpenResponses request timings in the status bar, supports both PageUp/PageDown and mouse-wheel Canvas scrollback, gives user prompts a subtle visual distinction from agent output, exposes a visible key-hint line in the input chrome, reports recent tool usage/completion instead of only currently-running work, and shows a concrete continuity/session breadcrumb in provider-error drill-downs so failures can be inspected via thread event logs.
- Implemented (2026-04-18): fullscreen TUI palette foundation v0.1. `Ctrl-K` now opens a generic overlay state model designed for command/navigation/model/session/options modes, with model selection shipped first. The model picker is config-backed for now, supports typed `provider/model_id` fallback routes, persists the selected OpenResponses endpoint/model as the next-turn preference inside the TUI session, and applies selection by mutating per-turn OpenResponses overrides rather than rewriting global config.
- Config UX is still partial: layered config + doctor exist, but mutating config commands (`init`/`set`/`get`) are not shipped yet.
- OpenResponses provider compatibility: stateless history mode + tool schema strict=false; fix provider_errors without dropping raw fidelity.
- Output view: human-friendly aggregation (no tool arg deltas), aligned with Codex exec expectations.
- Background tool tasks are implemented (`pipes` + policy-gated `pty`) with deterministic replay fixtures.
- Operator gate: capability delivery order is `cli_h(local)` -> `tui` -> `server` -> `remote` -> `sdk`.
- Terminology: see `docs/02_architecture/runtime_and_control_plane.md` (runtime vs control plane vs remote runtime).
- Implemented: stream-aware frame envelope on the wire (`stream_kind`, `stream_id`) + per-stream replay validation.
- Implemented: continuity store (`ensure_default`, `append_message`, `append_run_spawned`, `append_run_ended`, `branch`, `handoff`) + local `rip run` posts to the default continuity before spawning a run.
- Implemented: continuity run lifecycle frames carry provenance (`continuity_run_spawned.actor_id/origin`) and completion (`continuity_run_ended.reason`).
- Implemented: context compiler kernel v1 (`recent_messages_v1`) writes `rip.context_bundle.v1` artifacts + emits `continuity_context_compiled`; OpenResponses runs start from compiled bundles (fresh provider conversation per run).
- Implemented: compaction foundations v0.1: `continuity_compaction_checkpoint_created` + `rip.compaction_summary.v1` artifacts + compiler strategy `summaries_recent_messages_v1` (summary_ref + recent raw messages; fallback-safe; cache-backed O(k) when sidecars exist).
- Implemented: `compaction.manual` surface parity (cli_h/server/sdk) via `rip threads compaction-checkpoint` (local + `--server`) and `POST /threads/{id}/compaction-checkpoint`.
- Implemented: compaction auto v0.1: deterministic `compaction.cut_points` (message-stride, cache-backed with truth fallbacks) + `compaction.auto` summarizer jobs emitting `continuity_job_spawned`/`continuity_job_ended` (ADR-0012) and deterministic checkpoint frames + summary artifacts; exposed across cli_h/tui/server/sdk.
- Implemented: compaction auto v0.2 scheduling: `compaction.auto.schedule` emits `continuity_compaction_auto_schedule_decided` (policy + reasons) and triggers compaction work off the hot path; exposed across cli_h/tui/server/sdk.
- Implemented: auto summaries v0.2 (ADR-0014): compaction summaries are real cumulative markdown derived from messages (base summary chaining + bounded delta highlights), still immutable artifacts referenced from truth frames.
- Implemented: `compaction.status` surface UX + capability across cli_h/tui/server/sdk (`rip threads compaction-status`, `POST /threads/{id}/compaction-status`, TUI keybinding).
- Implemented: branch/handoff posture is “link-only” in the continuity log (no history copying) (ADR-0009) + relationship frames (`continuity_branched`, `continuity_handoff_created`).
- Implemented: handoff writes an artifact-backed context bundle referenced by `continuity_handoff_created.summary_artifact_id` (`docs/03_contracts/handoff_context_bundle.md`).
- Implemented: server exposes `thread.*` (ensure/list/get/post_message/branch/handoff/stream_events) and OpenAPI is updated.
- Implemented: `rip threads ...` CLI adapter + TypeScript SDK `thread.*` wrappers (ensure/list/get/post_message/branch/handoff/stream_events) while keeping ADR-0006 transport (SDK spawns `rip`; no TS HTTP/SSE client).
- Implemented: TypeScript SDK task APIs are local-first on exec transport (no required `server`) with e2e spawn/output/status/cancel coverage + authority cleanup.
- Implemented: workspace mutation serialization across sessions + background tasks (workspace lock) with contract + replay tests.
- Implemented: continuity stream logs workspace-mutating tool side-effects with full provenance (`continuity_tool_side_effects`) and replay coverage under parallel runs/tasks.
- Implemented: local authority “one store just works” v0.1 (ADR-0019): per-store authority discovery + store lock; local `rip`/`rip run`/`rip threads`/`rip tasks` auto-start/auto-attach by default; deterministic multi-client integration coverage.
- Implemented: local authority v0.3 lifecycle hardening: stale-lock recovery for both clients and `rip serve` startup (no-client recovery), graceful SIGTERM/SIGINT shutdown (best-effort lock/meta cleanup), explicit “locked/unavailable” UX (deterministic backoff + clear errors), and deterministic crash+restart integration coverage (seq contiguity + workspace mutation ordering).
- Implemented: TS SDK taskEventsStreamed local exec e2e coverage (spawn → stream until terminal `tool_task_status`).
- Implemented: TS SDK taskEventsStreamed HTTP transport parity (server required + e2e streaming; client terminates on terminal `tool_task_status` because server SSE is open-ended/keep-alive).
- Implemented: TS SDK `run()` HTTP transport real-server e2e coverage (spawn `rip serve`; session SSE terminates on `session_ended`).
- Implemented: TS SDK `threadEventsStreamed()` HTTP transport real-server e2e coverage (spawn `rip serve`; client terminates via `maxEvents`; asserts key continuity frames).
- Implemented: TS SDK HTTP transport real-server e2e coverage for `threadBranch()`/`threadHandoff()` + branch/handoff stream frame assertions.
- Implemented: TS SDK HTTP transport real-server e2e coverage for thread.* JSON endpoints (list/get/context-selection/provider-cursor/compaction including auto + schedule via dry_run/no_execute).
- Implemented: TS SDK HTTP transport real-server e2e coverage for task.* JSON endpoints (spawn/list/status/output/cancel + PTY stdin/resize/signal behind `RIP_TASKS_ALLOW_PTY=1`).
- Decision locked (ADR-0010): `context.compile` is the canonical way runs “remember” across time; provider cursors are optional caches only.
- Implemented: provider cursor cache truth logging (ADR-0015): `continuity_provider_cursor_updated` + `thread.provider_cursor.{status,rotate}` across cli_h/tui/server/sdk; OpenResponses runs record `previous_response_id` on completion as a rebuildable cache.
- Implemented: context selection strategy evolution truth logging v0.1 (ADR-0016): `continuity_context_selection_decided` + `thread.context_selection.status` across cli_h/tui/server/sdk; compiler selection/budgets/inputs/reasons are now auditable truth.
- Implemented: context compiler perf v1.1: per-continuity sidecar seek indexes + bounded window reads for `recent_messages_v1` non-tail anchors (caches only; replay-safe fallbacks).
- Implemented: context compiler perf v1.2: messages+runs-only continuity sidecar + indexes; `recent_messages_v1` window reads are O(k) even with dense `continuity_tool_side_effects` between messages (caches only; replay-safe fallbacks).
- Drafted contracts (docs-first):
  - New continuity frame: `continuity_context_compiled` (`docs/03_contracts/event_frames.md`)
  - New artifact schema: `rip.context_bundle.v1` (`docs/03_contracts/context_bundle.md`)
  - Context compiler module contract: `docs/03_contracts/modules/phase-2/05_context_compiler.md`
- Keep CI/bench gates green; ratchet budgets only with replay coverage.

Reorientation (read in order after compaction)
- `AGENTS.md`
- `agent_state.md`
- `docs/00_index.md`
- `docs/00_doc_map.md`
- `docs/01_north_star.md`
- `docs/02_architecture/continuity_os.md`
- `docs/07_tasks/roadmap.md`
- `docs/03_contracts/openresponses_coverage.md`
- `docs/03_contracts/openresponses_capability_map.md`
- `docs/07_tasks/openresponses_coverage.md`

Open risks / notes
- Tests no longer write `./data` under the repo (ripd export test uses temp dirs).
- Note: local runs still default to `./data` unless `RIP_DATA_DIR` is set.
- Quality gate: `scripts/check` is green again after the config/coverage cleanup pass and the TUI confidence follow-up. Latest local workspace coverage gate is above the floor at 91.66% lines / 90.67% regions / 90.02% functions, with focused coverage in `ripd`, `rip-cli`, `rip-tui`, `rip-tools`, and `rip-kernel`, including local-first CLI integration tests, TUI state/render coverage, and direct/source-level probes for stubborn helper paths.
- Perf: context compiler hot path avoids global `events.jsonl` scans when caches exist (snapshot-first session aggregation + per-continuity sidecar replay); avoids full continuity stream loads for `recent_messages_v1` both for latest-message run starts (tail-read continuity v1), non-tail anchors (seekable window reads v1.1), high tool-event density between messages (messages+runs sidecar v1.2), and long-thread compaction selection (compaction checkpoint index sidecar v1.3) with hierarchical summary refs.
- Perf: prompt cache friendliness requires deterministic tool ordering + stable instruction blocks + append-only context changes within a run (`docs/03_contracts/modules/phase-1/02_provider_adapters.md`, `https://openai.com/index/unrolling-the-codex-agent-loop/`).
- Determinism: task output pumps retry EINTR (pipes + pty), fixing rare missing stderr in `tasks::tests::run_task_writes_stdout_and_stderr_logs`.
- Docs: clarified surface parity “active surfaces” semantics and aligned TUI capability statuses/gaps with current shipped fullscreen UX.
- OpenResponses: `tool_choice`/`allowed_tools` are now enforced for function tool execution (disallowed calls emit tool failure + `function_call_output.ok=false`).
- TUI: Canvas-first + X-ray posture is now gated by deterministic ratatui golden snapshots for the 3 v0 journeys (`crates/rip-tui/tests/snapshots/`); `scripts/check-fast` enforces them.
- TUI: Confidence is better, and the palette foundation now exists, but the workspace feel is still not done. Remaining gaps are broader palette modes (commands/navigation/sessions/options), inline tool-call visibility in the Canvas, smoother streaming polish, provider-debug clarity, and a more intentional visual system than the current minimal terminal chrome.
- Config: layered JSON/JSONC config foundations shipped (deep-merge) + `config.doctor` diagnostics to remove provider/model ambiguity (`docs/03_contracts/config.md`).
- Config: provider-scoped OpenResponses defaults now overlay global defaults; `config.doctor` reports effective route + per-field provenance; `rip run --server` forwards per-run OpenResponses overrides the same way local runs do.

Active priorities
- Keep roadmap Now/Next aligned with the implementation work.
- SDK (TS): opt-in direct HTTP/SSE transport shipped (ADR-0017); bundling binaries remains deferred/roadmapped.
- Keep OpenResponses boundary full-fidelity while wiring new surfaces/adapters.
- Keep OpenResponses follow-ups spec-canonical; any compatibility user message is opt-in.
- Keep stateless history compatibility opt-in; default remains `previous_response_id`.
- Prefer deterministic, replayable fixtures over ad-hoc behavior changes.
- Validate OpenAI + OpenRouter runs side-by-side; remove provider_errors in normal operation.

Next checkpoints
- CI runs `scripts/check-fast` on push/PR.
- SDK TS checks (local): `scripts/check-sdk-ts`.
- Bench harness includes TTFT + end-to-end loop and is CI-gated (`scripts/bench`).
- `rip run <prompt>` works without a manually started `rip serve` process (auto-start/auto-attach local authority).
- `rip run <prompt> --server <url>` targets a remote server and streams identical event frames.
- `rip serve` exposes the session API for remote clients; SDK can target it via `--server` (but defaults to local `rip` exec).
- Manual smoke: `cargo test -p ripd live_openresponses_smoke -- --ignored` observes real provider SSE + at least one tool call.
- Manual provider smoke (Option 1 flags, local CLI).
- OpenAI: `RIP_DATA_DIR="$(mktemp -d)" RIP_WORKSPACE_ROOT="$PWD/fixtures/repo_small" OPENAI_API_KEY=... cargo run -p rip-cli -- run "List what's in this directory. Use the ls tool, then answer with just the filenames." --provider openai --model gpt-5-nano-2025-08-07 --view output`
- OpenRouter: `RIP_DATA_DIR="$(mktemp -d)" RIP_WORKSPACE_ROOT="$PWD/fixtures/repo_small" OPENROUTER_API_KEY=... cargo run -p rip-cli -- run "List what's in this directory. Use the ls tool, then answer with just the filenames." --provider openrouter --model openai/gpt-oss-20b --stateless-history --view output`
- Parallel tool calls (request-only): append `--parallel-tool-calls` to the command above (execution remains sequential in Phase 1).
- Live API sweep script (real APIs): `scripts/live-openresponses-sweep` (supports `--provider` and `--skip-parallel-case`)
