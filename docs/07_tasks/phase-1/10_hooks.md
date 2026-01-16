# Task 10: Hooks engine

Goal
- Implement the phase-1 session hook engine in the runtime.

Inputs
- Module contract: docs/03_contracts/modules/phase-1/08_hooks.md

Outputs
- Hook registry and hook dispatch in rip-kernel.

Acceptance criteria
- Session hooks register, execute in order, and can abort.
- Hook dispatch adds negligible overhead to event emission.

Tests
- Unit tests for registration order and abort handling.
