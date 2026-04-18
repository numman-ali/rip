pub use rip_tools::{ToolInvocation, ToolOutput, ToolRegistry};

mod runtime_impl {
    #![allow(dead_code)]
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/runtime.rs"));

    #[cfg(test)]
    mod coverage_tests {
        use super::*;
        use serde_json::json;
        use std::path::PathBuf;

        #[test]
        fn direct_files_for_invocation_covers_private_helper() {
            let write = files_for_invocation(&ToolInvocation {
                name: "write".to_string(),
                args: json!({"path": "note.txt"}),
                timeout_ms: None,
            })
            .expect("write args")
            .expect("write paths");
            assert_eq!(write, vec![PathBuf::from("note.txt")]);

            let apply_patch = files_for_invocation(&ToolInvocation {
                name: "apply_patch".to_string(),
                args: json!({"patch": "*** Begin Patch\n*** Add File: hello.txt\n+hi\n*** End Patch\n"}),
                timeout_ms: None,
            })
            .expect("patch args")
            .expect("patch paths");
            assert_eq!(apply_patch, vec![PathBuf::from("hello.txt")]);
        }
    }
}

mod builtins_impl {
    pub mod real {
        #![allow(dead_code)]
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/builtins/mod.rs"));

        #[cfg(test)]
        mod coverage_tests {
            use super::*;
            use serde_json::json;
            use tempfile::tempdir;

            #[test]
            fn direct_grep_run_covers_private_entrypoint() {
                let dir = tempdir().expect("tmp");
                std::fs::write(dir.path().join("notes.txt"), "Hello\nworld\n").expect("write");
                let config = BuiltinToolConfig {
                    workspace_root: dir.path().to_path_buf(),
                    ..BuiltinToolConfig::default()
                };

                let output = grep::run_grep(
                    crate::ToolInvocation {
                        name: "grep".to_string(),
                        args: json!({
                            "pattern": "hello",
                            "path": ".",
                            "case_sensitive": false,
                        }),
                        timeout_ms: None,
                    },
                    &config,
                );

                assert_eq!(output.stdout, vec!["notes.txt:1:Hello".to_string()]);
            }

            #[tokio::test]
            async fn direct_shell_run_covers_private_entrypoint() {
                let dir = tempdir().expect("tmp");
                let config = BuiltinToolConfig {
                    workspace_root: dir.path().to_path_buf(),
                    artifact_max_bytes: 64,
                    max_bytes: 64,
                    max_results: 10,
                    max_depth: 4,
                    follow_symlinks: false,
                    include_hidden: false,
                };

                let output = shell::run_bash(
                    crate::ToolInvocation {
                        name: "bash".to_string(),
                        args: json!({"command": "printf '1234567890'", "max_bytes": 4}),
                        timeout_ms: None,
                    },
                    config,
                )
                .await;

                assert_eq!(output.exit_code, 0);
                assert!(output
                    .artifacts
                    .as_ref()
                    .and_then(|artifacts| artifacts.get("stdout"))
                    .and_then(|stdout| stdout.get("artifact"))
                    .and_then(|artifact| artifact.get("path"))
                    .and_then(serde_json::Value::as_str)
                    .map(|path| path.contains(".rip/artifacts/blobs"))
                    .unwrap_or(false));
            }
        }
    }
}

mod builtins {
    pub use super::builtins_impl::real::*;
}
