//! Request pane render. Composes editor + kv_editor + tab badge.

use crate::app::{AppState, ReqTab};
use crate::editor::BodyEditorState;
use crate::kv_editor::{KvEditor, KvMode};
use lazyfetch_core::catalog::BodyKind;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);
    render_tabs(f, chunks[0], state);
    match state.req_tab {
        ReqTab::Body => render_body_tab(f, chunks[1], state),
        ReqTab::Headers => render_kv(f, chunks[1], &state.headers_kv),
        ReqTab::Query => render_kv(f, chunks[1], &state.query_kv),
    }
}

fn render_tabs(f: &mut Frame, area: Rect, state: &AppState) {
    let mk = |label: &str, my: ReqTab| {
        let active = state.req_tab == my;
        Span::styled(
            format!(" {} ", label),
            if active {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )
    };
    let line = Line::from(vec![
        mk("1 Body", ReqTab::Body),
        Span::raw("  "),
        mk("2 Headers", ReqTab::Headers),
        Span::raw("  "),
        mk("3 Query", ReqTab::Query),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_body_tab(f: &mut Frame, area: Rect, state: &AppState) {
    let kind_color = match state.req_body_kind {
        BodyKind::Json => Color::Green,
        BodyKind::Form => Color::Cyan,
        BodyKind::Multipart => Color::Magenta,
        BodyKind::GraphQL => Color::Yellow,
        BodyKind::Raw => Color::Gray,
        _ => Color::DarkGray,
    };
    let header = Line::from(vec![Span::styled(
        format!("[{:?} ▾]", state.req_body_kind),
        Style::default().fg(kind_color).add_modifier(Modifier::BOLD),
    )]);
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    f.render_widget(Paragraph::new(header), split[0]);

    match &state.body_editor {
        BodyEditorState::None => {
            let p = Paragraph::new(Line::from(Span::styled(
                "(no body)  press i / a to edit  ·  Tab cycles body kind",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
            f.render_widget(p, split[1]);
        }
        BodyEditorState::Single(ta) => {
            f.render_widget(ta, split[1]);
        }
        BodyEditorState::Split {
            query, variables, ..
        } => {
            let halves = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(split[1]);
            f.render_widget(query, halves[0]);
            f.render_widget(variables, halves[1]);
        }
    }
}

fn render_kv(f: &mut Frame, area: Rect, kv: &KvEditor) {
    let lines: Vec<Line> = kv
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let cursor = if kv.cursor == i { "▌" } else { " " };
            let toggle = if r.enabled { "[x]" } else { "[ ]" };
            let value = if r.secret {
                "***".to_string()
            } else {
                r.value.clone()
            };
            Line::from(vec![
                Span::raw(cursor.to_string()),
                Span::raw(" "),
                Span::styled(
                    toggle,
                    Style::default().fg(if r.enabled {
                        Color::Green
                    } else {
                        Color::DarkGray
                    }),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:<24}", r.key),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(value, Style::default().fg(Color::White)),
            ])
        })
        .collect();
    let mode_hint = match kv.mode {
        KvMode::Normal => {
            " j/k · a add · i edit value · Tab swap (in edit) · x toggle · d del".to_string()
        }
        KvMode::InsertKey { .. } => format!(" [insert key] {}", kv.buf),
        KvMode::InsertValue { .. } => format!(" [insert value] {}", kv.buf),
    };
    let mut all = lines;
    all.push(Line::from(""));
    all.push(Line::from(Span::styled(
        mode_hint,
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(Paragraph::new(all), area);
}
