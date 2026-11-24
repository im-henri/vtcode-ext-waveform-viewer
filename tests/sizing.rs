use vtcode_waveform_viewer::{
    menus::MenuAction,
    vcd,
    viewer::{Event, Focus, Session, Viewer, Watch, WaveSize},
};

fn demo() -> Viewer {
    let mut v = Viewer::new("demo.vcd".into(), None);
    v.ready(vcd::parse(include_bytes!("../examples/demo.vcd")).unwrap());
    v.session.browser = true;
    v.width = 150;
    v.height = 32;
    // As after a real resize: whole cells for this width.
    v.clamp_window();
    v
}
fn lines(v: &Viewer) -> Vec<String> {
    v.render()
        .lines
        .iter()
        .map(|l| l.iter().map(|s| s.text.as_str()).collect())
        .collect()
}
fn pointer(v: &mut Viewer, kind: &str, x: u16, y: u16) {
    v.event(Event {
        kind: kind.into(),
        view_column: x,
        view_row: y,
        ..Event::default()
    });
}
fn key(v: &mut Viewer, text: &str) {
    v.event(Event {
        code: "char".into(),
        text: text.into(),
        ..Event::default()
    });
}
fn choose(v: &mut Viewer, action: MenuAction) {
    let index = v
        .menu_items()
        .iter()
        .position(|item| item.action == action)
        .unwrap();
    v.context_menu.as_mut().unwrap().selected = index;
    v.event(Event {
        code: "enter".into(),
        ..Event::default()
    });
}

#[test]
fn new_signals_default_to_square_edges_and_readable_bus_envelopes() {
    let mut v = demo();
    v.height = 44;
    let rows = v.rows();
    assert_eq!(rows.len(), 12);
    assert_eq!(rows.iter().map(|r| r.height).sum::<u16>(), 30);
    assert!(
        rows.iter()
            .all(|r| r.line + r.height <= v.layout().body_height)
    );
    assert_eq!(rows[0].height, 2);
    assert_eq!(rows[4].height, 3);
    assert_eq!(rows[11].height, 3);
    for row in &rows {
        if let Watch::Signal { path, .. } = &v.session.watches[row.watch]
            && v.wave.signals[v.signal_index[path]].width == 1
        {
            assert_eq!(v.session.watches[row.watch].size(None), WaveSize::Normal);
            assert_eq!(row.height, 2);
        }
    }
    let frame = lines(&v);
    assert!(frame.join("\n").contains("voltage"));
    assert!(!v.toolbar().iter().any(|button| button.action == "density"));
    let clock: String = frame
        .iter()
        .skip(5)
        .take(2)
        .map(|line| {
            line.chars()
                .skip(usize::from(v.layout().wave_x))
                .collect::<String>()
        })
        .collect();
    assert!(clock.contains('┌') && clock.contains('┘'));
    assert!(!clock.contains('╱') && !clock.contains('╲'));
}

#[test]
fn keyboard_size_changes_only_the_selected_wave_and_round_trips_with_undo() {
    let mut v = demo();
    v.selection = 8;
    v.session.cursor = 240;
    let before = v.session.clone();
    key(&mut v, "h");
    assert_eq!(v.selection, 8);
    assert_eq!(v.session.cursor, 240);
    for (index, watch) in v.session.watches.iter().enumerate() {
        if index == 8 {
            assert_eq!(watch.size(None), WaveSize::Small);
        } else {
            assert_eq!(watch, &before.watches[index]);
        }
    }
    let json = serde_json::to_string(&v.session).unwrap();
    assert!(!json.contains("density"));
    let restored: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, v.session);
    let changed = v.session.clone();
    v.undo(false);
    assert_eq!(v.session, before);
    v.undo(true);
    assert_eq!(v.session, changed);
    v.session.watches.push(Watch::Divider);
    v.selection = v.session.watches.len() - 1;
    let history = v.history.len();
    let session = v.session.clone();
    key(&mut v, "h");
    assert_eq!(v.session, session);
    assert_eq!(v.history.len(), history);
}

#[test]
fn context_menu_sizes_the_clicked_signal_and_reapplying_a_size_is_a_noop() {
    let mut v = demo();
    let before = v.session.clone();
    let l = v.layout();
    pointer(&mut v, "contextMenu", l.browser + 8, l.body_y + 2);
    let sizes: Vec<_> = v
        .menu_items()
        .into_iter()
        .filter(|item| matches!(item.action, MenuAction::Size(_)))
        .collect();
    assert_eq!(sizes.len(), 2);
    assert!(sizes[1].checked && !sizes[0].checked);
    choose(&mut v, MenuAction::Size(WaveSize::Small));
    assert_eq!(v.selection, 1);
    assert_eq!(v.rows()[1].height, 1);
    assert_eq!(v.session.watches[0], before.watches[0]);
    assert_eq!(&v.session.watches[2..], &before.watches[2..]);
    assert_eq!(v.history.len(), 1);
    pointer(&mut v, "contextMenu", l.browser + 8, l.body_y + 2);
    choose(&mut v, MenuAction::Size(WaveSize::Small));
    assert_eq!(v.history.len(), 1);
    v.undo(false);
    assert_eq!(v.session, before);
}

#[test]
fn expanded_bits_have_independent_sizes_that_survive_parent_changes_and_restore() {
    let mut v = demo();
    let watch = 4;
    if let Watch::Signal { expanded, .. } = &mut v.session.watches[watch] {
        *expanded = true;
    }
    v.selection = watch;
    v.selected_bit = Some(31);
    let before: Vec<_> = v
        .rows()
        .iter()
        .map(|row| (row.watch, row.bit, row.height))
        .collect();
    assert!(
        before
            .iter()
            .filter(|row| row.1.is_some())
            .all(|row| row.2 == 2)
    );
    key(&mut v, "h");
    for (index, row) in v.rows().iter().enumerate() {
        assert_eq!(
            row.height,
            if row.watch == watch && row.bit == Some(31) {
                1
            } else {
                before[index].2
            }
        );
    }
    v.selected_bit = None;
    key(&mut v, "h");
    assert_eq!(v.session.watches[watch].size(None), WaveSize::Small);
    assert_eq!(v.session.watches[watch].size(Some(31)), WaveSize::Small);
    assert_eq!(v.session.watches[watch].size(Some(30)), WaveSize::Normal);
    let json = serde_json::to_string(&v.session).unwrap();
    let mut restored = demo();
    restored
        .restore(serde_json::from_str(&json).unwrap())
        .unwrap();
    assert_eq!(restored.rows(), v.rows());
    assert_eq!(restored.session.watches, v.session.watches);
}

#[test]
fn older_global_sizes_use_normal_for_all_single_bits_and_preserve_bus_sizes() {
    for (density, size) in [
        (Some("Compact"), WaveSize::Small),
        (Some("Roomy"), WaveSize::Normal),
        (None, WaveSize::Small),
    ] {
        let mut v = demo();
        if let Watch::Signal { expanded, .. } = &mut v.session.watches[4] {
            *expanded = true;
        }
        let mut detached_bit = v.session.watches[4].clone();
        if let Watch::Signal { bit, expanded, .. } = &mut detached_bit {
            *bit = Some(7);
            *expanded = false;
        }
        v.session.watches.push(detached_bit);
        let mut old = serde_json::to_value(&v.session).unwrap();
        old["schema"] = 1.into();
        for watch in old["watches"].as_array_mut().unwrap() {
            watch.as_object_mut().unwrap().remove("size");
            watch.as_object_mut().unwrap().remove("bit_size");
            watch.as_object_mut().unwrap().remove("bit_sizes");
        }
        if let Some(density) = density {
            old["density"] = density.into();
        }
        v.restore(serde_json::from_value(old).unwrap()).unwrap();
        assert_eq!(v.session.schema, 2);
        for watch in &v.session.watches {
            if let Watch::Signal { path, bit, .. } = watch {
                let scalar = bit.is_some() || v.wave.signals[v.signal_index[path]].width == 1;
                assert_eq!(
                    watch.size(None),
                    if scalar { WaveSize::Normal } else { size }
                );
            }
        }
        for row in v.rows() {
            if row.bit.is_some() || row.watch == 12 {
                assert_eq!(row.height, 2);
            }
        }
        assert_eq!(v.session.watches[4].size(Some(31)), WaveSize::Normal);
        assert!(v.session.legacy_density.is_none());
        assert!(
            !serde_json::to_string(&v.session)
                .unwrap()
                .contains("density")
        );
        let saved = serde_json::to_value(&v.session).unwrap();
        v.session.watches[0].set_size(None, WaveSize::Small);
        v.session.watches[4].set_size(Some(31), WaveSize::Small);
        let explicit = v.session.clone();
        v.restore(serde_json::from_value(serde_json::to_value(&explicit).unwrap()).unwrap())
            .unwrap();
        assert_eq!(
            v.session, explicit,
            "explicit Small choices survive reopening"
        );
        v.restore(serde_json::from_value(saved.clone()).unwrap())
            .unwrap();
        assert_eq!(serde_json::to_value(&v.session).unwrap(), saved);
    }
}

#[test]
fn browser_size_choice_adds_one_configured_signal_and_undo_removes_it() {
    let mut v = demo();
    v.session.watches.clear();
    v.filter = "wdata".into();
    v.rebuild_browser();
    let l = v.layout();
    pointer(&mut v, "contextMenu", 6, l.body_y);
    choose(&mut v, MenuAction::Size(WaveSize::Normal));
    assert_eq!(v.session.watches.len(), 1);
    assert_eq!(v.rows()[0].height, 3);
    assert_eq!(v.history.len(), 1);
    assert!(
        matches!(&v.session.watches[0], Watch::Signal { path, size: WaveSize::Normal, .. } if path.ends_with("wdata[31:0]"))
    );
    v.undo(false);
    assert!(v.session.watches.is_empty());
}

#[test]
fn small_bus_keeps_real_boundaries_atomic_values_and_passive_hover() {
    let mut v = Viewer::new("bus.vcd".into(), None);
    v.ready(vcd::parse(b"$timescale 1 ns $end $var wire 32 ! data [31:0] $end $enddefinitions $end #0 b01100000000100001000000000000000 ! #100 b01100000000000001110111101000000 ! #220 b11011110101011011011111011101111 ! #1000").unwrap());
    v.width = 120;
    v.height = 24;
    v.session.browser = false;
    v.session.name_width = 12;
    v.session.watches[0].set_size(None, WaveSize::Small);
    v.session.cursor = 240;
    let l = v.layout();
    v.session.span = u64::from(l.wave_width - 1) * 10;
    let row: String = lines(&v)[5].chars().skip(l.wave_x as usize).collect();
    assert!(row.contains('┼'), "{row}");
    assert!(!row.contains(['<', '>', '╱', '╲']), "{row}");
    assert!(row.contains("dead_beef"), "{row}");
    assert!(!row.contains("6010_8000"), "cramped labels are omitted");
    assert!(row.contains("6000_ef40"));
    assert_eq!(v.rows()[0].height, 1);
    let before = v.session.clone();
    pointer(&mut v, "move", l.wave_x + 12, l.body_y);
    assert!(lines(&v).join("\n").contains("HEX  6000_ef40"));
    assert_eq!(v.session, before);
}

#[test]
fn drag_previews_respect_mixed_heights_and_keep_the_dropped_wave_size() {
    let mut v = demo();
    v.session.watches.truncate(3);
    v.session.watches[1].set_size(None, WaveSize::Small);
    v.filter = "wdata".into();
    v.rebuild_browser();
    v.focus = Focus::Browser;
    let original = v.session.clone();
    let l = v.layout();
    pointer(&mut v, "down", 6, l.body_y);
    pointer(&mut v, "drag", l.wave_x + 8, l.body_y + 2);
    assert_eq!(v.session, original);
    assert_eq!(v.display_rows().iter().position(|r| r.preview), Some(1));
    assert_eq!(
        v.display_rows()
            .iter()
            .map(|r| r.row.height)
            .collect::<Vec<_>>(),
        [2, 3, 1, 2]
    );
    pointer(&mut v, "up", l.wave_x + 8, l.body_y + 2);
    assert_eq!(v.session.watches.len(), 4);
    assert!(
        matches!(&v.session.watches[1], Watch::Signal { path, size: WaveSize::Normal, .. } if path.ends_with("wdata[31:0]"))
    );
    v.undo(false);
    assert_eq!(v.session, original);
}
