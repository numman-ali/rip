use std::fs::{self, OpenOptions};
use std::io::Write;

use serde::Deserialize;
use serde_json::json;

use crate::{ToolInvocation, ToolOutput};

use super::{normalize_rel_path, parse_args, resolve_path, BuiltinToolConfig};

#[derive(Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
    append: Option<bool>,
    create: Option<bool>,
    atomic: Option<bool>,
}

pub(super) fn run_write(invocation: ToolInvocation, config: &BuiltinToolConfig) -> ToolOutput {
    let args: WriteArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let path = match resolve_path(&config.workspace_root, &args.path) {
        Ok(path) => path,
        Err(err) => return ToolOutput::failure(vec![err]),
    };

    let create = args.create.unwrap_or(true);
    let append = args.append.unwrap_or(false);
    let atomic = args.atomic.unwrap_or(true);

    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
    }

    let bytes_written = if append {
        let mut file = match OpenOptions::new().create(create).append(true).open(&path) {
            Ok(file) => file,
            Err(err) => return ToolOutput::failure(vec![format!("write failed: {err}")]),
        };
        if let Err(err) = file.write_all(args.content.as_bytes()) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
        args.content.len()
    } else if atomic {
        let tmp_path = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
        if let Err(err) = fs::write(&tmp_path, args.content.as_bytes()) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
        if path.exists() {
            if let Err(err) = fs::remove_file(&path) {
                return ToolOutput::failure(vec![format!("write failed: {err}")]);
            }
        }
        if let Err(err) = fs::rename(&tmp_path, &path) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
        args.content.len()
    } else {
        if let Err(err) = fs::write(&path, args.content.as_bytes()) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
        args.content.len()
    };

    ToolOutput {
        stdout: vec![format!("wrote {bytes_written} bytes")],
        stderr: Vec::new(),
        exit_code: 0,
        artifacts: Some(json!({
            "path": normalize_rel_path(&config.workspace_root, &path),
            "bytes_written": bytes_written
        })),
    }
}
