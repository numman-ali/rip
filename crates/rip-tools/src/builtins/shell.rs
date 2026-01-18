use std::collections::HashMap;
use std::io::ErrorKind;
use std::process::Command;

use serde::Deserialize;

use crate::{ToolInvocation, ToolOutput};

use super::{default_shell_program, parse_args, resolve_path, split_output, BuiltinToolConfig};

#[derive(Deserialize)]
struct ShellArgs {
    command: String,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    max_bytes: Option<usize>,
}

pub(super) async fn run_bash(invocation: ToolInvocation, config: BuiltinToolConfig) -> ToolOutput {
    let args: ShellArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);
    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(&args.command);
    if let Some(cwd) = args.cwd.as_deref() {
        match resolve_path(&config.workspace_root, cwd) {
            Ok(path) => {
                cmd.current_dir(path);
            }
            Err(err) => return ToolOutput::failure(vec![err]),
        }
    }
    if let Some(envs) = &args.env {
        cmd.envs(envs);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = split_output(output.stdout, max_bytes);
            let stderr = split_output(output.stderr, max_bytes);
            ToolOutput {
                stdout,
                stderr,
                exit_code: output.status.code().unwrap_or(1),
                artifacts: None,
            }
        }
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                return run_shell_with_args(&args, &config, max_bytes);
            }
            ToolOutput::failure(vec![format!("bash failed: {err}")])
        }
    }
}

fn run_shell_with_args(
    args: &ShellArgs,
    config: &BuiltinToolConfig,
    max_bytes: usize,
) -> ToolOutput {
    let (program, mut program_args) = default_shell_program();
    program_args.push(args.command.clone());

    let mut cmd = Command::new(program);
    cmd.args(program_args);
    if let Some(cwd) = args.cwd.as_deref() {
        match resolve_path(&config.workspace_root, cwd) {
            Ok(path) => {
                cmd.current_dir(path);
            }
            Err(err) => return ToolOutput::failure(vec![err]),
        }
    }
    if let Some(envs) = &args.env {
        cmd.envs(envs);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = split_output(output.stdout, max_bytes);
            let stderr = split_output(output.stderr, max_bytes);
            ToolOutput {
                stdout,
                stderr,
                exit_code: output.status.code().unwrap_or(1),
                artifacts: None,
            }
        }
        Err(err) => ToolOutput::failure(vec![format!("shell failed: {err}")]),
    }
}
