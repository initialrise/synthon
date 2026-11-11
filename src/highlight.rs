#![allow(dead_code)]

use eframe::egui::{Color32, FontId, TextFormat, text::LayoutJob};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEMES: OnceLock<ThemeSet> = OnceLock::new();

fn color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

pub fn highlight(code: &str, lang: Option<&str>, dark: bool, font: FontId) -> LayoutJob {
    let ss = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let theme = &THEMES.get_or_init(ThemeSet::load_defaults).themes[if dark {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    }];
    let fg = color32(theme.settings.foreground.unwrap_or(Color::BLACK));
    let fmt = |color| TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };

    let mut job = LayoutJob::default();
    let Some(syntax) = lang.and_then(|l| ss.find_syntax_by_token(l)) else {
        job.append(code, 0.0, fmt(fg));
        return job;
    };
    let mut hl = HighlightLines::new(syntax, theme);
    for line in LinesWithEndings::from(code) {
        match hl.highlight_line(line, ss) {
            Ok(ranges) => {
                for (style, text) in ranges {
                    job.append(text, 0.0, fmt(color32(style.foreground)));
                }
            }
            Err(_) => job.append(line, 0.0, fmt(fg)),
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn colors(job: &LayoutJob) -> HashSet<Color32> {
        job.sections.iter().map(|s| s.format.color).collect()
    }

    const RUST: &str = "fn main() {\n    let x = \"hi\"; // note\n}\n";

    #[test]
    fn rust_has_many_colors_and_same_text() {
        for (lang, dark) in [("rust", true), ("rs", false)] {
            let job = highlight(RUST, Some(lang), dark, FontId::monospace(12.0));
            assert!(colors(&job).len() > 1);
            assert_eq!(job.text, RUST);
        }
    }

    #[test]
    fn unknown_or_none_is_plain() {
        for lang in [Some("nope"), None] {
            let job = highlight(RUST, lang, false, FontId::monospace(12.0));
            assert_eq!(colors(&job).len(), 1);
            assert_eq!(job.sections.len(), 1);
            assert_eq!(job.text, RUST);
        }
    }

    #[test]
    fn tokens_resolve() {
        let ss = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
        for t in ["rs", "rust", "py", "sh"] {
            assert!(ss.find_syntax_by_token(t).is_some(), "{t}");
        }
    }
}
