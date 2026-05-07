//! Response body rendering: JSON colorizer + plain text wrapper + search highlight.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;

const KEY: Color = Color::Cyan;
const STR: Color = Color::Green;
const NUM: Color = Color::Magenta;
const BOOL: Color = Color::Yellow;
const NULL: Color = Color::Red;
const PUNCT: Color = Color::DarkGray;

/// Render a pretty-printed JSON value as colored lines.
pub fn colorize_json(text: &str) -> Vec<Vec<Span<'static>>> {
    let v: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return plain_lines(text),
    };
    let mut out: Vec<Vec<Span>> = vec![];
    let mut cur: Vec<Span> = vec![];
    write_value(&v, 0, &mut cur, &mut out);
    out.push(cur);
    out
}

pub fn plain_lines(text: &str) -> Vec<Vec<Span<'static>>> {
    text.lines()
        .map(|l| vec![Span::raw(l.to_string())])
        .collect()
}

fn indent_span(n: usize) -> Span<'static> {
    Span::raw("  ".repeat(n))
}

fn punct(s: &'static str) -> Span<'static> {
    Span::styled(s, Style::default().fg(PUNCT))
}

fn write_value(
    v: &Value,
    depth: usize,
    cur: &mut Vec<Span<'static>>,
    out: &mut Vec<Vec<Span<'static>>>,
) {
    match v {
        Value::Null => cur.push(Span::styled("null", Style::default().fg(NULL))),
        Value::Bool(b) => cur.push(Span::styled(
            b.to_string(),
            Style::default().fg(BOOL).add_modifier(Modifier::BOLD),
        )),
        Value::Number(n) => cur.push(Span::styled(n.to_string(), Style::default().fg(NUM))),
        Value::String(s) => cur.push(Span::styled(
            format!("\"{}\"", escape(s)),
            Style::default().fg(STR),
        )),
        Value::Array(arr) => {
            if arr.is_empty() {
                cur.push(punct("[]"));
                return;
            }
            cur.push(punct("["));
            out.push(std::mem::take(cur));
            for (i, item) in arr.iter().enumerate() {
                let mut line: Vec<Span> = vec![indent_span(depth + 1)];
                write_value(item, depth + 1, &mut line, out);
                if i + 1 < arr.len() {
                    line.push(punct(","));
                }
                out.push(line);
            }
            cur.push(indent_span(depth));
            cur.push(punct("]"));
        }
        Value::Object(map) => {
            if map.is_empty() {
                cur.push(punct("{}"));
                return;
            }
            cur.push(punct("{"));
            out.push(std::mem::take(cur));
            let len = map.len();
            for (i, (k, val)) in map.iter().enumerate() {
                let mut line: Vec<Span> = vec![
                    indent_span(depth + 1),
                    Span::styled(
                        format!("\"{}\"", escape(k)),
                        Style::default().fg(KEY).add_modifier(Modifier::BOLD),
                    ),
                    punct(": "),
                ];
                write_value(val, depth + 1, &mut line, out);
                if i + 1 < len {
                    line.push(punct(","));
                }
                out.push(line);
            }
            cur.push(indent_span(depth));
            cur.push(punct("}"));
        }
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Highlight occurrences of `needle` (case-insensitive) by splitting any matched span,
/// keeping the surrounding fg style and adding a bright reverse-video mark on the match.
pub fn apply_search_highlight(
    lines: Vec<Vec<Span<'static>>>,
    needle: &str,
) -> (Vec<Vec<Span<'static>>>, Vec<usize>) {
    if needle.is_empty() {
        return (lines, vec![]);
    }
    let mut out_lines: Vec<Vec<Span>> = Vec::with_capacity(lines.len());
    let mut hit_lines: Vec<usize> = vec![];
    let needle_lc = needle.to_lowercase();
    let highlight = Style::default()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    for (i, spans) in lines.into_iter().enumerate() {
        let mut new_line: Vec<Span> = Vec::with_capacity(spans.len());
        let mut hit_in_this_line = false;
        for span in spans {
            let style = span.style;
            let content = span.content.to_string();
            let lower = content.to_lowercase();
            let mut last = 0;
            let mut matched_here = false;
            let mut search_from = 0;
            while let Some(rel) = lower[search_from..].find(&needle_lc) {
                let start = search_from + rel;
                let end = start + needle_lc.len();
                if start > last {
                    new_line.push(Span::styled(content[last..start].to_string(), style));
                }
                new_line.push(Span::styled(content[start..end].to_string(), highlight));
                last = end;
                search_from = end;
                matched_here = true;
            }
            if last < content.len() {
                new_line.push(Span::styled(content[last..].to_string(), style));
            }
            if matched_here {
                hit_in_this_line = true;
            }
        }
        if hit_in_this_line {
            hit_lines.push(i);
        }
        out_lines.push(new_line);
    }
    (out_lines, hit_lines)
}

pub fn lines_to_ratatui(lines: Vec<Vec<Span<'static>>>) -> Vec<Line<'static>> {
    lines.into_iter().map(Line::from).collect()
}
