#!/usr/bin/env python3
import json
from pathlib import Path
from typing import Any, Dict

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS_DIR = ROOT / "temp/openresponses/schema/components/schemas"
OUT_DIR = ROOT / "crates/rip-provider-openresponses/fixtures/openresponses"
ALLOWED_TYPES_PATH = ROOT / "schemas/openresponses/streaming_event_types.json"

SCHEMA_CACHE: Dict[Path, Any] = {}
SAMPLE_CACHE: Dict[Path, Any] = {}


def load_schema(path: Path) -> Any:
    if path in SCHEMA_CACHE:
        return SCHEMA_CACHE[path]
    with path.open() as f:
        data = json.load(f)
    SCHEMA_CACHE[path] = data
    return data


def resolve_ref(ref: str, base_dir: Path) -> Path:
    return (base_dir / ref).resolve()


def sample_from_schema(schema: Any, base_dir: Path, depth: int = 0) -> Any:
    if depth > 40:
        return None
    if not isinstance(schema, dict):
        return schema

    if "$ref" in schema:
        ref_path = resolve_ref(schema["$ref"], base_dir)
        if ref_path in SAMPLE_CACHE:
            return SAMPLE_CACHE[ref_path]
        ref_schema = load_schema(ref_path)
        value = sample_from_schema(ref_schema, ref_path.parent, depth + 1)
        SAMPLE_CACHE[ref_path] = value
        return value

    if "const" in schema:
        return schema["const"]
    if "enum" in schema:
        return schema["enum"][0]

    if "oneOf" in schema:
        options = schema["oneOf"]
        # Prefer a non-null option if available
        for opt in options:
            if isinstance(opt, dict) and opt.get("type") == "null":
                continue
            return sample_from_schema(opt, base_dir, depth + 1)
        return None

    if "anyOf" in schema:
        options = schema["anyOf"]
        for opt in options:
            if isinstance(opt, dict) and opt.get("type") == "null":
                return None
        return sample_from_schema(options[0], base_dir, depth + 1)

    if "allOf" in schema:
        merged = None
        for opt in schema["allOf"]:
            val = sample_from_schema(opt, base_dir, depth + 1)
            if merged is None:
                merged = val
            elif isinstance(merged, dict) and isinstance(val, dict):
                merged.update(val)
        return merged

    schema_type = schema.get("type")
    if schema_type == "object" or "properties" in schema:
        props = schema.get("properties", {})
        obj: Dict[str, Any] = {}
        for name in schema.get("required", []):
            obj[name] = sample_from_schema(props.get(name, {}), base_dir, depth + 1)
        if not obj and schema.get("additionalProperties"):
            return {}
        return obj

    if schema_type == "array":
        items = schema.get("items", {})
        min_items = schema.get("minItems", 0)
        if min_items > 0:
            return [sample_from_schema(items, base_dir, depth + 1) for _ in range(min_items)]
        return []

    if schema_type == "string":
        if "default" in schema:
            return schema["default"]
        if "minLength" in schema:
            return "x" * int(schema["minLength"])
        return ""

    if schema_type == "integer":
        if "default" in schema:
            return schema["default"]
        if "minimum" in schema:
            return int(schema["minimum"])
        return 0

    if schema_type == "number":
        if "default" in schema:
            return schema["default"]
        if "minimum" in schema:
            return schema["minimum"]
        return 0

    if schema_type == "boolean":
        if "default" in schema:
            return schema["default"]
        return False

    if schema_type == "null":
        return None

    # Fallback for underspecified schemas
    return {}


def ensure_type_field(event: Dict[str, Any], schema: Dict[str, Any]) -> None:
    if "type" in event:
        return
    props = schema.get("properties", {})
    type_schema = props.get("type", {})
    if isinstance(type_schema, dict) and "enum" in type_schema:
        event["type"] = type_schema["enum"][0]


def generate_events() -> list:
    allowed_types = json.loads(ALLOWED_TYPES_PATH.read_text())
    type_to_event: Dict[str, Any] = {}
    for path in sorted(SCHEMAS_DIR.glob("*StreamingEvent.json")):
        schema = load_schema(path)
        event = sample_from_schema(schema, path.parent)
        if not isinstance(event, dict):
            event = {}
        ensure_type_field(event, schema)
        event_type = event.get("type")
        if isinstance(event_type, str) and event_type in allowed_types:
            type_to_event.setdefault(event_type, event)

    missing = [t for t in allowed_types if t not in type_to_event]
    if missing:
        raise SystemExit(f"missing schemas for types: {', '.join(missing)}")

    events = [type_to_event[t] for t in allowed_types]
    # Assign monotonic sequence numbers
    seq = 1
    for event in events:
        event["sequence_number"] = seq
        seq += 1
    return events


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    events = generate_events()

    jsonl_path = OUT_DIR / "stream_all.jsonl"
    sse_path = OUT_DIR / "stream_all.sse"

    with jsonl_path.open("w") as jf, sse_path.open("w") as sf:
        for event in events:
            line = json.dumps(event, separators=(",", ":"), sort_keys=True)
            jf.write(line + "\n")
            event_type = event.get("type", "")
            if event_type:
                sf.write(f"event: {event_type}\n")
            sf.write(f"data: {line}\n\n")
        sf.write("data: [DONE]\n\n")

    print(f"wrote {jsonl_path}")
    print(f"wrote {sse_path}")


if __name__ == "__main__":
    main()
