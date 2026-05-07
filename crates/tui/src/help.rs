//! Single source of truth for the help overlay's contents. The data lives here so the
//! rendered help is one edit away from the keymap module that actually defines bindings.
//! Layout consumes via `entries()`; keymap currently doesn't (could in the future).

#[derive(Debug, Clone, Copy)]
pub enum HelpEntry {
    Section(&'static str),
    Row {
        key: &'static str,
        desc: &'static str,
    },
    Blank,
}

/// All help rows in display order. Section headers and blanks are part of the same flat
/// list — layout iterates and styles each variant.
pub fn entries() -> &'static [HelpEntry] {
    use HelpEntry::*;
    &[
        Section("Global"),
        Row {
            key: "1 2 3 4 5",
            desc: "jump to pane (Coll · URL · Req · Resp · Env)",
        },
        Row {
            key: "h j k l",
            desc: "(arrows) — spatial pane move",
        },
        Row {
            key: "Tab / S-Tab",
            desc: "cycle pane focus",
        },
        Row {
            key: "?",
            desc: "toggle this help",
        },
        Row {
            key: ":",
            desc: "command mode",
        },
        Row {
            key: "q  /  C-c",
            desc: "quit",
        },
        Blank,
        Section("Send / save"),
        Row {
            key: "F5",
            desc: "send — works in any pane / mode (universal)",
        },
        Row {
            key: "s",
            desc: "send (any pane in Normal mode)",
        },
        Row {
            key: "Enter",
            desc: "send (when URL bar focused)",
        },
        Row {
            key: "Ctrl-s",
            desc: "send (any pane, any mode)",
        },
        Row {
            key: "Ctrl-w",
            desc: "save URL+method as request (popup, any pane)",
        },
        Row {
            key: ":save api/users",
            desc: "save URL+method as <coll>/<name>",
        },
        Row {
            key: ":messages",
            desc: "open scrollable history of all toasts",
        },
        Blank,
        Section("Response pane"),
        Row {
            key: "j / k",
            desc: "line up/down",
        },
        Row {
            key: "h / l",
            desc: "char left/right",
        },
        Row {
            key: "0 / $",
            desc: "line start / end",
        },
        Row {
            key: "w / b",
            desc: "word forward / back",
        },
        Row {
            key: "Ctrl-d / Ctrl-u",
            desc: "half page",
        },
        Row {
            key: "Ctrl-f / Ctrl-b",
            desc: "full page",
        },
        Row {
            key: "gg / G",
            desc: "top / bottom",
        },
        Row {
            key: "{ / }",
            desc: "prev / next blank line",
        },
        Row {
            key: "H / M / L",
            desc: "viewport top / mid / bottom",
        },
        Row {
            key: "%",
            desc: "matching brace { } [ ]",
        },
        Row {
            key: "] / [",
            desc: "next / prev sibling block",
        },
        Row {
            key: "v",
            desc: "toggle visual select",
        },
        Row {
            key: "y",
            desc: "yank selection (or line) → clipboard",
        },
        Row {
            key: "/  n  N",
            desc: "search · next · prev",
        },
        Row {
            key: "Esc",
            desc: "exit visual / clear search",
        },
        Blank,
        Section("URL bar"),
        Row {
            key: "type / Bksp",
            desc: "edit URL inline",
        },
        Row {
            key: "Alt-↑ / Alt-↓",
            desc: "cycle HTTP method",
        },
        Row {
            key: ":method GET",
            desc: "set method by name (any pane)",
        },
        Row {
            key: "{{",
            desc: "open variable suggestions",
        },
        Row {
            key: "Tab / Enter",
            desc: "accept selected variable",
        },
        Row {
            key: "↑ / ↓",
            desc: "navigate suggestions",
        },
        Blank,
        Section("Collections pane"),
        Row {
            key: "j / k",
            desc: "move row cursor",
        },
        Row {
            key: "Space",
            desc: "expand / collapse collection",
        },
        Row {
            key: "Enter",
            desc: "expand collection · open request (loads URL + method)",
        },
        Row {
            key: "r",
            desc: "rename collection / request",
        },
        Row {
            key: "x",
            desc: "mark / unmark request for batch move",
        },
        Row {
            key: "M",
            desc: "move marked (or cursor) request → another collection",
        },
        Blank,
        Section("Env pane"),
        Row {
            key: "j / k",
            desc: "move row cursor",
        },
        Row {
            key: "a",
            desc: "add variable",
        },
        Row {
            key: "A",
            desc: "add secret variable",
        },
        Row {
            key: "e",
            desc: "edit selected row",
        },
        Row {
            key: "d",
            desc: "delete selected row",
        },
        Row {
            key: "m",
            desc: "toggle secret flag",
        },
        Row {
            key: "r",
            desc: "reveal / hide secret value",
        },
        Row {
            key: ":env <name>",
            desc: "switch active env",
        },
        Row {
            key: ":newenv <name>",
            desc: "create new env (becomes active)",
        },
        Blank,
        Section("Insert mode  (a / A)"),
        Row {
            key: "Tab",
            desc: "swap key ↔ value field",
        },
        Row {
            key: "Enter",
            desc: "commit, save to disk",
        },
        Row {
            key: "Esc",
            desc: "cancel",
        },
        Blank,
        Section("Command mode  (:)"),
        Row {
            key: ":env <name>",
            desc: "switch active environment",
        },
        Row {
            key: ":q",
            desc: "quit",
        },
        Row {
            key: "Esc",
            desc: "cancel",
        },
    ]
}
