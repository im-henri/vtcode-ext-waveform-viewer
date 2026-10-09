use unicode_width::UnicodeWidthStr;
use vtcode_waveform_viewer::{
    model::Radix,
    vcd,
    viewer::{BrowserNode, Event, Focus, Viewer, Watch, WaveSize},
};

fn viewer(names: &[&str]) -> Viewer {
    let mut v = Viewer::new("transfers.vcd".into(), None);
    v.ready(vcd::parse(include_bytes!("../examples/bus_transfers.vcd")).unwrap());
    v.width = 160;
    v.height = 32;
    v.session.watches = names
        .iter()
        .map(|name| {
            let signal = v.wave.signals.iter().find(|s| s.name == *name).unwrap();
            Watch::signal(signal.path.clone(), 2)
        })
        .collect();
    for watch in &mut v.session.watches {
        watch.set_size(None, WaveSize::Normal);
    }
    v.filter = "tb.cpu.".into();
    v.rebuild_browser();
    v.session.cursor = 240;
    v.session.alt_cursor = Some(420);
    v
}

fn pointer(v: &mut Viewer, kind: &str, x: u16, y: u16) {
    v.event(Event {
        kind: kind.into(),
        view_column: x,
        view_row: y,
        ..Event::default()
    });
}

fn press(v: &mut Viewer, name: &str) {
    let node = v
        .browser_nodes
        .iter()
        .position(|n| matches!(n, BrowserNode::Signal(id, _) if v.wave.signals[*id].name == name))
        .unwrap();
    let y = v.layout().body_y + (node - v.browser_scroll) as u16;
    pointer(v, "down", 6, y);
}

fn names(v: &Viewer) -> Vec<String> {
    v.session
        .watches
        .iter()
        .map(|w| match w {
            Watch::Signal { path, .. } => v.wave.signals[*v.signal_index.get(path).unwrap()]
                .name
                .clone(),
            Watch::Group { name, .. } => name.clone(),
            Watch::Divider => "divider".into(),
        })
        .collect()
}

fn lines(v: &Viewer) -> Vec<String> {
    v.render()
        .lines
        .iter()
        .map(|l| l.iter().map(|s| s.text.as_str()).collect())
        .collect()
}

#[test]
fn browser_drop_previews_the_actual_bus_between_rows_and_commits_once() {
    let mut v = viewer(&["clk", "reset_n", "ex_valid"]);
    let original = v.session.clone();
    let l = v.layout();
    press(&mut v, "ex_reg_wdata");
    assert_eq!(v.session, original, "a press must not append a watch");
    pointer(&mut v, "drag", l.wave_x + 20, l.body_y + 2);
    assert_eq!(v.session, original);
    assert!(v.history.is_empty());
    let preview = v.display_rows();
    assert_eq!(preview.iter().position(|r| r.preview), Some(1));
    assert_eq!(preview[1].row.height, 1);
    assert_eq!(
        preview[2].row.line, 3,
        "existing rows make room for the bus"
    );
    let frame = v.render();
    assert!(lines(&v).join("\n").contains("↓ ex_reg_wdata[31:0]"));
    assert!(
        lines(&v)
            .join("\n")
            .contains("Release to add tb.cpu.ex_reg_wdata[31:0] before tb.cpu.reset_n")
    );
    assert!(
        frame.lines[usize::from(l.body_y + 2)]
            .iter()
            .any(|s| s.background == "selection" && s.text.contains('═')),
        "the proposed row must contain a real waveform on its preview background"
    );
    for _ in 0..30 {
        pointer(&mut v, "drag", l.wave_x + 20, l.body_y + 2);
        assert_eq!(
            v.signal_drag.as_ref().unwrap().target,
            Some(1),
            "stationary preview must not jump"
        );
    }
    pointer(&mut v, "up", l.wave_x + 20, l.body_y + 2);
    assert_eq!(names(&v), ["clk", "ex_reg_wdata", "reset_n", "ex_valid"]);
    assert_eq!(v.history.len(), 1);
    assert!(v.signal_drag.is_none());
    assert_eq!(
        (
            v.session.cursor,
            v.session.alt_cursor,
            v.session.start,
            v.session.span
        ),
        (
            original.cursor,
            original.alt_cursor,
            original.start,
            original.span
        )
    );
    v.undo(false);
    assert_eq!(v.session, original);
    v.undo(true);
    assert_eq!(names(&v), ["clk", "ex_reg_wdata", "reset_n", "ex_valid"]);
}

#[test]
fn drops_support_first_last_empty_and_the_name_column() {
    for (offset, expected) in [(0, 0), (3, 2), (9, 3)] {
        let mut v = viewer(&["clk", "reset_n", "ex_valid"]);
        let l = v.layout();
        press(&mut v, "ex_reg_addr");
        pointer(&mut v, "drag", l.browser + 8, l.body_y + offset);
        pointer(&mut v, "up", l.browser + 8, l.body_y + offset);
        assert_eq!(names(&v)[expected], "ex_reg_addr");
    }
    let mut v = viewer(&[]);
    let l = v.layout();
    press(&mut v, "ex_reg_wdata");
    pointer(&mut v, "drag", l.wave_x + 4, l.body_y + 3);
    assert!(v.display_rows()[0].preview);
    assert!(!lines(&v).join("\n").contains("Drag signals here"));
    pointer(&mut v, "up", l.wave_x + 4, l.body_y + 3);
    assert_eq!(names(&v), ["ex_reg_wdata"]);
}

#[test]
fn existing_browser_signals_move_in_both_directions_without_losing_their_settings() {
    for (signal, y, expected) in [
        ("ex_valid", 0, vec!["ex_valid", "clk", "reset_n"]),
        ("clk", 9, vec!["reset_n", "ex_valid", "clk"]),
    ] {
        let mut v = viewer(&["clk", "reset_n", "ex_valid"]);
        let index = names(&v).iter().position(|name| name == signal).unwrap();
        if let Watch::Signal {
            label,
            color,
            radix,
            ..
        } = &mut v.session.watches[index]
        {
            *label = "renamed".into();
            *color = 6;
            *radix = Radix::Unsigned;
        }
        let watch = v.session.watches[index].clone();
        let l = v.layout();
        press(&mut v, signal);
        pointer(&mut v, "drag", l.wave_x + 8, l.body_y + y);
        assert_eq!(
            v.display_rows().len(),
            3,
            "moving never duplicates the source row"
        );
        pointer(&mut v, "up", l.wave_x + 8, l.body_y + y);
        assert_eq!(names(&v), expected);
        assert_eq!(v.session.watches[if y == 0 { 0 } else { 2 }], watch);
        assert_eq!(v.history.len(), 1);
    }
}

#[test]
fn expanded_buses_move_as_a_unit_from_their_name_and_keep_bit_order() {
    let mut v = viewer(&["clk", "state", "reset_n"]);
    if let Watch::Signal { expanded, .. } = &mut v.session.watches[1] {
        *expanded = true;
    }
    let original = v.session.clone();
    let l = v.layout();
    // Grab an expanded bit name, then place the whole bus first.
    pointer(&mut v, "down", l.browser + 8, l.body_y + 6);
    pointer(&mut v, "drag", l.wave_x + 8, l.body_y);
    let preview: Vec<_> = v.display_rows().into_iter().filter(|r| r.preview).collect();
    assert_eq!(
        preview.iter().map(|r| r.row.bit).collect::<Vec<_>>(),
        [None, Some(2), Some(1), Some(0)]
    );
    assert_eq!(v.session, original);
    pointer(&mut v, "up", l.wave_x + 8, l.body_y);
    assert_eq!(names(&v), ["state", "clk", "reset_n"]);
    assert_eq!(v.session.watches[0], original.watches[1]);
}

#[test]
fn cancelled_drags_leave_session_and_undo_history_untouched() {
    for cancel in ["escape", "outside", "browser", "header", "cancel", "resize"] {
        let mut v = viewer(&["clk", "reset_n"]);
        let original = v.session.clone();
        let l = v.layout();
        press(&mut v, "ex_reg_wdata");
        pointer(&mut v, "drag", l.wave_x + 8, l.body_y);
        match cancel {
            "escape" => v.event(Event {
                code: "escape".into(),
                ..Event::default()
            }),
            "outside" => v.event(Event {
                kind: "up".into(),
                view_column: 0,
                view_row: 0,
                pointer_inside: Some(false),
                ..Event::default()
            }),
            "browser" => pointer(&mut v, "up", 6, l.body_y),
            "header" => pointer(&mut v, "up", l.wave_x + 8, 3),
            "resize" => v.event(Event {
                kind: "resize".into(),
                width: 80,
                height: 24,
                ..Event::default()
            }),
            _ => pointer(&mut v, "cancel", 0, 0),
        }
        assert_eq!(v.session, original, "{cancel}");
        assert!(v.history.is_empty(), "{cancel}");
        assert!(v.signal_drag.is_none(), "{cancel}");
        assert!(!v.needs_drag_tick());
    }
}

#[test]
fn click_still_adds_on_release_and_narrow_browser_stays_visible_through_a_drop() {
    let mut v = viewer(&["clk"]);
    v.width = 80;
    v.focus = Focus::Browser;
    let l = v.layout();
    press(&mut v, "reset_n");
    let (x, y) = v.signal_drag.as_ref().unwrap().press;
    assert_eq!(names(&v), ["clk"]);
    pointer(&mut v, "up", x + 1, y);
    assert_eq!(names(&v), ["clk", "reset_n"]);
    press(&mut v, "ex_reg_addr");
    pointer(&mut v, "drag", l.wave_x + 4, l.body_y);
    assert_eq!(v.layout().browser, l.browser);
    for line in lines(&v) {
        assert!(line.width() <= 80);
    }
    pointer(&mut v, "up", l.wave_x + 4, l.body_y);
    assert_eq!(names(&v), ["ex_reg_addr", "clk", "reset_n"]);
    assert_eq!(v.layout().browser, l.browser);
}

#[test]
fn dropping_into_a_collapsed_group_previews_its_contents_and_undo_restores_the_fold() {
    let mut v = viewer(&["clk", "reset_n", "ex_valid"]);
    v.session.watches.insert(
        0,
        Watch::Group {
            name: "Control".into(),
            collapsed: true,
        },
    );
    v.session.watches.insert(
        3,
        Watch::Group {
            name: "Other".into(),
            collapsed: false,
        },
    );
    let original = v.session.clone();
    let l = v.layout();
    press(&mut v, "ex_reg_wdata");
    pointer(&mut v, "drag", l.wave_x + 5, l.body_y);
    assert_eq!(v.signal_drag.as_ref().unwrap().target, Some(1));
    assert_eq!(v.session, original);
    assert!(lines(&v).join("\n").contains("▾ Control"));
    assert!(
        v.display_rows()
            .iter()
            .any(|r| !r.preview && r.row.watch == 1)
    );
    pointer(&mut v, "up", l.wave_x + 5, l.body_y);
    assert_eq!(
        names(&v),
        [
            "Control",
            "ex_reg_wdata",
            "clk",
            "reset_n",
            "Other",
            "ex_valid"
        ]
    );
    assert!(matches!(
        v.session.watches[0],
        Watch::Group {
            collapsed: false,
            ..
        }
    ));
    v.undo(false);
    assert_eq!(v.session, original);
}

#[test]
fn held_drag_autoscrolls_to_the_last_slot_and_wheel_does_not_pan_time() {
    let mut v = viewer(&["clk"; 20]);
    v.height = 24;
    v.session.span = 400;
    let l = v.layout();
    let window = (v.session.start, v.session.span);
    press(&mut v, "ex_reg_wdata");
    pointer(&mut v, "drag", l.wave_x + 8, l.body_y + l.body_height - 1);
    assert!(v.needs_drag_tick());
    for _ in 0..5 {
        v.drag_tick();
    }
    assert_eq!(v.scroll, 1);
    v.event(Event {
        direction: 1,
        view_column: l.wave_x + 8,
        view_row: l.body_y + l.body_height - 1,
        ..Event::default()
    });
    assert!(v.scroll >= 4);
    assert_eq!((v.session.start, v.session.span), window);
    let scroll = v.scroll;
    let target = v.signal_drag.as_ref().unwrap().target;
    v.event(Event {
        direction: 1,
        shift: true,
        view_column: l.wave_x + 8,
        view_row: l.body_y + l.body_height - 1,
        ..Event::default()
    });
    assert_eq!((v.session.start, v.session.span), (80, 400));
    assert_eq!(v.scroll, scroll);
    assert_eq!(v.signal_drag.as_ref().unwrap().target, target);
    for _ in 0..1000 {
        v.drag_tick();
    }
    assert!(!v.needs_drag_tick(), "scroll tasks stop at the end");
    assert_eq!(v.signal_drag.as_ref().unwrap().target, Some(20));
    pointer(&mut v, "up", l.wave_x + 8, l.body_y + l.body_height - 1);
    assert_eq!(names(&v).last().unwrap(), "ex_reg_wdata");
    assert_eq!(v.history.len(), 1);
}

#[test]
fn full_watch_lists_allow_moves_but_reject_new_signals_without_a_preview() {
    let mut v = viewer(&["clk"; 256]);
    let l = v.layout();
    press(&mut v, "ex_reg_wdata");
    assert!(v.error);
    assert!(v.signal_drag.is_none());
    assert_eq!(v.session.watches.len(), 256);
    press(&mut v, "clk");
    assert!(!v.error);
    pointer(&mut v, "drag", l.wave_x + 8, l.body_y + 5);
    pointer(&mut v, "up", l.wave_x + 8, l.body_y + 5);
    assert_eq!(v.session.watches.len(), 256);
    assert!(v.signal_drag.is_none());
}

#[test]
fn grabbing_empty_name_space_does_not_move_the_selected_signal() {
    let mut v = viewer(&["clk", "reset_n"]);
    let original = v.session.clone();
    let l = v.layout();
    pointer(&mut v, "down", l.browser + 8, l.body_y + 8);
    pointer(&mut v, "drag", l.wave_x + 8, l.body_y + 3);
    pointer(&mut v, "up", l.wave_x + 8, l.body_y + 3);
    assert!(v.signal_drag.is_none());
    assert_eq!(v.session, original);
    assert!(v.history.is_empty());
}
