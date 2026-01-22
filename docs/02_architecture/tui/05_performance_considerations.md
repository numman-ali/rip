# Performance Considerations

Design-level performance requirements for the TUI. These are not implementation prescriptions — they describe what needs to feel fast and what scale the UI should handle.

---

## Guiding Principle

**Instant feedback, bounded resources.**

Users should never wait for the UI. The TUI should handle large sessions without degradation. If something can't be instant, show progress.

---

## Response Time Expectations

| Interaction | Expected Response | Notes |
|-------------|-------------------|-------|
| Keystroke → visual feedback | < 16ms (60fps) | Focus changes, selection moves |
| Panel switch / focus cycle | < 16ms | Instant |
| Command palette open | < 50ms | Should feel instant |
| Frame arrives → displayed | < 50ms | Real-time streaming |
| Filter toggle | < 100ms | May recompute visible set |
| Search (local) | < 100ms | Fuzzy match over frames |
| Search (remote/threads) | < 500ms | Network; show loading |
| Screen transition | < 100ms | Clear previous, show new |
| Help overlay | < 50ms | Preloaded |

---

## Scale Expectations

The TUI should handle these volumes without degradation:

| Dimension | Expected Scale | Constraint |
|-----------|---------------|------------|
| Frames in session | 10,000+ | May need windowing/virtualization |
| Lines of tool output | 100,000+ | Definitely needs virtualization |
| Threads in browser | 1,000+ | Virtualized list |
| Files in autocomplete | 10,000+ | Fuzzy search must stay fast |
| Artifacts in session | 100+ | List, not all loaded |
| Concurrent background tasks | 10+ | Visible; output selectable |
| Characters in output | 1MB+ | Ring buffer or truncation |

---

## What Must Feel Instant

These interactions are on the critical path for user experience:

### Always Instant
- Key press → cursor move
- Selection change in any list
- Focus cycle between panels
- Toggle filter (show/hide frame types)
- Scroll in any panel
- Dismiss overlay

### Should Feel Instant
- Command palette search (local)
- File autocomplete typing
- Frame detail expansion
- Help overlay

### Can Show Loading
- Thread search (if remote)
- Artifact fetch (if large)
- Summary generation (LLM call)
- Reconnection

---

## Bounded Resources

The TUI should not grow unboundedly. Consider caps for:

| Resource | Consideration |
|----------|---------------|
| Frame history | Keep last N frames in memory; older can be fetched |
| Output text | Cap rendered output; full available via artifact |
| Undo/history | Bounded history stack |
| Autocomplete results | Cap visible suggestions |
| Notification queue | Auto-dismiss or cap |

Specific numbers are implementation decisions — the design just requires *that* bounds exist.

---

## Real-Time Update Handling

During active streaming:

### Frame Arrival
- New frames should appear without jarring the user
- Auto-follow mode: scroll to bottom automatically
- Manual mode: don't move viewport; show "N new frames" indicator
- Consider batching rapid frames for display (render at 60fps, not per-frame)

### Output Streaming
- Text appears character-by-character or line-by-line
- User can scroll up without losing position when new text arrives
- Consider "pause" mode to freeze output for reading

### Task Progress
- Progress indicators update smoothly
- Don't overwhelm with updates (throttle to ~10fps for progress bars)

---

## Large Content Handling

When content exceeds reasonable display:

### Long Tool Output
- Show truncated preview in Tool Detail
- "View full output" opens Artifact Viewer
- Artifact Viewer can page through large content

### Many Frames
- Timeline virtualizes: only render visible rows
- Filter aggressively: most users want subset
- Jump-to-end and jump-to-start are critical

### Large Diffs
- Collapse unchanged sections
- Show summary first, expand on demand
- Consider side-by-side vs unified based on width

### Many Threads
- Virtual list in browser
- Search is primary navigation, not scrolling
- Preview loads on selection, not upfront

---

## Considerations for Implementers

These are flags, not requirements:

### Rendering
- Consider double-buffering or diff-based updates
- Ratatui handles much of this, but be aware of cost
- Avoid full redraws when partial update suffices

### Data Structures
- Ring buffers for bounded history
- Indices for fast filtering (by type, by tool_id)
- Lazy loading for large artifacts

### Network
- Don't block UI on network calls
- Show loading states for any fetch > 100ms
- Consider optimistic UI for actions

### Memory
- Set explicit caps rather than growing forever
- Profile with large sessions during development

### Testing
- Include benchmarks for scroll performance with 10k frames
- Test autocomplete with 10k files
- Test reconnect with large replay

---

## Non-Goals

Things that are explicitly *not* performance requirements:

- **Sub-millisecond response** — 16ms (60fps) is the target, not faster
- **Unlimited history** — Bounded is fine; users can access logs
- **Offline-first** — TUI assumes server connection
- **Pixel-perfect on every constrained device** — Phone/SSH/web terminals are supported, but the goal is usability + graceful degradation, not perfection
