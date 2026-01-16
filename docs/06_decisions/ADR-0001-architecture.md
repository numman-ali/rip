# ADR-0001: Core architecture selection

Status
- Accepted

Context
- Need fastest possible agent runtime with modular, pluggable components.
- Server must expose the agent, not an Open Responses API.
- Open Responses is needed only for provider adapters.

Decision
- Rust core runtime (ripd) for hot path.
- Internal compact frames for speed; JSON only at edges.
- Provider adapters translate Open Responses at the boundary.
- Plugins default to WASM; hot path modules may be native in-process.
- Heavy modules may run out-of-process.

Consequences
- Maximum performance on the hot path.
- Strong modularity with safe plugin boundaries.
- Clear separation of agent server vs provider protocol.
