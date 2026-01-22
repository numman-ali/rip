# Experience Review (TUI)

Purpose: make sure the TUI is optimized for RIP’s actual goal (Continuity OS + autonomous agent runs), not a generic “chat UI that also shows diffs”.

Status: **Design** | Phase: 2 | Last updated: 2026-01-22

---

## Problem Statement

Most coding agents feel like: *chat transcript + occasional tool call/diff spam*.

RIP should feel like: **conversational by default**, with **ambient awareness** of background work (tools/tasks/subagents/continuities), and **drill-down** into any detail on demand — without requiring users to “read the code” unless they want to.

The TUI is important but secondary: it must not drive runtime behavior. It should render the canonical event stream and make control discoverable.

---

## Experience Pillars (What “Wow” Means)

1. **Conversational-first**
   - The primary surface is still “talk to the system”.
   - The UI should not require frequent mode switching to understand progress.

2. **Ambient transparency**
   - At a glance: what’s running, what’s blocked, what changed, what’s next.
   - Background activity should feel “alive” without being noisy.

3. **Progressive disclosure**
   - Summary first; details via drill-down (tool args, raw frames, diffs, artifacts, task logs).
   - The default view should be readable and calm.

4. **User agency + trust**
   - UI never hides important state (errors, stalls, permission gates).
   - The user can always pause, inspect, override, or revert.

5. **Continuity OS alignment**
   - “One chat forever” is the default mental model.
   - Runs/sessions are compute units; the continuity log is truth; provider cursors are cache.

---

## Responsive Targets (Phone/SSH/Web Terminals)

We explicitly target terminals used via SSH on phones and web terminals. “Supported” means **usable**, not pixel-perfect.

### Breakpoints (Guidance)

| Class | Typical Size | Layout Behavior | Default Visible |
|------:|--------------|----------------|-----------------|
| XS | 60×20 → 79×23 | Single column | Output + minimal status; everything else via palette/overlays |
| S | 80×24 → 99×30 | Two regions | Timeline + Output; Inspector as overlay |
| M | 100×31 → 139×45 | Three regions | Sidebar + Timeline/Output + Inspector |
| L | 140×46+ | Expanded | Add richer diff/side-by-side where useful |

### Required Degradation Behaviors

- **16-color / no-color** support (never rely on color alone for meaning).
- **Unicode optional** (provide ASCII fallbacks for icons/markers).
- **Low input bandwidth** (phone keyboards): command palette must do more work.
- **Small viewport**: never force users to “track” multiple scrolling panes.

---

## Visual Language (Icons + Color Without Noise)

Use a consistent visual vocabulary to encode state quickly:

- **State markers**: running / blocked / error / done / attention-needed.
- **Provenance markers**: actor/origin (human vs agent vs background job).
- **Domain markers**: code edits, tool I/O, artifacts, tasks, provider events.

Rules:
- Icons and colors are *affordances*, not truth. Truth is in the frames.
- Every visual cue must have a text fallback.
- Prefer stable, low-saturation palettes; reserve “bright” for exceptions (errors, permission prompts, stalls).

---

## Agent-Controllable Views (Phase 2 Concept)

The agent should be able to *suggest* UI actions like “open the tool output”, “show the diff”, or “highlight the error”.

Design constraints:
- Must be **frame-driven and replayable** (a UI suggestion is just another frame).
- Must not be **TUI-only business logic**; other surfaces can render suggestions as hints or ignore them.
- Must be **non-authoritative** by default (user can dismiss/override).

Practical shape:
- Add a data-only “UI intent / suggestion” frame type that references existing entities (`tool_id`, `artifact_id`, `checkpoint_id`, `thread_id`, `task_id`) and a desired view (`open_overlay`, `focus_panel`, `pin_item`, etc.).
- The TUI treats these as “assistive navigation”, not commands.

---

## Design Gates (What We Require Before Shipping TUI UX Work)

For any Phase 2 TUI feature beyond simple wiring:

1. **Journey doc** (short): what the user does, what they see, what success feels like.
2. **Responsive evidence**: ratatui golden snapshots at XS/S/M sizes for the journey.
3. **Parity check**: confirm information/action parity via `docs/02_architecture/tui/04_graceful_degradation.md`.
4. **Performance check**: verify it stays smooth at 10k+ frames and large tool output (virtualization/bounds).
5. **No hidden state**: everything reconstructable from frames + minimal UI-local layout preferences.

