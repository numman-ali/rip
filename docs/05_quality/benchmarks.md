# Benchmarks

Summary
- Benchmarks are CI gates; regressions fail PRs.

Phase 1 benchmarks
- Event parse overhead per SSE event (`sse_parse_us_per_event`).
- TTFT overhead (`ttft_overhead_us`): first provider byte received -> first internal frame emitted.
- Tool dispatch latency (`tool_runner_noop_us`).
- Patch apply throughput (`workspace_apply_patch_us`).
- End-to-end loop latency (`e2e_loop_us`): parse deterministic tool-call SSE -> run `apply_patch` -> build follow-up request -> parse follow-up SSE -> write snapshot.

Fixture requirements
- Small repo (fast CI).
- Medium repo (realistic).
- Deterministic prompts and tool outputs.

Harness (Phase 1)
- Budgets: `docs/05_quality/benchmarks_budgets.json`
- Runner: `scripts/bench` (release mode; fails if budgets are exceeded)
- Fixture root: `fixtures/`
