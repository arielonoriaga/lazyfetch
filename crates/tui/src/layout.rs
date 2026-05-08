use crate::app::{AppState, CollRow, Focus, InsertField, Mode};
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
        Mode::Rename => ("RENAME ", Color::Black, Color::Magenta),
        Mode::Move => (" MOVE  ", Color::Black, Color::Magenta),
        Mode::ImportCurl => (" IMPORT ", Color::Black, Color::Magenta),
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
        Mode::Rename => format!("Rename to: {}", state.rename_buf),
        Mode::Move => format!("Move {} → {}", state.marked_requests.len(), state.move_buf),
        Mode::ImportCurl => format!("Import cURL ({} chars) — Esc cancel", state.import_curl_buf.len()),
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
            Focus::Collections => {
                "Collections: j/k · Space toggle · Enter open request · Ctrl-w save".into()
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
    if state.mode == Mode::Rename {
        draw_rename_modal(f, state);
    }
    if state.mode == Mode::Move {
        draw_move_modal(f, state);
    }
    if state.focus == Focus::Url {
        draw_url_suggestions(f, url_rect, state);
    }
    if state.help_open {
        draw_help(f, state);
    }
    if state.messages_open {
        draw_messages(f, state);
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
    state
        .last_response_pretty
        .as_deref()
        .map(|b| b.lines().count())
        .unwrap_or(0)
}

fn draw_move_modal(f: &mut Frame, state: &AppState) {
    use ratatui::widgets::Clear;
    let n = state.marked_requests.len();
    let area = f.area();
    let w = area.width.min(64);
    let h = 11u16.min(area.height);
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
            format!(" Move {} request{} ", n, if n == 1 { "" } else { "s" }),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " → target collection ",
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
                .fg(Color::Magenta)
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
    let known: Vec<String> = state.collections.iter().map(|c| c.name.clone()).collect();
    let label = Line::from(Span::styled(
        " Target collection (existing or new)",
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
            state.move_buf.clone(),
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
    let known_line = Line::from(Span::styled(
        format!(" Existing: {}", known.join(", ")),
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
        Span::styled(" move  ", Style::default().fg(Color::Gray)),
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
    f.render_widget(Paragraph::new(known_line), rows[4]);
    f.render_widget(Paragraph::new(footer), rows[5]);
}

fn draw_rename_modal(f: &mut Frame, state: &AppState) {
    use crate::app::RenameTarget;
    use ratatui::widgets::Clear;
    let Some(target) = state.rename_target.as_ref() else {
        return;
    };
    let (kind, old) = match target {
        RenameTarget::Collection { old, .. } => ("collection", old.as_str()),
        RenameTarget::Request { old, .. } => ("request", old.as_str()),
    };
    let area = f.area();
    let w = area.width.min(60);
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
            format!(" Rename {} ", kind),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} ", old),
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
                .fg(Color::Magenta)
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
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);
    let label = Line::from(Span::styled(" New name", Style::default().fg(Color::Gray)));
    let input = Line::from(vec![
        Span::styled(
            "▌ ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            state.rename_buf.clone(),
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
    let footer = Line::from(vec![
        Span::styled(
            " Enter ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" rename  ", Style::default().fg(Color::Gray)),
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
    f.render_widget(Paragraph::new(footer), rows[4]);
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

fn draw_messages(f: &mut Frame, state: &AppState) {
    use ratatui::widgets::Clear;
    let area = f.area();
    let w = area.width.min(80);
    let h = area.height.min(24);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let title = Line::from(vec![
        Span::styled(
            " Messages ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" (last {} · any key closes) ", state.messages.len()),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .title(title);
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    if state.messages.is_empty() {
        let p = Paragraph::new(Line::from(Span::styled(
            "  (no messages yet — toasts will accumulate here)",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
        f.render_widget(p, inner);
        return;
    }
    let visible = inner.height as usize;
    let total = state.messages.len();
    let start = total.saturating_sub(visible);
    let lines: Vec<Line> = state
        .messages
        .iter()
        .enumerate()
        .skip(start)
        .map(|(i, m)| {
            let n = i + 1;
            Line::from(vec![
                Span::styled(
                    format!(" {:>3} ", n),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(m.clone(), Style::default().fg(Color::White)),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn draw_help(f: &mut Frame, state: &AppState) {
    use ratatui::widgets::Clear;

    let area = f.area();
    let w = area.width.min(78);
    let h = area.height.min(30);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    let title = if state.help_filter.is_empty() {
        Line::from(Span::styled(
            " Help — keyboard shortcuts (type to filter, Esc closes) ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(vec![
            Span::styled(
                " Help ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" filter: {}▏ ", state.help_filter),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(popup);
    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    use crate::help::HelpEntry;

    let dim = Style::default().fg(Color::DarkGray);
    let kw = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    // Build Lines from the static data table — single source of truth lives in `help.rs`.
    let mut all_lines: Vec<Line> = crate::help::entries()
        .iter()
        .map(|e| match *e {
            HelpEntry::Section(s) => Line::from(Span::styled(
                s.to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            HelpEntry::Row { key, desc } => Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<14}", key), kw),
                Span::styled(desc.to_string(), Style::default().fg(Color::Gray)),
            ]),
            HelpEntry::Blank => Line::from(""),
        })
        .collect();
    all_lines.push(Line::from(""));
    all_lines.push(Line::from(Span::styled(
        "Type to filter · Backspace clears · Esc closes",
        dim.add_modifier(Modifier::ITALIC),
    )));

    // Filter rows when help_filter is non-empty. Section headers + blank lines pass through.
    let needle = state.help_filter.to_lowercase();
    let filtered: Vec<Line> = if needle.is_empty() {
        all_lines
    } else {
        all_lines
            .into_iter()
            .filter(|line| {
                let text: String = line
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>();
                text.is_empty() || text.to_lowercase().contains(&needle)
            })
            .collect()
    };

    f.render_widget(
        Paragraph::new(Text::from(filtered)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn pane(f: &mut Frame, area: Rect, title: &str, my: Focus, state: &AppState) {
    let block = pane_block(title, my, state);
    let inner = block.inner(area);
    f.render_widget(block, area);

    match my {
        Focus::Collections => {
            render_collections(f, inner, state, state.focus == Focus::Collections)
        }
        Focus::Env => render_env(f, inner, state, state.focus == Focus::Env),
        Focus::Request => crate::request_pane::render(f, inner, state),
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

    // Body lines are computed once on response receipt and cached on AppState.
    // We avoid re-pretty-printing + re-colorizing every frame.
    //
    // Search highlight is also cached: highlighted_cache stores (body_gen, needle, lines).
    // Hit → no work this frame; Miss (different gen, different needle) → recompute once.
    let needle = state
        .search_active
        .as_deref()
        .filter(|n| !n.is_empty())
        .unwrap_or("");
    let mut highlighted: Vec<Vec<Span<'static>>> = if needle.is_empty() {
        state
            .last_response_lines
            .clone()
            .unwrap_or_else(|| resp_render::plain_lines(""))
    } else if let Some((gen, cached_needle, cached)) = state.highlighted_cache.as_ref() {
        if *gen == state.body_gen && cached_needle == needle {
            cached.clone()
        } else {
            // Stale cache — caller signals dirtiness; we render the base lines for this
            // frame and let the next state mutation recompute. Since render is &state,
            // we cannot populate the cache here; SearchSubmit / new-response paths do it.
            let base = state
                .last_response_lines
                .clone()
                .unwrap_or_else(|| resp_render::plain_lines(""));
            resp_render::apply_search_highlight(base, needle).0
        }
    } else {
        let base = state
            .last_response_lines
            .clone()
            .unwrap_or_else(|| resp_render::plain_lines(""));
        resp_render::apply_search_highlight(base, needle).0
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

// pretty_body / looks_like_json / render_kind moved to crate::response — single source of truth.
use crate::response::render_kind;

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

fn render_collections(f: &mut Frame, area: Rect, state: &AppState, focused: bool) {
    use lazyfetch_core::catalog::Item;
    if state.collections.is_empty() {
        empty(
            f,
            area,
            "No collections yet",
            &[
                "Get started:",
                "  Ctrl-w then type api/health",
                "  lazyfetch import-postman <file>",
                "  lazyfetch import-postman <file> --local",
                "",
                "Files live in",
                "  .lazyfetch/collections/   (project)",
                "  ~/.config/lazyfetch/collections/   (global)",
            ],
        );
        return;
    }
    let rows = state.coll_rows();
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = focused && i == state.coll_cursor;
            let cursor_mark = if selected { "▌ " } else { "  " };
            let cursor_style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            match *row {
                CollRow::Coll { idx, expanded } => {
                    let c = &state.collections[idx];
                    let chevron = if expanded { "▾" } else { "▸" };
                    let coll_count = c
                        .root
                        .items
                        .iter()
                        .filter(|i| matches!(i, Item::Request(_)))
                        .count();
                    Line::from(vec![
                        Span::styled(cursor_mark.to_string(), cursor_style),
                        Span::styled(
                            format!("{} ", chevron),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            c.name.clone(),
                            Style::default()
                                .fg(if selected { Color::Yellow } else { Color::Cyan })
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" ({})", coll_count),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ])
                }
                CollRow::Req { coll, item } => {
                    let r = match &state.collections[coll].root.items[item] {
                        Item::Request(r) => r,
                        _ => return Line::from(""),
                    };
                    let marked = state.marked_requests.contains(&(coll, item));
                    let m = r.method.as_str();
                    let m_color = match m {
                        "GET" => Color::Green,
                        "POST" => Color::Yellow,
                        "PUT" => Color::Cyan,
                        "PATCH" => Color::Magenta,
                        "DELETE" => Color::Red,
                        _ => Color::Gray,
                    };
                    let mark_glyph = if marked { "✓ " } else { "  " };
                    let mark_style = if marked {
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    Line::from(vec![
                        Span::styled(cursor_mark.to_string(), cursor_style),
                        Span::raw("  "),
                        Span::styled(mark_glyph.to_string(), mark_style),
                        Span::styled(
                            format!("{:<6}", m),
                            Style::default().fg(m_color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            r.name.clone(),
                            Style::default().fg(if selected {
                                Color::Yellow
                            } else {
                                Color::White
                            }),
                        ),
                    ])
                }
            }
        })
        .collect();
    f.render_widget(Paragraph::new(Text::from(lines)), area);
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
