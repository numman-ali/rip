# Contract: Hooks Engine (Phase 1)

Summary
- Provides deterministic lifecycle hooks for sessions (Phase 1 minimal).
- Hooks are opinionated defaults; plugin/skill registration is additive only.
- Hook execution must be fast, ordered, and side-effect safe.

Scope
- Core hook registry in runtime.
- Hook events: session start/end and output only.
- Hook outcomes: continue or abort with reason.

Interfaces
- Register hook: name + event + handler.
- Execute hooks for a given event with a context payload.
- Hook context includes session id, seq, timestamp, and optional output/tool payload.

Behavior
- Hooks execute in registration order.
- First abort stops hook chain and surfaces a single abort reason.
- Hook execution must be deterministic for a given event and context.

Non-goals (Phase 1)
- Tool, permission, and compaction hooks (Phase 2).
- Dynamic hook configuration UI.
- Remote hook execution.
- Hook result mutations beyond abort/continue.

Acceptance Tests
- Registers multiple hooks and preserves order.
- Abort hook stops subsequent hooks.
- Hook execution adds no more than 0.1ms p50 overhead per event on a sample stream.

Performance
- Target p50 hook dispatch under 0.1ms, p99 under 0.5ms per event.
