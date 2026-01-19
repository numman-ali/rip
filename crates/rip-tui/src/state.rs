use rip_kernel::{Event, EventKind};

use crate::FrameStore;

const DEFAULT_MAX_FRAMES: usize = 10_000;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_000_000;

#[derive(Debug, Clone)]
pub struct TuiState {
    pub frames: FrameStore,
    pub selected_seq: Option<u64>,
    pub auto_follow: bool,
    pub session_id: Option<String>,
    pub start_ms: Option<u64>,
    pub first_output_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub output_text: String,
    pub output_truncated: bool,
    max_output_bytes: usize,
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_FRAMES, DEFAULT_MAX_OUTPUT_BYTES)
    }
}

impl TuiState {
    pub fn new(max_frames: usize, max_output_bytes: usize) -> Self {
        Self {
            frames: FrameStore::new(max_frames),
            selected_seq: None,
            auto_follow: true,
            session_id: None,
            start_ms: None,
            first_output_ms: None,
            end_ms: None,
            output_text: String::new(),
            output_truncated: false,
            max_output_bytes: max_output_bytes.max(1),
        }
    }

    pub fn update(&mut self, event: Event) {
        if self.session_id.is_none() {
            self.session_id = Some(event.session_id.clone());
        }

        match &event.kind {
            EventKind::SessionStarted { .. } => {
                if self.start_ms.is_none() {
                    self.start_ms = Some(event.timestamp_ms);
                }
            }
            EventKind::OutputTextDelta { delta } => {
                if self.first_output_ms.is_none() {
                    self.first_output_ms = Some(event.timestamp_ms);
                }
                self.push_output(delta);
            }
            EventKind::SessionEnded { .. } => {
                if self.end_ms.is_none() {
                    self.end_ms = Some(event.timestamp_ms);
                }
            }
            _ => {}
        }

        let seq = event.seq;
        self.frames.push(event);
        if self.auto_follow || self.selected_seq.is_none() {
            self.selected_seq = Some(seq);
        }
    }

    pub fn selected_event(&self) -> Option<&Event> {
        let seq = self.selected_seq?;
        self.frames.get_by_seq(seq)
    }

    pub fn ttft_ms(&self) -> Option<u64> {
        Some(self.first_output_ms?.saturating_sub(self.start_ms?))
    }

    pub fn e2e_ms(&self) -> Option<u64> {
        Some(self.end_ms?.saturating_sub(self.start_ms?))
    }

    fn push_output(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }

        self.output_text.push_str(delta);
        if self.output_text.len() <= self.max_output_bytes {
            return;
        }

        self.output_truncated = true;
        let keep = self.max_output_bytes / 2;
        let start = self.output_text.len().saturating_sub(keep);
        self.output_text = self.output_text[start..].to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::{Event, EventKind};

    fn event(seq: u64, timestamp_ms: u64, kind: EventKind) -> Event {
        Event {
            id: format!("e{seq}"),
            session_id: "s1".to_string(),
            timestamp_ms,
            seq,
            kind,
        }
    }

    #[test]
    fn computes_ttft_and_e2e() {
        let mut state = TuiState::new(100, 1024);
        state.update(event(
            0,
            1000,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        state.update(event(
            1,
            1300,
            EventKind::OutputTextDelta {
                delta: "a".to_string(),
            },
        ));
        state.update(event(
            2,
            1800,
            EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        ));
        assert_eq!(state.ttft_ms(), Some(300));
        assert_eq!(state.e2e_ms(), Some(800));
    }
}
