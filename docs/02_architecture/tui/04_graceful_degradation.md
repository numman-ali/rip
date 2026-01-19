# Graceful Degradation

How TUI-specific features map to simpler surfaces. The TUI is the richest presentation layer, but every capability must have equivalents in CLI, headless, and SDK modes.

---

## Degradation Philosophy

1. **Information parity** — All surfaces can access the same data, even if presentation differs
2. **Action parity** — Every action in TUI has a CLI/SDK equivalent
3. **No TUI-only capabilities** — TUI visualizes capabilities; it doesn't create them
4. **Progressive richness** — TUI adds visual affordances, but core functionality works everywhere

---

## Degradation Matrix

| TUI Feature | CLI Interactive | CLI Headless | SDK |
|-------------|-----------------|--------------|-----|
| **Visual Elements** |
| Syntax-highlighted diff | ANSI-colored diff | Plain unified diff | Structured diff object |
| ASCII thread graph | Indented tree text | JSON tree structure | Tree data structure |
| Progress bars | Percentage text | JSON progress events | Progress callback |
| Spinner animations | Static "working..." | No output (or JSON status) | Status polling |
| Color-coded frame types | ANSI colors or prefixes | JSON with `type` field | Typed objects |
| **Interactive Elements** |
| Command palette | Tab completion + `/commands` | N/A (use flags) | Method calls |
| File autocomplete (`@`) | Tab completion | `--context file` flag | Context parameter |
| Multi-select checkboxes | Comma-separated args | Array flags | Array parameters |
| Confirmation dialogs | Y/N prompts | `--yes` flag or error | Return value/exception |
| **Real-time Elements** |
| Live streaming output | Streaming to stdout | JSON line stream | Event iterator/callback |
| Auto-follow (tail) mode | Default behavior | Line-buffered output | Stream consumption |
| Split panel layout | Sequential output | Separate JSON objects | Separate API calls |
| **Navigation** |
| Screen transitions | Subcommands | N/A | Separate methods |
| Focus cycling | N/A (single flow) | N/A | N/A |
| Overlay modals | Inline prompts | Flags or separate commands | Method parameters |

---

## Feature-by-Feature Degradation

### Timeline / Frame Display

| TUI | Degradation |
|-----|-------------|
| Virtualized scrolling list | CLI: `--limit N` pagination; SDK: iterator |
| Frame type filtering | CLI: `--filter type=tool`; SDK: filter parameter |
| Visual selection marker | CLI: N/A; SDK: programmatic access |
| Time delta column | CLI: timestamp in output; SDK: timestamp field |

### Output Panel

| TUI | Degradation |
|-----|-------------|
| Streamed text with cursor | CLI: line-by-line output; SDK: delta events |
| Markdown rendering | CLI: plain text; SDK: raw markdown |
| Collapsed sections | CLI: full output; SDK: full data |
| Render hints (syntax, diff) | CLI: ANSI formatting; Headless: raw; SDK: structured |

### Tool Detail

| TUI | Degradation |
|-----|-------------|
| Modal overlay | CLI: `rip tool <id>`; SDK: `getToolDetail(id)` |
| Diff view with highlighting | CLI: `rip diff --tool <id>`; SDK: diff object |
| Revert button | CLI: `rip revert --to <cp>`; SDK: `revert(cp)` |

### Thread Browser

| TUI | Degradation |
|-----|-------------|
| Search with preview | CLI: `rip threads --search`; SDK: `searchThreads(q)` |
| Tag filtering | CLI: `rip threads --tag`; SDK: filter parameter |
| Multi-select actions | CLI: `rip thread archive <id> <id>`; SDK: batch method |

### Command Palette

| TUI | Degradation |
|-----|-------------|
| Fuzzy search | CLI: `rip help --search`; SDK: N/A (direct calls) |
| Recent commands | CLI: shell history; SDK: N/A |
| Inline preview | CLI: `rip help <command>`; SDK: N/A |

### Permissions

| TUI | Degradation |
|-----|-------------|
| Visual modal with options | CLI: interactive prompt; Headless: `--allow` flags |
| Edit command before approve | CLI: prompt allows editing; SDK: modify before call |
| Remember choice checkbox | CLI: `--allow-session`; SDK: policy configuration |

### Background Tasks

| TUI | Degradation |
|-----|-------------|
| Live task panel | CLI: `rip tasks --watch`; SDK: event stream |
| Inline progress | CLI: progress to stderr; SDK: progress events |
| Cancel button | CLI: `rip task cancel <id>`; SDK: `cancelTask(id)` |

---

## Headless-Specific Rules

Headless mode (`--headless` or `rip run --json`) has additional constraints:

1. **No interactivity** — All inputs via flags; no prompts
2. **JSON output** — Every line is valid JSON (JSONL)
3. **Exit codes** — Success = 0, failure = non-zero
4. **Permissions** — Must be pre-approved via flags (`--allow`, `--allow-tools`)
5. **No color** — Unless explicitly requested

---

## SDK-Specific Rules

SDK consumers get structured data, not presentation:

1. **No formatting** — Raw data structures, not formatted strings
2. **Callbacks over visuals** — Progress via callbacks, not display
3. **Full access** — Can access all frame data, not just rendered summary
4. **Stateless** — Each call is independent; no "current selection" state

---

## TUI-Only Affordances

Some things only make sense in TUI:

| TUI Feature | Why TUI-only |
|-------------|--------------|
| Focus rings | Visual navigation aid |
| Keyboard shortcuts display | Context-sensitive hints |
| Panel resize | Layout preference |
| Animation/transitions | Visual polish |
| Vim mode | Editor emulation |

These don't need parity — they're presentation enhancements, not capabilities.

---

## Testing Degradation

For each screen, verify:

1. **Info available**: Can CLI/SDK get the same information?
2. **Action possible**: Can CLI/SDK perform the same action?
3. **Automation works**: Can a script do what the TUI does?

If any answer is "no", either:
- Add the missing CLI/SDK capability, or
- Document as intentional TUI-only enhancement
