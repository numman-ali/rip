use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::{OutputViewMode, TuiState};

use super::theme::ThemeStyles;
use super::util::fmt_ms;

pub(super) fn render_status_bar(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    let session = state.session_id.as_deref().unwrap_or("-");
    let last_seq = state
        .frames
        .last_seq()
        .map(|seq| seq.to_string())
        .unwrap_or("-".to_string());
    let ttft = state
        .ttft_ms()
        .map(|ms| format!("{ms}ms"))
        .unwrap_or("-".to_string());
    let e2e = state
        .e2e_ms()
        .map(|ms| format!("{ms}ms"))
        .unwrap_or("-".to_string());

    let view = match state.output_view {
        OutputViewMode::Rendered => "canvas",
        OutputViewMode::Raw => "xray",
    };
    let theme_name = state.theme.as_str();

    let tool_count = state.running_tool_ids().count();
    let task_count = state.running_task_ids().count();
    let job_count = state.running_job_ids().count();
    let artifact_count = state.artifacts.len();
    let stalled = state.is_stalled(5_000);
    let error = state.has_error();
    let headers = fmt_ms(state.openresponses_headers_ms());
    let first_byte = fmt_ms(state.openresponses_first_byte_ms());
    let provider_event = fmt_ms(state.openresponses_first_provider_event_ms());
    let endpoint = state
        .openresponses_endpoint
        .as_deref()
        .or(state.preferred_openresponses_endpoint.as_deref());
    let model = state
        .openresponses_model
        .as_deref()
        .or(state.preferred_openresponses_model.as_deref());
    let llm = endpoint
        .map(|endpoint| {
            if endpoint.contains("openrouter.ai") {
                "openrouter"
            } else if endpoint.contains("api.openai.com") || endpoint.contains("openai.com") {
                "openai"
            } else {
                "openresponses"
            }
        })
        .map(|provider| match model {
            Some(model) if !model.trim().is_empty() => format!("{provider}:{model}"),
            _ => provider.to_string(),
        });

    let mut line = String::new();
    if let Some(msg) = state.status_message.as_deref() {
        line.push_str(" msg:");
        line.push_str(msg);
        line.push_str(" |");
    }
    if let Some(llm) = llm.as_deref() {
        line.push_str(" llm:");
        line.push_str(llm);
        line.push_str(" |");
    }
    line.push_str(&format!(
        " view:{view}  session:{session}  seq:{last_seq}  hdr:{headers}  fb:{first_byte}  evt:{provider_event}  TTFT:{ttft}  E2E:{e2e}  tools:{tool_count}/{}  tasks:{task_count}/{}  jobs:{job_count}/{}  arts:{artifact_count}  stalled:{stalled}  error:{error}  theme:{theme_name}",
        state.tools.len(),
        state.tasks.len(),
        state.jobs.len()
    ));
    let widget = Paragraph::new(Line::from(line)).style(theme.chrome).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from("RIP").style(theme.header)),
    );
    frame.render_widget(widget, area);
}
