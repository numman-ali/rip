# Continuity OS (One Chat, Many Jobs)

Summary
- RIP is a **continuity OS**, not a chat app: the user experience is "one chat forever".
- The **continuity event log is the source of truth** (append-only, replayable).
- Provider conversation state (Open Responses `previous_response_id`, vendor thread ids) is a **cache** that can be rotated/rebuilt at any time.

Core invariants
- **Truth lives in RIP**: internal logs + artifacts + snapshots; providers are replaceable transports.
- **No destructive edits**: nothing is "replaced" in history; new events supersede old state; compaction creates new artifacts + cut-point events.
- **Determinism is non-negotiable**: every behavior-changing decision must be logged (context selection, routing, tool dispatch, worker outputs).
- **User does not see sessions**: sessions are compute jobs; continuities are user-facing identity/state.

Entity model (conceptual)
- **Continuity** (`continuity_id`): durable, user-facing identity bound to a workspace (default UX: exactly one active continuity).
- **Turn**: an input event added to a continuity (from a user or another actor); may trigger foreground work.
- **Session / Run** (`session_id`): a single agent execution unit that produces an ordered stream of frames (one run/turn).
- **Task** (`task_id`): background tool execution with its own event stream (may outlive a session).
- **Artifact** (`artifact_id`): stored blobs (tool outputs, summaries, indexes); range-readable; referenced by frames.
- **Provider cursor**: ephemeral per-provider handle (e.g., `previous_response_id` chain) used only to reduce prompt size / latency.

How "one chat forever" works
- All user/actor inputs append to the continuity log.
- Foreground responsiveness comes from spawning **runs** that read from the continuity log and emit frames.
- Long histories do **not** expand provider history forever:
  - Background workers produce **summary checkpoints** and other derived artifacts.
  - The **context compiler** builds a replayable "compiled context" from: recent raw events + relevant artifacts (summaries/memory/files) + policy.
    - It writes a versioned **context bundle artifact** and logs the compilation decision to the continuity stream (`continuity_context_compiled`).
  - When context exceeds thresholds, RIP **rotates provider cursors** (starts a fresh provider conversation) while keeping continuity unchanged internally.

Parallelism (foreground vs "subconscious" work)
- Multiple jobs may operate on the same continuity in parallel:
  - Foreground run: immediate response + tool loop.
  - Background workers: summarization, indexing, pruning, cost accounting, audits, etc.
- Side-effecting actions (tool calls that modify the workspace) must be **scheduled/serialized** and logged to preserve determinism.

Multi-actor + shared continuities (team model)
- A continuity may have multiple actors (human users, automation, leads, bots).
- Inputs and actions carry an `actor_id` and `origin` metadata; the control plane enforces auth/ACL.
- "Broadcast" is modeled as appending actor messages to multiple continuities and/or emitting notification events; everything remains append-only.

Implications for contracts
- Event frames must evolve from session-scoped to **stream-scoped** envelopes (continuity/session/task streams).
- Surfaces should default to the "single continuity" UX, even if advanced browsing/branching exists for power users.
