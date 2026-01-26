# Journey: Recover From Stall / Error (Visibility + Control)

Purpose: when something goes wrong (provider error, tool failure, or “no progress”), the user is never confused and always has a safe next action.

Status: **Design** | Phase: 2 | Last updated: 2026-01-26

---

## Scenario

The run stops making progress, or fails. The user needs to:
- understand what is happening (without raw log digging),
- inspect the exact failure context,
- recover by cancelling, retrying, or continuing on the continuity.

---

## Preconditions

- Errors are visible as frames (`provider_event` with errors, `tool_failed`, terminal task status).
- Any “stall detection” is UI-only (derived from last frame timestamps) and must not mutate truth.

---

## Steps (User Journey)

1. Run begins normally (output streaming).
2. A failure occurs or progress stops:
   - provider error (HTTP error, invalid JSON, response.failed),
   - tool failure (`tool_failed`),
   - task failure/cancelled (`tool_task_status: failed/cancelled`),
   - or “no new frames for N seconds” (stall hint).
3. UI surfaces this immediately:
   - status bar shows `⚠ error` or `⏸ stalled`,
   - timeline selection jumps (or hints) to the most recent error-relevant frame.
4. User drills down to see:
   - the error message,
   - the relevant raw payload (provider_event raw/data),
   - and any available remediation hint (help overlay / suggested actions).
5. User takes a safe action:
   - cancel the run/task,
   - retry (start a new run on the continuity),
   - rotate provider cursor if relevant (admin/advanced only; must be explicit).

---

## Layout Targets (Wireframes)

### XS: status-forward, overlay for error detail

```
┌─ RIP ────────────────┐
│ ⚠ error  last:tool   │
├──────────────────────┤
│ Output…              │
│                      │
│ [Enter] details      │
│ [Ctrl+C] cancel run  │
└──────────────────────┘

┌─ Error Detail ───────┐
│ tool_failed          │
│ error: "permission…" │
│                      │
│ [y] copy  [Esc] back │
└──────────────────────┘
```

### S/M: timeline highlights errors; inspector shows structured view

At S, use overlays; at M, keep an inspector region visible to reduce mode switching.

---

## Controls (Intent)

Documented in `docs/02_architecture/tui/03_interaction_patterns.md`. This journey assumes:
- `e`: filter timeline to errors.
- `Enter`: open detail.
- `Ctrl+C`: cancel current run (with clear confirmation rules).
- `?`: help overlay includes “what now” guidance for common failure modes.

---

## Parity (Must Hold)

- Error visibility: present in JSON frames and output renderers on all surfaces.
- Cancel: `rip` / `rip run` signal + server/session cancel endpoint + SDK.
- Retry/continue: always by targeting the continuity (post message → new run), never by mutating past runs.

---

## Evidence Required (To Ship)

- Golden snapshots at XS/S/M for:
  - provider error (HTTP or invalid JSON),
  - tool_failed with a remediation hint,
  - “stalled” hint derived from frame timestamps (no truth mutation).
- Deterministic fixtures that simulate each case.
- Performance check: error highlighting must not scan unbounded history (use bounded indexes/state).

