//! Whole-tick cells, the exact limit and its soft lock, and honest previews.
use unicode_width::UnicodeWidthStr;
use vtcode_waveform_viewer::{
    vcd,
    viewer::{Event, Viewer, Watch},
};

const DEMO: &[u8] = include_bytes!("../examples/demo.vcd");

fn demo(width: u16) -> Viewer {
    let mut v = Viewer::new("demo.vcd".into(), None);
    v.ready(vcd::parse(DEMO).unwrap());
    v.event(Event {
        kind: "resize".into(),
        width,
        height: 40,
        ..Event::default()
    });
    v
}
fn lines(v: &Viewer) -> Vec<String> {
    v.render()
        .lines
        .iter()
        .map(|line| line.iter().map(|s| s.text.as_str()).collect())
        .collect()
}
fn background(v: &Viewer, x: u16, y: u16) -> String {
    let frame = v.render();
    let mut column = 0;
    for span in &frame.lines[usize::from(y)] {
        column += span.text.width();
        if column > usize::from(x) {
            return span.background.clone();
        }
    }
    String::new()
}
fn row_y(v: &Viewer, name: &str) -> u16 {
    let watch = v
        .session
        .watches
        .iter()
        .position(|w| matches!(w, Watch::Signal { path, .. } if path.ends_with(name)))
        .unwrap();
    let row = v.rows().into_iter().find(|r| r.watch == watch).unwrap();
    v.layout().body_y + row.line - v.scroll as u16
}
/// Columns of drawn transitions in a 2-line digital row.
fn edge_columns(v: &Viewer, name: &str) -> Vec<u16> {
    let l = v.layout();
    let y = usize::from(row_y(v, name));
    let frame = lines(v);
    frame[y]
        .chars()
        .skip(usize::from(l.wave_x))
        .zip(frame[y + 1].chars().skip(usize::from(l.wave_x)))
        .enumerate()
        .filter(|(_, (top, bottom))| matches!(top, '┌' | '┐') && matches!(bottom, '┘' | '└'))
        .map(|(x, _)| x as u16)
        .collect()
}

#[test]
fn opens_exact_and_a_steady_clock_never_beats() {
    for width in [100, 130, 157, 200] {
        let v = demo(width);
        assert_eq!(v.exact_limit(), Some(10), "demo clock toggles every 10 ns");
        assert!(!v.preview_scale(), "{width}: opens on an exact scale");
        assert!(lines(&v)[0].contains("Exact · 10 ns / cell"), "{width}");
        let edges = edge_columns(&v, "clk");
        assert!(edges.len() > 10);
        // Every half period is the same number of cells.
        let gaps: Vec<u16> = edges.windows(2).map(|p| p[1] - p[0]).collect();
        assert!(gaps.iter().all(|g| *g == gaps[0]), "{width}: {gaps:?}");
    }
}

#[test]
fn simultaneous_transitions_share_a_column_at_every_zoom() {
    // valid and tx toggle on clock edges: wherever both are drawn as edges,
    // a transition at the same time is in the same column.
    let mut v = demo(150);
    for _ in 0..6 {
        let l = v.layout();
        for x in l.wave_x..l.wave_x + l.wave_width {
            let time = v.time_at(x);
            if time < v.wave.end {
                assert_eq!(
                    v.column_at(time),
                    Some(x),
                    "time {time} maps back to its cell"
                );
            }
        }
        if !v.preview_scale() {
            let valid = edge_columns(&v, "valid");
            let clock = edge_columns(&v, "clk");
            assert!(
                valid.iter().all(|x| clock.contains(x)),
                "{valid:?} ⊄ {clock:?}"
            );
        }
        v.zoom(false, 0);
    }
}

#[test]
fn zoom_out_snaps_to_the_exact_limit_before_a_tinted_preview() {
    let mut v = demo(150);
    v.zoom(true, 0);
    v.zoom(true, 0);
    assert!(v.cell_ticks() < 10);
    while v.cell_ticks() < 10 {
        v.zoom(false, 0);
        assert!(!v.preview_scale(), "every step below the limit is exact");
    }
    assert!(v.status.contains("Exact limit"));
    v.zoom(false, 0);
    assert!(v.preview_scale());
    let frame = lines(&v);
    assert!(frame[0].contains("≈ Preview"), "{}", frame[0]);
    // valid's edges no longer land on cell boundaries: its row is tinted.
    let y = row_y(&v, "valid");
    assert_eq!(background(&v, v.layout().wave_x + 3, y), "hover");
    // The clock is too fast to place: one uniform activity band, no beats.
    let clk = row_y(&v, "clk");
    let band: String = frame[usize::from(clk)]
        .chars()
        .skip(usize::from(v.layout().wave_x + 1))
        .take(40)
        .collect();
    assert!(band.chars().all(|c| c == '▒'), "{band}");
    // Clicking the badge returns to the exact limit.
    let (_, x, _) = v.scale_badge().unwrap();
    v.event(Event {
        kind: "down".into(),
        button: "left".into(),
        view_column: x + 1,
        view_row: 0,
        ..Event::default()
    });
    assert_eq!(v.cell_ticks(), 10);
    assert!(!v.preview_scale());
}

#[test]
fn resizing_keeps_the_cell_size_and_the_end_of_the_recording_is_marked() {
    let mut v = demo(150);
    v.fit();
    let cell = v.cell_ticks();
    assert!(lines(&v)[0].contains("all shown"));
    v.event(Event {
        kind: "resize".into(),
        width: 120,
        height: 40,
        ..Event::default()
    });
    assert_eq!(v.cell_ticks(), cell);
    assert!(lines(&v)[0].contains("% shown"));
    // Wider than the whole recording needs: the finest scale that still shows
    // it all (here the exact limit), and cells after its end are blank and marked.
    v.event(Event {
        kind: "resize".into(),
        width: 220,
        height: 40,
        ..Event::default()
    });
    assert_eq!(v.cell_ticks(), 10);
    assert!(lines(&v)[0].contains("Exact · 10 ns / cell · all shown"));
    let end = v.column_at(v.wave.end).unwrap();
    let y = row_y(&v, "rst_n");
    assert_eq!(
        lines(&v)[usize::from(y)].chars().nth(usize::from(end + 1)),
        Some('▏')
    );
    assert_eq!(background(&v, end + 4, y), "panel");
}
