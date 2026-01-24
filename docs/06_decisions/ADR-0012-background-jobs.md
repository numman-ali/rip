# ADR-0012: Background jobs are continuity-truth events + artifacts (no hidden mutable state)

Status
- Accepted (2026-01-24)

Decision packet
- Decision: how to represent background “jobs” (summarizers/indexers/etc.) so they are replayable, deterministic, and surface-parity friendly without introducing hidden mutable state.
- Options:
  1) **Server-local scheduler state** (jobs exist only as in-memory/server DB records).
     - Pros: simplest to implement initially.
     - Cons: violates Continuity OS posture (hidden mutable state); hard to replay/audit; breaks cursor rotation/rebuild assumptions.
  2) **Standalone job entity streams** (new `{job_id}` streams + dedicated APIs).
     - Pros: rich progress streams; easy “watch job” semantics.
     - Cons: expands the control plane surface area; requires new streaming infra and parity work; easy to re-introduce hidden state if job inputs aren’t fully logged.
  3) **Continuity-truth job frames** (jobs are modeled as continuity events; outputs are artifact refs + other continuity frames).
     - Pros: fully replayable/auditable; no hidden state; streaming is already solved via `thread.stream_events`; matches “jobs over event streams”.
     - Cons: progress is co-mingled with other continuity frames; long-running jobs must keep progress events bounded.
- Recommendation: Option 3 (continuity-truth job frames), with a future path to Option 2 if/when we need richer job-centric streams (without changing determinism rules).
- Reversibility: job frames are versioned; we can later add a dedicated job stream/API that is a pure projection of the continuity job frames (no change to truth or replay semantics).

Context
- Continuity OS posture (ADR-0008): the continuity event log is truth (append-only + replayable).
- Background/subconscious agents are jobs over event streams; they must emit structured events + artifacts; no hidden mutable state (AGENTS.md).
- Compaction Auto v0.1 requires a background summarizer that:
  - selects deterministic cut points (ADR-0011),
  - produces immutable `rip.compaction_summary.v1` artifacts,
  - appends `continuity_compaction_checkpoint_created` with provenance,
  - remains safe under cache rotation and parallel job execution.

Decision

1) Jobs are represented in continuity truth
- Introduce continuity frames:
  - `continuity_job_spawned`
  - `continuity_job_ended`
- These frames live in the **continuity stream** (`stream_kind="continuity"`, `stream_id=thread_id`).
- Streaming:
  - Jobs are observed by subscribing to `thread.stream_events` (past + live). No separate “job stream” is required in v0.1.

2) Job identity and provenance are explicit
- Every job has a stable `job_id` (uuid).
- `job_kind` identifies the job implementation contract (example: `compaction_summarizer_v1`).
- Both `spawned` and `ended` frames carry provenance:
  - `actor_id`, `origin` (multi-actor/shared continuity readiness).

3) Job inputs that affect behavior must be logged
- Any decision that would change outputs must be present in continuity truth at the time the job is spawned.
  - Example (compaction summarizer): selected `{cut_rule_id, stride_messages, target_message_ordinal, to_seq, to_message_id, base_summary_artifact_id?}`.
- Jobs may read caches for performance, but correctness is defined by:
  - continuity truth,
  - explicitly referenced artifact ids,
  - and the job’s recorded inputs + `job_kind`.

4) Job outputs are artifacts + continuity frames
- Jobs may write artifacts (immutable blobs) and then append continuity frames referencing them.
  - Example (compaction summarizer):
    - write `rip.compaction_summary.v1` artifact
    - append `continuity_compaction_checkpoint_created` referencing the artifact id
- `continuity_job_ended` includes the outcome:
  - `status`: `completed | failed | cancelled` (cancelled is future; v0.1 may only emit completed/failed)
  - `result`: job-specific, but must only include stable ids/refs (checkpoint ids, artifact ids, cut points)
  - `error`: string (when failed)

5) Concurrency and failure semantics are replay-safe
- Concurrency:
  - Multiple jobs may run concurrently for the same continuity and even the same cut point.
  - Duplicated outputs are allowed; selection/override rules are deterministic from the stream (e.g., “latest checkpoint wins” per ADR-0011).
- Failures:
  - A failed job must still append `continuity_job_ended(status=failed, error=...)`.
  - Retries are explicit: re-invoke the capability to spawn a new job (new `job_id`).

Non-goals (v0.1)
- A general-purpose job scheduler/daemon with persistence independent of the continuity log.
- Hidden auto-wakeup loops (any orchestration must be explicit and logged).
- Multi-host/distributed execution (future; requires an explicit logged RPC boundary).

References
- `AGENTS.md`
- `docs/06_decisions/ADR-0008-continuity-os.md`
- `docs/06_decisions/ADR-0010-context-compiler-truth.md`
- `docs/06_decisions/ADR-0011-compaction-cutpoints-summaries.md`
- `docs/03_contracts/event_frames.md`
