use std::collections::{HashMap, HashSet, VecDeque};

const MAX_HIGHLIGHTS: usize = 12;
const MAX_NOTABLE_LINES: usize = 32;
const MAX_KEYWORDS: usize = 12;
const MAX_MESSAGE_SCAN_CHARS: usize = 8_192;
const MAX_LINE_CHARS: usize = 200;
pub(crate) const MAX_SUMMARY_MARKDOWN_CHARS: usize = 20_000;

pub(crate) struct AutoSummaryDelta {
    pub(crate) message_count: u64,
    pub(crate) actor_counts: Vec<(String, u64)>,
    pub(crate) top_keywords: Vec<String>,
    pub(crate) notable_lines: Vec<String>,
    pub(crate) recent_highlights: Vec<String>,
}

#[derive(Default)]
pub(crate) struct AutoSummaryAccumulator {
    message_count: u64,
    actor_counts: HashMap<String, u64>,
    word_counts: HashMap<String, u32>,
    notable: VecDeque<String>,
    notable_set: HashSet<String>,
    recent: VecDeque<String>,
}

impl AutoSummaryAccumulator {
    pub(crate) fn observe_message(&mut self, actor_id: &str, content: &str) {
        self.message_count = self.message_count.saturating_add(1);
        *self.actor_counts.entry(actor_id.to_string()).or_insert(0) += 1;

        self.observe_words(content);
        self.observe_notable_lines(content);
        self.observe_highlight(actor_id, content);
    }

    pub(crate) fn finish(self) -> AutoSummaryDelta {
        let mut actor_counts: Vec<(String, u64)> = self.actor_counts.into_iter().collect();
        actor_counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        let mut words: Vec<(String, u32)> = self.word_counts.into_iter().collect();
        words.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let top_keywords = words
            .into_iter()
            .take(MAX_KEYWORDS)
            .map(|(word, _)| word)
            .collect();

        AutoSummaryDelta {
            message_count: self.message_count,
            actor_counts,
            top_keywords,
            notable_lines: self.notable.into_iter().collect(),
            recent_highlights: self.recent.into_iter().collect(),
        }
    }

    fn observe_words(&mut self, content: &str) {
        for word in tokenize_words_bounded(content, MAX_MESSAGE_SCAN_CHARS) {
            if is_stopword(&word) {
                continue;
            }
            *self.word_counts.entry(word).or_insert(0) += 1;
        }
    }

    fn observe_notable_lines(&mut self, content: &str) {
        for line in extract_notable_lines(content) {
            if !self.notable_set.insert(line.clone()) {
                continue;
            }
            self.notable.push_back(line);
            while self.notable.len() > MAX_NOTABLE_LINES {
                if let Some(removed) = self.notable.pop_front() {
                    self.notable_set.remove(&removed);
                }
            }
        }
    }

    fn observe_highlight(&mut self, actor_id: &str, content: &str) {
        let mut first: Option<&str> = None;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            first = Some(trimmed);
            break;
        }
        let Some(first) = first else {
            return;
        };
        let value = format!("{actor_id}: {}", truncate_chars(first, MAX_LINE_CHARS));
        self.recent.push_back(value);
        while self.recent.len() > MAX_HIGHLIGHTS {
            self.recent.pop_front();
        }
    }
}

pub(crate) fn summary_markdown_is_legacy_metadata_placeholder(markdown: &str) -> bool {
    let trimmed = markdown.trim();
    if !trimmed.starts_with("# Compaction summary (auto)") {
        return false;
    }

    let has_required_fields = [
        "- kind:",
        "- cut_rule_id:",
        "- stride_messages:",
        "- target_message_ordinal:",
        "- to_seq:",
        "- to_message_id:",
    ]
    .iter()
    .all(|needle| trimmed.contains(needle));

    has_required_fields && !trimmed.contains("## Cumulative Summary")
}

pub(crate) fn extract_cumulative_summary_section(markdown: &str) -> Option<String> {
    let mut lines = markdown.lines();
    for line in lines.by_ref() {
        if line.trim() == "## Cumulative Summary" {
            break;
        }
    }

    let mut out = String::new();
    for line in lines {
        if line.trim_start().starts_with("## ") {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }

    let trimmed = out.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) struct RenderAutoSummaryMarkdownParams<'a> {
    pub(crate) thread_id: &'a str,
    pub(crate) cut_rule_id: &'a str,
    pub(crate) stride_messages: u64,
    pub(crate) target_message_ordinal: u64,
    pub(crate) to_seq: u64,
    pub(crate) to_message_id: &'a str,
    pub(crate) base_summary_artifact_id: Option<&'a str>,
    pub(crate) base_summary_markdown: Option<&'a str>,
    pub(crate) basis_note: Option<&'a str>,
    pub(crate) delta: AutoSummaryDelta,
    pub(crate) bootstrap: bool,
}

pub(crate) fn render_auto_compaction_summary_markdown_v0_2(
    params: RenderAutoSummaryMarkdownParams<'_>,
) -> String {
    let mut out = String::new();
    push_bounded(
        &mut out,
        &format!(
            "# Compaction summary (auto)\n\n- kind: cumulative_v1\n- thread_id: {}\n- cut_rule_id: {}\n- stride_messages: {}\n- target_message_ordinal: {}\n- to_seq: {}\n- to_message_id: {}\n",
            params.thread_id,
            params.cut_rule_id,
            params.stride_messages,
            params.target_message_ordinal,
            params.to_seq,
            params.to_message_id
        ),
        MAX_SUMMARY_MARKDOWN_CHARS,
    );

    if let Some(base_id) = params.base_summary_artifact_id {
        push_bounded(
            &mut out,
            &format!("- base_summary_artifact_id: {base_id}\n"),
            MAX_SUMMARY_MARKDOWN_CHARS,
        );
    } else {
        push_bounded(
            &mut out,
            "- base_summary_artifact_id: null\n",
            MAX_SUMMARY_MARKDOWN_CHARS,
        );
    }

    if let Some(note) = params.basis_note.filter(|n| !n.trim().is_empty()) {
        push_bounded(
            &mut out,
            &format!("- basis_note: {note}\n"),
            MAX_SUMMARY_MARKDOWN_CHARS,
        );
    }

    push_bounded(
        &mut out,
        &format!(
            "- delta_message_count: {}\n- delta_actors: {}\n\n",
            params.delta.message_count,
            render_actor_counts(&params.delta.actor_counts)
        ),
        MAX_SUMMARY_MARKDOWN_CHARS,
    );

    push_bounded(
        &mut out,
        "## Cumulative Summary\n\n",
        MAX_SUMMARY_MARKDOWN_CHARS,
    );

    if params.bootstrap {
        if !params.delta.top_keywords.is_empty() {
            push_bounded(
                &mut out,
                &format!(
                    "Topics so far (best-effort): {}\n\n",
                    params.delta.top_keywords.join(", ")
                ),
                MAX_SUMMARY_MARKDOWN_CHARS,
            );
        }
    } else if let Some(base) = params.base_summary_markdown {
        let base_section =
            extract_cumulative_summary_section(base).unwrap_or_else(|| base.trim().to_string());
        push_bounded(&mut out, &base_section, MAX_SUMMARY_MARKDOWN_CHARS);
        push_bounded(&mut out, "\n\n", MAX_SUMMARY_MARKDOWN_CHARS);
    }

    if !params.delta.top_keywords.is_empty() {
        push_bounded(
            &mut out,
            &format!("Delta topics: {}\n\n", params.delta.top_keywords.join(", ")),
            MAX_SUMMARY_MARKDOWN_CHARS,
        );
    }

    if !params.delta.notable_lines.is_empty() {
        push_bounded(
            &mut out,
            "Notable items (best-effort):\n",
            MAX_SUMMARY_MARKDOWN_CHARS,
        );
        for line in &params.delta.notable_lines {
            push_bounded(
                &mut out,
                &format!("- {}\n", line),
                MAX_SUMMARY_MARKDOWN_CHARS,
            );
        }
        push_bounded(&mut out, "\n", MAX_SUMMARY_MARKDOWN_CHARS);
    }

    push_bounded(
        &mut out,
        "## Recent Delta Highlights\n\n",
        MAX_SUMMARY_MARKDOWN_CHARS,
    );
    if params.delta.recent_highlights.is_empty() {
        push_bounded(&mut out, "(none)\n", MAX_SUMMARY_MARKDOWN_CHARS);
    } else {
        for line in &params.delta.recent_highlights {
            push_bounded(
                &mut out,
                &format!("- {}\n", line),
                MAX_SUMMARY_MARKDOWN_CHARS,
            );
        }
    }

    truncate_chars(&out, MAX_SUMMARY_MARKDOWN_CHARS)
}

fn render_actor_counts(counts: &[(String, u64)]) -> String {
    let mut parts = Vec::new();
    for (actor, count) in counts.iter().take(6) {
        parts.push(format!("{actor}={count}"));
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

fn push_bounded(out: &mut String, chunk: &str, max_chars: usize) {
    if out.chars().count() >= max_chars {
        return;
    }
    let remaining = max_chars.saturating_sub(out.chars().count());
    out.push_str(&truncate_chars(chunk, remaining));
}

fn tokenize_words_bounded(input: &str, max_chars: usize) -> Vec<String> {
    const MAX_WORDS: usize = 512;
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut seen = 0usize;

    for ch in input.chars() {
        seen += 1;
        if seen > max_chars {
            break;
        }

        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            cur.push(ch.to_ascii_lowercase());
            continue;
        }

        push_word(&mut words, &mut cur);
        if words.len() >= MAX_WORDS {
            break;
        }
    }
    push_word(&mut words, &mut cur);
    words
}

fn push_word(out: &mut Vec<String>, cur: &mut String) {
    if cur.len() < 3 {
        cur.clear();
        return;
    }
    if cur.chars().all(|ch| ch.is_ascii_digit()) {
        cur.clear();
        return;
    }
    if !cur.chars().any(|ch| ch.is_alphabetic()) {
        cur.clear();
        return;
    }
    out.push(std::mem::take(cur));
}

fn is_stopword(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "and"
            | "for"
            | "that"
            | "with"
            | "this"
            | "from"
            | "into"
            | "when"
            | "then"
            | "than"
            | "have"
            | "has"
            | "had"
            | "are"
            | "was"
            | "were"
            | "will"
            | "would"
            | "should"
            | "could"
            | "can"
            | "cant"
            | "won"
            | "don"
            | "not"
            | "you"
            | "your"
            | "yours"
            | "our"
            | "ours"
            | "they"
            | "them"
            | "their"
            | "what"
            | "why"
            | "how"
            | "its"
            | "it"
            | "in"
            | "on"
            | "of"
            | "to"
            | "a"
            | "an"
            | "as"
            | "at"
            | "by"
            | "or"
            | "is"
            | "be"
            | "if"
            | "we"
            | "i"
            | "me"
            | "my"
    )
}

fn extract_notable_lines(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in content.lines().take(64) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let lower = line.to_ascii_lowercase();
        let looks_notable = lower.starts_with("todo")
            || lower.starts_with("fix")
            || lower.starts_with("decision")
            || lower.starts_with("plan")
            || lower.starts_with("goal")
            || lower.starts_with("acceptance")
            || lower.starts_with("risk")
            || lower.starts_with("next")
            || lower.starts_with("open question")
            || lower.starts_with("- todo")
            || lower.starts_with("- fix")
            || lower.starts_with("- decision")
            || lower.starts_with("- plan")
            || lower.starts_with("- goal");

        if !looks_notable {
            continue;
        }

        out.push(truncate_chars(line, MAX_LINE_CHARS));
        if out.len() >= 8 {
            break;
        }
    }
    out
}

fn truncate_chars(input: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    if input.chars().count() <= max_len {
        return input.to_string();
    }
    input
        .chars()
        .take(max_len.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_legacy_metadata_placeholder() {
        let legacy = "# Compaction summary (auto)\n\n- kind: cumulative_v1\n- cut_rule_id: stride_messages_v1/2\n- stride_messages: 2\n- target_message_ordinal: 2\n- to_seq: 4\n- to_message_id: m2\n";
        assert!(summary_markdown_is_legacy_metadata_placeholder(legacy));

        let v0_2 = format!("{legacy}\n## Cumulative Summary\n\nok\n");
        assert!(!summary_markdown_is_legacy_metadata_placeholder(&v0_2));
    }

    #[test]
    fn accumulator_extracts_words_and_notables() {
        let mut acc = AutoSummaryAccumulator::default();
        acc.observe_message(
            "alice",
            "Goal: ship compaction status\nTODO: add endpoint\nhello world",
        );
        let delta = acc.finish();
        assert_eq!(delta.message_count, 1);
        assert!(!delta.top_keywords.is_empty());
        assert!(delta
            .notable_lines
            .iter()
            .any(|l| l.to_ascii_lowercase().contains("todo")));
        assert!(!delta.recent_highlights.is_empty());
    }
}
