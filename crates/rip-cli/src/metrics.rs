use rip_kernel::{Event, EventKind, ProviderEventStatus};
use serde_json::{json, Value};

#[derive(Default, Debug, Clone)]
pub(crate) struct RunMetrics {
    pub(crate) session_started_ms: Option<u64>,
    pub(crate) session_ended_ms: Option<u64>,
    pub(crate) session_end_reason: Option<String>,
    pub(crate) first_output_ms: Option<u64>,
    pub(crate) openresponses: OpenResponsesMetrics,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct OpenResponsesMetrics {
    pub(crate) endpoint: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) status: Option<u16>,
    pub(crate) request_id: Option<String>,
    pub(crate) content_type: Option<String>,
    pub(crate) request_started_ms: Option<u64>,
    pub(crate) response_headers_ms: Option<u64>,
    pub(crate) response_first_byte_ms: Option<u64>,
    pub(crate) first_provider_event_ms: Option<u64>,
    pub(crate) invalid_json: bool,
}

impl RunMetrics {
    pub(crate) fn observe(&mut self, event: &Event) {
        match &event.kind {
            EventKind::SessionStarted { .. } => {
                if self.session_started_ms.is_none() {
                    self.session_started_ms = Some(event.timestamp_ms);
                }
            }
            EventKind::OutputTextDelta { .. } => {
                if self.first_output_ms.is_none() {
                    self.first_output_ms = Some(event.timestamp_ms);
                }
            }
            EventKind::SessionEnded { reason } => {
                if self.session_ended_ms.is_none() {
                    self.session_ended_ms = Some(event.timestamp_ms);
                    self.session_end_reason = Some(reason.clone());
                }
            }
            EventKind::OpenResponsesRequestStarted {
                endpoint,
                model,
                request_index,
                ..
            } => {
                if *request_index == 0 && self.openresponses.request_started_ms.is_none() {
                    self.openresponses.request_started_ms = Some(event.timestamp_ms);
                    self.openresponses.endpoint = Some(endpoint.clone());
                    self.openresponses.model = model.clone();
                }
            }
            EventKind::OpenResponsesResponseHeaders {
                request_index,
                status,
                request_id,
                content_type,
                ..
            } => {
                if *request_index == 0 && self.openresponses.response_headers_ms.is_none() {
                    self.openresponses.response_headers_ms = Some(event.timestamp_ms);
                    self.openresponses.status = Some(*status);
                    self.openresponses.request_id = request_id.clone();
                    self.openresponses.content_type = content_type.clone();
                }
            }
            EventKind::OpenResponsesResponseFirstByte { request_index, .. } => {
                if *request_index == 0 && self.openresponses.response_first_byte_ms.is_none() {
                    self.openresponses.response_first_byte_ms = Some(event.timestamp_ms);
                }
            }
            EventKind::ProviderEvent {
                provider, status, ..
            } => {
                if provider == "openresponses" {
                    if self.openresponses.first_provider_event_ms.is_none() {
                        self.openresponses.first_provider_event_ms = Some(event.timestamp_ms);
                    }
                    if *status == ProviderEventStatus::InvalidJson {
                        self.openresponses.invalid_json = true;
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        let ttft_ms = delta(self.session_started_ms, self.first_output_ms);
        let e2e_ms = delta(self.session_started_ms, self.session_ended_ms);

        let openresponses = if self.openresponses.request_started_ms.is_some() {
            json!({
                "endpoint": self.openresponses.endpoint,
                "model": self.openresponses.model,
                "status": self.openresponses.status,
                "request_id": self.openresponses.request_id,
                "content_type": self.openresponses.content_type,
                "invalid_json": self.openresponses.invalid_json,
                "session_overhead_ms": delta(self.session_started_ms, self.openresponses.request_started_ms),
                "headers_ms": delta(self.openresponses.request_started_ms, self.openresponses.response_headers_ms),
                "first_byte_ms": delta(self.openresponses.request_started_ms, self.openresponses.response_first_byte_ms),
                "first_provider_event_ms": delta(self.openresponses.request_started_ms, self.openresponses.first_provider_event_ms),
                "first_output_ms": delta(self.openresponses.request_started_ms, self.first_output_ms),
            })
        } else {
            Value::Null
        };

        json!({
            "session_started_ms": self.session_started_ms,
            "session_ended_ms": self.session_ended_ms,
            "session_end_reason": self.session_end_reason,
            "ttft_ms": ttft_ms,
            "e2e_ms": e2e_ms,
            "openresponses": openresponses,
        })
    }
}

fn delta(start: Option<u64>, end: Option<u64>) -> Option<u64> {
    Some(end?.saturating_sub(start?))
}
