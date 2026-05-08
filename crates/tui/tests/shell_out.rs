//! $EDITOR shell-out round-trip via a deterministic stand-in.
//!
//! `cat` reads the file and writes its contents to stdout — when invoked as
//! `cat <tmpfile>` it leaves the file unchanged on disk. So the round-trip
//! property we can verify here is: input text written to the tempfile
//! survives the editor invocation and reaches `ShellOutResult.text`
//! unchanged. That covers the read-write-restore code path without needing
//! a fake terminal.
//!
//! Skipped when stdout isn't a TTY (CI without PTY): `term.suspend()` itself
//! works in non-tty environments via crossterm — we just don't render. The
//! TerminalGuard::new will fail without a tty, in which case the test marks
//! itself ignored at runtime.

#[cfg(unix)]
#[test]
fn cat_round_trip_returns_initial_text() {
    use lazyfetch_tui::editor::shell_out;
    use lazyfetch_tui::terminal::TerminalGuard;

    // No tty → can't open TerminalGuard. Bail without failing.
    if !is_tty() {
        eprintln!("skipped: stdout is not a tty");
        return;
    }
    let mut guard = match TerminalGuard::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipped: TerminalGuard::new failed ({e})");
            return;
        }
    };
    std::env::set_var("EDITOR", "cat");
    let initial = "hello\nworld\n";
    let result = shell_out(&mut guard, initial, ".txt").expect("shell_out");
    assert_eq!(result.text, initial);
    assert!(
        result.resume_err.is_none(),
        "resume should succeed on tty: {:?}",
        result.resume_err
    );
}

#[cfg(unix)]
fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
