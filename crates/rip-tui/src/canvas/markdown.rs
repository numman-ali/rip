//! Markdown parser for canvas blocks (Phase B.6).
//!
//! Turns a complete markdown source string into a `Vec<Block>` the
//! canvas model can store. The `StreamCollector` (B.5) feeds this
//! parser only *complete* regions — whole paragraphs, whole fences,
//! whole headings — so the parser never sees half-written syntax.
//!
//! Scope matches the Plan's "Markdown support (intentional subset)":
//! - ATX headings (levels 1–6) → `Block::Heading`
//! - Paragraphs → `Block::Paragraph`
//! - Unordered / ordered lists → `Block::List { ordered, items }`
//! - Block quotes → `Block::BlockQuote(children)`
//! - Fenced code blocks → `Block::CodeFence { lang, text }`
//! - Thematic breaks (`---`) → `Block::Thematic`
//! - Inline emphasis, strong, strikethrough, inline code, soft / hard
//!   breaks, links (rendered as `text ↗` — full URL goes to X-ray)
//!
//! Out of scope for B.6 (stay as plain text if encountered):
//! tables, images, raw HTML, footnotes. See plan Part 4.7.
//!
//! The parser never styles output — it only assembles `CachedText`
//! instances with the right line shape. Per-token styling (heading
//! weight, emphasis dim, etc.) is the renderer's job (B.7 adds
//! syntect; the theme work already owns colors).
//!
//! Invariant: `parse_blocks(source)` is pure. The same input always
//! produces the same `Vec<Block>` — important because the stream
//! collector re-parses a stable region once and caches the result.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::text::{Line, Span, Text};

use super::model::{Block, CachedText};

/// Parse a complete markdown source into canvas blocks. Empty input
/// yields an empty vec; whitespace-only input yields an empty vec.
pub fn parse_blocks(source: &str) -> Vec<Block> {
    if source.trim().is_empty() {
        return Vec::new();
    }
    // Intentionally skip `ENABLE_SMART_PUNCTUATION`: curly quotes /
    // em-dashes look nice in a browser but in a terminal they fight
    // with copy-paste into code and surprise users whose font doesn't
    // have the full Unicode range. Preserve what the agent wrote.
    let options = Options::empty() | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options);
    let mut builder = BlockBuilder::new();
    for event in parser {
        builder.handle(event);
    }
    builder.finish()
}

/// Stack-based assembler. Each `Tag::*` start pushes a frame; each
/// `TagEnd::*` pops it into the parent. Inline events (Text, Code,
/// Emphasis...) accumulate into the current inline buffer on the top
/// frame, flushed into a `Line` on `SoftBreak` / `HardBreak` and on
/// block boundaries.
struct BlockBuilder {
    stack: Vec<Frame>,
}

enum Frame {
    /// Root level — holds the final Vec<Block>.
    Root {
        blocks: Vec<Block>,
    },
    Paragraph(InlineBuf),
    Heading {
        level: u8,
        buf: InlineBuf,
    },
    CodeFence {
        lang: Option<String>,
        text: String,
    },
    BlockQuote {
        blocks: Vec<Block>,
    },
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    ListItem {
        blocks: Vec<Block>,
    },
}

/// Inline buffer: a series of `Line`s being assembled, plus the
/// currently-open line's spans. Emphasis/strong/code/link wrap text
/// with tagged spans so the renderer can theme them later; B.6 ships
/// them plain-styled (no color yet) — B.7/theme land the styling.
#[derive(Default)]
struct InlineBuf {
    lines: Vec<Line<'static>>,
    current: Vec<Span<'static>>,
    // Simple nesting marker: used only to decide how to render the
    // trailing ` ↗` suffix on links vs keep the link text plain.
    link_depth: u32,
}

impl InlineBuf {
    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        // Inline text may itself contain newlines (e.g. from
        // `SoftBreak` collapsed into text by smart punctuation). Split
        // so the line shape stays correct.
        let mut first = true;
        for segment in text.split('\n') {
            if !first {
                self.break_line();
            }
            if !segment.is_empty() {
                self.current.push(Span::raw(segment.to_string()));
            }
            first = false;
        }
    }

    fn push_code(&mut self, text: &str) {
        // Inline code renders bare in B.6 — theme work gives it an
        // accent later. Keep it in its own span so the renderer can
        // style by span tag when that lands.
        if text.is_empty() {
            return;
        }
        self.current.push(Span::raw(text.to_string()));
    }

    fn push_link_suffix(&mut self) {
        // Tiny affordance: a superscript arrow hints the reader that
        // the underlying URL exists (shown full-width in X-ray). No
        // color so it doesn't fight the body text.
        self.current.push(Span::raw(" ↗".to_string()));
    }

    fn break_line(&mut self) {
        let spans = std::mem::take(&mut self.current);
        self.lines.push(Line::from(spans));
    }

    fn finish(mut self) -> Text<'static> {
        if !self.current.is_empty() || self.lines.is_empty() {
            self.break_line();
        }
        Text::from(self.lines)
    }
}

impl BlockBuilder {
    fn new() -> Self {
        Self {
            stack: vec![Frame::Root { blocks: Vec::new() }],
        }
    }

    fn finish(mut self) -> Vec<Block> {
        // Any un-closed frames (should only happen on malformed input)
        // fold up as best-effort plain blocks.
        while self.stack.len() > 1 {
            let frame = self.stack.pop().expect("non-empty stack");
            self.push_closed(frame);
        }
        match self.stack.pop() {
            Some(Frame::Root { blocks }) => blocks,
            _ => Vec::new(),
        }
    }

    fn top_inline_mut(&mut self) -> Option<&mut InlineBuf> {
        match self.stack.last_mut()? {
            Frame::Paragraph(buf) | Frame::Heading { buf, .. } => Some(buf),
            _ => None,
        }
    }

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(end) => self.end(end),
            Event::Text(text) => {
                if let Some(buf) = self.top_inline_mut() {
                    buf.push_text(&text);
                } else if let Some(Frame::CodeFence { text: buffer, .. }) = self.stack.last_mut() {
                    buffer.push_str(&text);
                } else if matches!(self.stack.last(), Some(Frame::ListItem { .. })) {
                    // Tight list: pulldown-cmark emits Text directly
                    // inside the Item with no wrapping Paragraph tag.
                    // Open an implicit paragraph so inline events
                    // accumulate into a Line; closed on End(Item).
                    self.stack.push(Frame::Paragraph(InlineBuf::default()));
                    if let Some(buf) = self.top_inline_mut() {
                        buf.push_text(&text);
                    }
                } else if matches!(self.stack.last(), Some(Frame::BlockQuote { .. })) {
                    // Same pattern for tight block quotes.
                    self.stack.push(Frame::Paragraph(InlineBuf::default()));
                    if let Some(buf) = self.top_inline_mut() {
                        buf.push_text(&text);
                    }
                }
            }
            Event::Code(text) => {
                if let Some(buf) = self.top_inline_mut() {
                    buf.push_code(&text);
                }
            }
            Event::SoftBreak => {
                if let Some(buf) = self.top_inline_mut() {
                    // Soft breaks in markdown are semantic line wraps;
                    // in a terminal the renderer does its own
                    // wrapping, so collapse to a space.
                    buf.current.push(Span::raw(" ".to_string()));
                }
            }
            Event::HardBreak => {
                if let Some(buf) = self.top_inline_mut() {
                    buf.break_line();
                }
            }
            Event::Rule => {
                self.push_block(Block::Thematic);
            }
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) => {
                // Out of scope per plan Part 4.7.
            }
            Event::TaskListMarker(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                self.stack.push(Frame::Paragraph(InlineBuf::default()));
            }
            Tag::Heading { level, .. } => {
                self.stack.push(Frame::Heading {
                    level: heading_level_to_u8(level),
                    buf: InlineBuf::default(),
                });
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
                self.stack.push(Frame::CodeFence {
                    lang,
                    text: String::new(),
                });
            }
            Tag::BlockQuote(_) => {
                self.stack.push(Frame::BlockQuote { blocks: Vec::new() });
            }
            Tag::List(start) => {
                self.stack.push(Frame::List {
                    ordered: start.is_some(),
                    items: Vec::new(),
                });
            }
            Tag::Item => {
                self.stack.push(Frame::ListItem { blocks: Vec::new() });
            }
            Tag::Emphasis | Tag::Strong | Tag::Strikethrough => {
                // Inline formatting doesn't change the frame stack;
                // tokens flow into the current inline buf. Theme work
                // (B.7+) will distinguish via span tags.
            }
            Tag::Link { .. } => {
                if let Some(buf) = self.top_inline_mut() {
                    buf.link_depth = buf.link_depth.saturating_add(1);
                }
            }
            Tag::Image { .. } | Tag::HtmlBlock => {
                // Out of scope per plan Part 4.7.
            }
            Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::MetadataBlock(_)
            | Tag::Superscript
            | Tag::Subscript => {}
        }
    }

    fn end(&mut self, end: TagEnd) {
        match end {
            TagEnd::Item | TagEnd::BlockQuote(_) => {
                // Tight variants can leave an implicit `Frame::Paragraph`
                // on top (opened by our Text handler when pulldown-cmark
                // skipped emitting a Paragraph tag). Close it first so
                // the item/quote receives its paragraph block.
                if matches!(self.stack.last(), Some(Frame::Paragraph(_))) {
                    if let Some(frame) = self.stack.pop() {
                        self.push_closed(frame);
                    }
                }
                if let Some(frame) = self.stack.pop() {
                    self.push_closed(frame);
                }
            }
            TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::List(_) => {
                if let Some(frame) = self.stack.pop() {
                    self.push_closed(frame);
                }
            }
            TagEnd::Link => {
                if let Some(buf) = self.top_inline_mut() {
                    buf.link_depth = buf.link_depth.saturating_sub(1);
                    buf.push_link_suffix();
                }
            }
            _ => {}
        }
    }

    fn push_closed(&mut self, frame: Frame) {
        match frame {
            Frame::Root { .. } => {}
            Frame::Paragraph(buf) => {
                let text = buf.finish();
                let cached = CachedText {
                    source_hash: hash_text(&text),
                    text,
                };
                self.push_block(Block::Paragraph(cached));
            }
            Frame::Heading { level, buf } => {
                let text = buf.finish();
                let cached = CachedText {
                    source_hash: hash_text(&text),
                    text,
                };
                self.push_block(Block::Heading {
                    level,
                    text: cached,
                });
            }
            Frame::CodeFence { lang, text } => {
                let cached = CachedText::plain(text.trim_end_matches('\n'));
                self.push_block(Block::CodeFence { lang, text: cached });
            }
            Frame::BlockQuote { blocks } => {
                self.push_block(Block::BlockQuote(blocks));
            }
            Frame::List { ordered, items } => {
                self.push_block(Block::List { ordered, items });
            }
            Frame::ListItem { blocks } => {
                if let Some(Frame::List { items, .. }) = self.stack.last_mut() {
                    items.push(blocks);
                }
            }
        }
    }

    fn push_block(&mut self, block: Block) {
        match self.stack.last_mut() {
            Some(Frame::Root { blocks }) => blocks.push(block),
            Some(Frame::BlockQuote { blocks }) => blocks.push(block),
            Some(Frame::ListItem { blocks }) => blocks.push(block),
            _ => {
                // Inline frames (Paragraph / Heading) can't contain
                // nested blocks; drop the extraneous block rather
                // than corrupt the tree. pulldown-cmark doesn't emit
                // this in practice for well-formed markdown.
            }
        }
    }
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn hash_text(text: &Text<'_>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for line in &text.lines {
        for span in &line.spans {
            span.content.hash(&mut hasher);
        }
        "\n".hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_paragraph(blocks: &[Block]) -> String {
        for block in blocks {
            if let Block::Paragraph(cached) = block {
                return cached
                    .text
                    .lines
                    .iter()
                    .map(|l| {
                        l.spans
                            .iter()
                            .map(|s| s.content.as_ref())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
        String::new()
    }

    #[test]
    fn empty_input_produces_no_blocks() {
        assert!(parse_blocks("").is_empty());
        assert!(parse_blocks("   \n\t\n").is_empty());
    }

    #[test]
    fn single_paragraph_round_trips() {
        let blocks = parse_blocks("hello world");
        assert_eq!(blocks.len(), 1);
        assert_eq!(first_paragraph(&blocks), "hello world");
    }

    #[test]
    fn heading_has_level_and_text() {
        let blocks = parse_blocks("## A Title\n");
        assert_eq!(blocks.len(), 1);
        let Block::Heading { level, text } = &blocks[0] else {
            panic!("expected heading, got {blocks:?}");
        };
        assert_eq!(*level, 2);
        assert_eq!(text.text.lines.len(), 1);
    }

    #[test]
    fn code_fence_preserves_lang_and_body() {
        let blocks = parse_blocks("```rust\nfn main() {}\n```\n");
        assert_eq!(blocks.len(), 1);
        let Block::CodeFence { lang, text } = &blocks[0] else {
            panic!("expected code fence");
        };
        assert_eq!(lang.as_deref(), Some("rust"));
        let joined: String = text
            .text
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(joined, "fn main() {}");
    }

    #[test]
    fn unordered_list_captures_items() {
        let blocks = parse_blocks("- one\n- two\n- three\n");
        assert_eq!(blocks.len(), 1);
        let Block::List { ordered, items } = &blocks[0] else {
            panic!("expected list");
        };
        assert!(!*ordered);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn ordered_list_is_flagged_ordered() {
        let blocks = parse_blocks("1. alpha\n2. beta\n");
        let Block::List { ordered, items } = &blocks[0] else {
            panic!("expected list");
        };
        assert!(*ordered);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn blockquote_wraps_inner_paragraph() {
        let blocks = parse_blocks("> quoted line\n");
        let Block::BlockQuote(inner) = &blocks[0] else {
            panic!("expected blockquote");
        };
        assert_eq!(inner.len(), 1);
        assert!(matches!(inner[0], Block::Paragraph(_)));
    }

    #[test]
    fn thematic_break_emits_thematic_block() {
        let blocks = parse_blocks("---\n");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Thematic));
    }

    #[test]
    fn link_appends_arrow_suffix() {
        let blocks = parse_blocks("[click](https://example.com)");
        let rendered = first_paragraph(&blocks);
        assert!(rendered.contains("click"));
        assert!(rendered.contains("↗"));
    }

    #[test]
    fn inline_code_and_emphasis_render_as_plain_spans_in_b6() {
        let blocks = parse_blocks("before *em* and `code` after");
        let text = first_paragraph(&blocks);
        assert!(text.contains("em"));
        assert!(text.contains("code"));
        assert!(text.contains("before"));
        assert!(text.contains("after"));
    }

    #[test]
    fn multiple_paragraphs_stay_separate() {
        let blocks = parse_blocks("first\n\nsecond\n\nthird");
        assert_eq!(blocks.len(), 3);
        assert!(blocks.iter().all(|b| matches!(b, Block::Paragraph(_))));
    }

    #[test]
    fn heading_level_to_u8_maps_each_level_to_its_index() {
        // `HeadingLevel` has six variants and we flatten them into a
        // compact u8 for `Block::Heading`. The mapping is boring but
        // load-bearing — Part 2 of the plan key the heading shell off
        // this exact integer, so regressions are silent misrenders.
        assert_eq!(heading_level_to_u8(HeadingLevel::H1), 1);
        assert_eq!(heading_level_to_u8(HeadingLevel::H2), 2);
        assert_eq!(heading_level_to_u8(HeadingLevel::H3), 3);
        assert_eq!(heading_level_to_u8(HeadingLevel::H4), 4);
        assert_eq!(heading_level_to_u8(HeadingLevel::H5), 5);
        assert_eq!(heading_level_to_u8(HeadingLevel::H6), 6);
    }

    #[test]
    fn heading_blocks_capture_every_level_from_pulldown() {
        for n in 1..=6 {
            let markdown = format!("{} heading {n}\n", "#".repeat(n));
            let blocks = parse_blocks(&markdown);
            let Block::Heading { level, .. } = &blocks[0] else {
                panic!("expected heading for `{markdown}`");
            };
            assert_eq!(*level, n as u8);
        }
    }

    #[test]
    fn ordered_and_unordered_lists_preserve_items() {
        let ordered = parse_blocks("1. first\n2. second\n");
        assert_eq!(ordered.len(), 1);
        let Block::List {
            ordered: is_ord,
            items,
        } = &ordered[0]
        else {
            panic!("expected list");
        };
        assert!(*is_ord);
        assert_eq!(items.len(), 2);

        let bullets = parse_blocks("- a\n- b\n- c\n");
        assert_eq!(bullets.len(), 1);
        let Block::List {
            ordered: is_ord,
            items,
        } = &bullets[0]
        else {
            panic!("expected list");
        };
        assert!(!*is_ord);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn source_hash_is_stable_across_identical_parses() {
        let a = parse_blocks("**bold** text");
        let b = parse_blocks("**bold** text");
        match (&a[0], &b[0]) {
            (Block::Paragraph(x), Block::Paragraph(y)) => {
                assert_eq!(x.source_hash, y.source_hash);
            }
            _ => panic!("expected paragraphs"),
        }
    }
}
