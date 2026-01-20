# Live Session

## Purpose

The primary workspace. Where users observe agent activity, read output, inspect tools, and send messages. This is where 90% of time is spent.

## Entry Conditions

- New thread created
- Existing thread resumed
- Attached to running session

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `session.stream_events` | Real-time frame display |
| `session.send_input` | Message submission |
| `session.cancel` | Cancel current operation |
| `tool.*` | Tool execution display |
| `checkpoint.*` | Checkpoint indicators |
| `ui.multiline` | Multi-line input |
| `ui.autocomplete` | @file, /command completion |
| `ui.message_queue` | Queue during streaming |
| `ui.raw_events` | Rendered output ↔ raw frame mode |
| `ui.clipboard` | Copy selected frame / output |

---

## Wireframe (Full Layout)

```
┌─────────────────────────────────────── RIP ────────────────────────────────────────┐
│ ● feat/auth ▸ turn 24  │ gpt-4.1 │ TTFT 142ms │ $0.47 │ ⟳ streaming │ [?] help    │
├────────────────────────────────────────────────────────────────────────────────────┤
│┌─ Threads ──────────────┐┌─ Timeline ──────────────────────┐┌─ Inspector ────────┐│
││                        ││                                  ││                    ││
││ ▾ main                 ││  seq │ type          │ Δt       ││ Frame #127         ││
││   ├─ feat/auth ◀       ││ ─────┼───────────────┼───────── ││ ────────────────── ││
││   │  └─ experiment     ││  122 │ provider      │ +0ms     ││                    ││
││   └─ refactor/db       ││  123 │ provider      │ +12ms    ││ type: tool_ended   ││
││                        ││  124 │ tool_started  │ +45ms    ││ tool: bash         ││
││ [+] new branch         ││  125 │ tool_stdout   │ +52ms    ││ exit: 0            ││
││                        ││  126 │ tool_stdout   │ +89ms    ││ duration: 847ms    ││
│├────────────────────────┤│ ▸127 │ tool_ended    │ +847ms   ││                    ││
││ ▾ Tasks (2)            ││  128 │ provider      │ +12ms    ││ args:              ││
││   ⟳ npm test    2:14   ││  129 │ provider      │ +8ms     ││   command: "npm    ││
││   ⟳ build       0:47   ││      │               │          ││    test"           ││
││   ✓ lint        done   ││                                  ││                    ││
││                        ││ [p]rovider [t]ool [c]hkpt [e]rr ││ [Tab] JSON/decode  ││
│└────────────────────────┘│ [/]search [f]ollow [0]clear     ││ [y]ank [o]pen      ││
│                          │                                  │└────────────────────┘│
│                          ├──────────────────────────────────┤┌─ Artifacts ────────┐│
│                          │ ▾ Output                         ││                    ││
│                          │                                  ││ 📄 patch.diff      ││
│                          │ I'll help you implement the      ││ 📄 test.log        ││
│                          │ authentication flow. Let me      ││                    ││
│                          │ first check the existing code... ││ [Enter] view       ││
│                          │                                  ││ [d] diff           ││
│                          │ Looking at `src/auth.ts`, I can  │└────────────────────┘│
│                          │ see the current implementation   │                      │
│                          │ uses basic session tokens. I'll  │                      │
│                          │ add refresh token support...     │                      │
│                          │                                  │                      │
│                          │ █                                │                      │
│                          │                                  │                      │
├──────────────────────────┴──────────────────────────────────┴──────────────────────┤
│ ┌─ Input ────────────────────────────────────────────────────────────────────────┐ │
│ │ > Add error handling for expired tokens @src/auth.ts                           │ │
│ └────────────────────────────────────────────────────────────────────────────────┘ │
│ [Ctrl+K] palette  [Ctrl+B] sidebar  [Ctrl+T] tasks  [Tab] focus  [Ctrl+C] cancel   │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Panel Breakdown

### Status Bar (Top)

```
│ ● feat/auth ▸ turn 24  │ gpt-4.1 │ TTFT 142ms │ $0.47 │ ⟳ streaming │ [?] help    │
```

| Element | Meaning |
|---------|---------|
| `●` | Connected (○ = disconnected) |
| `feat/auth` | Current thread name |
| `▸ turn 24` | Current turn number |
| `gpt-4.1` | Active model |
| `TTFT 142ms` | Time to first token |
| `$0.47` | Session cost |
| `⟳ streaming` | Current status |
| `[?] help` | Help hint |

**Status values**: `⟳ streaming`, `◐ thinking`, `● idle`, `⏸ paused`, `⚠ error`

---

### Sidebar: Threads Panel

```
┌─ Threads ──────────────┐
│                        │
│ ▾ main                 │
│   ├─ feat/auth ◀       │  ◀ = current
│   │  └─ experiment     │
│   └─ refactor/db       │
│                        │
│ [+] new branch         │
└────────────────────────┘
```

- Tree view of thread structure
- Current thread marked with `◀`
- Click or navigate to switch threads
- `[+]` creates branch from current point

---

### Sidebar: Tasks Panel

```
┌─ Tasks (2) ────────────┐
│                        │
│ ⟳ npm test      2:14   │  running, elapsed time
│ ⟳ cargo build   0:47   │
│ ✓ lint          done   │  completed
│ ✗ typecheck     fail   │  failed (expandable)
│                        │
└────────────────────────┘
```

- Shows background tool tasks
- Status indicators: `⟳` running, `✓` done, `✗` failed
- Elapsed/completed time
- Expandable for details

---

### Timeline Panel

```
┌─ Timeline ──────────────────────────────────────────────────────┐
│                                                                 │
│  seq │ type            │ summary                    │ Δt       │
│ ─────┼─────────────────┼────────────────────────────┼───────── │
│  122 │ provider        │ response.created           │ +0ms     │
│  123 │ provider        │ delta "I'll help..."       │ +12ms    │
│  124 │ tool_started    │ bash: npm test            │ +45ms    │
│  125 │ tool_stdout     │ "PASS src/auth..."        │ +52ms    │
│  126 │ tool_stdout     │ "PASS src/middle..."      │ +89ms    │
│ ▸127 │ tool_ended      │ exit=0, 847ms             │ +847ms   │
│  128 │ checkpoint      │ auto: pre-edit            │ +2ms     │
│  129 │ provider        │ delta "Now I'll..."       │ +8ms     │
│                                                                 │
│ [p]rovider [t]ool [c]hkpt [e]rr  [/]search [f]ollow [0]clear   │
└─────────────────────────────────────────────────────────────────┘
```

| Column | Content |
|--------|---------|
| seq | Frame sequence number |
| type | Frame type (color-coded) |
| summary | Contextual summary |
| Δt | Time delta from previous |

**Filters** (toggle):
- `p` - provider events only
- `t` - tool events only
- `c` - checkpoint events only
- `e` - errors only
- `0` - clear all filters

**Modes**:
- `f` - auto-follow (tail new frames)
- When not following, selection stays fixed

---

### Output Panel

```
┌─ Output ────────────────────────────────────────────────────────┐
│                                                                 │
│ I'll help you implement the authentication flow. Let me        │
│ first check the existing code structure...                     │
│                                                                 │
│ Looking at `src/auth.ts`, I can see the current implementation │
│ uses basic session tokens. I'll add refresh token support      │
│ with the following changes:                                    │
│                                                                 │
│ 1. Add a `refreshToken` field to the session model            │
│ 2. Create a `/auth/refresh` endpoint                          │
│ 3. Update the middleware to check token expiry                │
│                                                                 │
│ Let me start by modifying the session model...                 │
│                                                                 │
│ █                                                              │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

- Rendered assistant output (text deltas aggregated)
- Markdown rendering where possible
- Code blocks with syntax hints
- `█` cursor shows streaming position
- Scrollable when content exceeds viewport

**Toggle**: `r` switches between rendered and raw frame view

---

### Inspector Panel

```
┌─ Inspector ────────────────────────────────────────────────────┐
│ Frame #127                                                     │
│ ───────────────────────────────────────────────────────────── │
│                                                                │
│ type:        tool_ended                                        │
│ tool_id:     7a3f8b2c-...                                      │
│ name:        bash                                              │
│ exit_code:   0                                                 │
│ duration_ms: 847                                               │
│                                                                │
│ args:                                                          │
│   command: "npm test --coverage"                               │
│   timeout_ms: 30000                                            │
│                                                                │
│ ───────────────────────────────────────────────────────────── │
│ [Tab] toggle JSON view    [y] copy    [o] open detail         │
└────────────────────────────────────────────────────────────────┘
```

- Shows details of selected Timeline frame
- Decoded view (default) or raw JSON (`Tab` to toggle)
- `Enter` or `o` opens full Tool Detail overlay

---

### Artifacts Panel

```
┌─ Artifacts ────────────────────────────────────────────────────┐
│                                                                │
│ 📄 patch_auth.diff          2.1 KB                            │
│ 📄 test_output.log          147 KB                            │
│ 📄 coverage_report.json     12 KB                             │
│                                                                │
│ [Enter] view    [d] diff view    [s] save    [y] copy         │
└────────────────────────────────────────────────────────────────┘
```

- Lists artifacts from current session
- Size indicator
- Actions to view, save, copy

---

### Input Panel

```
┌─ Input ────────────────────────────────────────────────────────────────────────┐
│ > Add error handling for expired tokens @src/auth.ts                           │
│   ▲                                      ▲                                     │
│   prompt text                            file reference                        │
└────────────────────────────────────────────────────────────────────────────────┘
```

- Natural language input
- Supports `@file` references (autocomplete)
- Supports `/commands` (autocomplete)
- Multi-line with `Shift+Enter`
- History with `↑/↓`

---

## Sidebar Toggle States

### Sidebar Hidden (`Ctrl+B`)

```
┌─────────────────────────────────────── RIP ────────────────────────────────────────┐
│ ● feat/auth ▸ turn 24  │ gpt-4.1 │ TTFT 142ms │ $0.47 │ ⟳ streaming │ [?] help    │
├────────────────────────────────────────────────────────────────────────────────────┤
│┌─ Timeline ────────────────────────────────┐┌─ Inspector ─────────────────────────┐│
││                                           ││                                     ││
││  seq │ type          │ summary   │ Δt     ││ Frame #127                          ││
││ ─────┼───────────────┼───────────┼─────── ││ type: tool_ended                    ││
││  ... │ ...           │ ...       │ ...    ││ ...                                 ││
│├───────────────────────────────────────────┤│                                     ││
││ ▾ Output                                  ││                                     ││
││ ...                                       │└─────────────────────────────────────┘│
│└───────────────────────────────────────────┘                                       │
└────────────────────────────────────────────────────────────────────────────────────┘
```

### Tasks Expanded (`Ctrl+T`)

```
┌─ Tasks (expanded) ─────────────────────────────────────────────────────────────────┐
│                                                                                    │
│ ┌─ npm test ─────────────────────────────────────────────────────────────────────┐│
│ │ Status: running (2:14 elapsed)                                                 ││
│ │                                                                                ││
│ │ PASS src/auth.test.ts                                                         ││
│ │   ✓ login with valid credentials (45ms)                                       ││
│ │   ✓ refresh token flow (89ms)                                                 ││
│ │ RUNS src/middleware.test.ts                                                   ││
│ │   ◌ auth middleware validation...                                             ││
│ │ █                                                                             ││
│ │                                                                                ││
│ │ [c] cancel    [f] focus    [l] full log                                       ││
│ └────────────────────────────────────────────────────────────────────────────────┘│
│                                                                                    │
│ [Esc] minimize                                                                     │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Actions

### Global (always available)

| Key | Action |
|-----|--------|
| `Ctrl+K` | Command palette |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+T` | Toggle/expand tasks |
| `Ctrl+C` | Cancel current operation |
| `Ctrl+L` | Clear output |
| `?` | Help overlay |
| `Tab` | Cycle focus |

### Timeline (when focused)

| Key | Action |
|-----|--------|
| `j/k` | Navigate frames |
| `Enter` | Open tool detail |
| `f` | Toggle auto-follow |
| `p/t/c/e` | Filter by type |
| `0` | Clear filters |
| `/` | Search |
| `y` | Copy frame JSON |

### Input (when focused)

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Shift+Enter` | Newline |
| `↑/↓` | History |
| `Tab` | Autocomplete |
| `Ctrl+E` | External editor |

---

## Streaming States

### Active Streaming

```
│ ⟳ streaming │
```
- Output shows live text with cursor `█`
- Timeline auto-follows (if enabled)
- Input still accepts text (queued)

### Thinking/Processing

```
│ ◐ thinking │
```
- Waiting for response
- Spinner animation
- No output yet

### Idle

```
│ ● idle │
```
- Ready for input
- No active operation

### Error

```
│ ⚠ error │
```
- Something went wrong
- Error details in Timeline/Output

---

## Considerations for Implementers

- **Panel resize**: Users may want to resize panels. Consider draggable borders or presets.
- **Streaming performance**: High-frequency frame updates need efficient rendering.
- **Output accumulation**: Output panel aggregates deltas; consider how to handle very long outputs.
- **Focus management**: Clear visual indication of which panel has focus.
- **Keyboard capture**: Input panel needs most keys, but globals must still work.

---

## Surface Parity

| TUI Feature | CLI | Headless | SDK |
|-------------|-----|----------|-----|
| Timeline | `--verbose` | JSON frames | Frame iterator |
| Output | Default output | `--output text` | Event stream |
| Inspector | N/A | Full JSON | Frame access |
| File refs | `@file` in prompt | `--context file` | `context` param |
| Commands | `/command` | `--command` | Method calls |
