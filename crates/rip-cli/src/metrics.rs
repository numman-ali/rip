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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn observe_tracks_openresponses_timings_and_serializes_json() {
        let mut metrics = RunMetrics::default();
        for event in [
            event(
                0,
                100,
                EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
            ),
            event(
                1,
                110,
                EventKind::OpenResponsesRequestStarted {
                    endpoint: "https://ignored.invalid".to_string(),
                    model: Some("ignored".to_string()),
                    request_index: 1,
                    kind: "response.create".to_string(),
                },
            ),
            event(
                2,
                120,
                EventKind::OpenResponsesRequestStarted {
                    endpoint: "https://api.openai.com/v1/responses".to_string(),
                    model: Some("gpt-5".to_string()),
                    request_index: 0,
                    kind: "response.create".to_string(),
                },
            ),
            event(
                3,
                150,
                EventKind::OpenResponsesResponseHeaders {
                    request_index: 1,
                    status: 500,
                    request_id: Some("ignored".to_string()),
                    content_type: Some("text/plain".to_string()),
                },
            ),
            event(
                4,
                160,
                EventKind::OpenResponsesResponseHeaders {
                    request_index: 0,
                    status: 200,
                    request_id: Some("req_123".to_string()),
                    content_type: Some("text/event-stream".to_string()),
                },
            ),
            event(
                5,
                170,
                EventKind::OpenResponsesResponseFirstByte { request_index: 0 },
            ),
            event(
                6,
                180,
                EventKind::ProviderEvent {
                    provider: "openresponses".to_string(),
                    status: ProviderEventStatus::Event,
                    event_name: Some("response.created".to_string()),
                    data: None,
                    raw: None,
                    errors: Vec::new(),
                    response_errors: Vec::new(),
                },
            ),
            event(
                7,
                190,
                EventKind::ProviderEvent {
                    provider: "openresponses".to_string(),
                    status: ProviderEventStatus::InvalidJson,
                    event_name: None,
                    data: None,
                    raw: Some("{".to_string()),
                    errors: vec!["bad json".to_string()],
                    response_errors: Vec::new(),
                },
            ),
            event(
                8,
                210,
                EventKind::OutputTextDelta {
                    delta: "hello".to_string(),
                },
            ),
            event(
                9,
                300,
                EventKind::SessionEnded {
                    reason: "done".to_string(),
                },
            ),
            event(
                10,
                320,
                EventKind::SessionEnded {
                    reason: "ignored".to_string(),
                },
            ),
        ] {
            metrics.observe(&event);
        }

        let actual = metrics.to_json();
        assert_eq!(
            actual,
            json!({
                "session_started_ms": 100,
                "session_ended_ms": 300,
                "session_end_reason": "done",
                "ttft_ms": 110,
                "e2e_ms": 200,
                "openresponses": {
                    "endpoint": "https://api.openai.com/v1/responses",
                    "model": "gpt-5",
                    "status": 200,
                    "request_id": "req_123",
                    "content_type": "text/event-stream",
                    "invalid_json": true,
                    "session_overhead_ms": 20,
                    "headers_ms": 40,
                    "first_byte_ms": 50,
                    "first_provider_event_ms": 60,
                    "first_output_ms": 90,
                }
            })
        );
    }

    #[test]
    fn to_json_uses_null_without_openresponses_request_and_delta_saturates() {
        let mut metrics = RunMetrics {
            session_started_ms: Some(100),
            session_ended_ms: Some(90),
            session_end_reason: Some("done".to_string()),
            first_output_ms: None,
            openresponses: OpenResponsesMetrics::default(),
        };

        metrics.observe(&event(
            0,
            80,
            EventKind::ProviderEvent {
                provider: "other".to_string(),
                status: ProviderEventStatus::InvalidJson,
                event_name: None,
                data: None,
                raw: None,
                errors: vec!["ignored".to_string()],
                response_errors: Vec::new(),
            },
        ));

        assert_eq!(
            metrics.to_json(),
            json!({
                "session_started_ms": 100,
                "session_ended_ms": 90,
                "session_end_reason": "done",
                "ttft_ms": Value::Null,
                "e2e_ms": 0,
                "openresponses": Value::Null,
            })
        );
    }
}
