# ADR-0013: Compaction Auto scheduling is a policy decision logged in continuity truth

Status
- Accepted (2026-01-24)

Decision packet
- Decision: how to determine **when/why** to run `compaction.auto` without introducing hidden mutable state or slowing `thread.post_message`.
- Options:
  1) **Run `compaction.auto` synchronously inside `thread.post_message`.**
     - Pros: simplest wiring; “automatic” by default.
     - Cons: violates hot-path latency; couples foreground UX to background work; hard to tune without accidental regressions.
  2) **Run `compaction.auto` asynchronously after `thread.post_message` (fire-and-forget).**
     - Pros: avoids blocking the request; easy to add.
     - Cons: scheduling decisions become implicit/hidden; concurrency/duplication is harder to audit; still adds per-message orchestration overhead.
  3) **Introduce an explicit scheduling capability with a logged decision frame.**
     - Pros: scheduling is a first-class, replayable decision; orchestration can be moved off the hot path; supports external/background workers; preserves auditability and surface parity.
     - Cons: adds a new capability + frame type; requires explicit orchestration (server/worker/cron) to call the scheduler.
- Recommendation: Option 3.
- Reversibility: the scheduler capability and decision frame are versioned; future policies can be added by introducing new `policy_id`s and/or a v2 capability without rewriting history.

Context
- Continuity OS posture (ADR-0008 / AGENTS.md): the continuity event log is truth (append-only, replayable); caches are rebuildable; background workers emit structured frames + artifact refs (no hidden mutable state).
- Background jobs posture (ADR-0012): compaction work is represented as continuity job frames; outputs are artifact refs + checkpoint frames.
- Compaction cut points posture (ADR-0011): cut points are deterministic message-boundary anchors; selection and concurrency must be replay-safe.
- Requirement: compaction scheduling must not slow `thread.post_message` and must be auditable as continuity truth.

Decision

1) Add a dedicated scheduling capability
- Introduce capability: `compaction.auto.schedule` (v1).
- It evaluates a **policy** over continuity truth and decides whether to start a compaction summarizer job.
- The capability is callable by:
  - servers (control plane),
  - local CLI/TUI (manual or “background” UX),
  - external orchestrators (cron/worker) without relying on hidden server DB state.

2) Log the scheduling decision as continuity truth
- Introduce a new continuity frame:
  - `continuity_compaction_auto_schedule_decided`
- The frame records:
  - `policy_id` and resolved parameters (`stride_messages`, `max_new_checkpoints`, `block_on_inflight`),
  - computed `message_count` and planned cut point(s),
  - decision outcome (`scheduled` vs `skipped_inflight`),
  - the spawned `{job_id, job_kind}` when scheduled,
  - provenance (`actor_id`, `origin`).
- This ensures the “when/why” is replayable and auditable without reconstructing ephemeral scheduler state.

3) Keep `thread.post_message` hot path unchanged
- `thread.post_message` continues to only:
  - append the message frame,
  - append `continuity_run_spawned`,
  - spawn the run session.
- Scheduling is invoked out-of-band via `compaction.auto.schedule` (or by future workers that observe the continuity stream).

4) Concurrency + failure semantics
- Concurrency:
  - Multiple schedulers may race; duplicated work is allowed and resolved deterministically by stream order (ADR-0011).
  - Policy may optionally block overlapping compaction work by detecting in-flight summarizer jobs derived from continuity truth.
- Failures:
  - If a job is started, it must terminate with `continuity_job_ended(status=completed|failed, ...)` (ADR-0012).
  - Retries are explicit: invoke the scheduler again (new decision id; possibly new job id).

Non-goals (v1)
- A persistent, always-on scheduler daemon with its own database.
- Time-based policies (wall-clock) as primary triggers (nondeterministic; future policies may incorporate time only when the decision is explicitly logged).
- Hierarchical summaries or richer summary payloads (tracked separately).

References
- `AGENTS.md`
- `docs/06_decisions/ADR-0010-context-compiler-truth.md`
- `docs/06_decisions/ADR-0011-compaction-cutpoints-summaries.md`
- `docs/06_decisions/ADR-0012-background-jobs.md`
- `docs/03_contracts/compaction.md`
- `docs/03_contracts/event_frames.md`
- `docs/03_contracts/capability_registry.md`
