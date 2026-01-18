use serde::Deserialize;
use serde_json::json;

use crate::{ToolInvocation, ToolOutput};

use super::{parse_args, BuiltinToolConfig};

#[derive(Deserialize)]
struct ApplyPatchArgs {
    patch: String,
}

pub(super) fn run_apply_patch(
    invocation: ToolInvocation,
    config: &BuiltinToolConfig,
) -> ToolOutput {
    let args: ApplyPatchArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let workspace = match rip_workspace::Workspace::new(&config.workspace_root) {
        Ok(workspace) => workspace,
        Err(err) => return ToolOutput::failure(vec![format!("apply_patch failed: {err}")]),
    };

    match workspace.apply_patch(&args.patch) {
        Ok(result) => ToolOutput {
            stdout: vec![format!("patched {} file(s)", result.changed_files.len())],
            stderr: Vec::new(),
            exit_code: 0,
            artifacts: Some(json!({
                "changed_files": result.changed_files,
            })),
        },
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
            ToolOutput::invalid_args(format!("invalid patch: {err}"))
        }
        Err(err) => ToolOutput::failure(vec![format!("apply_patch failed: {err}")]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn apply_patch_creates_file() {
        let dir = tempdir().expect("tmp");
        let root = dir.path().to_path_buf();
        let config = BuiltinToolConfig {
            workspace_root: root.clone(),
            ..BuiltinToolConfig::default()
        };

        let patch = r#"*** Begin Patch
*** Add File: note.txt
+hi
*** End Patch"#;

        let out = run_apply_patch(
            ToolInvocation {
                name: "apply_patch".to_string(),
                args: json!({ "patch": patch }),
                timeout_ms: None,
            },
            &config,
        );

        assert_eq!(out.exit_code, 0);
        assert_eq!(fs::read_to_string(root.join("note.txt")).unwrap(), "hi\n");
    }
}
