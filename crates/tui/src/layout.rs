use crate::app::{AppState, Focus, InsertField, Mode};
use crate::response as resp_render;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// Result of a draw pass — geometry the event loop needs to feed back into AppState.
#[derive(Default, Debug, Clone, Copy)]
pub struct DrawInfo {
    pub response_height: u16,
    pub response_total_lines: usize,
}

pub fn draw(f: &mut Frame, state: &AppState) -> DrawInfo {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(f.area());
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(outer[0]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(body[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(body[1]);

    pane(f, left[0], "Collections", Focus::Collections, state);
    pane(f, left[1], "Environment", Focus::Env, state);
    render_url_bar(f, right[0], state);
    pane(f, right[1], "Request", Focus::Request, state);
    let resp_info = pane_response(f, right[2], state);

    let toast = Paragraph::new(Line::from(state.toast.as_deref().unwrap_or(""))).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::ITALIC),
    );
    f.render_widget(toast, outer[1]);

    let status_text = match state.mode {
        Mode::Search => format!("/{}", state.search_buf),
        Mode::Command => format!(":{}", state.command_buf),
        Mode::Insert => match state.insert_buf.as_ref() {
            Some(b) => {
                let mark_key = if b.field == InsertField::Key {
                    ">"
                } else {
                    " "
                };
                let mark_val = if b.field == InsertField::Value {
                    ">"
                } else {
                    " "
                };
                let preview = if b.secret {
                    "*".repeat(b.value.len())
                } else {
                    b.value.clone()
                };
                format!(
                    "[insert{}] {}key: {}    {}value: {}    (Tab next · Enter save · Esc cancel)",
                    if b.secret { " secret" } else { "" },
                    mark_key,
                    b.key,
                    mark_val,
                    preview
                )
            }
            None => "[insert]".into(),
        },
        Mode::Normal => match state.focus {
            Focus::Env => {
                "Env: j/k · a add · A add-secret · m toggle-secret · d delete · :env <name>".into()
            }
            Focus::Url => {
                "URL: type to edit · Backspace · Enter commit · arrows leave pane · ? help".into()
            }
            Focus::Response => {
                let nav = if let Some(needle) = &state.search_active {
                    format!(
                        "Response: j/k scroll · /search ({}/{}: \"{}\") · n/N · Esc clear",
                        if state.search_match_lines.is_empty() {
                            0
                        } else {
                            state.search_match_idx + 1
                        },
                        state.search_match_lines.len(),
                        needle
                    )
                } else {
                    "Response: j/k · g/G · Ctrl-d/Ctrl-u · / search · ? help".into()
                };
                nav
            }
            _ => ":  Tab cycle  ?  help  q quit".into(),
        },
    };
    let status =
        Paragraph::new(Line::from(status_text)).style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, outer[2]);

    if state.help_open {
        draw_help(f);
    }

    DrawInfo {
        response_height: resp_info.0,
        response_total_lines: resp_info.1,
    }
}

fn pane_response(f: &mut Frame, area: Rect, state: &AppState) -> (u16, usize) {
    let focused = state.focus == Focus::Response;
    let border_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Response")
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);
    render_response_inner(f, inner, state);
    let body_height = inner.height.saturating_sub(2);
    let total = compute_total_lines(state);
    (body_height, total)
}

fn compute_total_lines(state: &AppState) -> usize {
    let Some(executed) = &state.last_response else {
        return 0;
    };
    let ct = executed
        .response
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    let body = pretty_body(ct, &executed.response.body_bytes);
    let body = executed.secrets.redact(&body);
    body.lines().count()
}

fn draw_help(f: &mut Frame) {
    use ratatui::widgets::Clear;

    let area = f.area();
    let w = area.width.min(72);
    let h = area.height.min(28);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help — keyboard shortcuts ")
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let dim = Style::default().fg(Color::DarkGray);
    let kw = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let key = |k: &str| Span::styled(format!("{:<14}", k), kw);
    let desc = |d: &str| Span::styled(d.to_string(), Style::default().fg(Color::Gray));
    let section = |s: &str| {
        Line::from(Span::styled(
            s.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
    };
    let row = |k: &str, d: &str| Line::from(vec![Span::raw("  "), key(k), desc(d)]);

    let lines: Vec<Line> = vec![
        section("Global"),
        row("h j k l", "(arrows) — spatial pane move"),
        row("Tab / S-Tab", "cycle pane focus"),
        row("?", "toggle this help"),
        row(":", "command mode"),
        row("q  /  C-c", "quit"),
        Line::from(""),
        section("Send"),
        row("s", "send current request (any pane)"),
        row("Enter", "send (when URL bar focused)"),
        row("Ctrl-s", "send (any pane)"),
        Line::from(""),
        section("Response pane"),
        row("j / k", "scroll line"),
        row("Ctrl-d / Ctrl-u", "scroll half page"),
        row("g / G", "top / bottom"),
        row("/", "search (vim-style)"),
        row("n / N", "next / prev match"),
        row("Esc", "clear search"),
        Line::from(""),
        section("URL bar"),
        row("type / Bksp", "edit URL inline"),
        row("Alt-↑ / Alt-↓", "cycle HTTP method"),
        row(":method GET", "set method by name (any pane)"),
        Line::from(""),
        section("Env pane"),
        row("j / k", "move row cursor"),
        row("a", "add variable"),
        row("A", "add secret variable"),
        row("m", "toggle secret on selected row"),
        row("d", "delete selected row"),
        Line::from(""),
        section("Insert mode  (a / A)"),
        row("Tab", "swap key ↔ value field"),
        row("Enter", "commit, save to disk"),
        row("Esc", "cancel"),
        Line::from(""),
        section("Command mode  (:)"),
        row(":env <name>", "switch active environment"),
        row(":q", "quit"),
        row("Esc", "cancel"),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            dim.add_modifier(Modifier::ITALIC),
        )),
    ];
    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn pane(f: &mut Frame, area: Rect, title: &str, my: Focus, state: &AppState) {
    let focused = state.focus == my;
    let border_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    match my {
        Focus::Collections => {
            if state.collections.is_empty() {
                empty(
                    f,
                    inner,
                    "No collections yet",
                    &[
                        "Get started:",
                        "  lazyfetch import-postman <file>",
                        "  lazyfetch import-postman <file> --local",
                        "",
                        "Files live in",
                        "  .lazyfetch/collections/   (project)",
                        "  ~/.config/lazyfetch/collections/   (global)",
                    ],
                );
            } else {
                let lines: Vec<Line> = state
                    .collections
                    .iter()
                    .map(|c| Line::from(format!("▸ {}", c.name)))
                    .collect();
                f.render_widget(Paragraph::new(Text::from(lines)), inner);
            }
        }
        Focus::Env => render_env(f, inner, state, focused),
        Focus::Request => empty(
            f,
            inner,
            "No request open",
            &[
                "Pick one from Collections (Tab),",
                "or run from your shell:",
                "",
                "  lazyfetch run <coll>/<request>",
            ],
        ),
        Focus::Response => {} // handled by pane_response
        Focus::Url => {}      // rendered by render_url_bar above this pane()
    }
}

#[allow(clippy::too_many_lines)]
fn render_response_inner(f: &mut Frame, area: Rect, state: &AppState) {
    if state.inflight.is_some() {
        empty(f, area, "Sending…", &["press Ctrl-c to cancel"]);
        return;
    }
    if let Some(err) = &state.last_error {
        let lines = vec![
            Line::from(Span::styled(
                "Request failed",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(err.clone(), Style::default().fg(Color::Red))),
        ];
        f.render_widget(
            Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let Some(executed) = &state.last_response else {
        empty(
            f,
            area,
            "No response yet",
            &[
                "Press 's' (any pane) or Enter (URL bar)",
                "to send the current request.",
            ],
        );
        return;
    };
    let resp = &executed.response;
    let status_color = match resp.status / 100 {
        2 => Color::Green,
        3 => Color::Cyan,
        4 => Color::Yellow,
        5 => Color::Red,
        _ => Color::Gray,
    };
    let content_type = resp
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str())
        .unwrap_or("");

    let kind_label = render_kind(content_type, &resp.body_bytes).unwrap_or("raw");

    // Header line: status + meta
    let header: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                format!("{} ", resp.status),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "· {}ms · {} · {}",
                    resp.elapsed.as_millis(),
                    human_bytes(resp.size),
                    kind_label
                ),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(""),
    ];

    // Body: redact, pretty-print, colorize
    let body_text = pretty_body(content_type, &resp.body_bytes);
    let body_text = executed.secrets.redact(&body_text);
    let body_lines = if matches!(kind_label, "json") {
        resp_render::colorize_json(&body_text)
    } else {
        resp_render::plain_lines(&body_text)
    };

    // Search highlight (if active)
    let (mut highlighted, _hits) = match state.search_active.as_deref() {
        Some(n) if !n.is_empty() => resp_render::apply_search_highlight(body_lines.clone(), n),
        _ => (body_lines, vec![]),
    };

    let total = highlighted.len() as u16;
    let body_height = area.height.saturating_sub(2);
    let max_scroll = total.saturating_sub(body_height);
    let scroll = state.response_scroll.min(max_scroll);
    let start = scroll as usize;
    let end = (start + body_height as usize).min(highlighted.len());

    // Mark cursor line with reverse-video left margin.
    let cursor = state.response_cursor;
    if cursor < highlighted.len() {
        let marker = Span::styled(
            "▌ ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        highlighted[cursor].insert(0, marker);
    }
    // Pad non-cursor lines with two spaces so columns align.
    for (i, line) in highlighted.iter_mut().enumerate() {
        if i != cursor {
            line.insert(0, Span::raw("  "));
        }
    }

    let mut lines: Vec<Line> = header;
    for spans in &highlighted[start..end] {
        lines.push(Line::from(spans.clone()));
    }

    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        area,
    );
}

/// Format a byte count with binary units (KiB, MiB, GiB) and one decimal of precision.
fn human_bytes(n: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if n < KIB {
        format!("{} B", n)
    } else if n < MIB {
        format!("{:.1} KiB", n as f64 / KIB as f64)
    } else if n < GIB {
        format!("{:.1} MiB", n as f64 / MIB as f64)
    } else {
        format!("{:.2} GiB", n as f64 / GIB as f64)
    }
}

/// Returns a label describing how the body was rendered ("json", "raw"), or `None` if unknown.
fn render_kind(content_type: &str, body: &[u8]) -> Option<&'static str> {
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("json") || looks_like_json(body) {
        Some("json")
    } else if ct.contains("xml") || ct.contains("html") {
        Some("xml/html")
    } else if ct.starts_with("text/") {
        Some("text")
    } else if !body.is_empty() {
        Some("raw")
    } else {
        None
    }
}

fn looks_like_json(body: &[u8]) -> bool {
    let s = std::str::from_utf8(body).unwrap_or("").trim_start();
    s.starts_with('{') || s.starts_with('[')
}

/// JSON → 2-space pretty print via serde_json.
/// Other content types → return as-is (UTF-8 lossy).
fn pretty_body(content_type: &str, body: &[u8]) -> String {
    let ct = content_type.to_ascii_lowercase();
    if ct.contains("json") || looks_like_json(body) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) {
            if let Ok(pretty) = serde_json::to_string_pretty(&v) {
                return pretty;
            }
        }
    }
    String::from_utf8_lossy(body).into_owned()
}

fn render_url_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let focused = state.focus == Focus::Url;
    let border = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" URL ")
        .border_style(border);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let method_color = match state.method.as_str() {
        "GET" => Color::Green,
        "POST" => Color::Yellow,
        "PUT" => Color::Cyan,
        "PATCH" => Color::Magenta,
        "DELETE" => Color::Red,
        _ => Color::Gray,
    };
    let url_display = if state.url_buf.is_empty() && !focused {
        Span::styled(
            "(press Tab here, then type your URL — e.g. {{API_URL}}/users)",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )
    } else {
        Span::styled(state.url_buf.clone(), Style::default().fg(Color::White))
    };
    let cursor = if focused {
        Span::styled(
            "▏",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::SLOW_BLINK),
        )
    } else {
        Span::raw("")
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" {:<6} ", state.method.as_str()),
            Style::default()
                .fg(method_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        url_display,
        cursor,
    ]);
    f.render_widget(Paragraph::new(line), inner);
}

fn render_env(f: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    let env = match state.active_env_ref() {
        Some(e) => e,
        None => {
            empty(
                f,
                area,
                "No environments yet",
                &[
                    "Press 'a' to add a variable",
                    "(creates a 'default' env)",
                    "",
                    "Or drop a YAML file in",
                    "  <config>/environments/<name>.yaml",
                ],
            );
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::with_capacity(env.vars.len() + 2);
    let env_count = state.envs.len();
    let header = if env_count > 1 {
        format!(
            "[{}]   ({}/{} envs · :env <name>)",
            env.name,
            state.active_env.map(|i| i + 1).unwrap_or(0),
            env_count
        )
    } else {
        format!("[{}]", env.name)
    };
    lines.push(Line::from(Span::styled(
        header,
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if env.vars.is_empty() {
        lines.push(Line::from(Span::styled(
            "(empty)  press 'a' to add",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, _) in env.vars.iter().enumerate() {
            if let Some((k, v, secret)) = state.env_var_at(i) {
                let cursor = if focused && state.env_cursor == i {
                    "▸ "
                } else {
                    "  "
                };
                let display_value = if secret {
                    "***".to_string()
                } else {
                    v.to_string()
                };
                let lock = if secret { "🔒 " } else { "   " };
                let key_style = if focused && state.env_cursor == i {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let line = Line::from(vec![
                    Span::raw(cursor),
                    Span::styled(lock, Style::default().fg(Color::Red)),
                    Span::styled(format!("{:<20}", k), key_style),
                    Span::raw(" = "),
                    Span::styled(display_value, Style::default().fg(Color::Green)),
                ]);
                lines.push(line);
            }
        }
    }

    f.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn empty(f: &mut Frame, area: Rect, headline: &str, hints: &[&str]) {
    let mut lines: Vec<Line> = Vec::with_capacity(hints.len() + 2);
    lines.push(Line::from(Span::styled(
        headline,
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    for h in hints {
        lines.push(Line::from(Span::styled(
            h.to_string(),
            Style::default().fg(Color::DarkGray),
        )));
    }
    let p = Paragraph::new(Text::from(lines))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::human_bytes;

    #[test]
    fn formats_bytes() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(human_bytes(1024 * 1024 * 1024), "1.00 GiB");
    }
}
