//! Go To palette mode (Phase C.5).
//!
//! A flat, fuzzy-searchable view of the current canvas — every user
//! turn, agent turn, tool card, task card, notice, and compaction
//! checkpoint is one entry. Applying an entry focuses that canvas
//! message (the driver pushes its `message_id` onto
//! `TuiState.focused_message_id`; the canvas renderer then scrolls
//! it into view via its existing focus-tracking path).
//!
//! Unlike the Command mode, Go To entries are generated from live
//! state, so the mode takes a `&CanvasModel` on construction and
//! snapshots its current shape. Re-entering the palette rebuilds
//! entries — there's no retained history to stale.

use crate::canvas::{Block, CachedText, CanvasMessage, CanvasModel};
use crate::PaletteEntry;

use super::super::PaletteSource;

#[derive(Debug, Clone, Default)]
pub struct GoToMode {
    entries: Vec<PaletteEntry>,
}

impl GoToMode {
    pub fn from_canvas(canvas: &CanvasModel) -> Self {
        let mut entries = Vec::with_capacity(canvas.messages.len());
        for message in &canvas.messages {
            entries.push(entry_for_message(message));
        }
        Self { entries }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl PaletteSource for GoToMode {
    fn id(&self) -> &'static str {
        "go-to"
    }

    fn label(&self) -> &str {
        "Go To"
    }

    fn placeholder(&self) -> &str {
        "jump to a canvas item"
    }

    fn entries(&self) -> Vec<PaletteEntry> {
        self.entries.clone()
    }

    fn empty_state(&self) -> &str {
        "canvas is empty"
    }
}

fn entry_for_message(message: &CanvasMessage) -> PaletteEntry {
    match message {
        CanvasMessage::UserTurn {
            message_id, blocks, ..
        } => PaletteEntry {
            value: message_id.clone(),
            title: format!("› {}", first_line(blocks, 64)),
            subtitle: Some("USER".to_string()),
            chips: Vec::new(),
        },
        CanvasMessage::AgentTurn {
            message_id,
            blocks,
            model,
            streaming,
            ..
        } => {
            let preview = first_line(blocks, 64);
            let mut chips = Vec::new();
            if *streaming {
                chips.push("streaming".to_string());
            }
            let subtitle = model.clone().unwrap_or_else(|| "AGENT".to_string());
            PaletteEntry {
                value: message_id.clone(),
                title: format!("◉ {preview}"),
                subtitle: Some(subtitle),
                chips,
            }
        }
        CanvasMessage::ToolCard {
            message_id,
            tool_name,
            ..
        } => PaletteEntry {
            value: message_id.clone(),
            title: format!("⟡ tool · {tool_name}"),
            subtitle: Some("TOOL".to_string()),
            chips: Vec::new(),
        },
        CanvasMessage::TaskCard {
            message_id,
            tool_name,
            title,
            ..
        } => {
            let subtitle = title.clone().unwrap_or_else(|| tool_name.clone());
            PaletteEntry {
                value: message_id.clone(),
                title: format!("⧉ task · {tool_name}"),
                subtitle: Some(subtitle),
                chips: Vec::new(),
            }
        }
        CanvasMessage::JobNotice {
            message_id,
            job_kind,
            ..
        } => PaletteEntry {
            value: message_id.clone(),
            title: format!("⧉ job · {job_kind}"),
            subtitle: Some("JOB".to_string()),
            chips: Vec::new(),
        },
        CanvasMessage::SystemNotice {
            message_id, text, ..
        } => PaletteEntry {
            value: message_id.clone(),
            title: format!("· {}", truncate(text, 64)),
            subtitle: Some("NOTICE".to_string()),
            chips: Vec::new(),
        },
        CanvasMessage::ContextNotice {
            message_id,
            strategy,
            ..
        } => PaletteEntry {
            value: message_id.clone(),
            title: format!("⌖ context · {strategy}"),
            subtitle: Some("CONTEXT".to_string()),
            chips: Vec::new(),
        },
        CanvasMessage::CompactionCheckpoint {
            message_id,
            from_seq,
            to_seq,
            ..
        } => PaletteEntry {
            value: message_id.clone(),
            title: format!("······ compaction · seq {from_seq}–{to_seq}"),
            subtitle: Some("CHECKPOINT".to_string()),
            chips: Vec::new(),
        },
        CanvasMessage::ExtensionPanel {
            message_id, title, ..
        } => PaletteEntry {
            value: message_id.clone(),
            title: format!("⊞ ext · {title}"),
            subtitle: Some("EXTENSION".to_string()),
            chips: Vec::new(),
        },
    }
}

fn first_line(blocks: &[Block], limit: usize) -> String {
    for block in blocks {
        if let Some(text) = block_plain_text(block) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let first = trimmed.lines().next().unwrap_or(trimmed);
                return truncate(first, limit);
            }
        }
    }
    "(empty)".to_string()
}

fn block_plain_text(block: &Block) -> Option<String> {
    match block {
        Block::Paragraph(t)
        | Block::Markdown(t)
        | Block::Heading { text: t, .. }
        | Block::CodeFence { text: t, .. }
        | Block::ToolArgsJson(t)
        | Block::ToolStdout(t)
        | Block::ToolStderr(t) => Some(cached_to_string(t)),
        Block::BlockQuote(inner) => inner.iter().find_map(block_plain_text),
        Block::List { items, .. } => items
            .iter()
            .find_map(|block_vec| block_vec.iter().find_map(block_plain_text)),
        Block::Thematic | Block::ArtifactChip { .. } => None,
    }
}

fn cached_to_string(cached: &CachedText) -> String {
    let mut out = String::new();
    for (idx, line) in cached.text.lines.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        for span in &line.spans {
            out.push_str(&span.content);
        }
    }
    out
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx + 1 >= limit {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{AgentRole, Block, CachedText, CanvasMessage, ToolCardStatus};

    fn text_block(s: &str) -> Block {
        Block::Paragraph(CachedText::plain(s))
    }

    #[test]
    fn empty_canvas_produces_no_entries() {
        let canvas = CanvasModel::default();
        let mode = GoToMode::from_canvas(&canvas);
        assert!(mode.is_empty());
    }

    #[test]
    fn builds_entry_per_message_with_glyphed_titles() {
        let mut canvas = CanvasModel::default();
        canvas.messages.push(CanvasMessage::UserTurn {
            message_id: "u1".to_string(),
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
            blocks: vec![text_block("hello")],
            submitted_at_ms: 0,
        });
        canvas.messages.push(CanvasMessage::AgentTurn {
            message_id: "a1".to_string(),
            run_session_id: "r1".to_string(),
            agent_id: None,
            role: AgentRole::Primary,
            actor_id: "agent".to_string(),
            model: Some("gpt-5".to_string()),
            reasoning_text: String::new(),
            reasoning_summary: String::new(),
            blocks: vec![text_block("world")],
            streaming_tail: String::new(),
            streaming_collector: crate::canvas::StreamCollector::new(),
            streaming: false,
            started_at_ms: 0,
            ended_at_ms: Some(0),
        });
        canvas.messages.push(CanvasMessage::ToolCard {
            message_id: "t1".to_string(),
            tool_id: "call_1".to_string(),
            tool_name: "bash".to_string(),
            args_block: text_block("{}"),
            status: ToolCardStatus::Succeeded {
                duration_ms: 50,
                exit_code: 0,
            },
            body: Vec::new(),
            expanded: false,
            artifact_ids: Vec::new(),
            started_seq: 0,
            started_at_ms: 0,
        });

        let mode = GoToMode::from_canvas(&canvas);
        let entries = mode.entries();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].title.starts_with("› "));
        assert!(entries[1].title.starts_with("◉ "));
        assert!(entries[2].title.starts_with("⟡ tool"));
        assert_eq!(entries[0].value, "u1");
        assert_eq!(entries[1].value, "a1");
        assert_eq!(entries[2].value, "t1");
    }

    #[test]
    fn surface_metadata_describes_go_to_mode() {
        let mode = GoToMode::default();
        assert_eq!(mode.id(), "go-to");
        assert_eq!(mode.label(), "Go To");
        assert!(!mode.placeholder().is_empty());
        assert_eq!(mode.empty_state(), "canvas is empty");
    }

    #[test]
    fn streaming_agent_turn_gets_streaming_chip_and_model_subtitle() {
        use crate::canvas::JobLifecycle;

        let mut canvas = CanvasModel::default();
        canvas.messages.push(CanvasMessage::AgentTurn {
            message_id: "a-stream".into(),
            run_session_id: "r".into(),
            agent_id: None,
            role: AgentRole::Primary,
            actor_id: "agent".into(),
            model: None, // forces the "AGENT" fallback subtitle
            reasoning_text: String::new(),
            reasoning_summary: String::new(),
            blocks: vec![text_block("hi")],
            streaming_tail: String::new(),
            streaming_collector: crate::canvas::StreamCollector::new(),
            streaming: true,
            started_at_ms: 0,
            ended_at_ms: None,
        });
        canvas.messages.push(CanvasMessage::JobNotice {
            message_id: "j1".into(),
            job_id: "job-1".into(),
            job_kind: "indexer".into(),
            details: None,
            status: JobLifecycle::Running,
            actor_id: "agent".into(),
            origin: "kernel".into(),
            started_at_ms: None,
            ended_at_ms: None,
        });
        let entries = GoToMode::from_canvas(&canvas).entries();
        assert_eq!(entries[0].subtitle.as_deref(), Some("AGENT"));
        assert!(entries[0].chips.iter().any(|c| c == "streaming"));
        assert!(entries[1].title.contains("job · indexer"));
    }

    #[test]
    fn task_notice_compaction_and_extension_variants_produce_entries() {
        use crate::canvas::{ContextLifecycle, NoticeLevel, PanelPlacement, TaskCardStatus};
        use rip_kernel::ToolTaskExecutionMode;

        let mut canvas = CanvasModel::default();
        canvas.messages.push(CanvasMessage::TaskCard {
            message_id: "task-1".into(),
            task_id: "task-1".into(),
            tool_name: "bash".into(),
            title: Some("smoke".into()),
            execution_mode: ToolTaskExecutionMode::Pipes,
            status: TaskCardStatus::Running,
            body: Vec::new(),
            expanded: false,
            artifact_ids: Vec::new(),
            started_at_ms: None,
        });
        canvas.messages.push(CanvasMessage::SystemNotice {
            message_id: "n-1".into(),
            level: NoticeLevel::Info,
            text: "hello world".into(),
            origin_event_kind: "".into(),
            seq: 0,
        });
        canvas.messages.push(CanvasMessage::ContextNotice {
            message_id: "ctx-1".into(),
            run_session_id: "r".into(),
            strategy: "retrieval".into(),
            status: ContextLifecycle::Compiled,
            bundle_artifact_id: None,
            contributed_artifact_ids: Vec::new(),
        });
        canvas.messages.push(CanvasMessage::CompactionCheckpoint {
            message_id: "cp-1".into(),
            checkpoint_id: "cp".into(),
            from_seq: 2,
            to_seq: 7,
            summary_artifact_id: "art".into(),
        });
        canvas.messages.push(CanvasMessage::ExtensionPanel {
            message_id: "ext-1".into(),
            panel_id: "panel".into(),
            extension_id: "ext".into(),
            title: "Panel".into(),
            placement: PanelPlacement::Inline,
            lines: Vec::new(),
            keys: Vec::new(),
            artifact_ids: Vec::new(),
        });
        let entries = GoToMode::from_canvas(&canvas).entries();
        assert_eq!(entries.len(), 5);
        assert!(entries[0].title.contains("task · bash"));
        assert_eq!(entries[0].subtitle.as_deref(), Some("smoke"));
        assert!(entries[1].title.contains("hello world"));
        assert!(entries[2].title.contains("context · retrieval"));
        assert!(entries[3].title.contains("seq 2–7"));
        assert!(entries[4].title.contains("ext · Panel"));
    }

    #[test]
    fn first_line_falls_back_to_empty_marker_and_truncate_respects_char_limit() {
        // Empty blocks drop to the `"(empty)"` marker.
        let mut canvas = CanvasModel::default();
        canvas.messages.push(CanvasMessage::UserTurn {
            message_id: "u".into(),
            actor_id: "user".into(),
            origin: "cli".into(),
            blocks: vec![Block::Thematic],
            submitted_at_ms: 0,
        });
        let entries = GoToMode::from_canvas(&canvas).entries();
        assert!(entries[0].title.contains("(empty)"));

        // `truncate` clips long strings and appends an ellipsis; short
        // strings pass through verbatim.
        assert_eq!(truncate("hi", 10), "hi");
        let truncated = truncate("abcdefghij", 5);
        assert_eq!(truncated.chars().count(), 5);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn block_plain_text_walks_quote_list_and_structured_variants() {
        let plain = text_block("hello");
        assert_eq!(
            block_plain_text(&plain).as_deref(),
            Some("hello"),
            "paragraph",
        );
        let heading = Block::Heading {
            level: 1,
            text: CachedText::plain("title"),
        };
        assert_eq!(block_plain_text(&heading).as_deref(), Some("title"));
        let fence = Block::CodeFence {
            lang: Some("rust".into()),
            text: CachedText::plain("fn x() {}"),
        };
        assert_eq!(block_plain_text(&fence).as_deref(), Some("fn x() {}"));
        let stderr = Block::ToolStderr(CachedText::plain("boom"));
        assert_eq!(block_plain_text(&stderr).as_deref(), Some("boom"));
        let stdout = Block::ToolStdout(CachedText::plain("out"));
        assert_eq!(block_plain_text(&stdout).as_deref(), Some("out"));
        let args = Block::ToolArgsJson(CachedText::plain("{}"));
        assert_eq!(block_plain_text(&args).as_deref(), Some("{}"));
        let quote = Block::BlockQuote(vec![text_block("inside")]);
        assert_eq!(block_plain_text(&quote).as_deref(), Some("inside"));
        let list = Block::List {
            ordered: false,
            items: vec![vec![text_block("bullet")]],
        };
        assert_eq!(block_plain_text(&list).as_deref(), Some("bullet"));
        assert!(block_plain_text(&Block::Thematic).is_none());
        assert!(block_plain_text(&Block::ArtifactChip {
            artifact_id: "a".into(),
            bytes: None,
        })
        .is_none());
    }
}
