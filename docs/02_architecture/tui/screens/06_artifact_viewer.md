# Artifact Viewer

Status: **Sketch** | Phase: 2

This screen doc is conceptual. Canonical UX gates are the journey specs in `docs/02_architecture/tui/journeys/` plus [Canvas + X-ray](../07_canvas_and_xray.md).

## Purpose

View large tool outputs that were stored as artifacts rather than inline in frames. Provides search, navigation, and export for potentially very large content.

## Entry Conditions

- User selects artifact from Artifacts panel
- User presses `a` in Tool Detail for truncated output
- Direct navigation via artifact reference

## Capabilities Used

| Capability | Usage |
|------------|-------|
| `tool.output_fetch` | Retrieve artifact content |
| `context.refs.artifact` | Reference artifacts in prompts |
| `tool.output_store` | Artifact metadata |

---

## Wireframe

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│ Artifact: test_output.log                                      [Esc] close         │
├─────────────────────────────────────────────────────────────────────────────────────┤
│ ┌─ Metadata ────────────────────────────────────────────────────────────────────┐  │
│ │                                                                               │  │
│ │  ID:        art_8f2a3b4c               Size:     147.2 KB                    │  │
│ │  Created:   Frame #127 (tool_ended)    Lines:    2,847                       │  │
│ │  Digest:    sha256:7a3f8b2c...         Type:     text/plain                  │  │
│ │                                                                               │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ ┌─ Search ──────────────────────────────────────────────────────────────────────┐  │
│ │ > FAIL█                                                    [3 matches found] │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ ┌─ Content ─────────────────────────────────────────────────────────────────────┐  │
│ │      1 │ Running tests...                                                    │  │
│ │      2 │                                                                     │  │
│ │      3 │ PASS src/auth.test.ts                                               │  │
│ │      4 │   ✓ login with valid credentials (45ms)                             │  │
│ │      5 │   ✓ login with invalid credentials (12ms)                           │  │
│ │      6 │   ✓ refresh token flow (89ms)                                       │  │
│ │      7 │                                                                     │  │
│ │      8 │ PASS src/middleware.test.ts                                         │  │
│ │      9 │   ✓ auth middleware blocks unauthenticated (8ms)                    │  │
│ │     10 │   ✓ auth middleware passes valid tokens (5ms)                       │  │
│ │     11 │                                                                     │  │
│ │    ... │                                                                     │  │
│ │   1847 │ FAIL src/edge-cases.test.ts                         ◀ match 1/3    │  │
│ │   1848 │   ✗ handles null input (23ms)                                       │  │
│ │   1849 │                                                                     │  │
│ │    ... │                                                                     │  │
│ │   2847 │ Test Suites: 45 passed, 3 failed, 48 total                         │  │
│ │                                                                              │  │
│ │ ──────────────────────────────────────────────────────────────────────────── │  │
│ │ Line 1847 of 2847                                              [65%] ████░░ │  │
│ └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
│ [j/k] scroll  [g/G] top/bottom  [/] search  [n/N] next/prev match  [s] save  [y] copy│
└─────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Sections

### Metadata Bar

| Field | Description |
|-------|-------------|
| ID | Artifact identifier (for referencing) |
| Created | Source frame |
| Digest | Content hash (for verification) |
| Size | File size |
| Lines | Line count |
| Type | MIME type or content type |

### Search Bar

- Incremental search as you type
- Match count displayed
- Highlights matches in content
- `n`/`N` to navigate between matches

### Content Area

- Line numbers
- Scrollable with position indicator
- Progress bar shows position in file
- Match highlighting
- Current line indicator

---

## Large File Handling

For very large artifacts (>1MB):

```
┌─ Content (streaming) ───────────────────────────────────────────────────────────┐
│                                                                                 │
│  ⚠ Large file (12.4 MB). Loading in chunks...                                  │
│                                                                                 │
│  Loaded: lines 1-1000                                    [Load more ↓]         │
│                                                                                 │
│      1 │ ...                                                                   │
│    ... │                                                                       │
│   1000 │ ...                                                                   │
│                                                                                 │
│  [Enter] to load next 1000 lines                                               │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Jump to Line

Press `:` (or `Ctrl+G`) to jump:

```
┌─ Go to Line ──────────────────────────────────────────────────────────────────┐
│                                                                               │
│  Line number: 1847█                                                          │
│                                                                               │
│  [Enter] go    [Esc] cancel                                                  │
└───────────────────────────────────────────────────────────────────────────────┘
```

---

## Different Content Types

### Binary/Hex View

```
┌─ Content (hex) ─────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  Offset    │ 00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F │ ASCII           │
│  ──────────┼─────────────────────────────────────────────────┼───────────────  │
│  00000000  │ 89 50 4E 47 0D 0A 1A 0A 00 00 00 0D 49 48 44 52 │ .PNG........IHDR │
│  00000010  │ 00 00 02 00 00 00 02 00 08 06 00 00 00 F4 78 D4 │ ..............x. │
│  00000020  │ FA 00 00 00 09 70 48 59 73 00 00 0B 13 00 00 0B │ .....pHYs....... │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### JSON View (Pretty)

```
┌─ Content (JSON) ────────────────────────────────────────────────────────────────┐
│                                                                                 │
│  {                                                                              │
│    "testResults": {                                                             │
│      "passed": 45,                                                              │
│      "failed": 3,                                                               │
│      "skipped": 0                                                               │
│    },                                                                           │
│    "coverage": {                                                                │
│      "lines": 87.3,                                                             │
│      "branches": 72.1,                                                          │
│  ▾   "functions": 91.2                                                          │  ← collapsible
│    }                                                                            │
│  }                                                                              │
│                                                                                 │
│  [f] toggle fold    [Tab] collapse all                                         │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Image Reference

```
┌─ Content (image) ───────────────────────────────────────────────────────────────┐
│                                                                                 │
│                     ┌─────────────────────────────────────┐                    │
│                     │                                     │                    │
│                     │           [Image Preview]           │                    │
│                     │                                     │                    │
│                     │         screenshot.png              │                    │
│                     │         1920 x 1080                 │                    │
│                     │                                     │                    │
│                     └─────────────────────────────────────┘                    │
│                                                                                 │
│  Note: Full image viewing requires external viewer                              │
│                                                                                 │
│  [o] open in external viewer    [s] save to file                               │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Actions

| Key | Action | Effect |
|-----|--------|--------|
| `Esc` | Close | Return to previous screen |
| `j/k` | Scroll | Line by line |
| `Ctrl+D/U` | Page | Half-page scroll |
| `g` | Top | Jump to start |
| `G` | Bottom | Jump to end |
| `:` or `Ctrl+G` | Go to line | Jump to specific line |
| `/` | Search | Start search |
| `n` | Next match | Jump to next search match |
| `N` | Prev match | Jump to previous match |
| `s` | Save | Save artifact to file |
| `y` | Copy | Copy visible content or selection |
| `o` | Open external | Open in system viewer |
| `r` | Reference | Copy artifact reference for input |

---

## Considerations for Implementers

- **Memory management**: Don't load entire large files into memory. Stream/chunk as needed.
- **Search in large files**: Consider indexed search for very large artifacts.
- **Encoding detection**: Handle different text encodings gracefully.
- **Binary detection**: Detect and switch to hex view for binary content.

---

## Surface Parity

| TUI | CLI | Headless | SDK |
|-----|-----|----------|-----|
| Visual viewer | `rip artifact <id>` | Raw bytes | `client.getArtifact(id)` |
| Search | Interactive | `grep` on output | Programmatic |
| Save | GUI dialog | `--output file` | `artifact.saveTo(path)` |
| Reference | Copy $id | Direct use | Direct use |
