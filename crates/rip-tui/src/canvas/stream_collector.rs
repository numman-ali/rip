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

use super::markdown::parse_blocks;
use super::model::Block;

#[derive(Debug, Clone, Default)]
pub struct StreamCollector {
    /// Not-yet-terminated raw chunk. The renderer shows this as a
    /// transient paragraph beneath any stable blocks.
    tail: String,
}

/// One step of the streaming machine. The renderer blits `new_stable`
/// after the previously-stable blocks; `tail` replaces the previous
/// transient row.
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

    /// Rehydrate a collector from an existing tail. Used by
    /// `ingest.rs` — each `OutputTextDelta` rebuilds the collector from
    /// the tail stored on the `AgentTurn`, pushes the delta, and writes
    /// the new tail back. This keeps `CanvasMessage` owning the tail
    /// string (so `Clone`/serialization stays trivial) while still
    /// reusing the collector's paragraph-splitting logic.
    pub fn from_tail(tail: String) -> Self {
        Self { tail }
    }

    /// Consume the collector and return its tail. Pair with
    /// [`StreamCollector::from_tail`] to round-trip through an owner.
    pub fn into_tail(self) -> String {
        self.tail
    }

    /// Feed a raw chunk. Returns the stable blocks promoted by this
    /// chunk; the caller is responsible for appending them to the
    /// current `AgentTurn.blocks`.
    pub fn push(&mut self, delta: &str) -> CollectorStep {
        if delta.is_empty() {
            return CollectorStep::default();
        }
        self.tail.push_str(delta);
        self.drain_complete_paragraphs()
    }

    /// Drain whatever's in the tail as final block(s). Called when the
    /// turn ends — the stream guarantees no more deltas arrive so the
    /// trailing text is by definition complete, and we can let the
    /// markdown parser do its job on the whole thing.
    pub fn finalize(&mut self) -> Vec<Block> {
        if self.tail.is_empty() {
            return Vec::new();
        }
        let text = std::mem::take(&mut self.tail);
        parse_blocks(text.trim_end())
    }

    /// Peek the transient tail — the renderer shows this beneath the
    /// stable blocks so streaming feels continuous. Empty when there's
    /// nothing in flight.
    pub fn tail(&self) -> &str {
        &self.tail
    }

    fn drain_complete_paragraphs(&mut self) -> CollectorStep {
        let mut step = CollectorStep::default();
        // A block boundary is any run of ≥2 newlines outside an open
        // fence. We split on the first such boundary, feed the prefix
        // to the markdown parser, and append the resulting blocks.
        // The parser handles headings, lists, quotes, and fences
        // internally — we just need to make sure we don't cut a fence
        // in half.
        while let Some((region, rest)) = split_at_block_boundary(&self.tail) {
            let trimmed = region.trim_end_matches('\n');
            if !trimmed.is_empty() {
                step.new_stable.extend(parse_blocks(trimmed));
            }
            self.tail = rest;
        }
        step
    }
}

/// Count the number of unclosed code fences (``` or ~~~) in `tail`.
/// Used to avoid promoting a region whose fence is still open — the
/// renderer shows the unclosed fence as transient tail text until it
/// closes.
fn fence_is_open(tail: &str) -> bool {
    let mut in_fence = false;
    for line in tail.lines() {
        let trimmed = line.trim_start();
        let leading_ws = line.len() - trimmed.len();
        // Fences per CommonMark: ≥3 backticks or tildes, up to 3
        // spaces of leading indentation. We don't need full fidelity;
        // we just need to match the parser's behavior well enough to
        // keep open fences in the tail.
        if leading_ws <= 3 && (trimmed.starts_with("```") || trimmed.starts_with("~~~")) {
            in_fence = !in_fence;
        }
    }
    in_fence
}

/// Split the tail at its first block boundary (blank line, i.e. two
/// or more consecutive newlines) that does *not* fall inside an open
/// code fence. Returns `(region, remaining_tail)` with the region
/// *not* including the trailing newlines and the remaining tail
/// starting immediately after them. `None` when no such boundary
/// exists yet — meaning the tail is still in flight.
fn split_at_block_boundary(tail: &str) -> Option<(String, String)> {
    let bytes = tail.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\n' && bytes[i + 1] == b'\n' {
            // If splitting here would leave an unclosed fence in the
            // prefix, treat the blank line as fence-interior and keep
            // scanning.
            if fence_is_open(&tail[..i]) {
                i += 1;
                continue;
            }
            // Walk forward past any extra newlines so the boundary is
            // fully consumed from the tail (avoids producing empty
            // regions on triple-newlines).
            let mut end = i + 2;
            while end < bytes.len() && bytes[end] == b'\n' {
                end += 1;
            }
            return Some((tail[..i].to_string(), tail[end..].to_string()));
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_without_boundary_keeps_text_in_tail() {
        let mut c = StreamCollector::new();
        let step = c.push("hello world");
        assert!(step.new_stable.is_empty());
        assert_eq!(c.tail(), "hello world");
    }

    #[test]
    fn push_crosses_paragraph_boundary_promotes_prefix() {
        let mut c = StreamCollector::new();
        c.push("first paragraph.\n\n");
        assert_eq!(c.tail(), "");
        let step = c.push("second");
        assert!(step.new_stable.is_empty());
        assert_eq!(c.tail(), "second");
    }

    #[test]
    fn push_with_multiple_boundaries_promotes_all() {
        let mut c = StreamCollector::new();
        let step = c.push("p1\n\np2\n\np3");
        assert_eq!(step.new_stable.len(), 2);
        assert_eq!(c.tail(), "p3");
    }

    #[test]
    fn triple_newline_does_not_produce_an_empty_paragraph() {
        let mut c = StreamCollector::new();
        let step = c.push("alpha\n\n\nbeta");
        assert_eq!(step.new_stable.len(), 1);
        assert_eq!(c.tail(), "beta");
    }

    #[test]
    fn finalize_flushes_remaining_tail() {
        let mut c = StreamCollector::new();
        c.push("only paragraph");
        let last = c.finalize();
        assert_eq!(last.len(), 1);
        assert!(matches!(last[0], Block::Paragraph(_)));
        assert_eq!(c.tail(), "");
        assert!(c.finalize().is_empty());
    }

    #[test]
    fn open_code_fence_stays_in_tail_until_closed() {
        // A fenced block with a blank line INSIDE the fence must not
        // be promoted until the closing ``` arrives — otherwise the
        // renderer would flash a half-fenced paragraph mid-stream.
        let mut c = StreamCollector::new();
        let step = c.push("```rust\nfn main() {\n\n    println!(\"hi\");\n}\n");
        assert!(step.new_stable.is_empty());
        assert!(c.tail().contains("```rust"));

        let step = c.push("```\n\nafter");
        // Fence closes → the whole fenced block promotes as one
        // CodeFence; the `\n\n` that used to follow ends the region.
        assert!(step
            .new_stable
            .iter()
            .any(|b| matches!(b, Block::CodeFence { .. })));
        assert_eq!(c.tail(), "after");
    }

    #[test]
    fn heading_on_block_boundary_promotes_as_heading() {
        let mut c = StreamCollector::new();
        let step = c.push("## Section\n\nbody continues");
        assert_eq!(step.new_stable.len(), 1);
        assert!(matches!(
            step.new_stable[0],
            Block::Heading { level: 2, .. }
        ));
        assert_eq!(c.tail(), "body continues");
    }

    #[test]
    fn finalize_parses_tail_as_markdown() {
        let mut c = StreamCollector::new();
        c.push("# title\n\n- a\n- b");
        let blocks = c.finalize();
        // Heading promoted on the blank line, list finalized at end.
        assert!(blocks.iter().any(|b| matches!(b, Block::List { .. })));
    }

    #[test]
    fn empty_delta_is_noop() {
        let mut c = StreamCollector::new();
        let step = c.push("");
        assert!(step.new_stable.is_empty());
        assert_eq!(c.tail(), "");
    }
}
