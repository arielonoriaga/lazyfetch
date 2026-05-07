use lazyfetch_tui::app::AppState;
use lazyfetch_tui::layout::draw;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn renders_four_panes() {
    let backend = TestBackend::new(80, 24);
    let mut term = Terminal::new(backend).unwrap();
    let state = AppState::new(std::path::PathBuf::from("/tmp"));
    term.draw(|f| {
        draw(f, &state);
    })
    .unwrap();
    let buf = term.backend().buffer().clone();
    let s: String = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!("initial", s);
}
