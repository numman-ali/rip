# Capability Matrix (Phases & Hook Points)

Summary
- Maps capability groups to delivery phases and primary hook points.
- Keeps the core small while proving extensibility early.

Phase 1 — Foundation
- Sessions & threads: session manager + event log + server API + CLI/TUI; hook: session lifecycle events.
- Session storage & replay: append-only log + snapshot reader; hook: replay reader for deterministic tests.
- Configuration & policy: layered config loader + permission engine; hook: policy evaluation in tool runtime.
- Execution modes: interactive CLI, headless CLI, RPC, JSONL output + JSON Schema; hook: output renderer + RPC mux.
- Tools & tooling: tool registry, allowlist, sandbox policy; hook: tool dispatch pipeline.
- Hooks (minimal): session-level hook engine; hook: session lifecycle events.
- Commands (core): in-memory command registry; hook: CLI/router integration.
- Checkpointing & rewind: workspace snapshots + patch/rollback; hook: workspace engine.
- Security & safety: baseline sandbox + secret redaction; hook: output sanitizer.
- Programmatic SDK: session start/resume + event streaming; hook: server API.

Phase 2 — Expansion
- Extensions & hooks: tool/permission/compaction hooks + event bus + extension registry; hook: pre/post tool, permission, session, compaction.
- Skills: SKILL discovery + on-demand loader + skill-scoped hooks; hook: context compiler.
- Subagents: subagent manager + budgets + tool caps; hook: task scheduler.
- Output styles: style registry + command selector; hook: response renderer.
- Commands & automation: disk-based command loaders + hookable commands; hook: CLI command router.
- Integrations: MCP client/server, remote tools, IDE adapters, LSP; hook: tool runtime + server API.
- Context compiler: deterministic packing + summaries; hook: prompt assembly pipeline.

Phase 3 — Advanced
- Search/index & memory: local index + retrieval + provenance; hook: context compiler.
- Background workers: sync, analytics, cache warmers; hook: scheduler.
- Policy/steering: adaptive budgets + rule engine; hook: runtime policy controller.
- Enterprise config: managed scope policies + audit log; hook: config loader + log.
- Extended sandboxing: per-tool isolation and resource limits; hook: tool runner.
