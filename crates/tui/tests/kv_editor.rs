use lazyfetch_tui::kv_editor::{KvEditor, KvMode, KvRowKind};

fn ed() -> KvEditor {
    KvEditor::new()
}

#[test]
fn add_row_via_a_then_commit() {
    let mut e = ed();
    e.start_add();
    assert_eq!(e.rows.len(), 1);
    for c in "X-Trace".chars() {
        e.insert_char(c);
    }
    e.tab();
    for c in "abc".chars() {
        e.insert_char(c);
    }
    e.commit();
    assert_eq!(e.mode, KvMode::Normal);
    assert_eq!(e.rows[0].key, "X-Trace");
    assert_eq!(e.rows[0].value, "abc");
}

#[test]
fn i_edits_value_of_cursor_row() {
    let mut e = ed();
    e.push_row("Auth", "x");
    e.cursor = 0;
    e.start_edit_value();
    assert_eq!(e.mode, KvMode::InsertValue { row: 0 });
}

#[test]
fn x_toggles_enabled() {
    let mut e = ed();
    e.push_row("Auth", "x");
    e.cursor = 0;
    assert!(e.rows[0].enabled);
    e.toggle_enabled();
    assert!(!e.rows[0].enabled);
}

#[test]
fn d_deletes_row() {
    let mut e = ed();
    e.push_row("A", "1");
    e.push_row("B", "2");
    e.cursor = 0;
    e.delete();
    assert_eq!(e.rows.len(), 1);
    assert_eq!(e.rows[0].key, "B");
}

#[test]
fn esc_cancels_insert_without_row_creation() {
    let mut e = ed();
    e.start_add();
    e.insert_char('X');
    e.cancel();
    assert_eq!(e.mode, KvMode::Normal);
    assert_eq!(e.rows.len(), 0);
}

#[test]
fn multipart_row_kind_toggle() {
    let mut e = ed();
    e.push_row("avatar", "");
    e.cursor = 0;
    assert_eq!(e.rows[0].kind, KvRowKind::Text);
    e.toggle_kind();
    assert_eq!(e.rows[0].kind, KvRowKind::File);
}

#[test]
fn empty_key_commit_stays_in_insert() {
    let mut e = ed();
    e.start_add();
    e.tab();
    for c in "abc".chars() {
        e.insert_char(c);
    }
    e.commit();
    assert!(matches!(
        e.mode,
        KvMode::InsertKey { .. } | KvMode::InsertValue { .. }
    ));
}
