# Task 02: Provider adapter (Open Responses boundary)

Goal
- Implement Open Responses adapter at provider boundary.

Inputs
- Module contract: docs/03_contracts/modules/phase-1/02_provider_adapters.md

Outputs
- Adapter that maps Open Responses SSE to internal event frames.

Acceptance criteria
- Pass Open Responses acceptance fixtures.
- Preserve event order and timing.

Tests
- Schema validation tests.
- Golden stream replay tests.

Benchmarks
- Parse overhead per SSE event.
- TTFT overhead.
