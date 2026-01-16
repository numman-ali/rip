use once_cell::sync::Lazy;
use serde_json::Value;

static STREAM_EVENT_TYPES: Lazy<Vec<String>> = Lazy::new(|| {
    let raw = include_str!("../../../schemas/openresponses/streaming_event_types.json");
    serde_json::from_str(raw).expect("streaming_event_types.json valid")
});

#[derive(Debug)]
pub enum ValidationError {
    MissingType,
    InvalidType(String),
    InvalidJson,
}

pub fn allowed_stream_event_types() -> &'static [String] {
    &STREAM_EVENT_TYPES
}

pub fn validate_stream_event(value: &Value) -> Result<(), ValidationError> {
    let event_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or(ValidationError::MissingType)?;

    if STREAM_EVENT_TYPES.iter().any(|t| t == event_type) {
        Ok(())
    } else {
        Err(ValidationError::InvalidType(event_type.to_string()))
    }
}

pub fn validate_stream_event_json(json: &str) -> Result<(), ValidationError> {
    let value: Value = serde_json::from_str(json).map_err(|_| ValidationError::InvalidJson)?;
    validate_stream_event(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_list_is_non_empty() {
        assert!(!allowed_stream_event_types().is_empty());
    }

    #[test]
    fn validates_known_event_type() {
        let value = serde_json::json!({
            "type": "response.created",
            "sequence_number": 0,
            "response": {}
        });
        assert!(validate_stream_event(&value).is_ok());
    }

    #[test]
    fn rejects_unknown_event_type() {
        let value = serde_json::json!({"type": "unknown.event"});
        let err = validate_stream_event(&value).expect_err("invalid type");
        match err {
            ValidationError::InvalidType(t) => assert_eq!(t, "unknown.event"),
            _ => panic!("unexpected error"),
        }
    }

    #[test]
    fn rejects_missing_type() {
        let value = serde_json::json!({"foo": "bar"});
        let err = validate_stream_event(&value).expect_err("missing type");
        matches!(err, ValidationError::MissingType);
    }
}
