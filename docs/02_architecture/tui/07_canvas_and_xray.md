# Canvas + X-ray Model

Purpose: eliminate doc drift by defining the two canonical postures of the TUI:
1) **Canvas** (default): a beautiful narrative stream with semantic abstractions.
2) **X-ray** (peek inside): full fidelity internals (raw frames, artifacts, provenance).

Status: **Design** | Phase: 2 | Last updated: 2026-01-27

---

## Canvas (Default)

The Canvas is the primary workspace: one continuous, readable narrative stream.

Rules:
- **Story first**: default rendering is human language + semantic summaries.
- **Ambient awareness**: background activity is visible as compact “chips” (counts + state) without flooding the stream.
- **Progressive disclosure**: every chip/card can be opened to reveal the exact underlying frames + artifacts.
- **No hidden truth**: anything shown is derived from the event stream (or explicitly labeled as UI-local, e.g. stall hints).

### Ambient Chips (Examples)

Chips are a UI representation of underlying entities/frames (not a new source of truth).

- `tools:2` (running) → opens tool list → opens tool detail (args/stdout/stderr/artifacts)
- `tasks:1` (running) → opens tasks list → opens task log tail (artifact-backed)
- `jobs:3` (background agents) → opens job list → opens job detail (spawned/ended frames)
- `context` (compiling/compacted) → opens context selection/compiled bundle + artifact refs
- `artifacts:4` → opens artifacts picker/viewer
- `⚠ error` / `⏸ stalled` → opens the most relevant error/stall detail view

Chips must have text fallbacks and must never rely on color alone.

---

## X-ray (Peek Inside)

X-ray is the “intuitive interface for internals”:
- Timeline (frames) with filters/search
- Inspector (decoded + raw JSON)
- Artifact viewer (range fetch; never inline unbounded text)
- Provenance (actor_id, origin) and entity linking (tool_id, task_id, job_id, checkpoint_id)

Rules:
- X-ray is **always available**, but is not the default posture.
- X-ray views are **frame-driven and replayable** (no hidden mutable state).
- X-ray is the escape hatch for “I want certainty” and “show me exactly what happened”.

---

## Responsive Targets (Posture by Size)

- XS (60×20 → 79×23): Canvas only; X-ray via overlays.
- S (80×24 → 99×30): Canvas; an “Activity” overlay/drawer for chips → lists → details.
- M (100×31+): Canvas + optional pinned Activity rail; X-ray panels can be pinned as a power-user layout preset.

---

## Acceptance (Docs + Tests)

- Journey docs remain the gating specs (snapshot-backed).
- Any screen/wireframe doc must state whether it describes Canvas (default) or X-ray (peek inside).
- Golden snapshots at XS/S/M must show Canvas-first behavior (chips + drill-down), with X-ray reachable via an explicit action.

