use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    ops: Vec<PatchOp>,
}

impl Patch {
    pub fn parse(input: &str) -> Result<Self, PatchParseError> {
        parse_patch(input).map(|ops| Self { ops })
    }

    pub fn ops(&self) -> &[PatchOp] {
        &self.ops
    }

    pub fn affected_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for op in &self.ops {
            match op {
                PatchOp::AddFile { path, .. } => paths.push(path.clone()),
                PatchOp::DeleteFile { path } => paths.push(path.clone()),
                PatchOp::UpdateFile { path, moved_to, .. } => {
                    paths.push(path.clone());
                    if let Some(moved_to) = moved_to {
                        paths.push(moved_to.clone());
                    }
                }
            }
        }
        paths
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOp {
    AddFile {
        path: PathBuf,
        content: String,
    },
    DeleteFile {
        path: PathBuf,
    },
    UpdateFile {
        path: PathBuf,
        moved_to: Option<PathBuf>,
        hunks: Vec<PatchHunk>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchHunk {
    pub before: Vec<String>,
    pub after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchParseError {
    pub message: String,
}

impl fmt::Display for PatchParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PatchParseError {}

fn parse_patch(input: &str) -> Result<Vec<PatchOp>, PatchParseError> {
    let mut lines = input.lines();
    match lines.next() {
        Some("*** Begin Patch") => {}
        _ => {
            return Err(PatchParseError {
                message: "missing '*** Begin Patch' header".to_string(),
            })
        }
    }

    let mut ops = Vec::new();
    let mut pending = lines.peekable();
    while let Some(line) = pending.next() {
        if line == "*** End Patch" {
            return Ok(ops);
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let path = parse_rel_path(path)?;
            let mut content = Vec::new();
            while let Some(next) = pending.peek().copied() {
                if next.starts_with("*** ") {
                    break;
                }
                let next = pending.next().expect("peeked");
                let Some(rest) = next.strip_prefix('+') else {
                    return Err(PatchParseError {
                        message: format!("add file line must start with '+': {next}"),
                    });
                };
                content.push(rest.to_string());
            }
            let mut joined = content.join("\n");
            if !joined.is_empty() {
                joined.push('\n');
            }
            ops.push(PatchOp::AddFile {
                path,
                content: joined,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            let path = parse_rel_path(path)?;
            ops.push(PatchOp::DeleteFile { path });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let path = parse_rel_path(path)?;
            let mut moved_to = None;
            if let Some(next) = pending.peek().copied() {
                if let Some(dest) = next.strip_prefix("*** Move to: ") {
                    moved_to = Some(parse_rel_path(dest)?);
                    let _ = pending.next();
                }
            }

            let mut hunks: Vec<Vec<(char, String)>> = Vec::new();
            let mut current: Vec<(char, String)> = Vec::new();
            while let Some(next) = pending.peek().copied() {
                if next.starts_with("*** ") {
                    break;
                }
                let next = pending.next().expect("peeked");
                if next == "*** End of File" {
                    continue;
                }
                if next.starts_with("@@") {
                    if !current.is_empty() {
                        hunks.push(std::mem::take(&mut current));
                    }
                    continue;
                }
                let mut chars = next.chars();
                let prefix = chars.next().ok_or_else(|| PatchParseError {
                    message: "empty patch line".to_string(),
                })?;
                let rest = chars.as_str().to_string();
                match prefix {
                    ' ' | '+' | '-' => current.push((prefix, rest)),
                    _ => {
                        return Err(PatchParseError {
                            message: format!("invalid patch line prefix '{prefix}': {next}"),
                        })
                    }
                }
            }
            if !current.is_empty() {
                hunks.push(current);
            }
            if hunks.is_empty() {
                return Err(PatchParseError {
                    message: format!("update file has no hunks: {}", path.display()),
                });
            }

            let hunks = hunks
                .into_iter()
                .map(|lines| {
                    let mut before = Vec::new();
                    let mut after = Vec::new();
                    for (prefix, text) in lines {
                        match prefix {
                            ' ' => {
                                before.push(text.clone());
                                after.push(text);
                            }
                            '-' => before.push(text),
                            '+' => after.push(text),
                            _ => {}
                        }
                    }
                    PatchHunk { before, after }
                })
                .collect::<Vec<_>>();

            ops.push(PatchOp::UpdateFile {
                path,
                moved_to,
                hunks,
            });
            continue;
        }

        return Err(PatchParseError {
            message: format!("unexpected line: {line}"),
        });
    }

    Err(PatchParseError {
        message: "missing '*** End Patch' footer".to_string(),
    })
}

fn parse_rel_path(raw: &str) -> Result<PathBuf, PatchParseError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PatchParseError {
            message: "path cannot be empty".to_string(),
        });
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute() {
        return Err(PatchParseError {
            message: "absolute paths are not allowed".to_string(),
        });
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(PatchParseError {
            message: "path escapes workspace root".to_string(),
        });
    }
    Ok(path)
}

pub fn apply_hunks_to_text(
    original: &str,
    hunks: &[PatchHunk],
    file_display: &Path,
) -> Result<String, io::Error> {
    let line_ending = detect_line_ending(original);
    let (mut lines, trailing_newline) = split_lines(original);
    let mut cursor = 0usize;

    for hunk in hunks {
        if hunk.before.is_empty() {
            lines.extend_from_slice(&hunk.after);
            cursor = lines.len();
            continue;
        }
        let pos = find_subslice_from(&lines, &hunk.before, cursor).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "patch hunk does not apply to {} (missing context)",
                    file_display.display()
                ),
            )
        })?;
        let end = pos + hunk.before.len();
        lines.splice(pos..end, hunk.after.iter().cloned());
        cursor = pos + hunk.after.len();
    }

    Ok(join_lines(&lines, trailing_newline, line_ending))
}

fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn split_lines(text: &str) -> (Vec<String>, bool) {
    let trailing = text.ends_with('\n');
    let mut lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect::<Vec<_>>();
    if trailing {
        let _ = lines.pop();
    }
    (lines, trailing)
}

fn join_lines(lines: &[String], trailing_newline: bool, line_ending: &str) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut out = lines.join(line_ending);
    if trailing_newline {
        out.push_str(line_ending);
    }
    out
}

fn find_subslice_from(haystack: &[String], needle: &[String], start: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(start.min(haystack.len()));
    }
    if needle.len() > haystack.len() {
        return None;
    }
    (start..=(haystack.len() - needle.len()))
        .find(|&idx| &haystack[idx..idx + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_add_update_delete() {
        let patch = r#"*** Begin Patch
*** Add File: a.txt
+one
+two
*** Update File: b.txt
@@
 old
-gone
+new
*** Delete File: c.txt
*** End Patch"#;
        let parsed = Patch::parse(patch).expect("parse");
        assert_eq!(parsed.ops().len(), 3);
        assert_eq!(parsed.affected_paths().len(), 3);
    }

    #[test]
    fn apply_hunks_replaces_lines() {
        let hunks = vec![PatchHunk {
            before: vec!["a".to_string(), "b".to_string()],
            after: vec!["a".to_string(), "B".to_string(), "c".to_string()],
        }];
        let out = apply_hunks_to_text("a\nb\n", &hunks, Path::new("x.txt")).expect("apply");
        assert_eq!(out, "a\nB\nc\n");
    }
}
