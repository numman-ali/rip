# Journey: Follow A Run (Conversational-First + Tool Drill-Down)

Purpose: define the baseline “I can follow what RIP is doing” experience without requiring the user to read raw logs or code.

Status: **Design** | Phase: 2 | Last updated: 2026-01-26

---

## Scenario

The user starts a run, watches output, notices tool use, and drills into details (args/stdout/stderr/artifacts) only when needed.

Primary feeling: **calm, readable, always oriented**.

---

## Preconditions

- A session/run emits canonical frames (`session_started`, `output_text_delta`, `tool_*`, `provider_event`, `session_ended`).
- Tools can emit large output via artifacts; TUI must never inline unbounded text.

---

## Steps (User Journey)

1. User starts a run (`rip run "<prompt>"`) or attaches to a live session.
2. UI shows streaming output (rendered view by default).
3. A tool starts (e.g., `bash`, `apply_patch`), visible as an ambient signal (status + timeline).
4. User drills down:
   - Opens tool detail for the selected tool event (overlay or inspector) to see args, exit status, and stdout/stderr preview.
   - If output is large, opens the artifact viewer (range fetch) instead of inlining.
5. User returns to the conversational stream and continues reading; no mode confusion.

---

## Layout Targets (Wireframes)

These are directional wireframes; exact panel borders/labels can change as long as the journey gates hold.

### XS (60×20 → 79×23): single column, drill-down via overlays

```
┌─ RIP ────────────────┐
│ ● run 24  ⟳ tool 1   │
├──────────────────────┤
│ Output (rendered)    │
│ …streaming text…     │
│                      │
│ [r] raw  [?] help    │
│ [Enter] details      │
└──────────────────────┘
```

Drill-down:
- `Enter` opens a detail overlay for the most recent/selected “interesting” event (tool/error/provider).
- `Esc` returns.

### S (80×24 → 99×30): timeline + output, inspector as overlay

```
┌──────── RIP ────────┐
│ ● idle  TTFT 120ms  │
├────────┬────────────┤
│Timeline│ Output      │
│…events │ …text…      │
└────────┴────────────┘
```

### M (100×31 → 139×45): 3 regions, inspector visible

```
┌──────────── RIP ────────────┐
│ ● streaming   tasks:1       │
├───────┬─────────┬───────────┤
│Sidebar│Timeline │ Inspector  │
│(opt)  │         │           │
├───────┴─────────┴───────────┤
│ Output (rendered)           │
└─────────────────────────────┘
```

---

## Controls (Intent)

Key intent is documented in `docs/02_architecture/tui/03_interaction_patterns.md`. This journey assumes:
- `r`: rendered ↔ raw output toggle.
- `Enter`: open detail for selected timeline event (tool/provider/error).
- `y`: copy selected output/frame (with OSC52 fallback or local buffer).

---

## Parity (Must Hold)

Everything the user can learn/do here must exist on other surfaces:
- Inspect tool execution: available via frames (CLI output/JSONL, server SSE, SDK iterator).
- View artifact-backed output: available via `artifact_fetch` or the control plane artifact endpoint.
- Cancel run/task: available via CLI/server/SDK.

If a UI action is “navigation only”, it can be TUI-specific; if it triggers side effects, it must be a capability exposed across surfaces.

---

## Evidence Required (To Ship)

- Golden snapshots for this journey at XS/S/M.
- Fixture frames (deterministic) that cover:
  - tool_started → stdout/stderr → tool_ended (+ artifacts)
  - provider_event errors (invalid json or provider error)
  - output_text streaming (with truncation behavior)
- Performance check: smooth at 10k+ frames; output rendering bounded.

