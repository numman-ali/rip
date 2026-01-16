# Contract: tool runtime

Summary
- Executes tools with resource limits and streaming outputs.

Inputs
- Tool invocation events (name, args, budget).

Outputs
- Tool output events (stdout, stderr, exit code, artifacts).

Config
- Sandboxing mode and resource limits.
- Concurrency limits.

Invariants
- Tool outputs are streamed as structured events.
- Timeouts are enforced deterministically.

Tests
- Tool invocation fixture with deterministic outputs.
- Timeout and cancellation behavior.

Benchmarks
- Tool dispatch latency (call-ready -> process start).
