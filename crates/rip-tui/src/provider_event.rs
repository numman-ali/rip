use serde_json::Value;

pub(crate) fn event_type<'a>(
    event_name: Option<&'a str>,
    data: Option<&'a Value>,
) -> Option<&'a str> {
    if let Some(Value::Object(obj)) = data {
        if let Some(Value::String(value)) = obj.get("type") {
            return Some(value.as_str());
        }
    }
    event_name
}
