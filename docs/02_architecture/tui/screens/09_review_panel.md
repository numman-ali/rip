# Review Panel

## Purpose

Batch review of all changes made during a session. Allows selective acceptance, rejection, or reversion of agent modifications before committing.

## Entry Conditions

- User invokes `/review` command
- Session ends with uncommitted changes
- User presses review shortcut from Live Session

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `ui.review` | Review interface |
| `checkpoint.rewind` | Revert changes |
| `ui.undo` | Undo actions |
| `checkpoint.auto` | Change tracking |

---

## Wireframe

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│ Review Changes                                             [Esc] close  [a] approve │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│ ┌─ Summary ─────────────────────────────────────────────────────────────────────┐  │
│ │                                                                               │  │
│ │  Session: feat/auth (turns 1-24)                                              │  │
│ │  Files changed: 4        Lines: +89 -23        Checkpoints: 12               │  │
│ │                                                                               │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ ┌─ Changed Files ───────────────────────────────────────────────────────────────┐  │
│ │                                                                               │  │
│ │  [x] src/auth.ts               +45 -12   modified   cp_8, cp_12, cp_18       │  │
│ │  [x] src/middleware.ts         +23 -8    modified   cp_15                    │  │
│ │  [x] src/types/auth.ts         +18 -0    new file   cp_12                    │  │
│ │  [ ] tests/auth.test.ts        +3  -3    modified   cp_20      ← excluded   │  │
│ │                                                                               │  │
│ │  [Space] toggle    [a] select all    [n] select none                         │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ ┌─ src/auth.ts ─────────────────────────────────────────────────────────────────┐  │
│ │                                                                               │  │
│ │  Checkpoint cp_8 (turn 5):                                                    │  │
│ │  ─────────────────────────────────────────────────────────────────────────    │  │
│ │   45 │+  import { sanitize } from './utils';                                  │  │
│ │   46 │+  import { hash } from './crypto';                                     │  │
│ │                                                                               │  │
│ │  Checkpoint cp_12 (turn 12):                                                  │  │
│ │  ─────────────────────────────────────────────────────────────────────────    │  │
│ │   52 │-    return db.query(user, pass);                                       │  │
│ │   52 │+    const sanitized = sanitize(user);                                  │  │
│ │   53 │+    if (!sanitized) {                                                  │  │
│ │   54 │+      throw new AuthError('invalid_username');                         │  │
│ │   55 │+    }                                                                  │  │
│ │   56 │+    return db.query(sanitized, hash(pass));                            │  │
│ │                                                                               │  │
│ │  Checkpoint cp_18 (turn 18):                                                  │  │
│ │  ─────────────────────────────────────────────────────────────────────────    │  │
│ │   78 │+  export async function refreshToken(token: string) {                  │  │
│ │   79 │+    // ... (12 more lines)                                             │  │
│ │                                                                               │  │
│ │  [j/k] scroll    [Enter] expand checkpoint    [r] revert this file           │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ [Enter] approve selected   [r] revert selected   [R] revert all   [e] export diff  │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Sections

### Summary Bar

| Field | Description |
|-------|-------------|
| Session | Thread name and turn range |
| Files changed | Count of modified files |
| Lines | Total additions/deletions |
| Checkpoints | Number of checkpoints in range |

### File List

| Column | Description |
|--------|-------------|
| Checkbox | Include in approval (toggle with Space) |
| Filename | Relative path |
| +/- | Lines added/removed |
| Status | new/modified/deleted |
| Checkpoints | Which checkpoints touched this file |

### Diff View

Shows changes grouped by checkpoint:
- Checkpoint header with turn number
- Unified diff for that checkpoint's changes
- Expandable for full context

---

## File Selection

```
┌─ Changed Files ─────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  [x] src/auth.ts               +45 -12   modified                              │
│  [x] src/middleware.ts         +23 -8    modified                              │
│  [ ] package.json              +2  -0    modified    ← user excluded          │
│  [x] src/types/auth.ts         +18 -0    new file                              │
│                                                                                 │
│  3 of 4 files selected                                                         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

- `Space` toggles individual file
- `a` selects all
- `n` deselects all
- Excluded files won't be in final approval

---

## Checkpoint Navigation

```
┌─ src/auth.ts (3 checkpoints) ───────────────────────────────────────────────────┐
│                                                                                 │
│  ▸ cp_8  (turn 5)   "Add imports for auth utilities"           +2 lines       │
│    cp_12 (turn 12)  "Implement input sanitization"             +5 -1 lines    │
│    cp_18 (turn 18)  "Add refresh token function"               +38 lines      │
│                                                                                 │
│  [Enter] expand    [r] revert to before this checkpoint                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Revert Confirmation

When reverting a file:

```
┌─ Revert File ───────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  Revert src/auth.ts to state before cp_8?                                      │
│                                                                                 │
│  This will undo:                                                                │
│    • cp_8:  Add imports for auth utilities (+2 lines)                          │
│    • cp_12: Implement input sanitization (+5 -1 lines)                         │
│    • cp_18: Add refresh token function (+38 lines)                             │
│                                                                                 │
│  Total: -45 lines, +1 line                                                     │
│                                                                                 │
│  [y] Yes, revert    [n] Cancel                                                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Partial Checkpoint Revert

Revert to a specific checkpoint (not all):

```
┌─ Partial Revert ────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  Revert src/auth.ts to state after cp_12?                                      │
│                                                                                 │
│  Keep:                                                                          │
│    ✓ cp_8:  Add imports for auth utilities                                     │
│    ✓ cp_12: Implement input sanitization                                       │
│                                                                                 │
│  Undo:                                                                          │
│    ✗ cp_18: Add refresh token function (-38 lines)                             │
│                                                                                 │
│  [y] Yes, revert    [n] Cancel                                                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Export Options

```
┌─ Export Diff ───────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  Format:                                                                        │
│    [u] Unified diff (.patch)                                                    │
│    [g] Git patch (with metadata)                                               │
│    [j] JSON (structured)                                                        │
│    [h] HTML (viewable)                                                          │
│                                                                                 │
│  Include:                                                                       │
│    [x] Selected files only                                                      │
│    [ ] All changed files                                                        │
│                                                                                 │
│  Destination: ~/reviews/feat-auth-2024-01-15.patch█                            │
│                                                                                 │
│  [Enter] export    [Esc] cancel                                                │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Actions

| Key | Action | Context |
|-----|--------|---------|
| `Space` | Toggle file | File list |
| `a` | Select all | File list |
| `n` | Select none | File list |
| `j/k` | Navigate | Files or diff |
| `Enter` | Expand/approve | Checkpoint or final |
| `r` | Revert file | Selected file |
| `R` | Revert all | Confirmation |
| `e` | Export | Export dialog |
| `Esc` | Close | Return to session |

---

## Considerations for Implementers

- **Diff computation**: May need to compute diffs from checkpoints on demand.
- **Large changesets**: Consider pagination for sessions with many changes.
- **Checkpoint dependencies**: Some changes may depend on earlier checkpoints.
- **Git integration**: Consider git status/staging integration.

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual review | `rip review` | `--changes --json` | `client.getChanges()` |
| File selection | Interactive | `--files a,b,c` | Filter param |
| Revert | `[r]` key | `rip revert --file` | `client.revert(file)` |
| Export | Dialog | `rip diff --export` | `client.exportDiff()` |
