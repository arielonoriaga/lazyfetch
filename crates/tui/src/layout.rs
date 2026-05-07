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
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(body[1]);

    pane(f, left[0], "Collections", Focus::Collections, state);
    pane(f, left[1], "Environment", Focus::Env, state);
    pane(f, right[0], "Request", Focus::Request, state);
    pane(f, right[1], "Response", Focus::Response, state);

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
            _ => ":  Tab cycle  ?  help  q quit".into(),
        },
    };
    let status =
        Paragraph::new(Line::from(status_text)).style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, outer[2]);
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
    }
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
