# Task 07: Agent server

Goal
- Expose agent sessions via HTTP/SSE.

Inputs
- Module contract: docs/03_contracts/modules/phase-1/06_server.md

Outputs
- Server binary that proxies to ripd.

Acceptance criteria
- Start session, send input, stream events, cancel.
- Ordered SSE stream.

Tests
- Session lifecycle integration tests.
- SSE compliance tests.
