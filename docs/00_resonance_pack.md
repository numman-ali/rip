# Resonance Pack (Read This First)

Purpose
- This file is a "context rehydration pack" for autonomous agents.
- It exists so the operator never has to repeat product intent.
- If this conflicts with other docs, treat it as a defect: fix drift in the same change.

Non-negotiable product model
- RIP is a **continuity OS**, not a chat app: default UX is "one chat forever".
- The continuity event log is the source of truth (append-only, replayable).
- Provider conversation state (`previous_response_id`, vendor thread ids) is a cache and may be rotated/rebuilt at any time.
- Sessions are compute runs/turns (one run == one `session_ended`); sessions are not user-facing by default.
- Background/subconscious agents are jobs over event streams; they emit structured events + artifacts; no hidden mutable state.
- Multi-actor/shared continuities are first-class: every input/action must carry provenance (`actor_id`, `origin`).

Canonical references (required reading order)
1) `AGENTS.md` (operator intent + gates)
2) `docs/02_architecture/continuity_os.md` (the operating model)
3) `docs/06_decisions/ADR-0008-continuity-os.md` (decision: provider state is cache)
4) `agent_step.md` (current focus + next actions)
5) `docs/07_tasks/roadmap.md` (Now/Next/Later execution plan)

Current execution priority (as of 2026-01-21)
- Promote continuities (threads) to the primary control surface: "post message to continuity" spawns runs behind the scenes.
- Evolve frames to stream-scoped envelopes (continuity/session/task) with provenance, without breaking replay.
- Keep Open Responses full-fidelity at the provider boundary; never drop fields/events.
- Enforce surface parity and determinism via tests, fixtures, and CI gates.
