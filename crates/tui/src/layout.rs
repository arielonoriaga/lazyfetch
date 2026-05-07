use crate::app::{AppState, Focus};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, state: &AppState) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
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

    let status = Paragraph::new(Line::from(
        ":send  /search  e edit  s send  S save  ? help  q quit",
    ))
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, outer[1]);
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
                        "  :new collection <name>",
                        "",
                        "Files live in",
                        "  ~/.config/lazyfetch/collections/",
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
        Focus::Env => match state.active_env.and_then(|i| state.envs.get(i)) {
            Some(e) => {
                let lines = vec![
                    Line::from(Span::styled(
                        format!("[{}]", e.name),
                        Style::default().fg(Color::Cyan),
                    )),
                    Line::from(format!("{} vars", e.vars.len())),
                ];
                f.render_widget(Paragraph::new(Text::from(lines)), inner);
            }
            None => empty(
                f,
                inner,
                "No environment active",
                &[
                    "Switch with",
                    "  :env <name>",
                    "",
                    "Or create one in",
                    "  ~/.config/lazyfetch/environments/",
                ],
            ),
        },
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
