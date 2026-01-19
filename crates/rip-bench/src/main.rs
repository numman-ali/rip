use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[cfg(not(test))]
use std::sync::Arc;
#[cfg(not(test))]
use std::time::Instant;
#[cfg(not(test))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(test))]
use rip_kernel::{Event, EventKind, ProviderEventStatus};
#[cfg(not(test))]
use rip_log::write_snapshot;
#[cfg(not(test))]
use rip_provider_openresponses::{
    CreateResponseBuilder, ItemParam, ParsedEvent, ParsedEventKind, SseDecoder,
};
#[cfg(not(test))]
use rip_tools::{
    register_builtin_tools, BuiltinToolConfig, CheckpointHook, CheckpointRecord, CheckpointRequest,
    CheckpointRewindRecord, ToolInvocation, ToolOutput, ToolRegistry, ToolRunner,
};
#[cfg(not(test))]
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct BudgetFile {
    benchmarks: Vec<BudgetEntry>,
}

#[derive(Debug, Deserialize)]
struct BudgetEntry {
    id: String,
    max: f64,
}

#[derive(Debug)]
struct BenchResult {
    id: &'static str,
    value: f64,
    unit: &'static str,
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if values.is_empty() {
        return 0.0;
    }
    values[values.len() / 2]
}

fn load_budgets(path: &PathBuf) -> std::io::Result<Vec<BudgetEntry>> {
    let raw = fs::read_to_string(path)?;
    let file: BudgetFile = serde_json::from_str(&raw).map_err(std::io::Error::other)?;
    Ok(file.benchmarks)
}

fn lookup_budget(budgets: &[BudgetEntry], id: &str) -> Option<f64> {
    budgets.iter().find(|entry| entry.id == id).map(|e| e.max)
}

#[cfg(not(test))]
const TTFT_SSE_EVENT: &str = "event: response.output_text.delta\n\
data: {\"type\":\"response.output_text.delta\",\"sequence_number\":1,\"item_id\":\"item_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"hi\",\"logprobs\":[]}\n\n";

#[cfg(not(test))]
const TOOL_LOOP_FIRST_SSE_FIXTURE: &str = "fixtures/openresponses/tool_loop_apply_patch_first.sse";
#[cfg(not(test))]
const TOOL_LOOP_FOLLOWUP_SSE_FIXTURE: &str = "fixtures/openresponses/tool_loop_followup.sse";

#[cfg(not(test))]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(not(test))]
fn emit(session_id: &str, seq: &mut u64, kind: EventKind) -> Event {
    let event = Event {
        id: format!("bench-{session_id}-{}", *seq),
        session_id: session_id.to_string(),
        timestamp_ms: now_ms(),
        seq: *seq,
        kind,
    };
    *seq += 1;
    event
}

#[cfg(not(test))]
fn map_openresponses_event(session_id: &str, seq: &mut u64, parsed: &ParsedEvent) -> Vec<Event> {
    let (status, data, raw) = match parsed.kind {
        ParsedEventKind::Done => (ProviderEventStatus::Done, None, Some(parsed.raw.clone())),
        ParsedEventKind::InvalidJson => (
            ProviderEventStatus::InvalidJson,
            None,
            Some(parsed.raw.clone()),
        ),
        ParsedEventKind::Event => (ProviderEventStatus::Event, parsed.data.clone(), None),
    };

    let mut events = vec![emit(
        session_id,
        seq,
        EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status,
            event_name: parsed.event.clone(),
            data: data.clone(),
            raw,
            errors: parsed.errors.clone(),
            response_errors: parsed.response_errors.clone(),
        },
    )];

    if let Some(delta) = data
        .as_ref()
        .and_then(|value| value.as_object())
        .and_then(|obj| {
            if obj.get("type").and_then(|value| value.as_str())
                != Some("response.output_text.delta")
            {
                return None;
            }
            obj.get("delta")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
    {
        events.push(emit(session_id, seq, EventKind::OutputTextDelta { delta }));
    }

    events
}

#[cfg(not(test))]
#[derive(Debug, Clone)]
struct FunctionCallItem {
    call_id: String,
    name: String,
    arguments: String,
}

#[cfg(not(test))]
fn observe_response_id(parsed: &ParsedEvent, response_id: &mut Option<String>) {
    if parsed.kind != ParsedEventKind::Event {
        return;
    }
    let Some(data) = parsed.data.as_ref() else {
        return;
    };
    let Some(obj) = data.as_object() else {
        return;
    };
    let Some(id) = obj
        .get("response")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
    else {
        return;
    };
    if !id.is_empty() {
        *response_id = Some(id.to_string());
    }
}

#[cfg(not(test))]
fn observe_function_call_done(parsed: &ParsedEvent) -> Option<FunctionCallItem> {
    if parsed.kind != ParsedEventKind::Event {
        return None;
    }
    let data = parsed.data.as_ref()?;
    let obj = data.as_object()?;
    if obj.get("type").and_then(|value| value.as_str()) != Some("response.output_item.done") {
        return None;
    }
    let item = obj.get("item").and_then(|value| value.as_object())?;
    if item.get("type").and_then(|value| value.as_str()) != Some("function_call") {
        return None;
    }
    Some(FunctionCallItem {
        call_id: item
            .get("call_id")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        name: item
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        arguments: item
            .get("arguments")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
    })
}

#[cfg(not(test))]
fn tool_events_to_function_call_output(tool_name: &str, events: &[Event]) -> serde_json::Value {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code: i32 = 1;
    let mut artifacts: Option<serde_json::Value> = None;
    let mut tool_error: Option<String> = None;

    for event in events {
        match &event.kind {
            EventKind::ToolStdout { chunk, .. } => stdout.push_str(chunk),
            EventKind::ToolStderr { chunk, .. } => stderr.push_str(chunk),
            EventKind::ToolEnded {
                exit_code: code,
                artifacts: tool_artifacts,
                ..
            } => {
                exit_code = *code;
                artifacts = tool_artifacts.clone();
            }
            EventKind::ToolFailed { error, .. } => tool_error = Some(error.clone()),
            _ => {}
        }
    }

    let ok = exit_code == 0 && tool_error.is_none();
    let mut obj = serde_json::Map::new();
    obj.insert(
        "tool".to_string(),
        serde_json::Value::String(tool_name.to_string()),
    );
    obj.insert("ok".to_string(), serde_json::Value::Bool(ok));
    obj.insert(
        "exit_code".to_string(),
        serde_json::Value::Number(serde_json::Number::from(exit_code as i64)),
    );
    obj.insert("stdout".to_string(), serde_json::Value::String(stdout));
    obj.insert("stderr".to_string(), serde_json::Value::String(stderr));
    if let Some(artifacts) = artifacts {
        obj.insert("artifacts".to_string(), artifacts);
    }
    if let Some(error) = tool_error {
        obj.insert("error".to_string(), serde_json::Value::String(error));
    }
    serde_json::Value::Object(obj)
}

#[cfg(not(test))]
fn bench_sse_parse_us_per_event() -> std::io::Result<BenchResult> {
    let sse_path = PathBuf::from("fixtures/openresponses/stream_all.sse");
    let payload = fs::read_to_string(&sse_path)?;

    // Warm schema caches.
    {
        let mut decoder = SseDecoder::new();
        let _ = decoder.push(&payload);
        let _ = decoder.finish();
    }

    let mut samples = Vec::new();
    let iterations = 200usize;
    for _ in 0..iterations {
        let mut decoder = SseDecoder::new();
        let start = Instant::now();
        let events = decoder.push(&payload);
        let _ = decoder.finish();
        let elapsed = start.elapsed();
        let per_event_us = elapsed.as_secs_f64() * 1_000_000.0 / (events.len().max(1) as f64);
        samples.push(per_event_us);
    }

    Ok(BenchResult {
        id: "sse_parse_us_per_event",
        value: median(samples),
        unit: "us/event",
    })
}

#[cfg(not(test))]
fn ttft_overhead_us(payload: &[u8], chunk_size: usize) -> f64 {
    let mut decoder = SseDecoder::new();
    let mut seq = 0u64;

    let start = Instant::now();
    for chunk in payload.chunks(chunk_size.max(1)) {
        let text = std::str::from_utf8(chunk).unwrap_or("\u{FFFD}");
        let parsed = decoder.push(text);
        for event in parsed {
            let frames = map_openresponses_event("bench-session", &mut seq, &event);
            if !frames.is_empty() {
                return start.elapsed().as_secs_f64() * 1_000_000.0;
            }
        }
    }

    let parsed = decoder.finish();
    for event in parsed {
        let frames = map_openresponses_event("bench-session", &mut seq, &event);
        if !frames.is_empty() {
            return start.elapsed().as_secs_f64() * 1_000_000.0;
        }
    }

    start.elapsed().as_secs_f64() * 1_000_000.0
}

#[cfg(not(test))]
fn bench_ttft_overhead_us() -> BenchResult {
    let payload = TTFT_SSE_EVENT.as_bytes();
    let chunk_size = 16usize;

    // Warm schema caches.
    let _ = ttft_overhead_us(payload, chunk_size);

    let mut samples = Vec::new();
    let iterations = 400usize;
    for _ in 0..iterations {
        samples.push(ttft_overhead_us(payload, chunk_size));
    }

    BenchResult {
        id: "ttft_overhead_us",
        value: median(samples),
        unit: "us",
    }
}

#[cfg(not(test))]
async fn bench_tool_runner_noop_us() -> BenchResult {
    let registry = std::sync::Arc::new(ToolRegistry::default());
    registry.register(
        "noop",
        std::sync::Arc::new(|_invocation| Box::pin(async move { ToolOutput::success(Vec::new()) })),
    );
    let runner = ToolRunner::new(registry, 1);

    let mut samples = Vec::new();
    let mut seq = 0u64;
    let iterations = 300usize;
    for idx in 0..iterations {
        let start = Instant::now();
        let _events = runner
            .run(
                "bench-session",
                &mut seq,
                ToolInvocation {
                    name: "noop".to_string(),
                    args: serde_json::json!({ "i": idx }),
                    timeout_ms: Some(5_000),
                },
            )
            .await;
        let elapsed = start.elapsed();
        samples.push(elapsed.as_secs_f64() * 1_000_000.0);
    }

    BenchResult {
        id: "tool_runner_noop_us",
        value: median(samples),
        unit: "us",
    }
}

#[cfg(not(test))]
fn bench_workspace_apply_patch_us() -> BenchResult {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("a.txt"), "one\ntwo\n").expect("write");
    let workspace = rip_workspace::Workspace::new(root).expect("workspace");

    let patch_a = r#"*** Begin Patch
*** Update File: a.txt
@@
-one
+ONE
 two
*** End Patch"#;
    let patch_b = r#"*** Begin Patch
*** Update File: a.txt
@@
-ONE
+one
 two
*** End Patch"#;

    // Warm.
    workspace.apply_patch(patch_a).expect("apply");
    workspace.apply_patch(patch_b).expect("apply");

    let mut samples = Vec::new();
    let iterations = 200usize;
    for idx in 0..iterations {
        let patch = if idx % 2 == 0 { patch_a } else { patch_b };
        let start = Instant::now();
        workspace.apply_patch(patch).expect("apply");
        let elapsed = start.elapsed();
        samples.push(elapsed.as_secs_f64() * 1_000_000.0);
    }

    BenchResult {
        id: "workspace_apply_patch_us",
        value: median(samples),
        unit: "us",
    }
}

#[cfg(not(test))]
#[derive(Default)]
struct InMemoryCheckpointHook;

#[cfg(not(test))]
impl CheckpointHook for InMemoryCheckpointHook {
    fn create(&self, request: CheckpointRequest) -> Result<CheckpointRecord, String> {
        Ok(CheckpointRecord {
            id: format!("checkpoint-{}", request.session_id),
            label: request.label,
            created_at_ms: 0,
            files: request
                .files
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
        })
    }

    fn rewind(
        &self,
        _session_id: &str,
        checkpoint_id: &str,
    ) -> Result<CheckpointRewindRecord, String> {
        Err(format!(
            "rewind unsupported in bench hook ({checkpoint_id})"
        ))
    }
}

#[cfg(not(test))]
async fn bench_e2e_loop_us() -> std::io::Result<BenchResult> {
    let dir = tempdir().expect("tmp");
    let root = dir.path().to_path_buf();
    fs::write(root.join("a.txt"), "one\ntwo\n").expect("write");

    let first_sse = fs::read_to_string(TOOL_LOOP_FIRST_SSE_FIXTURE)?;
    let followup_sse = fs::read_to_string(TOOL_LOOP_FOLLOWUP_SSE_FIXTURE)?;

    let registry = Arc::new(ToolRegistry::default());
    register_builtin_tools(
        &registry,
        BuiltinToolConfig {
            workspace_root: root.clone(),
            ..BuiltinToolConfig::default()
        },
    );
    let tool_runner =
        ToolRunner::with_checkpoint_hook(registry, 1, Arc::new(InMemoryCheckpointHook));

    let patch_a = r#"*** Begin Patch
*** Update File: a.txt
@@
-one
+ONE
 two
*** End Patch"#;
    let patch_b = r#"*** Begin Patch
*** Update File: a.txt
@@
-ONE
+one
 two
*** End Patch"#;

    // Warm schema caches + tool path.
    {
        let mut decoder = SseDecoder::new();
        let mut seq = 0u64;
        let mut events = Vec::new();
        let mut response_id: Option<String> = None;
        let mut call: Option<FunctionCallItem> = None;
        for event in decoder.push(&first_sse) {
            observe_response_id(&event, &mut response_id);
            if call.is_none() {
                call = observe_function_call_done(&event);
            }
            events.extend(map_openresponses_event("bench-warm", &mut seq, &event));
        }
        for event in decoder.finish() {
            observe_response_id(&event, &mut response_id);
            if call.is_none() {
                call = observe_function_call_done(&event);
            }
            events.extend(map_openresponses_event("bench-warm", &mut seq, &event));
        }

        let call = call.expect("function_call fixture");
        let tool_events = tool_runner
            .run(
                "bench-warm",
                &mut seq,
                ToolInvocation {
                    name: "apply_patch".to_string(),
                    args: serde_json::json!({ "patch": patch_a }),
                    timeout_ms: Some(5_000),
                },
            )
            .await;
        events.extend(tool_events.clone());
        let output_value = tool_events_to_function_call_output(&call.name, &tool_events);
        let output_json =
            serde_json::to_string(&output_value).unwrap_or_else(|_| "{\"ok\":false}".to_string());
        let followup_payload = CreateResponseBuilder::new()
            .model("fixture-model")
            .insert_raw(
                "previous_response_id",
                serde_json::Value::String(response_id.unwrap_or_else(|| "resp_1".to_string())),
            )
            .input_items(vec![ItemParam::function_call_output(
                call.call_id,
                serde_json::Value::String(output_json),
            )])
            .insert_raw("stream", serde_json::Value::Bool(true))
            .build();
        std::hint::black_box(followup_payload.errors());
        let mut decoder = SseDecoder::new();
        for event in decoder.push(&followup_sse) {
            events.extend(map_openresponses_event("bench-warm", &mut seq, &event));
        }
        for event in decoder.finish() {
            events.extend(map_openresponses_event("bench-warm", &mut seq, &event));
        }
        let _ = write_snapshot(root.join("snapshots"), "bench-warm", &events);
        let mut seq = 0u64;
        let _ = tool_runner
            .run(
                "bench-warm",
                &mut seq,
                ToolInvocation {
                    name: "apply_patch".to_string(),
                    args: serde_json::json!({ "patch": patch_b }),
                    timeout_ms: Some(5_000),
                },
            )
            .await;
    }

    let mut samples = Vec::new();
    let iterations = 80usize;
    for idx in 0..iterations {
        let start = Instant::now();

        let session_id = format!("bench-e2e-{idx}");

        let mut decoder = SseDecoder::new();
        let mut seq = 0u64;
        let mut events = Vec::new();
        let mut response_id: Option<String> = None;
        let mut call: Option<FunctionCallItem> = None;
        for event in decoder.push(&first_sse) {
            observe_response_id(&event, &mut response_id);
            if call.is_none() {
                call = observe_function_call_done(&event);
            }
            events.extend(map_openresponses_event(&session_id, &mut seq, &event));
        }
        for event in decoder.finish() {
            observe_response_id(&event, &mut response_id);
            if call.is_none() {
                call = observe_function_call_done(&event);
            }
            events.extend(map_openresponses_event(&session_id, &mut seq, &event));
        }

        let call = call.expect("function_call fixture");
        let patch = if idx % 2 == 0 { patch_a } else { patch_b };
        let mut args_value = serde_json::from_str::<serde_json::Value>(&call.arguments)
            .unwrap_or_else(|_| serde_json::Value::String(call.arguments.clone()));
        if let serde_json::Value::Object(map) = &mut args_value {
            map.insert(
                "patch".to_string(),
                serde_json::Value::String(patch.to_string()),
            );
        }
        let tool_events = tool_runner
            .run(
                &session_id,
                &mut seq,
                ToolInvocation {
                    name: "apply_patch".to_string(),
                    args: args_value,
                    timeout_ms: Some(5_000),
                },
            )
            .await;
        events.extend(tool_events.clone());

        let output_value = tool_events_to_function_call_output(&call.name, &tool_events);
        let output_json =
            serde_json::to_string(&output_value).unwrap_or_else(|_| "{\"ok\":false}".to_string());
        let followup_payload = CreateResponseBuilder::new()
            .model("fixture-model")
            .insert_raw(
                "previous_response_id",
                serde_json::Value::String(response_id.unwrap_or_else(|| "resp_1".to_string())),
            )
            .input_items(vec![ItemParam::function_call_output(
                call.call_id,
                serde_json::Value::String(output_json),
            )])
            .insert_raw("stream", serde_json::Value::Bool(true))
            .build();
        std::hint::black_box(followup_payload.errors());
        let mut decoder = SseDecoder::new();
        for event in decoder.push(&followup_sse) {
            events.extend(map_openresponses_event(&session_id, &mut seq, &event));
        }
        for event in decoder.finish() {
            events.extend(map_openresponses_event(&session_id, &mut seq, &event));
        }

        write_snapshot(root.join("snapshots"), &session_id, &events)?;

        let elapsed = start.elapsed();
        samples.push(elapsed.as_secs_f64() * 1_000_000.0);
    }

    Ok(BenchResult {
        id: "e2e_loop_us",
        value: median(samples),
        unit: "us",
    })
}

#[cfg(not(test))]
fn print_results(results: &[BenchResult], budgets: &[BudgetEntry]) {
    println!(
        "{:<28} {:>12} {:>10} {:>12}",
        "id", "value", "unit", "budget"
    );
    for result in results {
        let budget = lookup_budget(budgets, result.id)
            .map(|b| format!("{b:.0}"))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<28} {:>12.0} {:>10} {:>12}",
            result.id, result.value, result.unit, budget
        );
    }
}

fn enforce_budgets(results: &[BenchResult], budgets: &[BudgetEntry]) -> Result<(), String> {
    let mut failures = Vec::new();
    for result in results {
        let Some(max) = lookup_budget(budgets, result.id) else {
            failures.push(format!("missing budget for {}", result.id));
            continue;
        };
        if result.value > max {
            failures.push(format!(
                "{} exceeded: {:.0}{} > {:.0}{}",
                result.id, result.value, result.unit, max, result.unit
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[cfg(not(test))]
fn parse_budgets_path() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--budgets" {
            if let Some(path) = args.next() {
                return PathBuf::from(path);
            }
        }
    }
    PathBuf::from("docs/05_quality/benchmarks_budgets.json")
}

#[cfg(not(test))]
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let budgets_path = parse_budgets_path();
    let budgets = load_budgets(&budgets_path)?;

    let start = Instant::now();
    let mut results = Vec::new();
    results.push(bench_sse_parse_us_per_event()?);
    results.push(bench_ttft_overhead_us());
    results.push(bench_tool_runner_noop_us().await);
    results.push(bench_workspace_apply_patch_us());
    results.push(bench_e2e_loop_us().await?);
    let duration = start.elapsed();

    print_results(&results, &budgets);
    println!("bench_duration_ms {}", duration.as_secs_f64() * 1000.0);

    if let Err(err) = enforce_budgets(&results, &budgets) {
        eprintln!("{err}");
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn median_empty_returns_zero() {
        assert_eq!(median(Vec::new()), 0.0);
    }

    #[test]
    fn median_sorts_and_picks_middle() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(vec![4.0, 1.0, 3.0, 2.0]), 3.0);
    }

    #[test]
    fn lookup_budget_finds_match() {
        let budgets = vec![
            BudgetEntry {
                id: "a".to_string(),
                max: 1.0,
            },
            BudgetEntry {
                id: "b".to_string(),
                max: 2.0,
            },
        ];
        assert_eq!(lookup_budget(&budgets, "b"), Some(2.0));
        assert_eq!(lookup_budget(&budgets, "c"), None);
    }

    #[test]
    fn load_budgets_reads_and_parses_json() {
        let dir = tempdir().expect("tmp");
        let path = dir.path().join("budgets.json");
        fs::write(&path, r#"{ "benchmarks": [ { "id": "x", "max": 12.0 } ] }"#).expect("write");
        let budgets = load_budgets(&path).expect("load");
        assert_eq!(budgets.len(), 1);
        assert_eq!(budgets[0].id, "x");
        assert_eq!(budgets[0].max, 12.0);
    }

    #[test]
    fn load_budgets_reports_invalid_json() {
        let dir = tempdir().expect("tmp");
        let path = dir.path().join("bad.json");
        fs::write(&path, "not json").expect("write");
        assert!(load_budgets(&path).is_err());
    }

    #[test]
    fn enforce_budgets_errors_on_missing_budget() {
        let budgets = Vec::new();
        let results = vec![BenchResult {
            id: "missing",
            value: 1.0,
            unit: "us",
        }];
        let err = enforce_budgets(&results, &budgets).expect_err("should fail");
        assert!(err.contains("missing budget for missing"));
    }

    #[test]
    fn enforce_budgets_errors_on_exceeded_budget() {
        let budgets = vec![BudgetEntry {
            id: "slow".to_string(),
            max: 10.0,
        }];
        let results = vec![BenchResult {
            id: "slow",
            value: 11.0,
            unit: "us",
        }];
        let err = enforce_budgets(&results, &budgets).expect_err("should fail");
        assert!(err.contains("slow exceeded"));
    }

    #[test]
    fn enforce_budgets_allows_within_budget() {
        let budgets = vec![BudgetEntry {
            id: "fast".to_string(),
            max: 10.0,
        }];
        let results = vec![BenchResult {
            id: "fast",
            value: 9.0,
            unit: "us",
        }];
        enforce_budgets(&results, &budgets).expect("ok");
    }
}
