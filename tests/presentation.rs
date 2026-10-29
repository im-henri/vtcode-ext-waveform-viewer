use unicode_width::UnicodeWidthStr;
use vtcode_waveform_viewer::{
    model::{Radix, Value},
    render::display_value,
    vcd,
    viewer::{Event, Focus, Marker, Viewer, Watch, WaveSize},
};

fn bus(body: &str) -> Viewer {
    let source = format!(
        "$timescale 1 ns $end $scope module top $end $var wire 32 ! data [31:0] $end $upscope $end $enddefinitions $end {body}"
    );
    let mut v = Viewer::new("transfers.vcd".into(), None);
    v.ready(vcd::parse(source.as_bytes()).unwrap());
    v.width = 100;
    v.height = 24;
    v.session.browser = false;
    for watch in &mut v.session.watches {
        watch.set_size(None, WaveSize::Normal);
    }
    v.session.name_width = 12;
    v.session.cursor = 0;
    v
}
fn lines(v: &Viewer) -> Vec<String> {
    v.render()
        .lines
        .iter()
        .map(|line| line.iter().map(|span| span.text.as_str()).collect())
        .collect()
}
fn timeline(v: &Viewer, offset: usize) -> String {
    let l = v.layout();
    lines(v)[l.body_y as usize + offset]
        .chars()
        .skip(l.wave_x as usize)
        .collect()
}
fn point(v: &mut Viewer, x: u16, y: u16) {
    v.event(Event {
        kind: "move".into(),
        view_column: x,
        view_row: y,
        ..Event::default()
    });
}

#[test]
fn bus_rails_enclose_centered_atomic_labels_and_real_transfers() {
    let mut v = bus(
        "#0 b01100000000100001000000000000000 ! #100 b01100000000000001110111101000000 ! #220 b11011110101011011011111011101111 ! #1000",
    );
    // One sample every 10 ns: the first transfer has 10 columns (too narrow
    // for nine hex characters plus padding); the second has exactly 12.
    v.session.span = u64::from(v.layout().wave_width - 1) * 10;
    assert_eq!(v.rows()[0].height, 3);
    let upper = timeline(&v, 0);
    let middle = timeline(&v, 1);
    let lower = timeline(&v, 2);
    assert!(!upper.contains('╳') && !lower.contains('╳'));
    assert!(
        !middle.contains("6010_8000"),
        "a cramped value must be completely omitted"
    );
    assert!(
        middle.contains("6000_ef40"),
        "an exact fit must remain readable"
    );
    assert!(middle.contains("dead_beef"));
    assert_eq!(upper.chars().nth(10), Some('┬'));
    assert_eq!(lower.chars().nth(10), Some('┴'));
    assert_eq!(middle.chars().nth(10), Some('│'));
    assert_eq!(
        middle.chars().skip(11).take(9).collect::<String>(),
        "6000_ef40"
    );
    assert!(upper.chars().all(|ch| matches!(ch, '─' | '┬')));
    assert!(lower.chars().all(|ch| matches!(ch, '─' | '┴')));
}

#[test]
fn hover_reveals_omitted_values_without_moving_measurements_and_can_copy_them() {
    let mut v = bus(
        "#0 b01100000000100001000000000000000 ! #100 b01100000000000001110111101000000 ! #1000",
    );
    v.session.cursor = 240;
    v.session.alt_cursor = Some(420);
    v.session.span = 1000;
    let session = v.session.clone();
    let history = v.history.len();
    let l = v.layout();
    assert!(!timeline(&v, 1).contains("6010_8000"));
    point(&mut v, l.wave_x + 2, l.body_y + 1);
    let inspected = lines(&v).join("\n");
    assert!(inspected.contains("HEX  6010_8000"));
    assert!(inspected.contains("Value @ A"));
    assert!(inspected.contains("Pointer @"));
    assert_eq!(v.session, session);
    assert_eq!(v.history.len(), history);
    v.event(Event {
        code: "command".into(),
        text: "copy".into(),
        ..Event::default()
    });
    assert_eq!(v.copy_requested.as_deref(), Some("60108000"));
    assert_eq!(v.session, session);
    v.event(Event {
        kind: "leave".into(),
        ..Event::default()
    });
    assert!(v.hover.is_none());
    assert!(lines(&v).join("\n").contains("Value @ A"));
}

#[test]
fn inspection_and_markers_never_replace_spaces_inside_ascii_values() {
    let mut v = bus("#0 b01000001001000000100001000100000 ! #1000");
    if let Watch::Signal { radix, .. } = &mut v.session.watches[0] {
        *radix = Radix::Ascii;
    }
    let original = timeline(&v, 1);
    let at = original.find("A B ").unwrap() as u16 + 1;
    let l = v.layout();
    v.session.cursor = v.time_at(l.wave_x + at);
    v.session.alt_cursor = Some(v.session.cursor);
    v.session.markers.push(Marker {
        time: v.session.cursor,
        name: "sample".into(),
    });
    point(&mut v, l.wave_x + at, l.body_y + 1);
    assert!(timeline(&v, 1).contains("A B "));
}

#[test]
fn four_state_buses_keep_their_actual_values_and_state_colors() {
    let mut v = bus("#0 bx ! #300 bz ! #600 b1010 ! #1000");
    let frame = v.render();
    assert!(timeline(&v, 1).contains("xxxx_xxxx"));
    assert!(timeline(&v, 1).contains("zzzz_zzzz"));
    assert!(timeline(&v, 1).contains('a'));
    assert!(frame.lines[5].iter().any(|span| span.role == "error"));
    assert!(frame.lines[5].iter().any(|span| span.role == "warning"));
    v.session.start = 650;
    v.session.span = 350;
    assert!(
        timeline(&v, 1).contains('a'),
        "clipped stable intervals still get a complete value"
    );
}

#[test]
fn toolbar_hits_match_adaptive_labels_and_narrow_tree_opens_on_the_first_click() {
    let mut v = bus("#0 b0 ! #1000");
    for width in [1, 16, 48, 80, 100, 150, 512] {
        v.width = width;
        let buttons = v.toolbar();
        for pair in buttons.windows(2) {
            assert!(pair[0].x + pair[0].width < pair[1].x);
        }
        for button in buttons {
            assert_eq!(button.label.width(), button.width as usize);
            assert!(button.x + button.width <= width);
        }
    }
    v.width = 80;
    v.session.browser = true;
    v.focus = Focus::Waves;
    assert_eq!(v.layout().browser, 0);
    let tree = v
        .toolbar()
        .into_iter()
        .find(|b| b.action == "add_signals")
        .unwrap();
    v.event(Event {
        kind: "down".into(),
        view_column: tree.x + 1,
        view_row: 1,
        ..Event::default()
    });
    assert!(v.layout().browser > 0);
    v.event(Event {
        code: "tab".into(),
        ..Event::default()
    });
    assert_eq!(v.layout().browser, 0);
    v.action("browser");
    assert!(v.layout().browser > 0);
    v.action("browser");
    assert_eq!(v.layout().browser, 0);
    v.width = 150;
    assert!(
        !v.toolbar()
            .iter()
            .any(|b| matches!(b.action, "radix" | "analog"))
    );
    v.event(Event {
        kind: "contextMenu".into(),
        view_column: v.layout().wave_x + 1,
        view_row: v.layout().body_y,
        ..Event::default()
    });
    let item = v
        .menu_items()
        .iter()
        .position(|item| item.label == "Unsigned decimal")
        .unwrap();
    let menu = v.menu_layout();
    v.event(Event {
        kind: "down".into(),
        view_column: menu.items.x + 4,
        view_row: menu.items.y + (item - menu.scroll) as u16,
        ..Event::default()
    });
    assert!(matches!(
        v.session.watches[0],
        Watch::Signal {
            radix: Radix::Unsigned,
            ..
        }
    ));
}

#[test]
fn hover_tooltips_fit_small_views_and_wide_values_are_wrapped_and_copyable() {
    let mut v = bus("#0 b0 ! #1000");
    let wide = Value::Bits("1".repeat(1024));
    v.wave.traces[0].changes[0].value = wide.clone();
    for (width, height) in [
        (80, 24),
        (100, 24),
        (150, 38),
        (48, 16),
        (16, 5),
        (1, 1),
        (512, 200),
    ] {
        v.width = width;
        v.height = height;
        let l = v.layout();
        point(&mut v, l.wave_x, l.body_y);
        for line in lines(&v) {
            assert!(line.width() <= width as usize);
        }
    }
    v.width = 150;
    v.height = 38;
    let l = v.layout();
    point(&mut v, l.wave_x + 1, l.body_y);
    assert!(
        lines(&v)
            .join("\n")
            .contains("Ctrl+C copies the full value")
    );
    v.event(Event {
        code: "char".into(),
        text: "c".into(),
        control: true,
        ..Event::default()
    });
    assert_eq!(
        v.copy_requested.as_deref(),
        Some(wide.format(Radix::Hex).as_str())
    );
    assert_eq!(
        display_value(&Value::Bits("1".repeat(36)), Radix::Hex),
        "f_ffff_ffff"
    );
}

#[test]
fn passive_hover_does_not_dismiss_errors_and_keyboard_navigation_clears_it() {
    let mut v = bus("#0 b0 ! #1000");
    v.fail("Example load error");
    let l = v.layout();
    point(&mut v, l.wave_x + 1, l.body_y);
    assert!(v.error);
    assert!(lines(&v).join("\n").contains("Example load error"));
    assert!(v.hover.is_some());
    v.event(Event {
        code: "right".into(),
        ..Event::default()
    });
    assert!(v.hover.is_none());
}

#[test]
fn unresolved_transfers_are_visible_and_do_not_invent_a_stable_value_label() {
    use std::fmt::Write;
    let mut body = String::from("#0 b0 !");
    for time in 1..20_000 {
        write!(body, " #{time} b{} !", time % 2).unwrap();
    }
    body.push_str(" #20000");
    let v = bus(&body);
    let upper = timeline(&v, 0);
    let middle = timeline(&v, 1);
    assert!(upper.matches('▒').count() > 30);
    assert!(middle.matches('▒').count() > 30);
    assert!(!middle.contains('0') && !middle.contains('1'));
    assert!(!upper.contains('╳') && !middle.contains('╳'));
    assert_eq!(display_value(&Value::Bits("0".repeat(32)), Radix::Hex), "0");
    assert_eq!(
        display_value(
            &Value::Bits("00000000000000000000000010101011".into()),
            Radix::Hex
        ),
        "ab"
    );
}

#[test]
fn dense_scalar_pulses_show_activity_instead_of_a_false_constant_level() {
    use std::fmt::Write;
    let mut source =
        String::from("$timescale 1 ns $end $var wire 1 ! clk $end $enddefinitions $end #0 0!");
    for time in 1..1000 {
        write!(source, " #{time} {}!", time % 2).unwrap();
    }
    let mut v = Viewer::new("dense.vcd".into(), None);
    v.ready(vcd::parse(source.as_bytes()).unwrap());
    v.width = 100;
    v.height = 24;
    v.session.browser = false;
    for size in [WaveSize::Small, WaveSize::Normal] {
        v.session.watches[0].set_size(None, size);
        for line in 0..v.rows()[0].height as usize {
            assert!(timeline(&v, line).matches('▒').count() > 30);
            assert!(!timeline(&v, line).contains('╳'));
        }
    }
}
