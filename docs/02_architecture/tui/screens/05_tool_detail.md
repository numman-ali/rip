# Tool Detail

## Purpose

Deep inspection of a tool execution. Shows inputs, outputs, timing, diffs, and allows revert actions.

## Entry Conditions

- User presses `Enter` on a tool frame in Timeline
- User presses `o` (open) in Inspector panel
- Direct navigation via command palette

## Capabilities Used

| Capability | Usage |
|------------|-------|
| Tool frame inspection | `tool_started`, `tool_ended`, `tool_stdout`, `tool_stderr` |
| `checkpoint.rewind` | Revert action |
| `tool.output_fetch` | Retrieve full output if truncated |
| `extension.tool_renderers` | Custom rendering hints |

---

## Wireframe

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│ Tool Detail: bash                                              [Esc] close  [f] full│
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ┌─ Summary ───────────────────────────────────────────────────────────────────────┐ │
│ │                                                                                 │ │
│ │  Tool:       bash                     Status:    ✓ success (exit 0)            │ │
│ │  Tool ID:    7a3f8b2c-1234-5678       Duration:  847ms                         │ │
│ │  Frame:      #124 → #127              Checkpoint: cp_18 (auto)                 │ │
│ │                                                                                 │ │
│ └─────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                     │
│ ┌─ Input (arguments) ─────────────────────────────────────────────────────────────┐ │
│ │                                                                                 │ │
│ │  {                                                                              │ │
│ │    "command": "npm test --coverage",                                           │ │
│ │    "timeout_ms": 30000,                                                        │ │
│ │    "working_dir": "/home/user/project"                                         │ │
│ │  }                                                                              │ │
│ │                                                                                 │ │
│ └─────────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                     │
│ ┌─ Output ─────────────────────────────────────────────────────────────────────────┐│
│ │                                                                                  ││
│ │  > npm test --coverage                                                          ││
│ │                                                                                  ││
│ │  PASS src/auth.test.ts                                                          ││
│ │    ✓ login with valid credentials (45ms)                                        ││
│ │    ✓ login with invalid credentials returns 401 (12ms)                          ││
│ │    ✓ refresh token flow (89ms)                                                  ││
│ │    ✓ token expiry handling (34ms)                                               ││
│ │                                                                                  ││
│ │  PASS src/middleware.test.ts                                                    ││
│ │    ✓ auth middleware blocks unauthenticated (8ms)                               ││
│ │    ✓ auth middleware passes valid tokens (5ms)                                  ││
│ │                                                                                  ││
│ │  Test Suites: 2 passed, 2 total                                                 ││
│ │  Tests:       6 passed, 6 total                                                 ││
│ │  Coverage:    87.3%                                                             ││
│ │                                                                                  ││
│ │  [showing 1-20 of 45 lines]                                   [↓] more below   ││
│ └──────────────────────────────────────────────────────────────────────────────────┘│
│                                                                                     │
│ [y] copy   [s] save   [r] revert to cp_17   [a] view artifact   [j/k] scroll       │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Sections

### Summary Bar

| Field | Description |
|-------|-------------|
| Tool | Tool name |
| Tool ID | Unique identifier |
| Frame | Start → end frame sequence |
| Status | Success/failure with exit code |
| Duration | Execution time |
| Checkpoint | Associated checkpoint (if any) |

### Input Section

Shows the arguments passed to the tool:
- JSON formatted
- Syntax highlighted
- Expandable for large inputs

### Output Section

Shows stdout/stderr combined:
- Scrollable
- Line numbers optional
- Truncation indicator if output was capped
- Link to full artifact if available

---

## Tool-Specific Rendering

Different tools may render output differently based on render hints:

### File Edit Tools (apply_patch, write, edit)

```
┌─ Output (diff view) ───────────────────────────────────────────────────────────────┐
│                                                                                    │
│  src/auth.ts                                                                       │
│  ──────────────────────────────────────────────────────────────────────────────    │
│   45 │   async function login(user: string, pass: string) {                       │
│   46 │-    return db.query(user, pass);                                           │
│   46 │+    const sanitized = sanitize(user);                                      │
│   47 │+    if (!sanitized) {                                                      │
│   48 │+      throw new AuthError('invalid_username');                             │
│   49 │+    }                                                                      │
│   50 │+    return db.query(sanitized, hash(pass));                                │
│   51 │   }                                                                        │
│                                                                                    │
└────────────────────────────────────────────────────────────────────────────────────┘
```

### Read Tools (read, glob, grep)

```
┌─ Output (file content) ────────────────────────────────────────────────────────────┐
│                                                                                    │
│  src/auth.ts (lines 40-60)                                                        │
│  ──────────────────────────────────────────────────────────────────────────────    │
│   40 │ import { db } from './db';                                                 │
│   41 │ import { hash } from './crypto';                                           │
│   42 │                                                                            │
│   43 │ export interface Session {                                                 │
│   44 │   userId: string;                                                          │
│   45 │   token: string;                                                           │
│   ...                                                                              │
└────────────────────────────────────────────────────────────────────────────────────┘
```

### Bash/Shell Tools

```
┌─ Output (terminal) ────────────────────────────────────────────────────────────────┐
│                                                                                    │
│  $ npm test --coverage                                                            │
│                                                                                    │
│  > myapp@1.0.0 test                                                               │
│  > jest --coverage                                                                │
│                                                                                    │
│  PASS src/auth.test.ts                                                            │
│  ...                                                                              │
└────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Failed Tool State

```
┌─ Summary ───────────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  Tool:       bash                     Status:    ✗ failed (exit 1)             │
│  Tool ID:    7a3f8b2c-1234-5678       Duration:  2341ms                        │
│  Frame:      #124 → #127              Checkpoint: cp_18 (auto)                 │
│                                                                                 │
│  Error: Command failed with exit code 1                                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─ stderr ────────────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  npm ERR! Test failed. See above for more details.                             │
│                                                                                 │
│  FAIL src/auth.test.ts                                                         │
│    ✗ login with expired token (45ms)                                           │
│                                                                                 │
│      Expected: 401                                                              │
│      Received: 500                                                              │
│                                                                                 │
│      at Object.<anonymous> (src/auth.test.ts:34:5)                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Actions

| Key | Action | Effect |
|-----|--------|--------|
| `Esc` | Close | Return to Live Session |
| `f` | Full screen | Expand to full terminal |
| `y` | Copy | Copy output to clipboard |
| `s` | Save | Save output to file |
| `r` | Revert | Rewind to checkpoint before this tool |
| `a` | Artifact | Open full artifact viewer (if truncated) |
| `j/k` | Scroll | Navigate output |
| `g/G` | Top/bottom | Jump to start/end of output |
| `/` | Search | Search within output |
| `Tab` | Toggle view | Switch between formatted/raw |

---

## Revert Confirmation

When pressing `r`:

```
┌─ Revert to Checkpoint ────────────────────────────────────────────────────────────┐
│                                                                                   │
│  This will revert the workspace to checkpoint cp_17 (before this tool ran).      │
│                                                                                   │
│  Changes that will be undone:                                                     │
│    • src/auth.ts (modified)                                                       │
│    • src/middleware.ts (modified)                                                 │
│                                                                                   │
│  The conversation will continue from turn 23.                                     │
│                                                                                   │
│  ┌────────────────────────────────────────────────────────────────────────────┐  │
│  │  [y] Yes, revert    [n] No, cancel    [d] View diff first                 │  │
│  └────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                   │
└───────────────────────────────────────────────────────────────────────────────────┘
```

---

## Considerations for Implementers

- **Large outputs**: Tool output can be very large. Consider pagination or virtual scrolling.
- **Syntax highlighting**: Diff and code views benefit from highlighting. Consider cost vs benefit.
- **Render hints**: Extensions may provide hints for custom rendering. Implement a hint dispatcher.
- **Checkpoint association**: Tools should link to their checkpoint for easy revert.

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual tool detail | `--verbose` shows all | Full JSON frames | Frame inspection |
| Diff view | `rip diff --tool <id>` | JSON diff | `getToolDiff(id)` |
| Revert | `rip revert --to <cp>` | Same | `client.revert(cp)` |
| Copy output | GUI clipboard | N/A | Direct access |
