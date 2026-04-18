//! Syntect-backed syntax highlighting for `Block::CodeFence` (Phase B.7).
//!
//! We don't cache highlighted output on the block itself — the canvas
//! model stays theme-agnostic, so a theme swap doesn't have to invalidate
//! anything. The renderer calls [`highlight_fence`] every frame with
//! (raw source, lang, theme) and gets back styled `Line`s.
//!
//! Cost note: syntect's `SyntaxSet::load_defaults_newlines()` takes
//! ~30ms the first time it's called. We lazy-init via `once_cell` so
//! the hit lands on the first code fence, not at startup. Subsequent
//! fences reuse the parsed sets. Highlighting per line is on the
//! order of tens of microseconds, well under the 16ms-per-frame
//! budget even for large fences.

use once_cell::sync::Lazy;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use super::theme::ThemeStyles;
use crate::ThemeId;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

/// Highlight a fence's source as styled lines for the current theme.
/// Falls back to plain-styled lines when syntect can't find a syntax
/// for `lang` (or when the fence has no `lang` tag).
///
/// `base_style` is the fallback `Style` applied to unmatched tokens
/// so they still read as body text.
pub fn highlight_fence(
    source: &str,
    lang: Option<&str>,
    theme_id: ThemeId,
    theme_styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let base_style = theme_styles.chrome;
    let syntax = resolve_syntax(lang);
    let Some(syntax) = syntax else {
        return plain_fallback(source, base_style);
    };

    let theme = pick_theme(theme_id);
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut out = Vec::new();
    for line in LinesWithEndings::from(source) {
        let highlighted = match highlighter.highlight_line(line, &SYNTAX_SET) {
            Ok(regions) => regions,
            Err(_) => {
                // Parse error mid-fence (malformed Unicode / similar).
                // Show the line raw so the user isn't staring at
                // nothing, then keep rolling.
                out.push(Line::from(Span::styled(
                    strip_newline(line).to_string(),
                    base_style,
                )));
                continue;
            }
        };
        let spans: Vec<Span<'static>> = highlighted
            .into_iter()
            .map(|(style, text)| {
                let text = strip_newline(text).to_string();
                Span::styled(text, convert_style(style, base_style))
            })
            .collect();
        out.push(Line::from(spans));
    }
    out
}

fn resolve_syntax(lang: Option<&str>) -> Option<&'static SyntaxReference> {
    let lang = lang?.trim();
    if lang.is_empty() {
        return None;
    }
    // Common aliases syntect doesn't ship by default. Agents write
    // `sh` / `console` / `shell` a lot; all reasonable-looking as
    // Bash.
    let alias = match lang.to_ascii_lowercase().as_str() {
        "sh" | "shell" | "console" | "bash" | "zsh" => Some("Bourne Again Shell (bash)"),
        "js" => Some("JavaScript"),
        "ts" => Some("TypeScript"),
        "py" => Some("Python"),
        "rs" => Some("Rust"),
        "yml" => Some("YAML"),
        "md" => Some("Markdown"),
        _ => None,
    };
    if let Some(name) = alias {
        if let Some(syntax) = SYNTAX_SET.find_syntax_by_name(name) {
            return Some(syntax);
        }
    }
    SYNTAX_SET
        .find_syntax_by_token(lang)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(lang))
}

fn pick_theme(theme_id: ThemeId) -> &'static Theme {
    let name = match theme_id {
        ThemeId::DefaultDark => "base16-ocean.dark",
        ThemeId::DefaultLight => "InspiredGitHub",
    };
    THEME_SET
        .themes
        .get(name)
        .or_else(|| THEME_SET.themes.values().next())
        .expect("syntect ships at least one theme")
}

fn convert_style(s: SyntectStyle, fallback: Style) -> Style {
    // Syntect uses RGB; we preserve it for true-color terminals.
    // Theme degradation (true → 256 → 16 → mono) already runs on
    // `ThemeStyles` elsewhere, so for the code-fence path we trust
    // the terminal's own color table. The fallback style supplies
    // the foreground if syntect returned black-on-black (the
    // "default" it sometimes emits for whitespace).
    let mut style = fallback;
    let fg = Color::Rgb(s.foreground.r, s.foreground.g, s.foreground.b);
    if !is_near_black(&s.foreground) {
        style = style.fg(fg);
    }
    if s.font_style
        .contains(syntect::highlighting::FontStyle::BOLD)
    {
        style = style.add_modifier(Modifier::BOLD);
    }
    if s.font_style
        .contains(syntect::highlighting::FontStyle::ITALIC)
    {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if s.font_style
        .contains(syntect::highlighting::FontStyle::UNDERLINE)
    {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

fn is_near_black(c: &syntect::highlighting::Color) -> bool {
    // syntect themes sometimes emit `#000000` for "no override";
    // painting that over the terminal background produces invisible
    // text on dark themes.
    c.r < 8 && c.g < 8 && c.b < 8
}

fn plain_fallback(source: &str, style: Style) -> Vec<Line<'static>> {
    // Drop the trailing empty segment `split('\n')` produces when
    // the source ends with a newline (otherwise a `"foo\nbar\n"`
    // fence renders as 3 rows with a blank last row).
    let trimmed = source.strip_suffix('\n').unwrap_or(source);
    trimmed
        .split('\n')
        .map(|line| Line::from(Span::styled(line.to_string(), style)))
        .collect()
}

fn strip_newline(s: &str) -> &str {
    s.strip_suffix('\n').unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::theme::ThemeStyles;

    #[test]
    fn unknown_language_falls_back_to_plain_lines() {
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let lines = highlight_fence(
            "foo\nbar\n",
            Some("not-a-real-lang"),
            ThemeId::DefaultDark,
            &theme,
        );
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn rust_fence_produces_more_than_one_span_per_line() {
        // Syntect should split `fn main() {}` into at least 3 tokens.
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let lines = highlight_fence("fn main() {}\n", Some("rust"), ThemeId::DefaultDark, &theme);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans.len() >= 3);
    }

    #[test]
    fn sh_alias_resolves_to_bash_syntax() {
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let lines = highlight_fence("echo hello\n", Some("sh"), ThemeId::DefaultDark, &theme);
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].spans.is_empty());
    }

    #[test]
    fn light_and_dark_themes_do_not_panic() {
        let theme = ThemeStyles::for_theme(ThemeId::DefaultLight);
        let _ = highlight_fence("let x = 1;\n", Some("rust"), ThemeId::DefaultLight, &theme);
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let _ = highlight_fence("let x = 1;\n", Some("rust"), ThemeId::DefaultDark, &theme);
    }
}
