# Component Map

Summary
- One core runtime (ripd) powers CLI, headless CLI, and server.
- The server exposes the coding agent (session API), not Open Responses.
- Open Responses is only the provider adapter layer.

System map

[ rip-cli ] (interactive) ----\
[ rip-cli --headless ] --------+--> [ ripd (agent runtime) ] --> [ provider adapters ] --> [ model providers ]
[ rip-server ] <---HTTP/SSE----/          |
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
- rip-cli: interactive UI for streaming, diffs, approvals.
- rip-cli --headless: machine-friendly JSON output.
- rip-server: agent session API for remote control.
- provider adapters: Open Responses ingress/egress to model providers.
- background workers: indexing, summarization, sync, prefetch.

Key constraints
- All inter-module traffic is structured events, not raw text.
- Every module is replaceable via strict contracts.
- Determinism: event log + snapshots enable full replay.
