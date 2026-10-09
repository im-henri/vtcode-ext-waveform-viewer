use unicode_width::UnicodeWidthStr;
use vtcode_waveform_viewer::{
    model::{Radix, Value},
    vcd,
    viewer::{Event, Prompt, Viewer, Watch, WaveSize},
};

fn demo() -> Viewer {
    let mut v = Viewer::new("demo.vcd".into(), None);
    v.ready(vcd::parse(include_bytes!("../examples/demo.vcd")).unwrap());
    v.width = 160;
    v.height = 24;
    v.session.cursor = 240;
    v.session.alt_cursor = Some(420);
    v
}
fn pointer(v: &mut Viewer, kind: &str, x: u16, y: u16, shift: bool) {
    v.event(Event {
        kind: kind.into(),
        view_column: x,
        view_row: y,
        shift,
        ..Event::default()
    });
}
fn key(v: &mut Viewer, code: &str) {
    v.event(Event {
        code: code.into(),
        ..Event::default()
    });
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
    panic!("Missing cell {x},{y}");
}

#[test]
fn range_zoom_previews_both_directions_without_applying_until_release() {
    for reverse in [false, true] {
        let mut v = demo();
        v.session.start = 100;
        v.session.span = 600;
        let l = v.layout();
        let (first, last) = if reverse {
            (l.wave_x + 45, l.wave_x + 10)
        } else {
            (l.wave_x + 10, l.wave_x + 45)
        };
        let a = v.time_at(first);
        let b = v.time_at(last);
        let session = v.session.clone();
        pointer(&mut v, "down", first, l.body_y + 2, true);
        pointer(&mut v, "drag", last, l.body_y + 2, true);
        assert_eq!(v.session, session);
        assert!(lines(&v).join("\n").contains("Zoom range:"));
        assert!(lines(&v).join("\n").contains("Release to zoom"));
        assert_eq!(background(&v, l.wave_x + 27, l.body_y + 12), "selection");
        assert_eq!(v.drag.unwrap().end_x, last);
        pointer(&mut v, "up", last, l.body_y + 2, true);
        assert_eq!((v.session.start, v.session.span), (a.min(b), a.abs_diff(b)));
        assert_eq!((v.session.cursor, v.session.alt_cursor), (240, Some(420)));
        assert!(v.drag.is_none() && v.history.is_empty());
    }
}

#[test]
fn range_cancel_and_ctrl_shift_name_resize_leave_time_unchanged() {
    let mut v = demo();
    v.session.start = 100;
    v.session.span = 600;
    let original = v.session.clone();
    let l = v.layout();
    for cancel in ["escape", "resize", "cancel"] {
        pointer(&mut v, "down", l.wave_x + 10, l.body_y, true);
        pointer(&mut v, "drag", l.wave_x + 40, l.body_y, true);
        if cancel == "escape" {
            key(&mut v, "escape");
        } else {
            v.event(Event {
                kind: cancel.into(),
                ..Event::default()
            });
        }
        assert!(v.drag.is_none());
        assert_eq!(v.session, original);
    }
    let split = l.browser + l.name - 1;
    pointer(&mut v, "down", split, l.body_y, true);
    pointer(&mut v, "drag", split + 5, l.body_y, true);
    pointer(&mut v, "up", split + 5, l.body_y, true);
    assert_eq!(
        (v.session.start, v.session.span),
        (original.start, original.span)
    );
    assert_ne!(v.session.name_width, original.name_width);
    let l = v.layout();
    v.event(Event {
        kind: "down".into(),
        control: true,
        shift: true,
        view_column: l.wave_x + 30,
        view_row: 4,
        ..Event::default()
    });
    pointer(&mut v, "drag", l.wave_x + 20, 4, true);
    pointer(&mut v, "up", l.wave_x + 20, 4, true);
    assert_eq!(
        v.session.span, original.span,
        "Ctrl+Shift drag pans without applying range zoom on release"
    );
}

#[test]
fn overview_drag_preserves_grab_offset_clamps_and_can_be_cancelled() {
    let mut v = demo();
    v.session.start = 200;
    v.session.span = 300;
    let original = v.session.clone();
    let l = v.layout();
    let y = v.height - 3;
    let grab = l.wave_x + l.wave_width / 3;
    pointer(&mut v, "down", grab, y, false);
    assert_eq!(
        v.session, original,
        "grabbing the viewport must not recenter it"
    );
    pointer(&mut v, "drag", grab + 10, y, false);
    let expected = 200 + 1000 * u64::from(grab + 10 - l.wave_x) / u64::from(l.wave_width - 1)
        - 1000 * u64::from(grab - l.wave_x) / u64::from(l.wave_width - 1);
    assert_eq!(v.session.start, expected);
    let outside = v.height + 8;
    let right = v.width - 1;
    pointer(&mut v, "up", grab + 10, outside, false);
    assert!(v.overview_drag.is_none());
    let committed = v.session.clone();
    for cancel in ["escape", "resize", "cancel"] {
        pointer(&mut v, "down", l.wave_x, y, false);
        pointer(&mut v, "drag", right, y, false);
        assert_eq!(v.session.start, 700);
        if cancel == "escape" {
            key(&mut v, "escape");
        } else {
            v.event(Event {
                kind: cancel.into(),
                ..Event::default()
            });
        }
        assert_eq!(v.session, committed);
    }
    for (x, expected) in [(l.wave_x, 0), (v.width - 1, 700)] {
        pointer(&mut v, "down", x, y, false);
        pointer(&mut v, "up", x, y, false);
        assert_eq!((v.session.start, v.session.span), (expected, 300));
        assert_eq!((v.session.cursor, v.session.alt_cursor), (240, Some(420)));
    }
    assert!(v.history.is_empty());
    let overview = &lines(&v)[usize::from(y)];
    assert!(overview.contains('A') && overview.contains('B') && overview.contains(']'));
}

#[test]
fn ruler_sets_cursors_and_hovered_names_and_values_inspect_without_selecting() {
    let mut v = demo();
    v.session.snap = false;
    let l = v.layout();
    let cursor = v.time_at(l.wave_x + 20);
    pointer(&mut v, "down", l.wave_x + 20, 4, false);
    pointer(&mut v, "up", l.wave_x + 20, 4, false);
    assert_eq!(v.session.cursor, cursor);
    v.event(Event {
        kind: "down".into(),
        alt: true,
        view_column: l.wave_x + 30,
        view_row: 4,
        ..Event::default()
    });
    pointer(&mut v, "up", l.wave_x + 30, 4, false);
    assert_eq!(v.session.alt_cursor, Some(v.time_at(l.wave_x + 30)));

    v.session.cursor = 240;
    let watch = v
        .session
        .watches
        .iter()
        .position(|w| matches!(w, Watch::Signal { path, .. } if path.ends_with("wdata[31:0]")))
        .unwrap();
    let row = v.rows().into_iter().find(|r| r.watch == watch).unwrap();
    let y = l.body_y + row.line;
    let session = v.session.clone();
    pointer(&mut v, "move", l.browser + 5, y, false);
    assert!(
        lines(&v)
            .join("\n")
            .contains("tb.dut.wdata[31:0] · 32 bits")
    );
    assert!(lines(&v).join("\n").contains("Value @ A"));
    pointer(&mut v, "move", l.browser + l.name + 2, y, false);
    v.event(Event {
        code: "command".into(),
        text: "copy".into(),
        ..Event::default()
    });
    assert_eq!(
        v.copy_requested.as_deref(),
        Some(
            v.watch_value(watch, None, 240)
                .unwrap()
                .format(Radix::Hex)
                .as_str()
        )
    );
    assert_eq!(v.selection, 0);
    assert_eq!(v.session, session);
    assert!(v.history.is_empty());
    v.selection = watch;
    key(&mut v, "enter");
    v.selection = 0;
    let bit = v
        .rows()
        .into_iter()
        .find(|r| r.watch == watch && r.bit == Some(31))
        .unwrap();
    pointer(
        &mut v,
        "move",
        l.browser + l.name + 2,
        l.body_y + bit.line,
        false,
    );
    assert!(lines(&v).join("\n").contains("tb.dut.wdata[31]"));
    assert!(!lines(&v).join("\n").contains("wdata[31:0][31]"));
    v.event(Event {
        code: "command".into(),
        text: "copy".into(),
        ..Event::default()
    });
    assert_eq!(
        v.copy_requested.as_deref(),
        Some(
            v.watch_value(watch, Some(31), 240)
                .unwrap()
                .format(Radix::Binary)
                .as_str()
        )
    );
    v.selection = watch;
    v.selected_bit = Some(31);
    v.event(Event {
        code: "function".into(),
        text: "10".into(),
        shift: true,
        ..Event::default()
    });
    let items = v.menu_items();
    assert!(
        items
            .iter()
            .any(|item| item.label == "Binary" && item.checked)
    );
    assert!(
        items
            .iter()
            .any(|item| item.label == "Analog plot" && !item.enabled)
    );
}

#[test]
fn cursor_guides_keep_bus_text_and_ascii_spaces_intact() {
    let mut v = Viewer::new("ascii.vcd".into(), None);
    v.ready(vcd::parse(b"$timescale 1 ns $end $var wire 32 ! data $end $enddefinitions $end #0 b01000001001000000100001000100000 ! #1000").unwrap());
    v.width = 150;
    v.height = 24;
    v.session.browser = false;
    if let Watch::Signal { radix, .. } = &mut v.session.watches[0] {
        *radix = Radix::Ascii;
    }
    let l = v.layout();
    let before = lines(&v)[usize::from(l.body_y)].clone();
    let column = before
        .chars()
        .collect::<Vec<_>>()
        .windows(4)
        .position(|chars| chars == ['A', ' ', 'B', ' '])
        .unwrap() as u16
        + 1;
    v.session.cursor = v.time_at(column);
    v.session.alt_cursor = Some(900);
    let a = v.column_at(v.session.cursor).unwrap();
    let b = v.column_at(900).unwrap();
    assert!(lines(&v)[usize::from(l.body_y)].contains("A B "));
    for y in l.body_y..l.body_y + l.body_height {
        assert_eq!(background(&v, a, y), "selection");
        assert_eq!(background(&v, b, y), "match");
    }
    assert_eq!(
        v.wave.traces[0].changes[0].value,
        Value::Bits("01000001001000000100001000100000".into())
    );
}

#[test]
fn help_wraps_scrolls_to_every_control_and_never_changes_the_waveform() {
    for (width, height) in [(160, 24), (80, 24), (48, 16), (16, 8), (1, 1), (512, 200)] {
        let mut v = demo();
        v.width = width;
        v.height = height;
        let session = v.session.clone();
        v.action("help");
        key(&mut v, "pageDown");
        assert!(v.help);
        v.event(Event {
            direction: 1,
            control: true,
            shift: true,
            ..Event::default()
        });
        assert!(v.help);
        key(&mut v, "end");
        let rendered = lines(&v);
        assert!(rendered.iter().all(|l| l.width() <= usize::from(width)));
        assert_eq!(rendered.len(), usize::from(height));
        if width >= 48 {
            assert!(rendered.join("\n").contains("scrollbar thumbs"));
        }
        key(&mut v, "home");
        assert_eq!(v.help_scroll, 0);
        key(&mut v, "escape");
        assert!(!v.help);
        assert_eq!(v.session, session);
    }
}

#[test]
fn empty_filters_can_be_cleared_and_overflow_is_visible_without_extra_rows() {
    let mut v = demo();
    let l = v.layout();
    v.filter = "this-signal-does-not-exist".into();
    v.rebuild_browser();
    assert!(v.browser_nodes.is_empty());
    let session = v.session.clone();
    assert!(lines(&v).join("\n").contains("No matching signals"));
    pointer(&mut v, "down", l.browser - 3, 4, false);
    assert!(v.filter.is_empty() && !v.browser_nodes.is_empty());
    assert!(v.prompt.is_none());
    assert_eq!(v.session, session);
    for watch in &mut v.session.watches {
        watch.set_size(None, WaveSize::Normal);
    }
    assert!(
        lines(&v)
            .iter()
            .skip(usize::from(l.body_y))
            .take(usize::from(l.body_height))
            .any(|line| line.chars().nth(usize::from(l.wave_x - 1)) == Some('┃'))
    );
    v.open_prompt(Prompt::Goto);
    let session = v.session.clone();
    v.event(Event {
        direction: 1,
        shift: true,
        view_column: l.wave_x + 10,
        ..Event::default()
    });
    assert_eq!(
        v.session, session,
        "scrolling must not change time behind a prompt"
    );
}

#[test]
fn unavailable_toolbar_controls_are_dimmed_and_do_not_apply() {
    let mut v = demo();
    let out = v
        .toolbar()
        .into_iter()
        .find(|b| b.action == "zoom_out")
        .unwrap();
    assert!(!out.enabled);
    let session = v.session.clone();
    pointer(&mut v, "down", out.x + 1, 1, false);
    assert_eq!(v.session, session);
    v.session.watches.insert(
        0,
        Watch::Group {
            name: "Control".into(),
            collapsed: false,
        },
    );
    let find = v
        .toolbar()
        .into_iter()
        .find(|b| b.action == "find")
        .unwrap();
    assert!(!find.enabled);
    pointer(&mut v, "move", find.x + 1, 1, false);
    assert!(
        lines(&v)
            .join("\n")
            .contains("Select a signal to use this control")
    );
    let session = v.session.clone();
    pointer(&mut v, "down", find.x + 1, 1, false);
    assert_eq!(v.session, session);
    assert!(v.history.is_empty());
}
