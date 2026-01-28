# Interaction Patterns

## Keybinding Philosophy

- **Vim-influenced but not vim** — `j/k` for navigation, but not full vim grammar
- **Discoverable** — `?` shows all keys, command palette shows commands
- **Consistent** — Same keys do same things across panels
- **Layered** — Globals always work, panel keys only when focused

---

## Phase 1 MVP (Implemented Today)

The current fullscreen TUI is a minimal, frame-driven client. It intentionally implements only a
small subset of the end-state keybinding philosophy.

Global keys (implemented):

| Key | Action |
|-----|--------|
| `Ctrl+C` | Quit |
| `Ctrl+D` | Quit |
| `Esc` | Close overlay (if any) |
| `Ctrl+B` | Toggle Activity overlay |
| `Ctrl+T` | Toggle tasks overlay |
| `Enter` | Submit input when idle; open detail when a session is running |
| `↑/↓` | Select previous/next frame |
| `Tab` | Toggle inspector details mode (decoded vs JSON) |
| `Ctrl+F` | Toggle auto-follow |
| `Ctrl+R` | Toggle rendered ↔ raw output view |
| `Alt+T` | Toggle theme |
| `Ctrl+Y` | Copy selected output/frame |

Notes:
- Advanced control-plane actions (compaction, cursor rotation, etc.) are not bound by default to
  avoid accidental execution; power users can bind them via `~/.rip/keybindings.json`.

---

## Global Keybindings

These work regardless of focus:

| Key | Action |
|-----|--------|
| `Ctrl+K` | Open command palette |
| `Ctrl+P` | Quick file picker (insert @file) |
| `Ctrl+B` | Toggle Activity (rail/drawer). At XS/S this is an overlay; at M it may pin as a side rail |
| `Ctrl+T` | Toggle tasks panel |
| `Ctrl+L` | Clear output |
| `Ctrl+D` | Quit (with confirmation if session active) |
| `Esc` | Close overlay / clear focus / cancel |
| `Tab` | Next panel |
| `Shift+Tab` | Previous panel |
| `?` | Help overlay |
| `F1` | Help overlay (alternative) |

---

## Navigation Keys

Used in lists, timelines, and anywhere with selectable items:

| Key | Action |
|-----|--------|
| `j` or `↓` | Next item |
| `k` or `↑` | Previous item |
| `g` | First item |
| `G` | Last item |
| `Ctrl+D` | Page down |
| `Ctrl+U` | Page up |
| `Enter` | Select / expand / open detail |
| `Space` | Toggle selection (multi-select contexts) |

---

## Panel-Specific Keys

### Timeline Panel

| Key | Action |
|-----|--------|
| `f` | Toggle auto-follow (tail mode) |
| `p` | Filter: provider events only |
| `t` | Filter: tool events only |
| `c` | Filter: checkpoint events only |
| `e` | Filter: errors only |
| `0` | Clear all filters |
| `/` | Search within frames |
| `y` | Copy selected frame as JSON |
| `Enter` | Open tool/frame detail |

### Output Panel

| Key | Action |
|-----|--------|
| `r` | Toggle raw vs rendered view |
| `w` | Toggle word wrap |
| `y` | Copy visible output |
| `/` | Search within output |

### Input Panel

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Shift+Enter` | Newline (multiline mode) |
| `Ctrl+Enter` | Send immediately (bypass queue) |
| `↑` | Previous message (history) |
| `↓` | Next message (history) |
| `Ctrl+E` | Open in external editor |
| `Ctrl+U` | Clear line |
| `Tab` | Autocomplete (context-dependent) |

### Inspector Panel

| Key | Action |
|-----|--------|
| `Tab` | Toggle JSON / decoded view |
| `y` | Copy content |
| `o` | Open in external viewer |

---

## Input Syntax

The input box supports special syntax:

### File References
```
@src/auth.ts              Include file content
@src/**/*.ts              Include glob matches
@!src/test/**             Exclude pattern
@clipboard                Include clipboard content
```

### Thread References
```
#auth                     Reference thread by tag
#thread:feat/auth         Reference by name
#turn:18                  Reference specific turn
```

### Artifact References
```
$art_8f2a3b               Reference artifact by ID
```

### Commands
```
/model gpt-4.1            Change model
/branch                   Start branch flow
/compact                  Trigger compaction
/help                     Show help
```

### Modifiers
```
!!message                 High priority (bypass queue)
??message                 Request confirmation before executing
```

### Inline Commands
```
/review: check for security issues
/model gpt-4.1: analyze this complex problem
```

---

## Command Palette

The command palette (`Ctrl+K`) provides fuzzy search across:

1. **Commands** — All slash commands
2. **Recent** — Recently used commands
3. **Threads** — Quick thread switching
4. **Files** — File references
5. **Actions** — Screen-specific actions

### Palette Behavior

```
┌─ Command Palette ─────────────────────────────────────────────┐
│ > bran█                                                       │
├───────────────────────────────────────────────────────────────┤
│ ▸ /branch           Branch from current point      Ctrl+B     │
│   /branch:handoff   Handoff with context summary             │
│   thread:branch-exp Resume "branch-experiment"               │
│ ──────────────────────────────────────────────────────────── │
│   Recent: /compact, /model gpt-4.1                           │
└───────────────────────────────────────────────────────────────┘
```

- Fuzzy matching on all text
- Results grouped by type
- Shortcuts shown on right
- Recent items at bottom
- `Enter` executes, `Tab` autocompletes, `Esc` closes

---

## Autocomplete Triggers

| Trigger | Context | Completes |
|---------|---------|-----------|
| `@` | Input | File paths |
| `/` | Input (start) | Commands |
| `#` | Input | Tags, threads |
| `$` | Input | Artifacts |
| `:` | After command | Command arguments |

### Autocomplete Popup

```
┌─ @src/au█ ────────────────────────┐
│ ▸ src/auth.ts                     │
│   src/auth/                       │
│   src/auth.test.ts                │
│   src/auth-utils.ts               │
└───────────────────────────────────┘
```

- `Tab` or `Enter` to select
- `Esc` to dismiss
- Typing continues filtering
- Arrow keys to navigate

---

## Multi-Select Patterns

Some contexts support selecting multiple items:

### Thread Browser (tagging, archiving)
```
┌─ Threads ─────────────────────────────────────────────────────┐
│ [x] feat/auth           2h ago                                │
│ [x] fix/login           5h ago                                │
│ [ ] refactor/db         1d ago                                │
│                                                               │
│ 2 selected: [a]rchive  [t]ag  [d]elete                       │
└───────────────────────────────────────────────────────────────┘
```

### Review Panel (file selection)
```
┌─ Changed Files ───────────────────────────────────────────────┐
│ [x] src/auth.ts         +24 -8                                │
│ [x] src/middleware.ts   +12 -3                                │
│ [ ] package.json        +2  -0    ← unchecked = excluded     │
└───────────────────────────────────────────────────────────────┘
```

- `Space` toggles selection
- `a` selects all
- `n` selects none
- Actions apply to selected items

---

## Vim Mode (Future)

Capability: `ui.vim_mode`

When enabled, adds vim-like editing in input:

- `i` to enter insert mode
- `Esc` to return to normal mode
- Standard vim motions (`w`, `b`, `e`, `0`, `$`)
- Basic editing (`d`, `c`, `y`, `p`)

### Considerations for Implementers
- Vim mode is additive, not replacement
- Normal TUI navigation should still work
- Mode indicator should be visible
- Consider how vim `Esc` interacts with overlay closing

---

## Interaction Feedback

Users should always know what's happening:

### Visual Feedback
- Selected items highlighted
- Focused panels have visible border change
- Loading states have spinner/indicator
- Actions confirm with brief flash or message

### Status Messages
```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│ ✓ Message sent                                    (fades after 2s)
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Error Feedback
```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│ ✗ Connection failed: timeout                     [r]etry [d]ismiss
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Accessibility Considerations

- All actions reachable by keyboard
- Color should not be only indicator (use symbols too)
- Consider screen reader compatibility for key announcements
- Consistent tab order
- Focus should be visually obvious
