use std::fs::File;
use std::io::{BufRead, BufReader};

use serde::Deserialize;
use serde_json::json;

use crate::{ToolInvocation, ToolOutput};

use super::{normalize_rel_path, parse_args, resolve_path, truncate_utf8, BuiltinToolConfig};

#[derive(Deserialize)]
struct ReadArgs {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
    max_bytes: Option<usize>,
}

pub(super) fn run_read(invocation: ToolInvocation, config: &BuiltinToolConfig) -> ToolOutput {
    let args: ReadArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    if let Some(start) = args.start_line {
        if start == 0 {
            return ToolOutput::invalid_args("line numbers are 1-based".to_string());
        }
    }
    if let Some(end) = args.end_line {
        if end == 0 {
            return ToolOutput::invalid_args("line numbers are 1-based".to_string());
        }
    }
    if let (Some(start), Some(end)) = (args.start_line, args.end_line) {
        if start > end {
            return ToolOutput::invalid_args("start_line must be <= end_line".to_string());
        }
    }

    let path = match resolve_path(&config.workspace_root, &args.path) {
        Ok(path) => path,
        Err(err) => return ToolOutput::failure(vec![err]),
    };

    let file = match File::open(&path) {
        Ok(file) => file,
        Err(err) => return ToolOutput::failure(vec![format!("read failed: {err}")]),
    };

    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);
    let mut reader = BufReader::new(file);
    let mut buffer = String::new();
    let mut output = Vec::new();
    let mut line_no = 0usize;
    let mut truncated = false;

    loop {
        buffer.clear();
        let _read = match reader.read_line(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) => return ToolOutput::failure(vec![format!("read failed: {err}")]),
        };
        line_no += 1;

        if let Some(start) = args.start_line {
            if line_no < start {
                continue;
            }
        }
        if let Some(end) = args.end_line {
            if line_no > end {
                break;
            }
        }

        output.extend_from_slice(buffer.as_bytes());
        if output.len() >= max_bytes {
            output.truncate(max_bytes);
            truncated = true;
            break;
        }
    }

    let (content, _, used_bytes) = truncate_utf8(&output, max_bytes);

    ToolOutput {
        stdout: vec![content],
        stderr: Vec::new(),
        exit_code: 0,
        artifacts: Some(json!({
            "path": normalize_rel_path(&config.workspace_root, &path),
            "bytes": used_bytes,
            "truncated": truncated,
            "start_line": args.start_line,
            "end_line": args.end_line
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn run_read_reads_file_from_workspace() {
        let dir = tempdir().expect("tmp");
        let root = dir.path().to_path_buf();
        let file = root.join("note.txt");
        fs::write(&file, "line1\nline2\n").expect("write");

        let config = BuiltinToolConfig {
            workspace_root: root.clone(),
            ..BuiltinToolConfig::default()
        };

        let output = run_read(
            ToolInvocation {
                name: "read".to_string(),
                args: json!({"path":"note.txt"}),
                timeout_ms: None,
            },
            &config,
        );

        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, vec!["line1\nline2\n".to_string()]);
        let artifacts = output.artifacts.expect("artifacts");
        assert_eq!(
            artifacts.get("path").and_then(|v| v.as_str()),
            Some("note.txt")
        );
    }
}
