//! Borderless Hero strip (Phase C.1).
//!
//! Replaces the old `┌RIP───┐ view:canvas session:s1 seq:6 hdr:- fb:-
//! evt:- …` status-bar soup with a typographic 1-row hero per
//! `docs/07_tasks/tui_revamp.md` Part 3.2:
//!
//! ```text
//! thread · agent · model                       state · ttft 120
//! ```
//!
//! - Left group (`thread · agent · model`) is separated by `·` in
//!   `fg_quiet`. Any segment we don't know collapses to `-`.
//! - Right group carries the current run state (`idle | thinking |
//!   streaming | stalled | error`) and the most recent TTFT. When
//!   there's no TTFT yet, the right side is state-only.
//! - When width forces a decision, the left group shrinks in this
//!   order: thread (to ≤20 chars + `…`) → agent (to its glyph) →
//!   model (to `…nano` suffix). The right group keeps state + TTFT
//!   unless there's no room for both, in which case TTFT drops.
//! - Debug tokens that used to live here (seq/hdr/fb/evt/tools/tasks
//!   counters) moved behind the `Debug` overlay (opened by
//!   `Command → Show debug info`). The hero is now for bearings,
//!   not telemetry.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::TuiState;

use super::theme::ThemeStyles;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeroClickTarget {
    Thread,
    Agent,
    Model,
}

pub(super) fn render_status_bar(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let hero = HeroContent::from_state(state);
    let line = hero.render_line(theme, area.width as usize);
    let widget = Paragraph::new(line).style(theme.chrome);
    frame.render_widget(widget, area);
}

pub fn hero_click_target(state: &TuiState, width: u16, column: u16) -> Option<HeroClickTarget> {
    if width == 0 {
        return None;
    }

    let hero = HeroContent::from_state(state);
    let right = right_segments(&hero);
    let right_len: usize = right.iter().map(|s| s.text.chars().count()).sum();
    let spacer_before_right = if right.is_empty() { 0 } else { 2 };
    let budget = (width as usize)
        .saturating_sub(right_len)
        .saturating_sub(spacer_before_right);
    let left = left_segments(&hero, budget);

    let mut cursor = 0usize;
    for seg in left {
        let seg_len = seg.text.chars().count();
        let start = cursor;
        let end = cursor.saturating_add(seg_len);
        let target = match seg.kind {
            SegmentKind::Thread => Some(HeroClickTarget::Thread),
            SegmentKind::Agent => Some(HeroClickTarget::Agent),
            SegmentKind::Model => Some(HeroClickTarget::Model),
            _ => None,
        };
        if target.is_some() && (column as usize) >= start && (column as usize) < end {
            return target;
        }
        cursor = end;
    }

    None
}

/// Pull the hero's raw strings out of `TuiState`. Kept separate from
/// rendering so the truncation cascade is testable without a frame.
#[derive(Debug, Clone)]
pub(super) struct HeroContent {
    /// Thread / continuity label. `None` collapses to `-`.
    pub thread: Option<String>,
    /// Agent label. `None` collapses to `-`.
    pub agent: Option<String>,
    /// Model label, already formatted as `<provider>:<model>` or just
    /// `<provider>` when the model id isn't known. `None` → `-`.
    pub model: Option<String>,
    /// Active reasoning posture for the next/current turn, rendered as
    /// a compact `r high/concise` chip when present.
    pub reasoning: Option<String>,
    /// Hosted web-search posture for the next/current turn, rendered as
    /// a compact chip when enabled or requested-but-unavailable.
    pub web_search: Option<String>,
    /// Aggregate run state.
    pub state: HeroState,
    /// Latest TTFT in ms (if known).
    pub ttft_ms: Option<u64>,
    /// Optional short status message (`sending…`, `working…`). Shown
    /// between state and TTFT when present.
    pub status_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeroState {
    Idle,
    Thinking,
    Streaming,
    Stalled,
    Error,
}

impl HeroState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Thinking => "thinking",
            Self::Streaming => "streaming",
            Self::Stalled => "stalled",
            Self::Error => "error",
        }
    }

    fn style(self, theme: &ThemeStyles) -> Style {
        match self {
            Self::Idle => theme.muted,
            Self::Thinking => theme.chrome,
            Self::Streaming => theme.header,
            Self::Stalled => theme.warn,
            Self::Error => theme.danger,
        }
    }
}

impl HeroContent {
    pub(super) fn from_state(state: &TuiState) -> Self {
        let thread = state
            .continuity_id
            .as_deref()
            .map(shorten_thread)
            .filter(|value| !value.is_empty());

        // There isn't an agent label on `TuiState` today — we don't
        // know the agent name until the kernel starts emitting one via
        // the frame stream. Until then, use "rip" as a neutral stand-in
        // so the hero doesn't look empty and the user has something to
        // click on (Phase C.5 Palette will spawn Models from here).
        let agent = Some("rip".to_string());

        let endpoint = state
            .openresponses_endpoint
            .as_deref()
            .or(state.preferred_openresponses_endpoint.as_deref());
        let raw_model = state
            .openresponses_model
            .as_deref()
            .or(state.preferred_openresponses_model.as_deref())
            .filter(|value| !value.trim().is_empty());
        let provider = endpoint.map(classify_provider);
        let model = match (provider, raw_model) {
            (Some(provider), Some(model)) => Some(format!("{provider}:{model}")),
            (Some(provider), None) => Some(provider.to_string()),
            (None, Some(model)) => Some(model.to_string()),
            (None, None) => None,
        };
        let reasoning = format_reasoning_label(
            state.preferred_openresponses_reasoning_effort.as_deref(),
            state.preferred_openresponses_reasoning_summary.as_deref(),
        );
        let web_search = state.preferred_openresponses_web_search.clone();

        let hero_state = classify_state(state);
        let ttft_ms = state.ttft_ms();

        Self {
            thread,
            agent,
            model,
            reasoning,
            web_search,
            state: hero_state,
            ttft_ms,
            status_message: state.status_message.clone(),
        }
    }

    pub(super) fn render_line(&self, theme: &ThemeStyles, width: usize) -> Line<'static> {
        let right = right_segments(self);
        let right_len: usize = right.iter().map(|s| s.text.chars().count()).sum();
        let spacer_before_right = if right.is_empty() { 0 } else { 2 };

        let budget = width
            .saturating_sub(right_len)
            .saturating_sub(spacer_before_right);
        let left = left_segments(self, budget);

        let mut spans: Vec<Span<'static>> = Vec::new();
        for seg in &left {
            spans.push(styled_segment(seg, theme));
        }

        let left_len: usize = left.iter().map(|s| s.text.chars().count()).sum();
        let pad = width
            .saturating_sub(left_len)
            .saturating_sub(right_len)
            .max(1);
        spans.push(Span::styled(" ".repeat(pad), theme.chrome));

        for seg in &right {
            spans.push(styled_segment(seg, theme));
        }

        Line::from(spans)
    }
}

#[derive(Debug, Clone)]
struct HeroSegment {
    text: String,
    kind: SegmentKind,
}

#[derive(Debug, Clone, Copy)]
enum SegmentKind {
    Thread,
    Agent,
    Model,
    Separator,
    State(HeroState),
    Reasoning,
    WebSearch,
    Status,
    Ttft,
    Muted,
}

fn styled_segment(seg: &HeroSegment, theme: &ThemeStyles) -> Span<'static> {
    let style = match seg.kind {
        SegmentKind::Thread => theme.chrome.add_modifier(Modifier::BOLD),
        SegmentKind::Agent => theme.chrome,
        SegmentKind::Model => theme.muted,
        SegmentKind::Separator => theme.quiet,
        SegmentKind::State(state) => state.style(theme),
        SegmentKind::Reasoning => theme.accent,
        SegmentKind::WebSearch => theme.accent,
        SegmentKind::Status => theme.muted,
        SegmentKind::Ttft => theme.muted,
        SegmentKind::Muted => theme.muted,
    };
    Span::styled(seg.text.clone(), style)
}

fn left_segments(hero: &HeroContent, budget: usize) -> Vec<HeroSegment> {
    // Full layout: `<thread> · <agent> · <model>`. When the budget
    // doesn't fit, shrink thread → agent → model in that order per the
    // plan. Each level we recompute the available chars for the
    // remaining pieces.
    let thread_full = hero.thread.clone().unwrap_or_else(|| "-".to_string());
    let agent_full = hero.agent.clone().unwrap_or_else(|| "-".to_string());
    let model_full = hero.model.clone().unwrap_or_else(|| "-".to_string());

    // Always produce something non-empty; at width 0 the caller will
    // just not render.
    let min_budget = budget.max(1);

    let sep_len = " · ".chars().count();

    // Attempt 0: everything full.
    let full_len = thread_full.chars().count()
        + sep_len
        + agent_full.chars().count()
        + sep_len
        + model_full.chars().count();
    if full_len <= min_budget {
        return segments(&thread_full, &agent_full, &model_full);
    }

    // Attempt 1: shrink thread to ≤20 chars.
    let thread_trim = truncate(&thread_full, 20);
    let trim_len = thread_trim.chars().count()
        + sep_len
        + agent_full.chars().count()
        + sep_len
        + model_full.chars().count();
    if trim_len <= min_budget {
        return segments(&thread_trim, &agent_full, &model_full);
    }

    // Attempt 2: agent → single glyph.
    let agent_glyph = agent_glyph_for(&agent_full);
    let agent_glyph_len = thread_trim.chars().count()
        + sep_len
        + agent_glyph.chars().count()
        + sep_len
        + model_full.chars().count();
    if agent_glyph_len <= min_budget {
        return segments(&thread_trim, &agent_glyph, &model_full);
    }

    // Attempt 3: model → `…<tail>`. We keep the last 6 chars at most
    // (mirrors the plan's `…nano` example).
    let model_tail = short_tail(&model_full, 6);
    let model_tail_len = thread_trim.chars().count()
        + sep_len
        + agent_glyph.chars().count()
        + sep_len
        + model_tail.chars().count();
    if model_tail_len <= min_budget {
        return segments(&thread_trim, &agent_glyph, &model_tail);
    }

    // Attempt 4: drop the agent segment entirely. Thread stays because
    // it's the most important bearing; model is already shortened.
    let thread_sep_model_len = thread_trim.chars().count() + sep_len + model_tail.chars().count();
    if thread_sep_model_len <= min_budget {
        return two_segments(&thread_trim, &model_tail);
    }

    // Attempt 5: thread-only. Hard truncate to the budget.
    let thread_only = truncate(&thread_trim, min_budget.max(1));
    vec![HeroSegment {
        text: thread_only,
        kind: SegmentKind::Thread,
    }]
}

fn segments(thread: &str, agent: &str, model: &str) -> Vec<HeroSegment> {
    vec![
        HeroSegment {
            text: thread.to_string(),
            kind: SegmentKind::Thread,
        },
        HeroSegment {
            text: " · ".to_string(),
            kind: SegmentKind::Separator,
        },
        HeroSegment {
            text: agent.to_string(),
            kind: SegmentKind::Agent,
        },
        HeroSegment {
            text: " · ".to_string(),
            kind: SegmentKind::Separator,
        },
        HeroSegment {
            text: model.to_string(),
            kind: SegmentKind::Model,
        },
    ]
}

fn two_segments(thread: &str, model: &str) -> Vec<HeroSegment> {
    vec![
        HeroSegment {
            text: thread.to_string(),
            kind: SegmentKind::Thread,
        },
        HeroSegment {
            text: " · ".to_string(),
            kind: SegmentKind::Separator,
        },
        HeroSegment {
            text: model.to_string(),
            kind: SegmentKind::Model,
        },
    ]
}

fn right_segments(hero: &HeroContent) -> Vec<HeroSegment> {
    let mut segs = Vec::new();
    segs.push(HeroSegment {
        text: hero.state.as_str().to_string(),
        kind: SegmentKind::State(hero.state),
    });
    if let Some(reasoning) = hero.reasoning.as_deref().filter(|value| !value.is_empty()) {
        segs.push(HeroSegment {
            text: "  ".to_string(),
            kind: SegmentKind::Muted,
        });
        segs.push(HeroSegment {
            text: reasoning.to_string(),
            kind: SegmentKind::Reasoning,
        });
    }
    if let Some(web_search) = hero.web_search.as_deref().filter(|value| !value.is_empty()) {
        segs.push(HeroSegment {
            text: "  ".to_string(),
            kind: SegmentKind::Muted,
        });
        segs.push(HeroSegment {
            text: web_search.to_string(),
            kind: SegmentKind::WebSearch,
        });
    }
    if let Some(msg) = hero.status_message.as_deref().filter(|m| !m.is_empty()) {
        let trimmed = truncate(msg, 24);
        segs.push(HeroSegment {
            text: "  ".to_string(),
            kind: SegmentKind::Muted,
        });
        segs.push(HeroSegment {
            text: trimmed,
            kind: SegmentKind::Status,
        });
    }
    if let Some(ttft) = hero.ttft_ms {
        segs.push(HeroSegment {
            text: "  ".to_string(),
            kind: SegmentKind::Muted,
        });
        segs.push(HeroSegment {
            text: format!("ttft {ttft}"),
            kind: SegmentKind::Ttft,
        });
    }
    segs
}

fn format_reasoning_label(effort: Option<&str>, summary: Option<&str>) -> Option<String> {
    let effort = effort
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "inherit");
    let summary = summary
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "inherit");
    match (effort, summary) {
        (Some(effort), Some(summary)) => Some(format!("r {effort}/{summary}")),
        (Some(effort), None) => Some(format!("r {effort}")),
        (None, Some(summary)) => Some(format!("r {summary}")),
        (None, None) => None,
    }
}

fn classify_state(state: &TuiState) -> HeroState {
    if state.has_error() {
        return HeroState::Error;
    }
    if state.is_stalled(5_000) {
        return HeroState::Stalled;
    }
    if state.awaiting_response {
        if state.first_output_ms.is_some() {
            return HeroState::Streaming;
        }
        return HeroState::Thinking;
    }
    HeroState::Idle
}

fn classify_provider(endpoint: &str) -> &'static str {
    if endpoint.contains("openrouter.ai") {
        "openrouter"
    } else if endpoint.contains("api.openai.com") || endpoint.contains("openai.com") {
        "openai"
    } else {
        "openresponses"
    }
}

/// Thread ids are typically UUIDs or slugs; strip any `thread-`/`cont-`
/// prefixes and take the tail so the hero doesn't get dominated by
/// namespace ceremony. For human-typed names we pass through unchanged.
fn shorten_thread(id: &str) -> String {
    if id.len() <= 20 {
        return id.to_string();
    }
    if let Some(tail) = id.split(':').next_back() {
        if tail.len() >= 4 && tail.len() < id.len() {
            return tail.to_string();
        }
    }
    id.to_string()
}

/// Reduce an agent name down to a single-character stand-in. Used when
/// the hero is bandwidth-constrained and we need to keep the shape of
/// `thread · agent · model` without dropping the agent slot.
fn agent_glyph_for(agent: &str) -> String {
    agent
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = input.chars().take(keep).collect();
    out.push('…');
    out
}

fn short_tail(input: &str, tail_chars: usize) -> String {
    if input.chars().count() <= tail_chars + 1 {
        return input.to_string();
    }
    let tail: String = input
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThemeId;

    fn hero(thread: Option<&str>, agent: &str, model: Option<&str>) -> HeroContent {
        HeroContent {
            thread: thread.map(|s| s.to_string()),
            agent: Some(agent.to_string()),
            model: model.map(|s| s.to_string()),
            reasoning: None,
            web_search: None,
            state: HeroState::Idle,
            ttft_ms: None,
            status_message: None,
        }
    }

    #[test]
    fn wide_budget_renders_full_triple() {
        let h = hero(Some("slide-prep"), "rip", Some("openai:gpt-5"));
        let line = h.render_line(&ThemeStyles::for_theme(ThemeId::DefaultDark), 60);
        let text = line.to_string();
        assert!(text.contains("slide-prep"), "got: {text}");
        assert!(text.contains("rip"));
        assert!(text.contains("openai:gpt-5"));
        assert!(text.contains("idle"));
    }

    #[test]
    fn thread_shrinks_to_twenty_chars_when_budget_is_tight() {
        let h = hero(
            Some("very-long-thread-name-that-blows-past-the-limit"),
            "rip",
            Some("openai:gpt-5"),
        );
        let segs = left_segments(&h, 40);
        let thread = &segs[0].text;
        assert!(thread.chars().count() <= 20);
        assert!(thread.ends_with('…'));
    }

    #[test]
    fn agent_shrinks_to_glyph_before_model() {
        let h = hero(Some("thread"), "subagent", Some("openai:gpt-5-very-long"));
        // Just enough room that shrinking the agent fits but full doesn't.
        let segs = left_segments(&h, 28);
        let agent = &segs[2].text;
        assert_eq!(agent, "s", "expected glyph fallback, got {segs:?}");
    }

    #[test]
    fn model_shrinks_to_tail_when_still_overflowing() {
        let h = hero(
            Some("thread"),
            "rip",
            Some("openresponses:extremely-long-model-id"),
        );
        let segs = left_segments(&h, 22);
        let model = &segs.last().unwrap().text;
        assert!(
            model.starts_with('…'),
            "expected tail elision, got {segs:?}"
        );
    }

    #[test]
    fn tiny_budget_keeps_thread_only() {
        let h = hero(Some("slide-prep"), "rip", Some("openai:gpt-5"));
        let segs = left_segments(&h, 6);
        assert_eq!(segs.len(), 1);
        assert!(segs[0].text.chars().count() <= 6);
    }

    #[test]
    fn state_classifier_routes_known_signals() {
        let mut state = TuiState::new(10);
        assert_eq!(classify_state(&state), HeroState::Idle);
        state.awaiting_response = true;
        assert_eq!(classify_state(&state), HeroState::Thinking);
        state.first_output_ms = Some(1);
        state.start_ms = Some(0);
        assert_eq!(classify_state(&state), HeroState::Streaming);
        state.last_error_seq = Some(3);
        assert_eq!(classify_state(&state), HeroState::Error);
    }

    #[test]
    fn hero_line_right_aligns_state_and_ttft() {
        let mut h = hero(Some("t"), "rip", Some("o:g"));
        h.state = HeroState::Streaming;
        h.ttft_ms = Some(120);
        let line = h.render_line(&ThemeStyles::for_theme(ThemeId::DefaultDark), 40);
        let text = line.to_string();
        assert!(text.ends_with("ttft 120"));
        assert!(text.contains("streaming"));
    }

    #[test]
    fn reasoning_label_renders_when_present() {
        let mut h = hero(Some("t"), "rip", Some("o:g"));
        h.reasoning = Some("r high/concise".to_string());
        let line = h.render_line(&ThemeStyles::for_theme(ThemeId::DefaultDark), 48);
        let text = line.to_string();
        assert!(text.contains("r high/concise"), "{text}");
    }

    #[test]
    fn web_search_label_renders_when_present() {
        let mut h = hero(Some("t"), "rip", Some("o:g"));
        h.web_search = Some("web on".to_string());
        let line = h.render_line(&ThemeStyles::for_theme(ThemeId::DefaultDark), 48);
        let text = line.to_string();
        assert!(text.contains("web on"), "{text}");
    }

    #[test]
    fn short_tail_handles_non_ascii_boundaries() {
        let out = short_tail("openai:gpt-5-née", 6);
        assert!(out.starts_with('…'));
        assert!(out.ends_with("5-née"));
    }

    #[test]
    fn hero_click_targets_left_segments() {
        let mut state = TuiState::new(10);
        state.continuity_id = Some("thread-alpha".to_string());
        state.preferred_openresponses_endpoint =
            Some("https://openrouter.ai/api/v1/responses".to_string());
        state.preferred_openresponses_model = Some("nvidia/nemotron".to_string());

        assert_eq!(
            hero_click_target(&state, 80, 1),
            Some(HeroClickTarget::Thread)
        );
        assert_eq!(
            hero_click_target(&state, 80, 15),
            Some(HeroClickTarget::Agent)
        );
        assert_eq!(
            hero_click_target(&state, 80, 22),
            Some(HeroClickTarget::Model)
        );
    }
}
