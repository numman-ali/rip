use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

const SURFACES: [&str; 6] = ["cli_i", "cli_h", "server", "sdk", "tui", "mcp"];
const ALLOWED_STATUSES: [&str; 3] = ["supported", "planned", "not_applicable"];

#[derive(Debug, Clone)]
struct Row {
    id: String,
    statuses: HashMap<&'static str, String>,
}

#[derive(Debug, Deserialize)]
struct GapList {
    gaps: Vec<GapEntry>,
}

#[derive(Debug, Deserialize)]
struct GapEntry {
    capability_id: String,
    surface: String,
    owner: String,
    reason: String,
    expires_on: String,
}

#[test]
fn parity_matrix_matches_registry() {
    let root = repo_root();
    let registry_path = root.join("docs/03_contracts/capability_registry.md");
    let matrix_path = root.join("docs/05_quality/surface-parity-matrix.md");

    let registry = fs::read_to_string(&registry_path).expect("read capability registry");
    let rows = parse_registry(&registry);
    let rendered = render_matrix(&rows);

    if std::env::var("RIP_UPDATE_PARITY_MATRIX").is_ok() {
        fs::write(&matrix_path, rendered).expect("write parity matrix");
        return;
    }

    let existing = fs::read_to_string(&matrix_path).expect("read parity matrix");
    assert_eq!(existing, rendered);
}

#[test]
fn gap_list_covers_supported_mismatches() {
    let root = repo_root();
    let registry_path = root.join("docs/03_contracts/capability_registry.md");
    let gaps_path = root.join("docs/05_quality/surface-parity-gaps.json");

    let registry = fs::read_to_string(&registry_path).expect("read capability registry");
    let rows = parse_registry(&registry);
    let active_surfaces = detect_active_surfaces(&rows);
    let mismatches = detect_mismatches(&rows, &active_surfaces);

    let gaps_raw = fs::read_to_string(&gaps_path).expect("read gap list");
    let gap_list: GapList = serde_json::from_str(&gaps_raw).expect("parse gap list");

    let mut declared = HashSet::new();
    for gap in &gap_list.gaps {
        if gap.capability_id.trim().is_empty()
            || gap.surface.trim().is_empty()
            || gap.owner.trim().is_empty()
            || gap.reason.trim().is_empty()
            || gap.expires_on.trim().is_empty()
        {
            panic!(
                "gap entries must include capability_id, surface, owner, reason, and expires_on"
            );
        }
        if !SURFACES.contains(&gap.surface.as_str()) {
            panic!("unknown surface in gap list: {}", gap.surface);
        }
        if !valid_date(&gap.expires_on) {
            panic!(
                "invalid expires_on format for {}: {}",
                gap.capability_id, gap.expires_on
            );
        }
        declared.insert((gap.capability_id.as_str(), gap.surface.as_str()));
    }

    for (capability_id, surface) in &mismatches {
        if !declared.contains(&(capability_id.as_str(), surface)) {
            panic!("missing gap entry for {capability_id} on {surface}");
        }
    }

    for (capability_id, surface) in declared {
        if !mismatches
            .iter()
            .any(|(id, surf)| id == capability_id && *surf == surface)
        {
            panic!("gap list contains unused entry for {capability_id} on {surface}");
        }
    }
}

fn parse_registry(contents: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cols: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if cols.len() < 11 {
            continue;
        }
        let id = cols[1];
        if id.is_empty() || id == "id" || id.starts_with("---") {
            continue;
        }

        let mut statuses = HashMap::new();
        for (idx, surface) in SURFACES.iter().enumerate() {
            let status = cols[4 + idx];
            if !ALLOWED_STATUSES.contains(&status) {
                panic!("unknown status '{status}' for {id} ({surface})");
            }
            statuses.insert(*surface, status.to_string());
        }
        rows.push(Row {
            id: id.to_string(),
            statuses,
        });
    }
    rows
}

fn detect_active_surfaces(rows: &[Row]) -> HashSet<&'static str> {
    let mut active = HashSet::new();
    for row in rows {
        for surface in SURFACES {
            if row
                .statuses
                .get(surface)
                .map(|value| value == "supported")
                .unwrap_or(false)
            {
                active.insert(surface);
            }
        }
    }
    active
}

fn detect_mismatches(rows: &[Row], active: &HashSet<&'static str>) -> Vec<(String, &'static str)> {
    let mut gaps = Vec::new();
    for row in rows {
        let supported_any = active.iter().any(|surface| {
            row.statuses
                .get(surface)
                .map(|status| status == "supported")
                .unwrap_or(false)
        });
        if !supported_any {
            continue;
        }
        for surface in active {
            if let Some(status) = row.statuses.get(surface) {
                if status == "planned" {
                    gaps.push((row.id.clone(), *surface));
                }
            }
        }
    }
    gaps
}

fn render_matrix(rows: &[Row]) -> String {
    let mut output = String::new();
    output.push_str("# Surface Parity Matrix\n\n");
    output.push_str("Generated from docs/03_contracts/capability_registry.md.\n\n");
    output.push_str("| id | cli_i | cli_h | server | sdk | tui | mcp |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");

    for row in rows {
        let mut line = format!("| {}", row.id);
        for surface in SURFACES {
            let status = row.statuses.get(surface).map(String::as_str).unwrap_or("-");
            line.push_str(&format!(" | {}", status));
        }
        line.push_str(" |\n");
        output.push_str(&line);
    }
    output
}

fn valid_date(value: &str) -> bool {
    if value.len() != 10 {
        return false;
    }
    let bytes = value.as_bytes();
    for (idx, ch) in bytes.iter().enumerate() {
        match idx {
            4 | 7 => {
                if *ch != b'-' {
                    return false;
                }
            }
            _ => {
                if !ch.is_ascii_digit() {
                    return false;
                }
            }
        }
    }
    true
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}
