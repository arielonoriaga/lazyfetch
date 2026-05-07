use crate::app::{AppState, Focus, InsertField, Mode};
use crate::response as resp_render;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;

/// Result of a draw pass — geometry the event loop needs to feed back into AppState.
#[derive(Default, Debug, Clone, Copy)]
pub struct DrawInfo {
    pub response_height: u16,
    pub response_width: u16,
    pub response_total_lines: usize,
    pub collections_rect: Rect,
    pub env_rect: Rect,
    pub url_rect: Rect,
    pub request_rect: Rect,
    pub response_rect: Rect,
    /// Inner area (inside borders + status header) of the response body — for cursor mapping.
    pub response_body_rect: Rect,
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
    let collections_rect = left[0];
    let env_rect = left[1];
    let url_rect = right[0];
    let request_rect = right[1];
    let response_rect = right[2];

    let toast = Paragraph::new(Line::from(state.toast.as_deref().unwrap_or(""))).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::ITALIC),
    );
    f.render_widget(toast, outer[1]);

    // Mode badge — lazygit-style colored chunk on the left of the status bar.
    let (mode_label, mode_fg, mode_bg) = match state.mode {
        Mode::Normal => (" NORMAL ", Color::Black, Color::Cyan),
        Mode::Insert => (" INSERT ", Color::Black, Color::Green),
        Mode::Command => ("COMMAND ", Color::Black, Color::Magenta),
        Mode::Search => (" SEARCH ", Color::Black, Color::Yellow),
        Mode::SaveAs => (" SAVE ", Color::Black, Color::Yellow),
    };
    let mode_span = Span::styled(
        mode_label.to_string(),
        Style::default()
            .fg(mode_fg)
            .bg(mode_bg)
            .add_modifier(Modifier::BOLD),
    );

    let status_text = match state.mode {
        Mode::Search => format!("/{}", state.search_buf),
        Mode::Command => format!(":{}", state.command_buf),
        Mode::Insert => "(insert popup — Tab swap · Enter save · Esc cancel)".into(),
        Mode::SaveAs => format!("Save as: {}", state.save_buf),
        Mode::Normal => match state.focus {
            Focus::Env => {
                "Env: j/k · a add · A add-sec · e edit · d del · m sec · r reveal · :env / :newenv"
                    .into()
            }
            Focus::Url => {
                if !state.url_var_suggestions().is_empty() {
                    "URL: ↓/↑ pick · Tab/Enter accept · Esc close · keep typing to filter".into()
                } else {
                    "URL: type to edit · Enter send · Ctrl-w save · type {{ for var hints".into()
                }
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
    let status_line = Line::from(vec![
        mode_span,
        Span::raw(" "),
        Span::styled(status_text, Style::default().fg(Color::DarkGray)),
    ]);
    let status = Paragraph::new(status_line);
    f.render_widget(status, outer[2]);

    if state.mode == Mode::Insert && state.focus == Focus::Env {
        draw_env_var_modal(f, state);
    }
    if state.mode == Mode::SaveAs {
        draw_save_modal(f, state);
    }
    if state.focus == Focus::Url {
        draw_url_suggestions(f, url_rect, state);
    }
    if state.help_open {
        draw_help(f);
    }

    DrawInfo {
        response_height: resp_info.0,
        response_width: resp_info.1,
        response_total_lines: resp_info.2,
        collections_rect,
        env_rect,
        url_rect,
        request_rect,
        response_rect,
        response_body_rect: resp_info.3,
    }
}

fn pane_response(f: &mut Frame, area: Rect, state: &AppState) -> (u16, u16, usize, Rect) {
    let block = pane_block("Response", Focus::Response, state);
    let inner = block.inner(area);
    f.render_widget(block, area);
    render_response_inner(f, inner, state);
    let body_height = inner.height.saturating_sub(2);
    // Body width = pane inner width minus 2 columns for cursor margin.
    let body_width = inner.width.saturating_sub(2);
    let total = compute_total_lines(state);
    // body_rect: skip the 2 header rows (status + blank), keep margin column on left.
    let body_rect = Rect {
        x: inner.x + 2,
        y: inner.y + 2,
        width: body_width,
        height: body_height,
    };
    (body_height, body_width, total, body_rect)
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

fn draw_url_suggestions(f: &mut Frame, url_rect: Rect, state: &AppState) {
    use ratatui::widgets::Clear;
    let suggestions = state.url_var_suggestions();
    if suggestions.is_empty() {
        return;
    }
    let max_rows = suggestions.len().min(6) as u16;
    let widest = suggestions.iter().map(|s| s.len()).max().unwrap_or(8) as u16;
    let w = (widest + 6).clamp(20, 40);
    let h = max_rows + 2; // +2 for borders
    let x = url_rect.x + 8; // align under the URL text (after method badge)
    let y = url_rect.y + url_rect.height; // just below the URL bar
    let area = f.area();
    let popup = Rect {
        x: x.min(area.width.saturating_sub(w)),
        y: y.min(area.height.saturating_sub(h)),
        width: w,
        height: h,
    };
    let title = Line::from(Span::styled(
        " {{ var }} ",
        Style::default()
            .fg(Color::Black)
            .bg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    ));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::Magenta))
        .title(title);
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let lines: Vec<Line> = suggestions
        .iter()
        .take(max_rows as usize)
        .enumerate()
        .map(|(i, name)| {
            let selected = i == state.url_suggest_idx;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Magenta)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let mark = if selected { "▌ " } else { "  " };
            Line::from(vec![Span::styled(format!("{}{}", mark, name), style)])
        })
        .collect();
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_save_modal(f: &mut Frame, state: &AppState) {
    use ratatui::widgets::Clear;
    let area = f.area();
    let w = area.width.min(70);
    let h = 9u16.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 3;
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    let title = Line::from(vec![
        Span::styled(
            " Save request ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} {} ", state.method, state.url_buf),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .title(title);
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let label = Line::from(Span::styled(
        " Save as  <collection>/<request_name>",
        Style::default().fg(Color::Gray),
    ));
    let input = Line::from(vec![
        Span::styled(
            "▌ ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            state.save_buf.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "▏",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);
    let hint = Line::from(Span::styled(
        " Saved to .lazyfetch/collections/<coll>/requests/<name>.yaml",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    ));
    let footer = Line::from(vec![
        Span::styled(
            " Enter ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" save  ", Style::default().fg(Color::Gray)),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(Color::Gray)),
    ]);

    f.render_widget(Paragraph::new(label), rows[0]);
    f.render_widget(Paragraph::new(input), rows[2]);
    f.render_widget(Paragraph::new(hint), rows[4]);
    f.render_widget(Paragraph::new(footer), rows[5]);
}

fn draw_env_var_modal(f: &mut Frame, state: &AppState) {
    use ratatui::widgets::Clear;
    let Some(buf) = state.insert_buf.as_ref() else {
        return;
    };
    let area = f.area();
    let w = area.width.min(64);
    let h = 9u16.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 3; // upper third feels nicer
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    let title_text = match (buf.edit_idx, buf.secret) {
        (Some(_), true) => " Edit secret variable ",
        (Some(_), false) => " Edit variable ",
        (None, true) => " New secret variable ",
        (None, false) => " New variable ",
    };
    let env_name = state
        .active_env_ref()
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "default".into());
    let title = Line::from(vec![
        Span::styled(
            title_text,
            Style::default()
                .fg(Color::Black)
                .bg(if buf.secret { Color::Red } else { Color::Green })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" → {} ", env_name),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(
            Style::default()
                .fg(if buf.secret { Color::Red } else { Color::Green })
                .add_modifier(Modifier::BOLD),
        )
        .title(title);
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    // Two-row layout: Key + Value, plus a footer hint.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    let active_key = buf.field == InsertField::Key;
    let active_val = buf.field == InsertField::Value;
    let label_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let key_marker = if active_key { "▌" } else { " " };
    let val_marker = if active_val { "▌" } else { " " };

    let key_line = Line::from(vec![
        Span::styled(key_marker.to_string(), label_style),
        Span::styled(" Key   ", Style::default().fg(Color::Gray)),
        Span::styled(
            buf.key.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        if active_key {
            Span::styled(
                "▏",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::SLOW_BLINK),
            )
        } else {
            Span::raw("")
        },
    ]);

    let value_display = if buf.secret {
        "*".repeat(buf.value.len())
    } else {
        buf.value.clone()
    };
    let val_line = Line::from(vec![
        Span::styled(val_marker.to_string(), label_style),
        Span::styled(" Value ", Style::default().fg(Color::Gray)),
        Span::styled(
            value_display,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        if active_val {
            Span::styled(
                "▏",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::SLOW_BLINK),
            )
        } else {
            Span::raw("")
        },
    ]);

    let secret_hint = if buf.secret {
        "secret · masked in logs / save / history"
    } else {
        "plain · visible in raw view"
    };

    let footer = Line::from(vec![
        Span::styled(
            " Tab ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" swap  ", Style::default().fg(Color::Gray)),
        Span::styled(
            " Enter ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" save  ", Style::default().fg(Color::Gray)),
        Span::styled(
            " Esc ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" cancel", Style::default().fg(Color::Gray)),
    ]);

    f.render_widget(Paragraph::new(key_line), rows[0]);
    f.render_widget(Paragraph::new(val_line), rows[1]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            secret_hint,
            Style::default()
                .fg(if buf.secret {
                    Color::Red
                } else {
                    Color::DarkGray
                })
                .add_modifier(Modifier::ITALIC),
        ))),
        rows[3],
    );
    f.render_widget(Paragraph::new(footer), rows[5]);
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
        row("1 2 3 4 5", "jump to pane (Coll · URL · Req · Resp · Env)"),
        row("h j k l", "(arrows) — spatial pane move"),
        row("Tab / S-Tab", "cycle pane focus"),
        row("?", "toggle this help"),
        row(":", "command mode"),
        row("q  /  C-c", "quit"),
        Line::from(""),
        section("Send / save"),
        row("F5", "send — works in any pane / mode (universal)"),
        row("s", "send (any pane in Normal mode)"),
        row("Enter", "send (when URL bar focused)"),
        row("Ctrl-s", "send (any pane, any mode)"),
        row("Ctrl-w", "save URL+method as request (popup, any pane)"),
        row(":save api/users", "save URL+method as <coll>/<name>"),
        Line::from(""),
        section("Response pane"),
        row("j / k", "line up/down"),
        row("h / l", "char left/right"),
        row("0 / $", "line start / end"),
        row("w / b", "word forward / back"),
        row("Ctrl-d / Ctrl-u", "half page"),
        row("Ctrl-f / Ctrl-b", "full page"),
        row("gg / G", "top / bottom"),
        row("{ / }", "prev / next blank line"),
        row("H / M / L", "viewport top / mid / bottom"),
        row("%", "matching brace { } [ ]"),
        row("] / [", "next / prev sibling block"),
        row("v", "toggle visual select"),
        row("y", "yank selection (or line) → clipboard"),
        row("/  n  N", "search · next · prev"),
        row("Esc", "exit visual / clear search"),
        Line::from(""),
        section("URL bar"),
        row("type / Bksp", "edit URL inline"),
        row("Alt-↑ / Alt-↓", "cycle HTTP method"),
        row(":method GET", "set method by name (any pane)"),
        row("{{", "open variable suggestions"),
        row("Tab / Enter", "accept selected variable"),
        row("↑ / ↓", "navigate suggestions"),
        Line::from(""),
        section("Env pane"),
        row("j / k", "move row cursor"),
        row("a", "add variable"),
        row("A", "add secret variable"),
        row("e", "edit selected row"),
        row("d", "delete selected row"),
        row("m", "toggle secret flag"),
        row("r", "reveal / hide secret value"),
        row(":env <name>", "switch active env"),
        row(":newenv <name>", "create new env (becomes active)"),
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
    let block = pane_block(title, my, state);
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
        Focus::Env => render_env(f, inner, state, state.focus == Focus::Env),
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

    // Apply selection highlight (visual mode).
    let cursor_line = state.response_cursor;
    let cursor_col = state.response_col;
    let selection = state.visual_anchor.map(|(al, ac)| {
        let s = (al, ac);
        let e = (cursor_line, cursor_col);
        if s <= e {
            (s, e)
        } else {
            (e, s)
        }
    });

    let hscroll = state.response_hscroll as usize;
    let body_width = area.width.saturating_sub(2) as usize;

    // Mark cursor line with reverse-video left margin.
    if cursor_line < highlighted.len() {
        let marker = Span::styled(
            "▌ ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        highlighted[cursor_line].insert(0, marker);
    }
    for (i, line) in highlighted.iter_mut().enumerate() {
        if i != cursor_line {
            line.insert(0, Span::raw("  "));
        }
    }

    let mut lines: Vec<Line> = header;
    for (i, spans) in highlighted[start..end].iter().enumerate() {
        let line_idx = start + i;
        let mut spans = spans.clone();

        // Apply visual selection highlight in-place
        if let Some(((sl, sc), (el, ec))) = selection {
            if line_idx >= sl && line_idx <= el {
                let from = if line_idx == sl { sc } else { 0 };
                let to_inclusive = if line_idx == el { ec } else { usize::MAX };
                spans = highlight_range(spans, from + 2, to_inclusive.saturating_add(2));
                // +2 for the left margin we just inserted.
            }
        }

        // Horizontal slice: drop the first `hscroll` chars (after the 2-char margin),
        // then truncate to `body_width + 2`.
        let total_take = body_width + 2;
        spans = slice_spans(spans, hscroll, total_take);

        // Place cursor column marker on cursor line (faint underline).
        if line_idx == cursor_line {
            spans = mark_cursor_col(spans, cursor_col.saturating_sub(hscroll) + 2);
        }

        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(Text::from(lines)), area);
}

/// Highlight chars in `[from, to_inclusive]` columns by splitting any covering Span and
/// applying reverse-video on top of the original style.
fn highlight_range(
    spans: Vec<Span<'static>>,
    from: usize,
    to_inclusive: usize,
) -> Vec<Span<'static>> {
    let highlight = Style::default().add_modifier(Modifier::REVERSED);
    let mut out: Vec<Span> = Vec::with_capacity(spans.len() + 2);
    let mut col = 0usize;
    for span in spans {
        let style = span.style;
        let s = span.content.into_owned();
        let len = s.chars().count();
        let span_end = col + len;
        if span_end <= from || col > to_inclusive {
            out.push(Span::styled(s, style));
        } else {
            let chars: Vec<char> = s.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let abs = col + i;
                let in_range = abs >= from && abs <= to_inclusive;
                let mut j = i + 1;
                while j < chars.len() {
                    let abs_j = col + j;
                    let j_in = abs_j >= from && abs_j <= to_inclusive;
                    if j_in != in_range {
                        break;
                    }
                    j += 1;
                }
                let chunk: String = chars[i..j].iter().collect();
                if in_range {
                    out.push(Span::styled(chunk, style.patch(highlight)));
                } else {
                    out.push(Span::styled(chunk, style));
                }
                i = j;
            }
        }
        col = span_end;
    }
    out
}

/// Slice spans horizontally: drop first `skip` chars, keep `take` chars total.
fn slice_spans(spans: Vec<Span<'static>>, skip: usize, take: usize) -> Vec<Span<'static>> {
    if take == 0 {
        return vec![];
    }
    let mut out: Vec<Span> = vec![];
    let mut dropped = 0usize;
    let mut taken = 0usize;
    for span in spans {
        if taken >= take {
            break;
        }
        let style = span.style;
        let s = span.content.into_owned();
        let len = s.chars().count();
        if dropped + len <= skip {
            dropped += len;
            continue;
        }
        let local_skip = skip.saturating_sub(dropped);
        dropped = skip;
        let chars: Vec<char> = s.chars().skip(local_skip).collect();
        let want = (take - taken).min(chars.len());
        if want == 0 {
            continue;
        }
        let chunk: String = chars[..want].iter().collect();
        out.push(Span::styled(chunk, style));
        taken += want;
    }
    out
}

/// Underline the character at `col` on the cursor line (visible focus indicator for
/// horizontal motion). If the line is shorter than `col`, no-op.
fn mark_cursor_col(spans: Vec<Span<'static>>, col: usize) -> Vec<Span<'static>> {
    let underline = Style::default().add_modifier(Modifier::UNDERLINED);
    let mut out: Vec<Span> = Vec::with_capacity(spans.len() + 1);
    let mut c = 0usize;
    for span in spans {
        let style = span.style;
        let s = span.content.into_owned();
        let len = s.chars().count();
        if c + len <= col || c > col {
            out.push(Span::styled(s, style));
        } else {
            let chars: Vec<char> = s.chars().collect();
            let local = col - c;
            if local > 0 {
                let pre: String = chars[..local].iter().collect();
                out.push(Span::styled(pre, style));
            }
            let mark: String = chars[local..local + 1].iter().collect();
            out.push(Span::styled(mark, style.patch(underline)));
            if local + 1 < chars.len() {
                let post: String = chars[local + 1..].iter().collect();
                out.push(Span::styled(post, style));
            }
        }
        c += len;
    }
    out
}

/// Build a styled pane block. Lazygit feel: solid white borders, numbered titles, focus accent.
fn pane_block<'a>(title: &'a str, my: Focus, state: &AppState) -> Block<'a> {
    let focused = state.focus == my;
    let (border_color, title_fg, badge_style) = if focused {
        (
            Color::Green,
            Color::White,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            Color::White,
            Color::White,
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    };
    let badge_n = pane_number(my);
    let title_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(format!(" {} ", badge_n), badge_style),
        Span::styled(
            format!(" {} ", title),
            Style::default().fg(title_fg).add_modifier(Modifier::BOLD),
        ),
    ]);
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(border_color).add_modifier(if focused {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }))
        .title(title_line)
}

fn pane_number(focus: Focus) -> u8 {
    match focus {
        Focus::Collections => 1,
        Focus::Url => 2,
        Focus::Request => 3,
        Focus::Response => 4,
        Focus::Env => 5,
    }
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
    let block = pane_block("URL", Focus::Url, state);
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
                let display_value = if secret && !state.is_revealed(i) {
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
