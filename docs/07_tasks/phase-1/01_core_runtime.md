# Task 01: Core runtime (ripd)

Goal
- Implement the agent runtime event loop and scheduler.

Inputs
- Module contract: docs/03_contracts/modules/phase-1/01_ripd_core.md

Outputs
- Core runtime crate with event routing and session lifecycle.

Acceptance criteria
- Can start a session, stream events, and terminate cleanly.
- Deterministic replay of a golden event stream.

Tests
- Session lifecycle integration test.
- Golden stream replay -> snapshot equivalence.

Benchmarks
- Event routing latency per event.
- Sub-agent spawn latency.
