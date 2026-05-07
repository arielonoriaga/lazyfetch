use crate::app::{AppState, Focus};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
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

    let status =
        Paragraph::new(Line::from(":send  /search  e edit  s send  S save  ? help  q quit"))
            .style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, outer[1]);
}

fn pane(f: &mut Frame, area: Rect, title: &str, my: Focus, state: &AppState) {
    let focused = state.focus == my;
    let style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(style);
    let body_text = match my {
        Focus::Collections => state
            .collections
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        Focus::Env => match state.active_env.and_then(|i| state.envs.get(i)) {
            Some(e) => format!("[{}]\n{} vars", e.name, e.vars.len()),
            None => "(no env)".into(),
        },
        Focus::Request => "(no request open)".into(),
        Focus::Response => "(no response yet)".into(),
    };
    let p = Paragraph::new(body_text).block(block);
    f.render_widget(p, area);
}
