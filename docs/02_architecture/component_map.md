# Component Map

Summary
- One core runtime (ripd) powers CLI, headless CLI, TUI, and server API.
- The server exposes the coding agent (session API), not Open Responses.
- Open Responses is only the provider adapter layer.

System map

[ rip-cli ] (interactive) ----\
[ rip-cli --headless ] --------+--> [ ripd (agent runtime + server API) ] --> [ provider adapters ] --> [ model providers ]
[ rip-tui ] (Phase 2) --------/          |
[ rip-mcp ] (Phase 2) <---MCP-/          |
                                            |--> scheduler + subagent manager
                                            |--> tool runtime + registry
                                            |--> context compiler
                                            |--> policy/steering
                                            |--> workspace engine
                                            |--> search/index (phase 2)
                                            |--> memory store (phase 2)
                                            |--> sync/replication (phase 2)
                                            |--> background workers
                                            |
                                            +--> event log + snapshots

Responsibilities
- ripd: agent loop, routing, scheduling, tool dispatch, logging, replay.
- rip-cli: interactive streaming renderer (no diffs/approvals in Phase 1).
- rip-cli --headless: machine-friendly JSON output.
- ripd server API: session HTTP/SSE + OpenAPI spec.
- rip-tui: rich terminal UI rendering (Phase 2).
- rip-mcp: MCP surface for capability exposure (Phase 2).
- provider adapters: Open Responses ingress/egress to model providers.
- background workers: indexing, summarization, sync, prefetch.

Key constraints
- All inter-module traffic is structured events, not raw text.
- Every module is replaceable via strict contracts.
- Determinism: event log + snapshots enable full replay.
