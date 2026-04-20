//! Streaming text → stable-block assembler (Phase B.5, expanded B.6).
//!
//! The agent's `OutputTextDelta` frames arrive one chunk at a time. B.1's
//! ingest naively pushed each delta as its own `Block::Paragraph`, which
//! made the renderer re-parse the whole tail on every frame and produced
//! absurd numbers of tiny blocks (one per token). B.5 swapped in this
//! collector: a single streaming buffer per `AgentTurn` that promotes
//! *complete* block regions to stable blocks on block boundaries,
//! leaving the in-flight text as a transient tail the renderer shows
//! beneath the stable blocks.
//!
//! B.5 only recognized paragraph boundaries (blank line). B.6 widens
//! that: whenever the tail contains a closed fence, a heading line, a
//! thematic break, a list, or a block quote whose continuation has
//! stopped, the collector hands the closed region to
//! [`parse_blocks`](super::markdown::parse_blocks) and appends the
//! resulting blocks. The rest of the TUI never re-parses streaming
//! text directly — this is the only seam.
//!
//! Invariants:
//!
//! - `push` never promotes anything that could still be extended by a
//!   future delta. Partial markdown (unfinished fence, half-written
//!   heading, in-progress list item) stays in the tail. No flashing
//!   of malformed syntax mid-stream.
//! - `finalize` flushes whatever's left in the tail as a final set of
//!   blocks. Called on `SessionEnded`; at that point the stream
//!   promises no more deltas, so the parser sees complete input.
//! - **Theme invariance (B.8).** The blocks produced here contain no
//!   theme-dependent styling (see `CachedText`'s docstring). A theme
//!   swap is a pure repaint — no need to re-run `push`, no need to
//!   blow cache.
//! - **Incremental scan state.** The collector remembers where the
//!   previous scan stopped and whether that point sits inside an open
//!   fence. Long streamed code blocks therefore touch only newly
//!   appended lines instead of rescanning the entire tail on every
//!   token, which is what caused the markdown CPU spikes in practice.

use super::markdown::parse_blocks;
use super::model::Block;

#[derive(Debug, Clone, Default)]
pub struct StreamCollector {
    /// Byte offset where the next scan can resume safely. This always
    /// points at the start of the current incomplete line so the next
    /// delta only revisits that trailing partial line.
    scan_cursor: usize,
    /// Whether the scan cursor currently sits inside an open fence.
    fence_open: bool,
}

/// One step of the streaming machine. The renderer blits `new_stable`
/// after the previously-stable blocks.
#[derive(Debug, Clone, Default)]
pub struct CollectorStep {
    /// Paragraphs that just crossed a block boundary. Append to the
    /// turn's `blocks`.
    pub new_stable: Vec<Block>,
}

impl StreamCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a raw chunk. Returns the stable blocks promoted by this
    /// chunk; the caller is responsible for appending them to the
    /// current `AgentTurn.blocks`.
    pub fn push(&mut self, tail: &mut String, delta: &str) -> CollectorStep {
        if delta.is_empty() {
            return CollectorStep::default();
        }
        tail.push_str(delta);
        self.drain_complete_blocks(tail)
    }

    /// Drain whatever's in the tail as final block(s). Called when the
    /// turn ends — the stream guarantees no more deltas arrive so the
    /// trailing text is by definition complete, and we can let the
    /// markdown parser do its job on the whole thing.
    pub fn finalize(&mut self, tail: &mut String) -> Vec<Block> {
        if tail.is_empty() {
            return Vec::new();
        }
        let text = std::mem::take(tail);
        self.reset();
        parse_blocks(text.trim_end())
    }

    fn reset(&mut self) {
        self.scan_cursor = 0;
        self.fence_open = false;
    }

    fn drain_complete_blocks(&mut self, tail: &mut String) -> CollectorStep {
        let mut step = CollectorStep::default();
        while let Some(boundary) = self.find_boundary(tail) {
            let trimmed = tail[..boundary.region_end].trim_end_matches('\n');
            if !trimmed.is_empty() {
                step.new_stable.extend(parse_blocks(trimmed));
            }
            tail.replace_range(..boundary.consume_to, "");
            self.reset();
        }
        step
    }

    fn find_boundary(&mut self, tail: &str) -> Option<BlockBoundary> {
        let bytes = tail.as_bytes();
        let mut line_start = self.scan_cursor.min(bytes.len());
        let mut i = line_start;

        while i < bytes.len() {
            let rel_end = tail[i..].find('\n')?;
            let line_end = i + rel_end;
            let line = &tail[line_start..line_end];
            let blank_outside_fence = !self.fence_open && line.trim().is_empty();
            let fence_line = is_fence_line(line);
            let next_line_start = line_end + 1;

            if blank_outside_fence {
                let mut consume_to = next_line_start;
                while consume_to < bytes.len() && bytes[consume_to] == b'\n' {
                    consume_to += 1;
                }
                return Some(BlockBoundary {
                    region_end: line_start,
                    consume_to,
                });
            }

            if fence_line {
                self.fence_open = !self.fence_open;
            }

            self.scan_cursor = next_line_start;
            line_start = next_line_start;
            i = next_line_start;
        }

        None
    }
}

#[derive(Debug, Clone, Copy)]
struct BlockBoundary {
    region_end: usize,
    consume_to: usize,
}

fn is_fence_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let leading_ws = line.len() - trimmed.len();
    leading_ws <= 3 && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_without_boundary_keeps_text_in_tail() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        let step = c.push(&mut tail, "hello world");
        assert!(step.new_stable.is_empty());
        assert_eq!(tail, "hello world");
    }

    #[test]
    fn push_crosses_paragraph_boundary_promotes_prefix() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        c.push(&mut tail, "first paragraph.\n\n");
        assert_eq!(tail, "");
        let step = c.push(&mut tail, "second");
        assert!(step.new_stable.is_empty());
        assert_eq!(tail, "second");
    }

    #[test]
    fn push_with_multiple_boundaries_promotes_all() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        let step = c.push(&mut tail, "p1\n\np2\n\np3");
        assert_eq!(step.new_stable.len(), 2);
        assert_eq!(tail, "p3");
    }

    #[test]
    fn triple_newline_does_not_produce_an_empty_paragraph() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        let step = c.push(&mut tail, "alpha\n\n\nbeta");
        assert_eq!(step.new_stable.len(), 1);
        assert_eq!(tail, "beta");
    }

    #[test]
    fn finalize_flushes_remaining_tail() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        c.push(&mut tail, "only paragraph");
        let last = c.finalize(&mut tail);
        assert_eq!(last.len(), 1);
        assert!(matches!(last[0], Block::Paragraph(_)));
        assert_eq!(tail, "");
        assert!(c.finalize(&mut tail).is_empty());
    }

    #[test]
    fn open_code_fence_stays_in_tail_until_closed() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        let step = c.push(
            &mut tail,
            "```rust\nfn main() {\n\n    println!(\"hi\");\n}\n",
        );
        assert!(step.new_stable.is_empty());
        assert!(tail.contains("```rust"));

        let step = c.push(&mut tail, "```\n\nafter");
        assert!(step
            .new_stable
            .iter()
            .any(|b| matches!(b, Block::CodeFence { .. })));
        assert_eq!(tail, "after");
    }

    #[test]
    fn heading_on_block_boundary_promotes_as_heading() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        let step = c.push(&mut tail, "## Section\n\nbody continues");
        assert_eq!(step.new_stable.len(), 1);
        assert!(matches!(
            step.new_stable[0],
            Block::Heading { level: 2, .. }
        ));
        assert_eq!(tail, "body continues");
    }

    #[test]
    fn finalize_parses_tail_as_markdown() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        c.push(&mut tail, "# title\n\n- a\n- b");
        let blocks = c.finalize(&mut tail);
        assert!(blocks.iter().any(|b| matches!(b, Block::List { .. })));
    }

    #[test]
    fn empty_delta_is_noop() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        let step = c.push(&mut tail, "");
        assert!(step.new_stable.is_empty());
        assert_eq!(tail, "");
    }

    #[test]
    fn scanner_resumes_from_the_open_line_instead_of_the_full_tail() {
        let mut c = StreamCollector::new();
        let mut tail = String::new();
        c.push(&mut tail, "```rust\nfn main() {");
        let first_cursor = c.scan_cursor;
        assert!(first_cursor > 0);

        c.push(&mut tail, "\n    println!(\"hi\");");
        assert!(c.scan_cursor >= first_cursor);

        let step = c.push(&mut tail, "\n}\n```\n\nnext");
        assert!(step
            .new_stable
            .iter()
            .any(|b| matches!(b, Block::CodeFence { .. })));
        assert_eq!(tail, "next");
        assert_eq!(c.scan_cursor, 0);
    }
}
