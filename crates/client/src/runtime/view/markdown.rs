use std::sync::OnceLock;

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Render assistant markdown: prose goes through `tui-markdown`, fenced code
/// blocks are highlighted with `syntect`. The fence scanner splits the text
/// so each ```` ``` ```` block is highlighted on its own.
pub(super) fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut prose = String::new();
    let mut code = String::new();
    let mut lang: Option<String> = None;
    let mut in_code = false;

    let flush_prose = |prose: &mut String, out: &mut Vec<Line<'static>>| {
        if !prose.is_empty() {
            out.extend(own_lines(tui_markdown::from_str(prose)));
            prose.clear();
        }
    };

    for line in text.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("```") {
            if in_code {
                out.extend(highlight_code_block(&code, lang.as_deref()));
                code.clear();
                lang = None;
                in_code = false;
            } else {
                flush_prose(&mut prose, &mut out);
                let tag = rest.trim();
                lang = (!tag.is_empty()).then(|| tag.to_string());
                in_code = true;
            }
        } else if in_code {
            code.push_str(line);
            code.push('\n');
        } else {
            prose.push_str(line);
            prose.push('\n');
        }
    }

    if in_code {
        // Unterminated fence: still render the code we collected.
        out.extend(highlight_code_block(&code, lang.as_deref()));
    } else {
        flush_prose(&mut prose, &mut out);
    }
    out
}

/// Highlight a fenced code block.
///
/// When `lang` is absent or unrecognised, the block is rendered as plain
/// monospaced text with **no** highlight theme applied, and the call never
/// fails. A recognised language is coloured via `syntect`.
///
/// ```
/// use mewcode_client::runtime::view::highlight_code_block;
/// use ratatui::style::Style;
///
/// let lines = highlight_code_block("hello world\nsecond line", None);
/// assert_eq!(lines.len(), 2);
/// assert_eq!(lines[0].spans[0].style, Style::default());
///
/// let lines = highlight_code_block("x = 1", Some("totally-not-a-language"));
/// assert_eq!(lines[0].spans[0].style, Style::default());
/// ```
pub fn highlight_code_block(code: &str, lang: Option<&str>) -> Vec<Line<'static>> {
    let syntaxes = syntax_set();
    let syntax = lang
        .filter(|l| !l.is_empty())
        .and_then(|l| syntaxes.find_syntax_by_token(l));

    let Some(syntax) = syntax else {
        // Plain monospaced fallback — no theme, never fails.
        return plain_lines(code);
    };

    let mut highlighter = HighlightLines::new(syntax, theme());
    let mut out = Vec::new();
    for line in LinesWithEndings::from(code) {
        match highlighter.highlight_line(line, syntaxes) {
            Ok(ranges) => {
                let spans = ranges
                    .into_iter()
                    .map(|(style, text)| {
                        let c = style.foreground;
                        Span::styled(
                            text.trim_end_matches('\n').to_string(),
                            Style::default().fg(Color::Rgb(c.r, c.g, c.b)),
                        )
                    })
                    .collect::<Vec<_>>();
                out.push(Line::from(spans));
            }
            // A highlighter hiccup must never fail the render.
            Err(_) => out.push(Line::from(Span::raw(
                line.trim_end_matches('\n').to_string(),
            ))),
        }
    }
    out
}

fn plain_lines(code: &str) -> Vec<Line<'static>> {
    code.lines()
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect()
}

/// Deep-clone a borrowed [`Text`] into owned (`'static`) lines so it can
/// outlive the local string `tui-markdown` borrowed from.
fn own_lines(text: Text<'_>) -> Vec<Line<'static>> {
    text.lines
        .into_iter()
        .map(|line| {
            let spans = line
                .spans
                .into_iter()
                .map(|s| Span::styled(s.content.into_owned(), s.style))
                .collect::<Vec<_>>();
            let mut owned = Line::from(spans);
            owned.style = line.style;
            owned.alignment = line.alignment;
            owned
        })
        .collect()
}

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme() -> &'static Theme {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    let set = THEMES.get_or_init(ThemeSet::load_defaults);
    // `base16-ocean.dark` ships with syntect's defaults.
    &set.themes["base16-ocean.dark"]
}
