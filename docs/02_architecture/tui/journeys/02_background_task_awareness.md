# Journey: Background Task Awareness (Ambient + Drill-Down)

Purpose: make background work feel “alive” without flooding the user; tasks are visible, inspectable, and controllable.

Status: **Design** | Phase: 2 | Last updated: 2026-01-26

---

## Scenario

While a run is ongoing (or after it ends), background tool tasks exist (pipes/pty). The user needs:
- at-a-glance awareness (running, failed, completed),
- quick drill-down into logs (bounded; artifact-backed),
- the ability to cancel safely.

Primary feeling: **alive, gentle, never noisy**.

---

## Preconditions

- Tasks exist as entities with their own event streams (`tool_task_*` frames) and status transitions.
- PTY control remains policy-gated; the default UX is safe and deterministic.

---

## Steps (User Journey)

1. A background task starts (spawned explicitly or indirectly by a tool).
2. UI shows an ambient indicator:
   - count of running tasks in the status bar,
   - and/or compact “task chips” in an Activity rail/drawer on larger screens.
3. User opens the tasks view (overlay/panel) and selects a task.
4. User drills down:
   - sees task metadata (tool, cwd/title, started_at),
   - sees status + exit code if terminal,
   - tails stdout/stderr via artifact-backed range reads (bounded view).
5. If needed, user cancels the task and sees the status transition reflected immediately.

---

## Layout Targets (Wireframes)

### XS: overlay-based tasks view

```
┌─ RIP ────────────────┐
│ ● run 24  tasks:2    │
├──────────────────────┤
│ Output…              │
│                      │
│ [Ctrl+T] tasks       │
└──────────────────────┘

┌─ Tasks ──────────────┐
│ ⟳ build   1:02       │
│ ✗ test    failed     │
│ ✓ lint    done       │
│                      │
│ [Enter] view  [c]ancel│
└──────────────────────┘
```

### S/M: tasks in sidebar or dedicated panel

At S, tasks should be an overlay/drawer by default; at M, tasks may be pinned as an Activity rail without stealing attention from the Canvas.

---

## Controls (Intent)

Documented in `docs/02_architecture/tui/03_interaction_patterns.md`. This journey assumes:
- `Ctrl+T`: open tasks overlay/panel.
- `Enter`: view task details/logs.
- `c`: cancel selected task (confirm if destructive).
- `Tab`: switch between stdout/stderr streams in the task log view.

---

## Parity (Must Hold)

- List tasks: `rip tasks list` / server JSON / SDK.
- Stream task events: `rip tasks stream` / server SSE / SDK iterator.
- Fetch logs (artifact): `artifact_fetch` (local) or server artifact endpoint.
- Cancel task: `rip tasks cancel` / server endpoint / SDK.

---

## Evidence Required (To Ship)

- Golden snapshots at XS/S/M for:
  - tasks list with mixed states,
  - task detail with stdout/stderr preview + “more via artifact” hint,
  - cancel flow + terminal status.
- Deterministic fixtures for `pipes` and policy-gated `pty` ordering (no reliance on wall-clock timing).
- Performance check: does not regress frame render budget; log tailing uses bounded range reads.
