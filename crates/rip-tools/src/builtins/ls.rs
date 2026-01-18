use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::json;

use crate::{ToolInvocation, ToolOutput};

use super::{
    build_globset, globsets_match, normalize_rel_path, parse_args, resolve_path, BuiltinToolConfig,
};

#[derive(Deserialize)]
struct LsArgs {
    path: Option<String>,
    recursive: Option<bool>,
    max_depth: Option<usize>,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    include_hidden: Option<bool>,
    follow_symlinks: Option<bool>,
}

pub(super) fn run_ls(invocation: ToolInvocation, config: &BuiltinToolConfig) -> ToolOutput {
    let args: LsArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let root = args.path.unwrap_or_else(|| ".".to_string());
    let root_path = match resolve_path(&config.workspace_root, &root) {
        Ok(path) => path,
        Err(err) => return ToolOutput::failure(vec![err]),
    };

    let include_hidden = args.include_hidden.unwrap_or(config.include_hidden);
    let follow_symlinks = args.follow_symlinks.unwrap_or(config.follow_symlinks);
    let recursive = args.recursive.unwrap_or(false);
    let max_depth = args.max_depth.unwrap_or(config.max_depth);

    let include_set = match build_globset(args.include.as_deref()) {
        Ok(set) => set,
        Err(err) => return ToolOutput::invalid_args(err),
    };
    let exclude_set = match build_globset(args.exclude.as_deref()) {
        Ok(set) => set,
        Err(err) => return ToolOutput::invalid_args(err),
    };

    let mut builder = WalkBuilder::new(&root_path);
    builder
        .hidden(!include_hidden)
        .follow_links(follow_symlinks);
    if recursive {
        builder.max_depth(Some(max_depth));
    } else {
        builder.max_depth(Some(1));
    }

    let mut stdout = Vec::new();
    let mut errors = Vec::new();

    for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                errors.push(err.to_string());
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.path();
        let rel = normalize_rel_path(&config.workspace_root, path);
        if !globsets_match(&include_set, &exclude_set, &rel) {
            continue;
        }
        stdout.push(rel);
    }

    ToolOutput {
        stdout,
        stderr: errors,
        exit_code: 0,
        artifacts: Some(json!({
            "root": normalize_rel_path(&config.workspace_root, &root_path)
        })),
    }
}
