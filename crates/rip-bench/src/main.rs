use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use rip_provider_openresponses::SseDecoder;
use rip_tools::{ToolInvocation, ToolOutput, ToolRegistry, ToolRunner};
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

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let budgets_path = parse_budgets_path();
    let budgets = load_budgets(&budgets_path)?;

    let start = Instant::now();
    let mut results = Vec::new();
    results.push(bench_sse_parse_us_per_event()?);
    results.push(bench_tool_runner_noop_us().await);
    results.push(bench_workspace_apply_patch_us());
    let duration = start.elapsed();

    print_results(&results, &budgets);
    println!("bench_duration_ms {}", duration.as_secs_f64() * 1000.0);

    if let Err(err) = enforce_budgets(&results, &budgets) {
        eprintln!("{err}");
        std::process::exit(1);
    }

    Ok(())
}
