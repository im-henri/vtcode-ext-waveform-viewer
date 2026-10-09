use unicode_width::UnicodeWidthStr;
use vtcode_waveform_viewer::{
    loader::{Loader, Source},
    model::{Radix, Timescale, Value, Waveform},
    vcd::{self, VcdParser},
    viewer::{Event, Focus, Marker, Prompt, Session, Viewer, Watch},
};

const DEMO: &[u8] = include_bytes!("../examples/demo.vcd");
fn viewer() -> Viewer {
    let mut viewer = Viewer::new("demo.vcd".into(), Some(1));
    viewer.ready(vcd::parse(DEMO).unwrap());
    viewer
}
fn char_key(v: &mut Viewer, key: &str) {
    v.event(Event {
        code: "char".into(),
        text: key.into(),
        ..Event::default()
    });
}
fn key(v: &mut Viewer, code: &str) {
    v.event(Event {
        code: code.into(),
        ..Event::default()
    });
}
fn select(v: &mut Viewer, name: &str) {
    v.selection = v.session.watches.iter().position(|w| matches!(w, Watch::Signal { path, .. } if path.rsplit('.').next().unwrap().starts_with(name))).unwrap();
    v.selected_bit = None;
}
fn text(v: &Viewer) -> String {
    v.render()
        .lines
        .iter()
        .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn small(body: &str) -> Vec<u8> {
    format!("$timescale 1 ns $end $scope module top $end $var wire 1 ! clk $end $var wire 8 # data [7:0] $end $upscope $end $enddefinitions $end {body}").into_bytes()
}

#[test]
fn parses_identically_at_every_chunk_boundary() {
    let expected = vcd::parse(DEMO).unwrap();
    for chunk_size in [1, 2, 3, 7, 16, 257, 4096] {
        let mut wave = Waveform::default();
        let mut parser = VcdParser::default();
        for chunk in DEMO.chunks(chunk_size) {
            parser.feed(chunk, &mut wave).unwrap();
        }
        parser.finish(&mut wave).unwrap();
        assert_eq!(wave.change_count, expected.change_count);
        assert_eq!(wave.end, 1000);
        for (actual, expected) in wave.traces.iter().zip(&expected.traces) {
            assert_eq!(actual.changes.len(), expected.changes.len());
            for (actual, expected) in actual.changes.iter().zip(&expected.changes) {
                assert_eq!(actual.time, expected.time);
                assert_eq!(actual.value, expected.value);
            }
        }
    }
}

#[test]
fn reloading_does_not_duplicate_default_watches_and_can_restore_the_old_view() {
    let mut v = viewer();
    let original = v.session.watches.clone();
    v.ready(vcd::parse(DEMO).unwrap());
    assert_eq!(v.session.watches, original);
    v.session.cursor = 240;
    v.pending_session = Some(v.session.clone());
    v.ready(vcd::parse(DEMO).unwrap());
    assert_eq!(v.session.cursor, 240);
    assert_eq!(v.session.watches, original);
    v.loading = Some((0, 100));
    v.event(Event {
        code: "function".into(),
        text: "5".into(),
        ..Event::default()
    });
    assert!(v.reload_requested);
}
#[test]
fn aliases_share_data_and_ascending_ranges_have_correct_bit_names() {
    let wave = vcd::parse(b"$scope module t $end $var wire 4 ! bus [0:3] $end $var wire 4 ! alias [0:3] $end $upscope $end $enddefinitions $end #0 b1010 ! #10 b0101 !").unwrap();
    assert_eq!(wave.traces.len(), 1);
    assert_eq!(wave.signals[0].trace, wave.signals[1].trace);
    assert_eq!(wave.signals[0].bit_name(0), 3);
    assert_eq!(wave.signals[0].bit_name(3), 0);
    assert_eq!(
        wave.traces[0].value_at(0).unwrap().bit(3),
        Value::Bits("1".into())
    );
}
#[test]
fn separately_dumped_bits_and_slices_keep_distinct_paths() {
    let wave = vcd::parse(b"$var wire 1 ! x [0] $end $var wire 1 # x [1] $end $var wire 8 $ b [15:8] $end $var wire 8 % b [7:0] $end $enddefinitions $end #0 0! 1# b0 $ b1 %").unwrap();
    let paths = wave
        .signals
        .iter()
        .map(|s| s.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["x[0]", "x[1]", "b[15:8]", "b[7:0]"]);
}
#[test]
fn abbreviated_four_state_vectors_extend_correctly() {
    let wave = vcd::parse(&small("#0 b1 # #10 bx # #20 bz # #30 b10xz #")).unwrap();
    let values: Vec<_> = wave.traces[1]
        .changes
        .iter()
        .map(|c| c.value.format(Radix::Binary))
        .collect();
    assert_eq!(values, ["00000001", "xxxxxxxx", "zzzzzzzz", "000010xz"]);
}
#[test]
fn whitespace_dump_directives_comments_and_bom_are_accepted() {
    let mut data = vec![0xef, 0xbb, 0xbf];
    data.extend_from_slice(&small(
        "$dumpvars\n0! b0 # $end $comment ignored #99 b111 $end #10\t1!\r\nb1 #",
    ));
    let wave = vcd::parse(&data).unwrap();
    assert_eq!(wave.end, 10);
    assert_eq!(wave.traces[0].changes.len(), 2);
}
#[test]
fn final_change_at_same_timestamp_wins_without_a_zero_length_glitch() {
    let wave = vcd::parse(&small("#0 0! #10 1! 0! 1! #20 1!")).unwrap();
    assert_eq!(wave.traces[0].changes.len(), 2);
    assert_eq!(wave.traces[0].value_at(10).unwrap().logic(), '1');
}
#[test]
fn malformed_vcd_reports_errors_instead_of_showing_plausible_data() {
    for data in [
        small("#bad 1!"),
        small("#10 1! #9 0!"),
        small("#0 b102 #"),
        small("#0 1unknown"),
        small("#0 b1"),
        small("$comment unterminated"),
        b"not a waveform".to_vec(),
        b"$scope module t $end $enddefinitions $end".to_vec(),
    ] {
        assert!(
            vcd::parse(&data).is_err(),
            "{}",
            String::from_utf8_lossy(&data)
        );
    }
}
#[test]
fn invalid_width_alias_and_hierarchy_are_rejected() {
    for data in [
        "$var wire 0 ! bad $end $enddefinitions $end",
        "$var wire 20000 ! bad $end $enddefinitions $end",
        "$var wire 1 ! a $end $var wire 8 ! b $end $enddefinitions $end",
        "$upscope $end $enddefinitions $end",
    ] {
        assert!(vcd::parse(data.as_bytes()).is_err());
    }
}
#[test]
fn radices_cover_wide_signed_ascii_and_mixed_unknowns() {
    let value = Value::bits("11111111", 8).unwrap();
    assert_eq!(value.format(Radix::Signed), "-1");
    assert_eq!(value.format(Radix::Unsigned), "255");
    assert_eq!(value.format(Radix::Hex), "ff");
    assert_eq!(value.format(Radix::Octal), "377");
    assert_eq!(
        Value::bits(&"1".repeat(128), 128)
            .unwrap()
            .format(Radix::Signed),
        "-1"
    );
    assert_eq!(
        Value::bits("0100000101000010", 16)
            .unwrap()
            .format(Radix::Ascii),
        "AB"
    );
    assert_eq!(Value::bits("10xz", 4).unwrap().format(Radix::Hex), "x");
    assert_eq!(Value::bits("z", 8).unwrap().format(Radix::Unsigned), "Z");
    assert_eq!(Value::Real(0.0).format(Radix::Hex), "0");
}

#[test]
fn signed_values_at_every_width_and_small_real_numbers_keep_their_meaning() {
    for width in 1..=128 {
        let negative_one = Value::bits(&"1".repeat(width), width).unwrap();
        assert_eq!(negative_one.format(Radix::Signed), "-1", "{width} bits");
        assert_eq!(negative_one.number(true), Some(-1.0));
    }
    assert_eq!(Value::Real(1.25e-12).format(Radix::Hex), "1.25000e-12");
    assert_eq!(Value::Real(1.25e12).format(Radix::Hex), "1.25000e12");
    assert_eq!(Value::Real(0.0).format(Radix::Hex), "0");
}

#[test]
fn hierarchy_and_identifier_metadata_are_bounded_and_long_comments_are_accepted() {
    let mut wave = Waveform::default();
    let name = "s".repeat(4096);
    let error = (0..40_000)
        .find_map(|_| wave.add_scope(0, &name).err())
        .unwrap();
    assert!(error.contains("48 MiB"));
    let code = "i".repeat(1025);
    assert!(
        vcd::parse(format!("$var wire 1 {code} x $end $enddefinitions $end").as_bytes())
            .unwrap_err()
            .contains("identifier")
    );
    let input = format!(
        "$comment {} $end {}",
        "a ".repeat(5000),
        String::from_utf8(small("#0 0! b0 #")).unwrap()
    );
    assert_eq!(vcd::parse(input.as_bytes()).unwrap().signals.len(), 2);
}
#[test]
fn time_labels_and_goto_do_not_lose_large_integer_precision() {
    let scale = Timescale::parse("10 ps").unwrap();
    assert_eq!(scale.parse_time("1.25 ns").unwrap(), 125);
    assert_eq!(scale.format(125), "1.25 ns");
    assert!(scale.parse_time("1.001 ps").is_err());
    assert!(scale.parse_time("-2 ns").is_err());
    let ticks = 9_007_199_254_740_993u64;
    let scale = Timescale::default();
    let formatted = scale.format(ticks);
    assert_eq!(scale.parse_time(&formatted).unwrap(), ticks);
    assert_eq!(
        scale.parse_time("18_446_744_073_709_551_615").unwrap(),
        u64::MAX
    );
}
#[test]
fn integer_zoom_pan_and_mapping_stay_in_bounds() {
    let mut v = viewer();
    v.width = 150;
    for _ in 0..20 {
        v.zoom(true, 240);
    }
    assert_eq!(v.session.span, 1);
    v.session.start = 0;
    for _ in 0..20 {
        v.zoom(false, 0);
    }
    assert_eq!(v.session.span, 1000);
    for _ in 0..20 {
        v.pan(true);
    }
    assert_eq!(v.session.start, 0);
    v.session.span = 100;
    v.session.start = 900;
    v.pan(true);
    assert_eq!(v.session.start, 900);
    let l = v.layout();
    assert_eq!(v.time_at(l.wave_x), 900);
    assert_eq!(v.time_at(l.wave_x + l.wave_width - 1), 1000);
}
#[test]
fn next_previous_and_polarity_edges_and_alt_cursor_are_correct() {
    let mut v = viewer();
    select(&mut v, "clk");
    v.edge(false, false, None);
    assert_eq!(v.session.cursor, 10);
    v.edge(false, false, Some('1'));
    assert_eq!(v.session.cursor, 30);
    v.edge(true, false, None);
    assert_eq!(v.session.cursor, 20);
    v.edge(false, true, Some('0'));
    assert_eq!(v.session.alt_cursor, Some(40));
    assert_eq!(v.session.cursor, 20);
}
#[test]
fn bus_expansion_selection_and_bit_edges_follow_the_selected_bit() {
    let mut v = viewer();
    select(&mut v, "state");
    char_key(&mut v, "e");
    assert_eq!(
        v.rows().iter().filter(|r| r.watch == v.selection).count(),
        4
    );
    key(&mut v, "down");
    assert_eq!(v.selected_bit, Some(2));
    v.edge(false, false, None);
    assert_eq!(v.session.cursor, 80);
    key(&mut v, "delete");
    assert_eq!(v.selected_bit, None);
    assert_eq!(
        v.rows().iter().filter(|r| r.watch == v.selection).count(),
        1
    );
}
#[test]
fn grouping_collapse_reorder_delete_undo_and_redo_preserve_rows() {
    let mut v = viewer();
    v.selection = 1;
    v.open_prompt(Prompt::Group);
    v.input = "Control".into();
    key(&mut v, "enter");
    assert!(matches!(&v.session.watches[1], Watch::Group { name, .. } if name == "Control"));
    char_key(&mut v, " ");
    assert_eq!(v.rows().len(), 2);
    v.undo(false);
    assert!(v.rows().len() > 2);
    v.undo(false);
    assert_eq!(v.session.watches.len(), 12);
    v.undo(true);
    assert_eq!(v.session.watches.len(), 13);
}
#[test]
fn browser_search_adds_signal_and_can_work_in_a_narrow_terminal() {
    let mut v = viewer();
    v.width = 80;
    char_key(&mut v, "/");
    v.event(Event {
        code: "paste".into(),
        text: "shift_reg".into(),
        ..Event::default()
    });
    key(&mut v, "enter");
    assert_eq!(v.focus, Focus::Browser);
    assert!(v.layout().browser > 0);
    assert_eq!(v.browser_nodes.len(), 1);
    key(&mut v, "enter");
    assert_eq!(v.session.watches.len(), 13);
    key(&mut v, "tab");
    assert_eq!(v.focus, Focus::Waves);
    assert_eq!(v.layout().browser, 0);
}
#[test]
fn value_search_wraps_and_respects_radix_and_unknown_values() {
    let mut v = viewer();
    select(&mut v, "rdata");
    v.session.cursor = 100;
    v.search_query = "z".into();
    v.start_search(false);
    v.search_step();
    assert_eq!(v.session.cursor, 420);
    v.search_query = "x".into();
    v.start_search(false);
    v.search_step();
    assert_eq!(v.session.cursor, 0);
    select(&mut v, "state");
    v.search_query = "0b100".into();
    v.start_search(false);
    v.search_step();
    assert_eq!(v.session.cursor, 80);
    v.search_query = "not-a-value".into();
    v.start_search(false);
    v.search_step();
    assert_eq!(v.status, "Value not found");
}
#[test]
fn search_is_bounded_and_cancelable() {
    let mut v = viewer();
    select(&mut v, "clk");
    let trace = v.selected().unwrap().1;
    v.wave.traces[trace].changes = (0..5000)
        .map(|time| vtcode_waveform_viewer::model::Change {
            time,
            value: Value::Bits((time % 2).to_string()),
        })
        .collect();
    v.search_query = "2".into();
    v.start_search(false);
    v.search_step();
    assert_eq!(v.search.as_ref().unwrap().visited, 1024);
    key(&mut v, "escape");
    assert!(v.search.is_none());
    assert_eq!(v.status, "Search cancelled");
}
#[test]
fn shift_wheel_pans_like_horizontal_wheel_without_scrolling_signals_or_moving_cursors() {
    let mut v = viewer();
    v.height = 16;
    v.session.span = 400;
    v.session.cursor = 240;
    v.session.alt_cursor = Some(420);
    v.scroll = 1;
    v.browser_scroll = 1;
    let l = v.layout();
    for x in [1, l.browser + 1, l.wave_x + l.wave_width / 2] {
        for (axis, shift) in [
            ("vertical", true),
            ("horizontal", false),
            ("horizontal", true),
        ] {
            for (direction, expected) in [(-1, 220), (1, 380)] {
                v.session.start = 300;
                v.event(Event {
                    direction,
                    axis: axis.into(),
                    shift,
                    view_column: x,
                    view_row: l.body_y,
                    ..Event::default()
                });
                assert_eq!((v.session.start, v.session.span), (expected, 400));
                assert_eq!((v.scroll, v.browser_scroll), (1, 1));
                assert_eq!((v.session.cursor, v.session.alt_cursor), (240, Some(420)));
            }
        }
    }
    for (start, direction) in [(0, -1), (600, 1)] {
        v.session.start = start;
        v.event(Event {
            direction,
            shift: true,
            view_column: l.wave_x,
            ..Event::default()
        });
        assert_eq!(
            v.session.start, start,
            "panning stops at the file boundaries"
        );
    }

    let window = (v.session.start, v.session.span);
    for x in [1, l.browser + 1] {
        v.event(Event {
            direction: -1,
            view_column: x,
            ..Event::default()
        });
        assert_eq!((v.session.start, v.session.span), window);
    }
    assert_eq!((v.scroll, v.browser_scroll), (0, 0));
    v.event(Event {
        direction: -1,
        shift: true,
        control: true,
        view_column: l.wave_x,
        ..Event::default()
    });
    assert_eq!(
        v.session.span, 200,
        "Ctrl+wheel still zooms when Shift is held"
    );
}

#[test]
fn cursor_mouse_zoom_modifier_scroll_and_range_zoom_are_consistent() {
    let mut v = viewer();
    let l = v.layout();
    let x = l.wave_x + l.wave_width / 2;
    v.event(Event {
        kind: "down".into(),
        view_column: x,
        view_row: 5,
        alt: true,
        ..Event::default()
    });
    assert!(v.session.alt_cursor.unwrap().abs_diff(500) < 30);
    v.event(Event {
        direction: -1,
        control: true,
        view_column: x,
        ..Event::default()
    });
    assert_eq!(v.session.span, 500);
    v.event(Event {
        direction: 1,
        axis: "horizontal".into(),
        view_column: x,
        ..Event::default()
    });
    assert!(v.session.start > 250);
    v.fit();
    v.event(Event {
        kind: "down".into(),
        view_column: l.wave_x + 5,
        view_row: 5,
        shift: true,
        ..Event::default()
    });
    v.event(Event {
        kind: "up".into(),
        view_column: l.wave_x + 20,
        view_row: 5,
        ..Event::default()
    });
    assert!(v.session.span < 1000);
}
#[test]
fn sessions_round_trip_and_missing_signals_and_invalid_input_are_handled() {
    let mut v = viewer();
    v.session.cursor = 200;
    v.session.alt_cursor = Some(480);
    v.session.markers.push(Marker {
        time: 250,
        name: "Handshake".into(),
    });
    v.session
        .watches
        .push(Watch::signal("missing.path".into(), 3));
    let json = serde_json::to_string(&v.session).unwrap();
    let mut other = viewer();
    other.restore(serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(other.session.cursor, 200);
    assert_eq!(other.session.markers.len(), 1);
    assert_eq!(other.session.watches.len(), 12);
    assert!(
        other
            .restore(Session {
                schema: 999,
                ..Session::default()
            })
            .is_err()
    );
    other.open_prompt(Prompt::Import);
    other.input = "bad json".into();
    key(&mut other, "enter");
    assert!(other.error);
    assert_eq!(other.prompt, Some(Prompt::Import));
}
#[test]
fn frames_fit_wide_narrow_tiny_and_unicode_terminal_cells() {
    let mut v = viewer();
    v.session.markers.push(Marker {
        time: 100,
        name: "握手\u{1b}[bad]".into(),
    });
    for (width, height) in [
        (150, 38),
        (100, 28),
        (80, 24),
        (48, 16),
        (16, 5),
        (1, 1),
        (512, 200),
    ] {
        v.width = width;
        v.height = height;
        let frame = v.render();
        assert_eq!((frame.width, frame.height), (width, height));
        assert!(frame.lines.len() <= height as usize);
        for line in &frame.lines {
            let text: String = line.iter().map(|s| s.text.as_str()).collect();
            assert!(
                text.width() <= width as usize,
                "{width} columns: {}",
                text.width()
            );
            assert!(!text.chars().any(char::is_control));
        }
        assert!(frame.lines.iter().map(Vec::len).sum::<usize>() < 32768);
        assert!(serde_json::to_vec(&frame).unwrap().len() < 2 * 1024 * 1024);
    }
}
#[test]
fn loading_yields_and_detects_truncation() {
    struct Bytes(Vec<u8>);
    impl Source for Bytes {
        fn read(&mut self, offset: u64, count: usize) -> Result<Vec<u8>, String> {
            Ok(self
                .0
                .get(offset as usize..(offset as usize + count).min(self.0.len()))
                .unwrap_or(&[])
                .to_vec())
        }
    }
    let mut source = Bytes(DEMO.to_vec());
    let mut loader = Loader::new(source.0.len() as u64, false).unwrap();
    assert!(loader.step(&mut source).unwrap().is_none());
    let wave = loop {
        if let Some(wave) = loader.step(&mut source).unwrap() {
            break wave;
        }
    };
    assert_eq!(wave.signals.len(), 15);
    let mut truncated = Loader::new(DEMO.len() as u64 + 100, false).unwrap();
    loop {
        match truncated.step(&mut source) {
            Ok(None) => {}
            Err(e) => {
                assert!(e.contains("changed"));
                break;
            }
            _ => panic!("Accepted a truncated source"),
        }
    }
}

#[test]
fn remaining_shared_memory_is_enforced_during_both_vcd_and_fst_decode() {
    struct Bytes(Vec<u8>);
    impl Source for Bytes {
        fn read(&mut self, offset: u64, count: usize) -> Result<Vec<u8>, String> {
            Ok(self.0[offset as usize..(offset as usize + count).min(self.0.len())].to_vec())
        }
    }
    for (bytes, fst) in [
        (DEMO.to_vec(), false),
        (include_bytes!("fixtures/gcd.fst").to_vec(), true),
    ] {
        let mut loader = Loader::new(bytes.len() as u64, fst).unwrap();
        loader.wave.data_limit = 256;
        let mut source = Bytes(bytes);
        let error = loop {
            match loader.step(&mut source) {
                Err(error) => break error,
                Ok(Some(_)) => panic!("A dump exceeded the remaining shared memory allowance"),
                Ok(None) => {}
            }
        };
        assert!(error.contains("shared memory budget"), "{error}");
    }
}
#[test]
fn binary_fst_decodes_with_real_upstream_fixture() {
    let wave =
        vtcode_waveform_viewer::fst::parse(include_bytes!("fixtures/gcd.fst").to_vec()).unwrap();
    assert_eq!(wave.format, "FST");
    assert!(!wave.signals.is_empty());
    assert!(wave.change_count > 0);
    assert_eq!(wave.end, 4);
}
#[test]
fn compressed_fst_wrapper_checks_crc_and_unwrapped_size() {
    let original = include_bytes!("fixtures/gcd.fst");
    let mut gzip = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];
    gzip.extend(miniz_oxide::deflate::compress_to_vec(original, 6));
    gzip.extend(crc32fast::hash(original).to_le_bytes());
    gzip.extend((original.len() as u32).to_le_bytes());
    let mut wrapped = vec![254];
    wrapped.extend((gzip.len() as u64 + 16).to_be_bytes());
    wrapped.extend((original.len() as u64).to_be_bytes());
    wrapped.extend(gzip);
    assert_eq!(
        vtcode_waveform_viewer::fst::parse(wrapped.clone())
            .unwrap()
            .end,
        4
    );
    let end = wrapped.len();
    wrapped[end - 8] ^= 1;
    assert!(
        vtcode_waveform_viewer::fst::parse(wrapped)
            .unwrap_err()
            .contains("checksum")
    );
}
#[test]
fn fst_metadata_limits_and_truncation_are_normal_errors() {
    let original = include_bytes!("fixtures/gcd.fst");
    for length in [0, 1, 8, 17, 128, 329, 350, 500, 707] {
        assert!(vtcode_waveform_viewer::fst::parse(original[..length].to_vec()).is_err());
    }
    let mut bad = original.to_vec();
    bad[1..9].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(vtcode_waveform_viewer::fst::parse(bad).is_err());
    let mut bad = original.to_vec();
    bad[57..65].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(
        vtcode_waveform_viewer::fst::parse(bad)
            .unwrap_err()
            .contains("limit")
    );
}
#[test]
fn searching_an_unchanging_bus_bit_yields_and_can_be_cancelled() {
    let mut v = viewer();
    select(&mut v, "state");
    let trace = v.selected().unwrap().1;
    v.selected_bit = Some(2);
    v.wave.traces[trace].changes = (0..10_000)
        .map(|time| vtcode_waveform_viewer::model::Change {
            time,
            value: Value::Bits(format!("00{}", time % 2)),
        })
        .collect();
    v.edge(false, false, None);
    assert!(v.edge_search.is_some());
    key(&mut v, "escape");
    assert!(v.edge_search.is_none());
}
#[test]
fn loading_and_search_escape_do_not_destroy_the_last_rendered_view() {
    let mut v = viewer();
    v.loading = Some((10, 100));
    let count = v.session.watches.len();
    key(&mut v, "escape");
    assert!(v.cancelled);
    assert!(v.loading.is_none());
    assert_eq!(v.session.watches.len(), count);
    assert!(!text(&v).is_empty());
}
