use vtcode_waveform_viewer::{
    vcd,
    viewer::{Event, Session, Viewer},
};

fn viewer() -> Viewer {
    let mut v = Viewer::new("workspace.vcd".into(), None);
    v.ready(vcd::parse(b"$timescale 1 ns $end $scope module top $end $var wire 1 ! clk $end $var wire 32 # data $end $upscope $end $enddefinitions $end #0 0! b11011110101011011011111011101111 # #11 1! b01100000000100001000000000000000 # #31 0! b10101011 # #1000").unwrap());
    v.width = 160;
    v.height = 38;
    v.selection = 1;
    v.session.inspector = true;
    // As after a real resize: whole cells for this width.
    v.clamp_window();
    v
}
fn lines(v: &Viewer) -> Vec<String> {
    v.render()
        .lines
        .iter()
        .map(|line| line.iter().map(|s| s.text.as_str()).collect())
        .collect()
}
fn dock(v: &Viewer) -> String {
    lines(v)
        .into_iter()
        .skip(usize::from(v.height - 7))
        .take(4)
        .collect::<Vec<_>>()
        .join("\n")
}
fn hover(v: &mut Viewer, x: u16, y: u16) {
    v.event(Event {
        kind: "move".into(),
        view_column: x,
        view_row: y,
        ..Event::default()
    });
}

#[test]
fn docked_edge_inspection_preserves_full_values_and_keeps_the_waveforms_clear() {
    let mut v = viewer();
    v.session.cursor = 11;
    v.session.alt_cursor = Some(31);
    assert_eq!(v.inspector_height(), 4);
    assert_eq!(v.layout().body_y + v.layout().body_height, v.height - 7);
    let text = dock(&v);
    assert!(text.contains("INSPECT  top.data"), "{text}");
    assert!(text.contains("Before HEX  dead_beef"), "{text}");
    assert!(text.contains("After  HEX  6010_8000"), "{text}");
    assert!(text.contains("B 31 ns: 6010_8000 → ab"), "{text}");
    assert!(text.contains("Δt(B−A) +20 ns"), "{text}");
    let row = v.rows().into_iter().find(|r| r.watch == 1).unwrap();
    let l = v.layout();
    hover(&mut v, l.wave_x - 3, l.body_y + row.line);
    let frame = lines(&v);
    assert!(
        frame
            .iter()
            .skip(5)
            .take(usize::from(l.body_height))
            .all(|line| !line.contains('╭'))
    );
    assert!(
        frame
            .iter()
            .any(|line| line.contains("dead_beef → 6010_8000"))
    );
}

#[test]
fn passive_pointer_inspection_keeps_a_and_b_fixed_and_labels_every_source() {
    let mut v = viewer();
    v.session.cursor = 0;
    v.session.alt_cursor = Some(11);
    let before = v.session.clone();
    let l = v.layout();
    let row = v.rows().into_iter().find(|r| r.watch == 1).unwrap();
    hover(&mut v, l.wave_x + 20, l.body_y + row.line);
    let text = dock(&v);
    assert!(text.contains("Pointer @"), "{text}");
    assert!(text.contains("HEX  ab"), "{text}");
    assert!(text.contains("A 0 ns: dead_beef"), "{text}");
    assert!(text.contains("B 11 ns:"), "{text}");
    assert_eq!(v.session, before);
    v.event(Event {
        kind: "leave".into(),
        ..Event::default()
    });
    assert!(dock(&v).contains("HEX  dead_beef"));
    assert!(!dock(&v).contains("Pointer @"));
}

#[test]
fn inspector_is_responsive_persisted_and_available_on_demand() {
    let mut v = viewer();
    let visible = v.layout().body_height;
    v.action("inspector");
    assert_eq!(v.inspector_height(), 0);
    assert_eq!(v.layout().body_height, visible + 4);
    let mut saved = serde_json::to_value(&v.session).unwrap();
    let restored: Session = serde_json::from_value(saved.clone()).unwrap();
    assert!(!restored.inspector);
    saved.as_object_mut().unwrap().remove("inspector");
    assert!(!serde_json::from_value::<Session>(saved).unwrap().inspector);
    v.action("inspector");
    for (width, height) in [(79, 38), (160, 29), (40, 12)] {
        v.width = width;
        v.height = height;
        assert_eq!(v.inspector_height(), 0);
        assert_eq!(v.render().height, height);
    }
}

#[test]
fn zoom_history_restores_exact_windows_and_keeps_measurement_cursors() {
    let mut v = viewer();
    assert!(
        !v.toolbar()
            .iter()
            .find(|b| b.action == "zoom_back")
            .unwrap()
            .enabled
    );
    v.fit();
    assert!(
        !v.toolbar()
            .iter()
            .find(|b| b.action == "zoom_back")
            .unwrap()
            .enabled
    );
    v.session.cursor = 11;
    v.session.alt_cursor = Some(31);
    let original = (v.session.start, v.session.span);
    v.zoom(true, 11);
    let zoomed = (v.session.start, v.session.span);
    v.action("fit_cursors");
    // Whole exact cells around both cursors, with a small margin.
    assert!(v.column_at(11).is_some() && v.column_at(31).is_some());
    assert!(v.cell_ticks() == 1 && !v.preview_scale());
    v.event(Event {
        code: "char".into(),
        text: "Z".into(),
        shift: true,
        ..Event::default()
    });
    assert_eq!((v.session.start, v.session.span), zoomed);
    v.action("zoom_back");
    assert_eq!((v.session.start, v.session.span), original);
    assert_eq!((v.session.cursor, v.session.alt_cursor), (11, Some(31)));
    assert!(
        !v.toolbar()
            .iter()
            .find(|b| b.action == "zoom_back")
            .unwrap()
            .enabled
    );
}
