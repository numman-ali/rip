# Task 12: Checkpointing

Goal
- Implement workspace checkpoints and rewind.

Inputs
- Module contract: docs/03_contracts/modules/phase-1/10_checkpointing.md

Outputs
- Checkpoint store and rewind API in workspace engine.

Acceptance criteria
- Checkpoint -> edit -> rewind restores files.
- Checkpoint list is stable and deterministic.

Tests
- Integration tests around file edits + rewind.
