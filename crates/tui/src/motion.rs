//! Pure motion + selection helpers for the Response pane. All functions take an
//! immutable `AppState` and return navigation targets — they never mutate. The
//! only side effect is `copy_to_clipboard`, which spawns a clipboard helper.
//!
//! Extracted from `keymap.rs` so the keymap reads as a pure dispatch table and
//! the algorithmic core (vim motion, brace matching, word boundaries) lives
//! somewhere unit-testable on its own merits.

use crate::app::AppState;

pub fn current_body(state: &AppState) -> Option<String> {
    state.last_response_pretty.clone()
}

pub fn first_non_space_col(state: &AppState) -> usize {
    current_body(state)
        .and_then(|b| {
            b.lines()
                .nth(state.response_cursor)
                .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        })
        .unwrap_or(0)
}

pub fn current_line_len(state: &AppState) -> usize {
    current_body(state)
        .and_then(|b| {
            b.lines()
                .nth(state.response_cursor)
                .map(|l| l.chars().count())
        })
        .unwrap_or(0)
}

pub fn current_line_text(state: &AppState) -> String {
    current_body(state)
        .and_then(|b| b.lines().nth(state.response_cursor).map(String::from))
        .unwrap_or_default()
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Walk forward (`dir=1`) or backward (`dir=-1`) to the next non-empty line at the same
/// indent depth as the cursor line. Pretty-printed JSON sibling = same-indent line.
pub fn sibling_target(state: &AppState, dir: i32) -> Option<usize> {
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let cur = state.response_cursor.min(lines.len().saturating_sub(1));
    let depth = indent_of(lines.get(cur)?);
    let n = lines.len();
    let mut i = cur as i32 + dir;
    while i >= 0 && (i as usize) < n {
        let line = lines[i as usize];
        if !line.trim().is_empty() && indent_of(line) == depth {
            return Some(i as usize);
        }
        if !line.trim().is_empty() && indent_of(line) < depth {
            return None;
        }
        i += dir;
    }
    None
}

/// Vim-style `%`: from the cursor position, find the next brace on the current line, then
/// jump to its matching pair (line + column). Skips characters inside string literals.
pub fn matching_brace_position(state: &AppState) -> Option<(usize, usize)> {
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let cur_line = state.response_cursor.min(lines.len().saturating_sub(1));
    let cur_col = state.response_col;

    let line_chars: Vec<char> = lines.get(cur_line)?.chars().collect();
    let (start_col, start_brace) = first_brace_at_or_after(&line_chars, cur_col)?;
    let opener = matches!(start_brace, '{' | '[' | '(');

    if opener {
        forward_match(&lines, cur_line, start_col + 1, start_brace)
    } else {
        backward_match(&lines, cur_line, start_col, start_brace)
    }
}

fn first_brace_at_or_after(chars: &[char], from: usize) -> Option<(usize, char)> {
    let mut in_str = false;
    let mut esc = false;
    for (i, &c) in chars.iter().enumerate() {
        if i < from {
            if esc {
                esc = false;
            } else if c == '\\' && in_str {
                esc = true;
            } else if c == '"' {
                in_str = !in_str;
            }
            continue;
        }
        if esc {
            esc = false;
            continue;
        }
        if c == '\\' && in_str {
            esc = true;
            continue;
        }
        if c == '"' {
            in_str = !in_str;
            continue;
        }
        if !in_str && matches!(c, '{' | '}' | '[' | ']' | '(' | ')') {
            return Some((i, c));
        }
    }
    None
}

fn forward_match(
    lines: &[&str],
    start_line: usize,
    start_col: usize,
    opener: char,
) -> Option<(usize, usize)> {
    let close = match opener {
        '{' => '}',
        '[' => ']',
        '(' => ')',
        _ => return None,
    };
    let mut depth: i32 = 1;
    let mut in_str = false;
    let mut esc = false;
    for (i, l) in lines.iter().enumerate().skip(start_line) {
        let chars: Vec<char> = l.chars().collect();
        let from = if i == start_line { start_col } else { 0 };
        for (off, &c) in chars[from..].iter().enumerate() {
            let abs = from + off;
            if esc {
                esc = false;
                continue;
            }
            if c == '\\' && in_str {
                esc = true;
                continue;
            }
            if c == '"' {
                in_str = !in_str;
                continue;
            }
            if in_str {
                continue;
            }
            if c == opener {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some((i, abs));
                }
            }
        }
    }
    None
}

fn backward_match(
    lines: &[&str],
    start_line: usize,
    start_col: usize,
    closer: char,
) -> Option<(usize, usize)> {
    let open = match closer {
        '}' => '{',
        ']' => '[',
        ')' => '(',
        _ => return None,
    };
    let mut depth: i32 = 1;
    let mut entries: Vec<(usize, usize, char)> = Vec::new();
    let mut in_str = false;
    let mut esc = false;
    for (li, l) in lines.iter().enumerate().take(start_line + 1) {
        let chars: Vec<char> = l.chars().collect();
        let upto = if li == start_line {
            start_col
        } else {
            chars.len()
        };
        for (col, &c) in chars[..upto].iter().enumerate() {
            if esc {
                esc = false;
                continue;
            }
            if c == '\\' && in_str {
                esc = true;
                continue;
            }
            if c == '"' {
                in_str = !in_str;
                continue;
            }
            if in_str {
                continue;
            }
            entries.push((li, col, c));
        }
    }
    for (li, col, c) in entries.into_iter().rev() {
        if c == closer {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                return Some((li, col));
            }
        }
    }
    None
}

/// Vim `w` — next word start. Returns `(line, col)`.
pub fn next_word_pos(state: &AppState) -> Option<(usize, usize)> {
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let cur_line = state.response_cursor;
    let cur_col = state.response_col;
    if cur_line >= lines.len() {
        return None;
    }
    let chars: Vec<char> = lines[cur_line].chars().collect();
    let mut i = cur_col;
    let was_word = chars.get(i).map(|c| is_word_char(*c)).unwrap_or(false);
    while i < chars.len()
        && chars.get(i).map(|c| is_word_char(*c)).unwrap_or(false) == was_word
        && !chars.get(i).map(|c| c.is_whitespace()).unwrap_or(false)
    {
        i += 1;
    }
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i < chars.len() {
        Some((cur_line, i))
    } else if cur_line + 1 < lines.len() {
        Some((cur_line + 1, 0))
    } else {
        None
    }
}

/// Vim `b` — previous word start. Returns `(line, col)`.
pub fn prev_word_pos(state: &AppState) -> Option<(usize, usize)> {
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let mut cur_line = state.response_cursor;
    if cur_line >= lines.len() {
        return None;
    }
    let mut chars: Vec<char> = lines[cur_line].chars().collect();
    let mut i = state.response_col;
    if i == 0 {
        if cur_line == 0 {
            return None;
        }
        cur_line -= 1;
        chars = lines[cur_line].chars().collect();
        i = chars.len();
    }
    i = i.saturating_sub(1);
    while i > 0 && chars.get(i).map(|c| c.is_whitespace()).unwrap_or(false) {
        i -= 1;
    }
    let was_word = chars.get(i).map(|c| is_word_char(*c)).unwrap_or(false);
    while i > 0
        && chars
            .get(i - 1)
            .map(|c| is_word_char(*c) == was_word && !c.is_whitespace())
            .unwrap_or(false)
    {
        i -= 1;
    }
    Some((cur_line, i))
}

/// Concatenate the visual selection (anchor → cursor) into a single String.
pub fn selection_text(state: &AppState) -> Option<String> {
    let anchor = state.visual_anchor?;
    let body = current_body(state)?;
    let lines: Vec<&str> = body.lines().collect();
    let (a, b) = (
        (anchor.0, anchor.1),
        (state.response_cursor, state.response_col),
    );
    let (start, end) = if (a.0, a.1) <= (b.0, b.1) {
        (a, b)
    } else {
        (b, a)
    };
    let mut out = String::new();
    for line in start.0..=end.0 {
        let chars: Vec<char> = lines.get(line)?.chars().collect();
        let from = if line == start.0 { start.1 } else { 0 };
        let to = if line == end.0 {
            (end.1 + 1).min(chars.len())
        } else {
            chars.len()
        };
        if from < chars.len() {
            out.extend(&chars[from..to.min(chars.len())]);
        }
        if line < end.0 {
            out.push('\n');
        }
    }
    Some(out)
}

/// Pipe text to a system clipboard helper. These tools daemonize and retain ownership of
/// the selection after our process exits, which `arboard` does not on X11/Wayland.
pub fn copy_to_clipboard(s: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let on_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let on_x11 = std::env::var_os("DISPLAY").is_some();
    let mut candidates: Vec<(&str, Vec<&str>)> = Vec::new();
    if on_wayland {
        candidates.push(("wl-copy", vec![]));
    }
    if on_x11 {
        candidates.push(("xclip", vec!["-selection", "clipboard"]));
        candidates.push(("xsel", vec!["--clipboard", "--input"]));
    }
    candidates.push(("pbcopy", vec![]));
    candidates.push(("clip.exe", vec![]));

    let mut last_err = String::from(
        "no clipboard tool found in PATH (tried: wl-copy, xclip, xsel, pbcopy, clip.exe)",
    );
    for (cmd, args) in candidates {
        let spawn = Command::new(cmd)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawn {
            Ok(mut child) => {
                if let Some(mut stdin) = child.stdin.take() {
                    if let Err(e) = stdin.write_all(s.as_bytes()) {
                        last_err = format!("{}: write failed: {}", cmd, e);
                        continue;
                    }
                }
                let _ = child.wait();
                return Ok(());
            }
            Err(e) => {
                last_err = format!("{}: {}", cmd, e);
            }
        }
    }
    Err(last_err)
}
