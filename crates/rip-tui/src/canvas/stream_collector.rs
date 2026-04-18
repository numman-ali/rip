//! Streaming text → stable-block assembler (Phase B.5).
//!
//! The agent's `OutputTextDelta` frames arrive one chunk at a time. B.1's
//! ingest naively pushed each delta as its own `Block::Paragraph`, which
//! made the renderer re-parse the whole tail on every frame and produced
//! absurd numbers of tiny blocks (one per token). B.5 swaps in this
//! collector: a single streaming buffer per `AgentTurn` that promotes
//! *complete* paragraphs to stable blocks on block boundaries, leaving
//! the in-flight text as a transient tail the renderer shows beneath the
//! stable blocks.
//!
//! B.5 only recognizes paragraph boundaries (`\n\n`, i.e. a blank line).
//! B.6 swaps in `pulldown-cmark` to detect fence / heading / list
//! boundaries as well; this collector stays the seam for that swap —
//! the rest of the TUI never re-parses streaming text directly.
//!
//! Invariants:
//!
//! - `push` never parses anything that hasn't crossed a paragraph
//!   boundary, so partial markdown (unfinished fence, half-written link)
//!   stays in the tail until it resolves. No flashing of malformed
//!   syntax.
//! - `finalize` flushes whatever's left in the tail as a final paragraph.
//!   Called on `SessionEnded`.

use super::model::{Block, CachedText};

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

    /// Drain whatever's in the tail as a final paragraph. Called when
    /// the turn ends — the stream guarantees no more deltas arrive so
    /// the trailing text is by definition complete.
    pub fn finalize(&mut self) -> Option<Block> {
        if self.tail.is_empty() {
            return None;
        }
        let text = std::mem::take(&mut self.tail);
        Some(Block::Paragraph(CachedText::plain(text.trim_end())))
    }

    /// Peek the transient tail — the renderer shows this beneath the
    /// stable blocks so streaming feels continuous. Empty when there's
    /// nothing in flight.
    pub fn tail(&self) -> &str {
        &self.tail
    }

    fn drain_complete_paragraphs(&mut self) -> CollectorStep {
        let mut step = CollectorStep::default();
        // A paragraph boundary is any run of ≥2 newlines. We split on
        // the first such run, promote the prefix as a stable block, and
        // keep scanning until the tail no longer contains a double-
        // newline.
        while let Some((para, rest)) = split_at_paragraph_boundary(&self.tail) {
            let trimmed = para.trim_end_matches('\n');
            if !trimmed.is_empty() {
                step.new_stable
                    .push(Block::Paragraph(CachedText::plain(trimmed)));
            }
            self.tail = rest;
        }
        step
    }
}

/// Split the tail at its first paragraph boundary (`\n\n` or longer).
/// Returns `(paragraph_text, remaining_tail)` with the paragraph *not*
/// including the trailing newlines and the remaining tail starting
/// immediately after them. `None` when the tail doesn't contain a
/// complete paragraph yet.
fn split_at_paragraph_boundary(tail: &str) -> Option<(String, String)> {
    let bytes = tail.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\n' && bytes[i + 1] == b'\n' {
            // Walk forward past any extra newlines so the boundary is
            // fully consumed from the tail (avoids producing empty
            // paragraphs on triple-newlines).
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
        let last = c.finalize().expect("final block");
        matches!(last, Block::Paragraph(_));
        assert_eq!(c.tail(), "");
        assert!(c.finalize().is_none());
    }

    #[test]
    fn empty_delta_is_noop() {
        let mut c = StreamCollector::new();
        let step = c.push("");
        assert!(step.new_stable.is_empty());
        assert_eq!(c.tail(), "");
    }
}
