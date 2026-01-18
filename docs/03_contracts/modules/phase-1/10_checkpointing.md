# Contract: Checkpointing (Phase 1)

Summary
- Provides workspace checkpoints for safe undo/rewind.
- Checkpoints are deterministic and linked to session events.
- Rewind restores file state and event metadata.

Scope
- Automatic checkpoint creation for file-edit tools.
- Manual checkpoint creation via command.
- Rewind to a checkpoint by id.

Interfaces
- create_checkpoint(session_id, label, metadata)
- list_checkpoints(session_id)
- rewind_to_checkpoint(session_id, checkpoint_id)

Behavior
- Checkpoints persist across sessions in the same workspace.
- Each checkpoint records files touched and a snapshot hash.
- Rewind must be atomic; partial rewinds are invalid.
- Emits checkpoint events (`checkpoint_created`, `checkpoint_rewound`, `checkpoint_failed`) on the session event stream.

Non-goals (Phase 1)
- Cross-repo checkpoints.
- Remote sync of checkpoints.
- Compression/dedup tuning.

Acceptance Tests
- Create checkpoint, apply edit, rewind restores files.
- Checkpoint list is ordered and stable.
- Rewind to invalid id fails with error.

Performance
- Checkpoint creation p50 under 10ms for <100 files.
