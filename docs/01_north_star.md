# North Star

Summary
- Build the fastest coding-agent harness possible.
- Modular, pluggable, and testable by default.
- Designed for autonomous agent development, not human collaboration.

Non-negotiables
- Extreme performance on streaming, parsing, scheduling, tool dispatch, repo ops.
- Modular boundaries with strict contracts and minimal coupling.
- Opinionated defaults aligned with the operator's workflow.
- Dynamic routing and hot-swappable capabilities.
- Programmatic SDK (not only CLI).

Performance budgets (phase 1)
- TTFT overhead (first byte received -> first useful output rendered).
- Event parse overhead per streaming event.
- Tool dispatch latency (call-ready -> process start).
- Patch apply throughput on real repos.
- End-to-end loop latency (plan -> tools -> patch -> verify -> done).

Product surfaces
- Interactive CLI (primary UX).
- Headless CLI (scriptable JSON output).
- Agent server (exposes agent sessions, not Open Responses API).
- SDK (TypeScript first; optional Python later).
- Rich TUI interactions (threads/palette/editor) and MCP surface are planned expansions (the base terminal UI ships as the default `rip` UX).

Design stance
- Open Responses used only for provider adapters (ingress/egress).
- Internal runtime uses compact frames for speed.
- Deterministic replay via event log + snapshots.

Continuity OS stance
- RIP is a **continuity OS**, not a chat app: the default UX is “one chat forever”.
- The continuity event log is the source of truth; provider conversation state is a cache (cursor rotation is allowed/expected).
- Sessions are compute jobs/runs (one run/turn) and are not user-visible by default.
