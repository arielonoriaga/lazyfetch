use crate::app::{AppState, Focus, InsertField, Mode};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, state: &AppState) {
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
    pane(f, right[2], "Response", Focus::Response, state);

    let toast = Paragraph::new(Line::from(state.toast.as_deref().unwrap_or(""))).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::ITALIC),
    );
    f.render_widget(toast, outer[1]);

    let status_text = match state.mode {
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
            _ => ":  Tab cycle  ?  help  q quit".into(),
        },
    };
    let status =
        Paragraph::new(Line::from(status_text)).style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, outer[2]);

    if state.help_open {
        draw_help(f);
    }
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
        section("URL bar"),
        row("type / Bksp", "edit URL inline"),
        row("Enter", "commit · jump to Request"),
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
        Focus::Response => empty(
            f,
            inner,
            "No response yet",
            &[
                "Open a request, then press",
                "  s         send",
                "  /  f      search · jq filter",
                "  S         save body / cURL",
            ],
        ),
        Focus::Url => {} // rendered by render_url_bar above this pane()
    }
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
