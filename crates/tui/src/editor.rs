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

/// Shell out to $EDITOR. Always restores the terminal even on panic.
pub fn shell_out(
    term: &mut TerminalGuard,
    initial: &str,
    ext: &str,
) -> std::io::Result<String> {
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

    struct SuspendGuard<'a> {
        term: &'a mut TerminalGuard,
    }
    impl Drop for SuspendGuard<'_> {
        fn drop(&mut self) {
            let _ = self.term.resume();
        }
    }

    term.suspend()?;
    let _suspend = SuspendGuard { term };

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
    let status = std::process::Command::new(&editor)
        .arg(tmp.path())
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "$EDITOR exited {:?}",
            status.code()
        )));
    }
    let mut out = String::new();
    let _ = std::fs::File::open(tmp.path())?.read_to_string(&mut out);
    Ok(out)
}
