use vtcode_waveform_viewer::{
    model::{Change, Trace, Value},
    sampling::Activity,
    vcd,
    viewer::{Event, Viewer, Watch, WaveSize},
};

fn viewer(declaration: &str, body: &str) -> Viewer {
    let source = format!("$timescale 1 ns $end {declaration} $enddefinitions $end {body}");
    let mut v = Viewer::new("resolution.vcd".into(), None);
    v.ready(vcd::parse(source.as_bytes()).unwrap());
    v.width = 100;
    v.height = 24;
    v.session.browser = false;
    v.session.name_width = 12;
    v.session.snap = false;
    v
}
fn lines(v: &Viewer) -> Vec<String> {
    v.render()
        .lines
        .iter()
        .map(|line| line.iter().map(|s| s.text.as_str()).collect())
        .collect()
}
fn timeline(v: &Viewer, offset: u16) -> String {
    let l = v.layout();
    lines(v)[usize::from(l.body_y + offset)]
        .chars()
        .skip(usize::from(l.wave_x))
        .collect()
}
fn hover(v: &mut Viewer, x: u16, y: u16) {
    v.event(Event {
        kind: "move".into(),
        view_column: x,
        view_row: y,
        ..Event::default()
    });
}
fn values(v: &Viewer) -> Vec<String> {
    let l = v.layout();
    lines(v)
        .into_iter()
        .skip(usize::from(l.body_y))
        .take(usize::from(l.body_height))
        .map(|line| {
            line.chars()
                .skip(usize::from(l.browser + l.name))
                .take(usize::from(l.value))
                .collect()
        })
        .collect()
}

#[test]
fn off_grid_pulses_are_shaded_once_in_their_own_cell_in_every_display() {
    for declaration in [
        "$var wire 1 ! signal $end",
        "$var wire 8 ! signal $end",
        "$var real 64 ! signal $end",
    ] {
        let body = if declaration.contains("real") {
            "#0 r0 ! #11 r100 ! #12 r0 ! #1000"
        } else if declaration.contains("8") {
            "#0 b0 ! #11 b11111111 ! #12 b0 ! #1000"
        } else {
            "#0 0! #11 1! #12 0! #1000"
        };
        let mut v = viewer(declaration, body);
        let l = v.layout();
        v.session.span = u64::from(l.wave_width - 1) * 10;
        let column = v.column_at(11).unwrap() - l.wave_x;
        for size in [WaveSize::Small, WaveSize::Normal] {
            v.session.watches[0].set_size(None, size);
            let row = v.rows()[0];
            for offset in 0..row.height {
                let wave = timeline(&v, offset);
                assert_eq!(
                    wave.chars().nth(usize::from(column)),
                    Some('▒'),
                    "{declaration}: {wave}"
                );
                assert_eq!(
                    wave.matches('▒').count(),
                    1,
                    "each pulse belongs to one cell"
                );
            }
        }
        let frame = lines(&v).join("\n");
        assert!(frame.contains("Overview"));
        assert!(frame.contains("multiple values"));
    }
}

#[test]
fn projected_bits_keep_hidden_pulses_and_ignore_unrelated_bus_changes() {
    let mut v = viewer(
        "$var wire 8 ! data $end",
        "#0 b0 ! #11 b1 ! #12 b10 ! #13 b11 ! #14 b0 ! #1000",
    );
    v.height = 32;
    let l = v.layout();
    v.session.span = u64::from(l.wave_width - 1) * 10;
    if let Watch::Signal { expanded, .. } = &mut v.session.watches[0] {
        *expanded = true;
    }
    for row in v.rows().into_iter().filter(|row| row.bit.is_some()) {
        let wave = timeline(&v, row.line);
        assert_eq!(
            wave.contains('▒'),
            row.bit.unwrap() < 2,
            "bit {:?}: {wave}",
            row.bit
        );
    }
}

#[test]
fn extreme_bit_density_is_explicitly_unresolved_instead_of_a_false_level() {
    let mut body = String::from("#0 b0 !");
    for time in 1..200 {
        body.push_str(&format!(" #{time} b{:b} !", time % 2));
    }
    body.push_str(" #10000");
    let mut v = viewer("$var wire 8 ! data $end", &body);
    if let Watch::Signal { bit, .. } = &mut v.session.watches[0] {
        *bit = Some(7);
    }
    v.session.span = 10000;
    assert!(timeline(&v, 0).contains('░'));
    assert!(lines(&v).join("\n").contains("unresolved"));
    v.session.span = 100;
    assert!(!timeline(&v, 0).contains('░'));
    assert!(!timeline(&v, 0).contains('▒'));
}

#[test]
fn cursor_column_and_rendered_transition_share_the_same_time_bucket() {
    let mut v = viewer("$var wire 1 ! signal $end", "#0 0! #11 1! #1000");
    let l = v.layout();
    v.session.span = u64::from(l.wave_width - 1) * 10;
    v.session.cursor = 11;
    let x = v.column_at(11).unwrap();
    assert_eq!(x, l.wave_x + 2);
    assert_eq!(v.cell_range(x), (10, 20));
    assert_eq!(timeline(&v, 0).chars().nth(2), Some('┌'));
    assert!(values(&v)[0].contains('1'));
    hover(&mut v, x, l.body_y);
    assert!(lines(&v).join("\n").contains("Pointer @ 20 ns"));
    assert_eq!(v.session.cursor, 11);
    for time in 0..=v.session.span {
        let x = v.column_at(time).unwrap();
        let (start, end) = v.cell_range(x);
        assert!(time <= end && (time > start || time == v.session.start));
    }
}

#[test]
fn value_column_always_reports_a_while_hover_and_copy_inspect_the_pointer() {
    let mut v = viewer("$var wire 8 ! data $end", "#0 b1 ! #100 b10101011 ! #1000");
    v.session.cursor = 0;
    let before = values(&v);
    let l = v.layout();
    hover(&mut v, l.wave_x + 20, l.body_y);
    assert_eq!(values(&v), before);
    let frame = lines(&v).join("\n");
    assert!(frame.contains("Value @ A"));
    assert!(frame.contains("Pointer @"));
    assert!(frame.contains("HEX  ab"));
    v.event(Event {
        code: "command".into(),
        text: "copy".into(),
        ..Event::default()
    });
    assert_eq!(v.copy_requested.as_deref(), Some("ab"));
    hover(&mut v, l.wave_x - 3, l.body_y);
    v.event(Event {
        code: "command".into(),
        text: "copy".into(),
        ..Event::default()
    });
    assert_eq!(v.copy_requested.as_deref(), Some("01"));
}

#[test]
fn zoom_out_stops_at_detail_limit_then_continues_and_fit_always_works() {
    let mut v = viewer(
        "$var wire 1 ! clk $end",
        "#0 0! #10 1! #20 0! #30 1! #40 0! #10000",
    );
    v.session.start = 0;
    v.session.span = 250;
    let limit = v.detail_limit().unwrap();
    assert!(limit > 250 && limit < 500);
    v.zoom(false, 0);
    assert_eq!(v.session.span, limit);
    assert!(v.status.contains("Terminal detail limit"));
    v.zoom(false, 0);
    assert_eq!(v.session.span, limit * 2);
    assert!(!v.status.contains("Terminal detail limit"));
    v.fit();
    assert_eq!(v.session.span, 10000);
    assert!(timeline(&v, 0).contains('▒'));
    v.session.cursor = 20;
    let history = v.history.len();
    v.action("detail");
    assert_eq!(v.session.cursor, 20);
    assert_eq!(v.history.len(), history);
    assert!(!timeline(&v, 0).contains('▒'));
    assert!(v.column_at(20).is_some());
    let width_limit = v.detail_limit().unwrap();
    v.width = 150;
    assert!(v.detail_limit().unwrap() > width_limit);
}

#[test]
fn final_cell_pulses_and_exact_boundary_edges_are_not_lost_or_double_counted() {
    let mut v = viewer("$var wire 1 ! clk $end", "#0 0! #10 1! #20 0! #1000");
    let l = v.layout();
    v.session.span = u64::from(l.wave_width - 1) * 10;
    assert!(!timeline(&v, 0).contains('▒'));
    let end = v.session.span;
    v.wave.push(0, end - 2, Value::Bits("1".into())).unwrap();
    v.wave.push(0, end, Value::Bits("0".into())).unwrap();
    assert_eq!(timeline(&v, 0).chars().last(), Some('▒'));
}

#[test]
fn no_recorded_value_is_distinct_from_simulator_x() {
    let v = viewer("$var wire 1 ! signal $end", "#20 x! #1000");
    assert!(timeline(&v, 0).starts_with('·'));
    assert!(timeline(&v, 0).contains('x'));
    assert!(values(&v)[0].contains('—'));
}

#[test]
fn snapped_cursor_does_not_choose_an_arbitrary_hidden_edge() {
    let mut v = viewer("$var wire 1 ! signal $end", "#0 0! #11 1! #12 0! #1000");
    let l = v.layout();
    v.session.span = u64::from(l.wave_width - 1) * 10;
    v.session.snap = true;
    v.set_cursor(l.wave_x + 2, false);
    assert_eq!(v.session.cursor, 20);
    v.set_cursor(l.wave_x + 2, true);
    assert_eq!(v.session.alt_cursor, Some(20));
    v.session.start = 10;
    v.session.span = 20;
    v.set_cursor(v.column_at(11).unwrap(), false);
    assert_eq!(v.session.cursor, 11);
}

#[test]
fn large_integer_timestamps_keep_exact_cursor_values_and_cell_ownership() {
    let time = (1u64 << 60) + 11;
    let mut v = viewer(
        "$var wire 1 ! signal $end",
        &format!("#0 0! #{time} 1! #{}", time + 1000),
    );
    v.session.start = time - 11;
    v.session.span = u64::from(v.layout().wave_width - 1) * 10;
    v.session.cursor = time;
    let x = v.column_at(time).unwrap();
    assert_eq!(v.cell_range(x), (time - 1, time + 9));
    assert!(values(&v)[0].contains('1'));
}

#[test]
fn activity_counts_projected_changes_instead_of_only_comparing_endpoints() {
    let trace = Trace {
        changes: ["00", "01", "00", "10"]
            .into_iter()
            .enumerate()
            .map(|(time, bits)| Change {
                time: time as u64,
                value: Value::Bits(bits.into()),
            })
            .collect(),
    };
    assert_eq!(trace.activity(1, 4, Some(0)), Activity::Mixed);
    assert_eq!(trace.activity(1, 4, Some(1)), Activity::Edge);
    assert_eq!(trace.activity(1, 3, Some(1)), Activity::Stable);
    assert_eq!(trace.held_interval(1, Some(1), 10), Some((0, 3)));
    assert_eq!(trace.held_interval(2, None, 10), Some((2, 3)));
}

#[test]
fn hover_drill_down_reveals_the_hidden_pulse_without_changing_cursors() {
    let mut v = viewer("$var wire 1 ! signal $end", "#0 0! #11 1! #12 0! #1000000");
    v.session.cursor = 800000;
    v.session.alt_cursor = Some(900000);
    let l = v.layout();
    let x = v.column_at(11).unwrap();
    let range = v.cell_range(x);
    assert!(range.1 > 1000);
    hover(&mut v, x, l.body_y);
    v.event(Event {
        code: "char".into(),
        text: "i".into(),
        ..Event::default()
    });
    assert_eq!(v.session.start, range.0);
    assert_eq!(v.session.span, range.1 - range.0);
    assert!(
        v.hover.is_some(),
        "stationary pointer inspection survives zoom"
    );
    let second_range = v.cell_range(x);
    v.event(Event {
        code: "char".into(),
        text: "i".into(),
        ..Event::default()
    });
    assert_eq!(v.session.span, second_range.1 - second_range.0);
    assert!(v.column_at(11).is_some());
    assert!(v.column_at(12).is_some());
    assert_eq!(v.session.cursor, 800000);
    assert_eq!(v.session.alt_cursor, Some(900000));
}

#[test]
fn same_timestamp_updates_cannot_leave_a_phantom_transition() {
    let v = viewer("$var wire 1 ! signal $end", "#0 0! #10 1! 0! #1000");
    assert_eq!(v.wave.traces[0].changes.len(), 1);
    assert_eq!(v.wave.change_count, 1);
    assert!(!timeline(&v, 0).contains('▒'));
    assert!(!timeline(&v, 0).contains('┌'));
}

#[test]
fn partially_visible_bus_rows_still_show_compression() {
    let mut v = viewer("$var wire 8 ! data $end", "#0 b0 ! #11 b1 ! #12 b0 ! #1000");
    v.session.watches[0].set_size(None, WaveSize::Normal);
    v.session.span = u64::from(v.layout().wave_width - 1) * 10;
    v.scroll = 2;
    assert!(timeline(&v, 0).contains('▒'));
    v.scroll = 0;
    v.height = 10;
    assert!(timeline(&v, 0).contains('▒'));
}

#[test]
fn edges_show_both_sides_and_state_which_value_applies_at_the_timestamp() {
    let mut v = viewer("$var wire 1 ! signal $end", "#0 0! #11 1! #31 0! #1000");
    let l = v.layout();
    v.session.span = u64::from(l.wave_width - 1) * 10;
    v.session.cursor = 11;
    assert!(values(&v)[0].contains("0 → 1"));
    let edge = v.column_at(11).unwrap();
    hover(&mut v, edge, l.body_y);
    let frame = lines(&v).join("\n");
    assert!(frame.contains("Edge @ 11 ns"), "{frame}");
    assert!(frame.contains("Before HEX  0"), "{frame}");
    assert!(frame.contains("After  HEX  1"), "{frame}");
    assert!(frame.contains("Pointer @ 20 ns"), "{frame}");
    assert!(frame.contains("At edge = After"), "{frame}");
    v.event(Event {
        code: "command".into(),
        text: "copy".into(),
        ..Event::default()
    });
    assert_eq!(v.copy_requested.as_deref(), Some("1"));
    assert_eq!(v.status, "After value copied");
    hover(&mut v, edge - 1, l.body_y);
    assert!(!lines(&v).join("\n").contains("Before HEX"));
    v.session.cursor = 31;
    v.hover = None;
    assert!(values(&v)[0].contains("1 → 0"));
    assert!(lines(&v).last().unwrap().contains("Before 1 → After 0"));
}

#[test]
fn clipped_bus_edge_values_keep_both_sides_and_hover_reveals_the_full_pair() {
    let mut v = viewer(
        "$var wire 32 ! data $end",
        "#0 b11011110101011011011111011101111 ! #11 b01100000000100001000000000000000 ! #1000",
    );
    v.session.cursor = 11;
    v.session.value_width = 13;
    let l = v.layout();
    let value_lines = values(&v);
    let pair = value_lines
        .iter()
        .find(|value| value.contains('→'))
        .unwrap();
    assert!(
        pair.contains('→') && pair.matches('…').count() == 2,
        "{pair}"
    );
    hover(&mut v, l.wave_x - 3, l.body_y);
    let frame = lines(&v).join("\n");
    assert!(frame.contains("Before HEX  dead_beef"), "{frame}");
    assert!(frame.contains("After  HEX  6010_8000"), "{frame}");
    assert!(frame.contains("Cursor A @ 11 ns"), "{frame}");
    v.event(Event {
        kind: "contextMenu".into(),
        view_column: l.wave_x - 3,
        view_row: 3,
        ..Event::default()
    });
    let index = v
        .menu_items()
        .iter()
        .position(|item| item.action == vtcode_waveform_viewer::menus::MenuAction::FitValues)
        .unwrap();
    v.context_menu.as_mut().unwrap().selected = index;
    v.event(Event {
        code: "enter".into(),
        ..Event::default()
    });
    assert!(
        values(&v)
            .iter()
            .any(|line| line.contains("dead_beef → 6010_8000"))
    );
}

#[test]
fn before_after_preserves_x_z_and_does_not_invent_edges_on_unchanged_bits() {
    let mut v = viewer("$var wire 1 ! signal $end", "#0 x! #11 z! #21 1! #1000");
    v.session.cursor = 11;
    assert!(values(&v)[0].contains("x → z"));
    let l = v.layout();
    hover(&mut v, l.wave_x - 3, l.body_y);
    let frame = lines(&v).join("\n");
    assert!(frame.contains("Before HEX  x"));
    assert!(frame.contains("After  HEX  z"));
    let wave =
        vcd::parse(b"$var wire 8 ! data $end $enddefinitions $end #0 b0 ! #11 b1 ! #1000").unwrap();
    assert!(wave.traces[0].transition_index(11, Some(7)).is_none());
    assert!(wave.traces[0].transition_index(11, Some(0)).is_some());
    assert!(wave.traces[0].transition_index(0, None).is_none());
}

#[test]
fn a_single_terminal_column_summarizes_the_whole_window() {
    let mut v = viewer("$var wire 1 ! signal $end", "#0 0! #11 1! #1000");
    let width = (1..100)
        .find(|width| {
            v.width = *width;
            v.layout().wave_width == 1
        })
        .unwrap();
    v.width = width;
    assert_eq!(v.cell_range(v.layout().wave_x), (0, 1000));
    assert_eq!(timeline(&v, 0), "▒");
}
