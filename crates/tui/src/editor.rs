//! Body editor: inline tui-textarea + $EDITOR shell-out.
//! Owns the body editor state machine. Knows nothing about KV.

use crate::terminal::TerminalGuard;
use lazyfetch_core::catalog::BodyKind;
use std::io::{Read, Write};
use tui_textarea::TextArea;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphQlFocus {
    Query,
    Variables,
}

pub enum BodyEditorState {
    None,
    Single(Box<TextArea<'static>>),
    Split {
        query: Box<TextArea<'static>>,
        variables: Box<TextArea<'static>>,
        focus: GraphQlFocus,
    },
}

impl BodyEditorState {
    pub fn for_kind(kind: BodyKind, prev_text: &str) -> Self {
        match kind {
            BodyKind::None | BodyKind::File => Self::None,
            BodyKind::Json | BodyKind::Raw => {
                let mut ta = TextArea::default();
                for line in prev_text.lines() {
                    ta.insert_str(line);
                    ta.insert_newline();
                }
                Self::Single(Box::new(ta))
            }
            BodyKind::Form | BodyKind::Multipart => Self::None,
            BodyKind::GraphQL => Self::Split {
                query: Box::new(TextArea::default()),
                variables: Box::new(TextArea::default()),
                focus: GraphQlFocus::Query,
            },
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Single(t) => t.lines().join("\n"),
            Self::Split { query, .. } => query.lines().join("\n"),
        }
    }

    pub fn graphql_parts(&self) -> Option<(String, String)> {
        if let Self::Split {
            query, variables, ..
        } = self
        {
            Some((query.lines().join("\n"), variables.lines().join("\n")))
        } else {
            None
        }
    }
}

/// Result of a shell-out. `text` is what came back from the editor (may be empty).
/// `resume_err` is set when the terminal-restore step failed — caller should surface
/// it as a user-visible warning, since silently swallowing it leaves the user staring
/// at a black screen with no idea why.
pub struct ShellOutResult {
    pub text: String,
    pub resume_err: Option<String>,
}

/// Shell out to $EDITOR. Always attempts to restore the terminal even on panic.
/// Resume errors are reported via `ShellOutResult::resume_err`, never swallowed.
pub fn shell_out(
    term: &mut TerminalGuard,
    initial: &str,
    ext: &str,
) -> std::io::Result<ShellOutResult> {
    let scratch_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&scratch_dir)?;
    let mut tmp = tempfile::Builder::new()
        .prefix("lazyfetch-")
        .suffix(ext)
        .tempfile_in(&scratch_dir)?;
    tmp.write_all(initial.as_bytes())?;
    tmp.as_file().sync_all()?;

    /// Drop guard captures resume's io::Error so the caller can report it.
    /// On a panic path, the error is logged via `eprintln!` (last-ditch surface
    /// before the process unwinds).
    struct SuspendGuard<'a, 'b> {
        term: &'a mut TerminalGuard,
        out: &'b mut Option<String>,
    }
    impl Drop for SuspendGuard<'_, '_> {
        fn drop(&mut self) {
            if let Err(e) = self.term.resume() {
                let msg = format!("terminal resume failed: {e}");
                if std::thread::panicking() {
                    eprintln!("{msg}");
                }
                *self.out = Some(msg);
            }
        }
    }

    term.suspend()?;
    let mut resume_err: Option<String> = None;
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    let status_result = {
        let _suspend = SuspendGuard {
            term,
            out: &mut resume_err,
        };
        std::process::Command::new(&editor).arg(tmp.path()).status()
    }; // SuspendGuard dropped here, terminal resumed
    let status = status_result?;
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "$EDITOR exited {:?}",
            status.code()
        )));
    }
    let mut text = String::new();
    let _ = std::fs::File::open(tmp.path())?.read_to_string(&mut text);
    Ok(ShellOutResult { text, resume_err })
}
