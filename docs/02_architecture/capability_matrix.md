# Capability Matrix (Phases & Hook Points)

Summary
- Maps capability groups to delivery phases and primary hook points.
- Keeps the core small while proving extensibility early.

Phase 1 — Foundation
- Sessions & threads: session manager + event log + server API + CLI (interactive + headless); hook: session lifecycle events.
- Session storage & replay: append-only log + snapshot reader; hook: replay reader for deterministic tests.
- Execution modes: interactive CLI, headless CLI, JSONL output + JSON Schema; hook: output renderer.
- Models & providers: Open Responses adapter boundary + full schema/streaming fidelity + model selection stub; hook: provider adapter boundary.
- OpenResponses boundary: request/response/streaming fidelity + item lifecycle + tool/content unions; hook: provider adapter boundary.
- Tools & tooling: tool registry, allowlist, sandbox policy; hook: tool dispatch pipeline.
- Hooks (minimal): session-level hook engine; hook: session lifecycle events.
- Commands (core): in-memory command registry; hook: CLI/router integration.
- Checkpointing & rewind: workspace snapshots + patch/rollback; hook: workspace engine.
- Security & safety: baseline sandbox + secret redaction; hook: output sanitizer.

Phase 2 — Expansion
- Configuration & policy: layered config loader + permission engine; hook: policy evaluation in tool runtime.
- Extensions & hooks: tool/permission/compaction hooks + event bus + extension registry; hook: pre/post tool, permission, session, compaction.
- Skills: SKILL discovery + on-demand loader + skill-scoped hooks; hook: context compiler.
- Subagents: subagent manager + budgets + tool caps; hook: task scheduler.
- Tools & tooling: background tool tasks + artifact-backed outputs + cancellation; hook: tool runner + task scheduler + event log.
- Output styles: style registry + command selector; hook: response renderer.
- UI/interaction: interactive affordances (palette, editor, shortcuts); hook: CLI/TUI UI layer.
- TUI surface: `rip-tui` (rich rendering over the same capabilities).
- Programmatic SDK: session start/resume + event streaming; hook: canonical event frames (local JSONL via `rip` + optional server API).
- Execution modes: RPC mode + streaming JSON input + structured output; hook: RPC mux.
- Models & providers expansion: multi-provider routing + model registry; hook: provider adapter boundary.
- OpenResponses extensions: vendor-prefixed items/events + schema extensions; hook: provider adapter boundary.
- Commands & automation: disk-based command loaders + hookable commands; hook: CLI command router.
- Integrations: MCP client/server, remote tools, IDE adapters, LSP; hook: tool runtime + server API.
- Context & guidance: project guidance loader + prompt scoping; hook: context compiler.
- Compaction & summarization: auto/manual compaction + branch summaries; hook: context compiler + event log.
- Context compiler: deterministic packing + summaries; hook: prompt assembly pipeline.

Phase 3 — Advanced
- Search/index & memory: local index + retrieval + provenance; hook: context compiler.
- Background workers: sync, analytics, cache warmers; hook: scheduler.
- Policy/steering: adaptive budgets + rule engine; hook: runtime policy controller.
- Enterprise config: managed scope policies + audit log; hook: config loader + log.
- Extended sandboxing: per-tool isolation and resource limits; hook: tool runner.
