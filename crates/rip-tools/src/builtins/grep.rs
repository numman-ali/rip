use std::fs::File;
use std::io::{BufRead, BufReader};

use ignore::WalkBuilder;
use regex::RegexBuilder;
use serde::Deserialize;
use serde_json::json;

use crate::{ToolInvocation, ToolOutput};

use super::{
    build_globset, globsets_match, normalize_rel_path, parse_args, resolve_path, BuiltinToolConfig,
};

#[derive(Deserialize)]
struct GrepArgs {
    pattern: String,
    path: Option<String>,
    regex: Option<bool>,
    case_sensitive: Option<bool>,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
    max_depth: Option<usize>,
    include_hidden: Option<bool>,
    follow_symlinks: Option<bool>,
}

pub(super) fn run_grep(invocation: ToolInvocation, config: &BuiltinToolConfig) -> ToolOutput {
    let args: GrepArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let root = args.path.unwrap_or_else(|| ".".to_string());
    let root_path = match resolve_path(&config.workspace_root, &root) {
        Ok(path) => path,
        Err(err) => return ToolOutput::failure(vec![err]),
    };

    let regex_enabled = args.regex.unwrap_or(true);
    let case_sensitive = args.case_sensitive.unwrap_or(true);
    let max_results = args.max_results.unwrap_or(config.max_results);
    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);
    let max_depth = args.max_depth.unwrap_or(config.max_depth);
    let include_hidden = args.include_hidden.unwrap_or(config.include_hidden);
    let follow_symlinks = args.follow_symlinks.unwrap_or(config.follow_symlinks);

    let include_set = match build_globset(args.include.as_deref()) {
        Ok(set) => set,
        Err(err) => return ToolOutput::invalid_args(err),
    };
    let exclude_set = match build_globset(args.exclude.as_deref()) {
        Ok(set) => set,
        Err(err) => return ToolOutput::invalid_args(err),
    };

    let pattern = if regex_enabled {
        args.pattern
    } else {
        regex::escape(&args.pattern)
    };

    let regex = match RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
    {
        Ok(regex) => regex,
        Err(err) => return ToolOutput::invalid_args(format!("invalid regex: {err}")),
    };

    let mut builder = WalkBuilder::new(&root_path);
    builder
        .hidden(!include_hidden)
        .follow_links(follow_symlinks);
    builder.max_depth(Some(max_depth));

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut matches = 0usize;

    'walk: for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                stderr.push(err.to_string());
                continue;
            }
        };

        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let path = entry.path();
            let rel = normalize_rel_path(&config.workspace_root, path);
            if !globsets_match(&include_set, &exclude_set, &rel) {
                continue;
            }

            let file = match File::open(path) {
                Ok(file) => file,
                Err(err) => {
                    stderr.push(format!("{rel}: {err}"));
                    continue;
                }
            };
            let mut reader = BufReader::new(file);
            let mut buffer = String::new();
            let mut bytes_read = 0usize;
            let mut line_no = 0usize;

            loop {
                buffer.clear();
                let read = match reader.read_line(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(err) => {
                        stderr.push(format!("{rel}: {err}"));
                        break;
                    }
                };
                line_no += 1;
                bytes_read += read;
                if bytes_read > max_bytes {
                    break;
                }

                if buffer.contains('\0') {
                    break;
                }

                let line = buffer.trim_end_matches(['\r', '\n']);
                if regex.is_match(line) {
                    stdout.push(format!("{rel}:{line_no}:{line}"));
                    matches += 1;
                    if matches >= max_results {
                        break 'walk;
                    }
                }
            }
        }
    }

    ToolOutput {
        stdout,
        stderr,
        exit_code: 0,
        artifacts: Some(json!({
            "root": normalize_rel_path(&config.workspace_root, &root_path),
            "matches": matches
        })),
    }
}
