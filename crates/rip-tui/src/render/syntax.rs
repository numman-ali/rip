//! Syntect-backed syntax highlighting for `Block::CodeFence` (Phase B.7).
//!
//! We don't cache highlighted output on the block itself — the canvas
//! model stays theme-agnostic, so a theme swap doesn't have to invalidate
//! anything. Instead, a small renderer-local cache reuses highlighted
//! output for stable fences keyed by source hash, theme, and language.
//!
//! Cost note: syntect's `SyntaxSet::load_defaults_newlines()` takes
//! ~30ms the first time it's called. We lazy-init via `once_cell` so
//! the hit lands on the first code fence, not at startup. Subsequent
//! fences reuse the parsed sets. The expensive part is re-highlighting
//! the same closed fence on every redraw, so we cache the rendered
//! lines at the syntax layer rather than re-running syntect forever.

use once_cell::sync::Lazy;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use super::theme::ThemeStyles;
use crate::ThemeId;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);
static FENCE_CACHE: Lazy<Mutex<FenceCache>> = Lazy::new(|| Mutex::new(FenceCache::default()));

const FENCE_CACHE_LIMIT: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FenceCacheKey {
    source_hash: u64,
    theme_key: u8,
    lang: Option<String>,
}

#[derive(Debug, Default)]
struct FenceCache {
    order: VecDeque<FenceCacheKey>,
    entries: HashMap<FenceCacheKey, Vec<Line<'static>>>,
}

impl FenceCache {
    fn get(&mut self, key: &FenceCacheKey) -> Option<Vec<Line<'static>>> {
        let value = self.entries.get(key)?.clone();
        self.bump(key);
        Some(value)
    }

    fn insert(&mut self, key: FenceCacheKey, value: Vec<Line<'static>>) {
        if self.entries.contains_key(&key) {
            self.entries.insert(key.clone(), value);
            self.bump(&key);
            return;
        }
        if self.entries.len() >= FENCE_CACHE_LIMIT {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }

    fn bump(&mut self, key: &FenceCacheKey) {
        if let Some(index) = self.order.iter().position(|entry| entry == key) {
            let current = self.order.remove(index).expect("existing cache key");
            self.order.push_back(current);
        }
    }
}

/// Highlight a fence's source as styled lines for the current theme.
/// Falls back to plain-styled lines when syntect can't find a syntax
/// for `lang` (or when the fence has no `lang` tag).
///
/// `base_style` is the fallback `Style` applied to unmatched tokens
/// so they still read as body text.
pub fn highlight_fence(
    source: &str,
    source_hash: u64,
    lang: Option<&str>,
    theme_id: ThemeId,
    theme_styles: &ThemeStyles,
) -> Vec<Line<'static>> {
    let key = FenceCacheKey {
        source_hash,
        theme_key: theme_cache_key(theme_id),
        lang: normalized_lang_key(lang),
    };
    if let Some(lines) = FENCE_CACHE.lock().expect("fence cache poisoned").get(&key) {
        return lines;
    }

    let lines = highlight_fence_uncached(source, lang, theme_id, theme_styles);
    FENCE_CACHE
        .lock()
        .expect("fence cache poisoned")
        .insert(key, lines.clone());
    lines
}

fn highlight_fence_uncached(
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

fn normalized_lang_key(lang: Option<&str>) -> Option<String> {
    let lang = lang?.trim();
    if lang.is_empty() {
        return None;
    }
    Some(lang.to_ascii_lowercase())
}

fn theme_cache_key(theme_id: ThemeId) -> u8 {
    match theme_id {
        ThemeId::DefaultDark => 0,
        ThemeId::DefaultLight => 1,
    }
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

    fn hash_source(source: &str) -> u64 {
        use std::hash::{DefaultHasher, Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn unknown_language_falls_back_to_plain_lines() {
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let lines = highlight_fence(
            "foo\nbar\n",
            hash_source("foo\nbar\n"),
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
        let lines = highlight_fence(
            "fn main() {}\n",
            hash_source("fn main() {}\n"),
            Some("rust"),
            ThemeId::DefaultDark,
            &theme,
        );
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans.len() >= 3);
    }

    #[test]
    fn sh_alias_resolves_to_bash_syntax() {
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let lines = highlight_fence(
            "echo hello\n",
            hash_source("echo hello\n"),
            Some("sh"),
            ThemeId::DefaultDark,
            &theme,
        );
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].spans.is_empty());
    }

    #[test]
    fn light_and_dark_themes_do_not_panic() {
        let theme = ThemeStyles::for_theme(ThemeId::DefaultLight);
        let _ = highlight_fence(
            "let x = 1;\n",
            hash_source("let x = 1;\n"),
            Some("rust"),
            ThemeId::DefaultLight,
            &theme,
        );
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let _ = highlight_fence(
            "let x = 1;\n",
            hash_source("let x = 1;\n"),
            Some("rust"),
            ThemeId::DefaultDark,
            &theme,
        );
    }

    #[test]
    fn fence_cache_reuses_entry_and_respects_theme_boundaries() {
        let source = "fn main() {}\n";
        let source_hash = hash_source(source);
        let dark = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let light = ThemeStyles::for_theme(ThemeId::DefaultLight);

        let dark_a = highlight_fence(
            source,
            source_hash,
            Some("rust"),
            ThemeId::DefaultDark,
            &dark,
        );
        let dark_b = highlight_fence(
            source,
            source_hash,
            Some("rust"),
            ThemeId::DefaultDark,
            &dark,
        );
        let light_a = highlight_fence(
            source,
            source_hash,
            Some("rust"),
            ThemeId::DefaultLight,
            &light,
        );

        assert_eq!(dark_a, dark_b);
        assert_eq!(dark_a.len(), light_a.len());
    }

    #[test]
    fn fence_cache_bumps_recent_entries_and_evicts_oldest() {
        let mut cache = FenceCache::default();
        let first = FenceCacheKey {
            source_hash: 0,
            theme_key: 0,
            lang: Some("rust".to_string()),
        };
        for idx in 0..FENCE_CACHE_LIMIT {
            cache.insert(
                FenceCacheKey {
                    source_hash: idx as u64,
                    theme_key: 0,
                    lang: Some("rust".to_string()),
                },
                vec![Line::from(format!("{idx}"))],
            );
        }
        // Touch the oldest entry so the next insertion evicts the
        // second-oldest, not the one we just reused.
        assert!(cache.get(&first).is_some());

        let newest = FenceCacheKey {
            source_hash: FENCE_CACHE_LIMIT as u64,
            theme_key: 0,
            lang: Some("rust".to_string()),
        };
        cache.insert(newest.clone(), vec![Line::from("new")]);

        assert!(cache.get(&first).is_some());
        assert!(cache.get(&newest).is_some());
        assert!(cache
            .get(&FenceCacheKey {
                source_hash: 1,
                theme_key: 0,
                lang: Some("rust".to_string()),
            })
            .is_none());
    }

    #[test]
    fn fence_cache_limit_is_enforced() {
        let mut cache = FenceCache::default();
        for idx in 0..=FENCE_CACHE_LIMIT {
            cache.insert(
                FenceCacheKey {
                    source_hash: idx as u64,
                    theme_key: 0,
                    lang: None,
                },
                vec![Line::from(format!("{idx}"))],
            );
        }

        assert_eq!(cache.entries.len(), FENCE_CACHE_LIMIT);
    }
}
