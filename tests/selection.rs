use vtcode_waveform_viewer::{
    menus::MenuAction,
    model::Radix,
    selection::RowKey,
    vcd,
    viewer::{Event, Focus, Viewer, Watch, WaveSize},
};

fn demo() -> Viewer {
    let mut v = Viewer::new("demo.vcd".into(), None);
    v.ready(vcd::parse(include_bytes!("../examples/demo.vcd")).unwrap());
    v.width = 180;
    v.height = 100;
    v
}
fn row(watch: usize, bit: Option<u32>) -> RowKey {
    RowKey { watch, bit }
}
fn click(v: &mut Viewer, key: RowKey, control: bool, shift: bool) {
    let wave = v
        .rows()
        .into_iter()
        .find(|r| RowKey::from(*r) == key)
        .unwrap();
    let x = v.layout().browser + 7;
    let y = v.layout().body_y + wave.line - v.scroll as u16;
    for kind in ["down", "up"] {
        v.event(Event {
            kind: kind.into(),
            view_column: x,
            view_row: y,
            control,
            shift,
            ..Event::default()
        });
    }
}
fn key(v: &mut Viewer, code: &str, text: &str, control: bool, shift: bool) {
    v.event(Event {
        code: code.into(),
        text: text.into(),
        control,
        shift,
        ..Event::default()
    });
}
fn menu(v: &mut Viewer, key: RowKey) {
    let wave = v
        .rows()
        .into_iter()
        .find(|r| RowKey::from(*r) == key)
        .unwrap();
    v.event(Event {
        kind: "contextMenu".into(),
        view_column: v.layout().browser + 7,
        view_row: v.layout().body_y + wave.line - v.scroll as u16,
        ..Event::default()
    });
}
fn choose(v: &mut Viewer, action: MenuAction) {
    let item = v
        .menu_items()
        .into_iter()
        .enumerate()
        .find(|(_, i)| i.action == action)
        .unwrap();
    assert!(item.1.enabled, "{:?} is disabled", action);
    v.context_menu.as_mut().unwrap().selected = item.0;
    key(v, "enter", "", false, false);
}
fn selected(v: &Viewer) -> Vec<usize> {
    v.selected_rows().iter().map(|r| r.watch).collect()
}
fn color(watch: &Watch) -> u8 {
    let Watch::Signal { color, .. } = watch else {
        panic!("Not a signal")
    };
    *color
}
fn format(watch: &Watch) -> Radix {
    let Watch::Signal { radix, .. } = watch else {
        panic!("Not a signal")
    };
    *radix
}

#[test]
fn control_click_toggles_individual_rows_without_editing_the_session_or_dragging() {
    let mut v = demo();
    let before = v.session.clone();
    click(&mut v, row(0, None), false, false);
    click(&mut v, row(3, None), true, false);
    click(&mut v, row(7, None), true, false);
    assert_eq!(selected(&v), [0, 3, 7]);
    click(&mut v, row(3, None), true, false);
    assert_eq!(selected(&v), [0, 7]);
    assert_eq!(v.selection, 0);
    assert_eq!(v.session, before);
    assert!(v.history.is_empty() && v.signal_drag.is_none() && v.drag.is_none());
    click(&mut v, row(7, None), true, false);
    click(&mut v, row(0, None), true, false);
    assert_eq!(v.selection_count(), 0);
    key(&mut v, "delete", "", false, false);
    assert_eq!(v.session, before);
    assert!(v.history.is_empty());
    key(&mut v, "escape", "", false, false);
    assert_eq!(selected(&v), [0]);
}

#[test]
fn shift_ranges_shrink_reverse_and_control_shift_adds_a_disjoint_range() {
    let mut v = demo();
    click(&mut v, row(3, None), false, false);
    click(&mut v, row(7, None), false, true);
    assert_eq!(selected(&v), [3, 4, 5, 6, 7]);
    click(&mut v, row(5, None), false, true);
    assert_eq!(selected(&v), [3, 4, 5]);
    click(&mut v, row(1, None), false, true);
    assert_eq!(selected(&v), [1, 2, 3]);
    click(&mut v, row(8, None), true, false);
    click(&mut v, row(10, None), true, true);
    assert_eq!(selected(&v), [1, 2, 3, 8, 9, 10]);
    assert_eq!(v.selection_anchor, Some(row(8, None)));
    click(&mut v, row(5, None), false, false);
    assert_eq!(selected(&v), [5]);
}

#[test]
fn keyboard_ranges_select_all_command_and_escape_preserve_time_and_history() {
    let mut v = demo();
    click(&mut v, row(2, None), false, false);
    let before = v.session.clone();
    key(&mut v, "down", "", false, true);
    key(&mut v, "down", "", false, true);
    assert_eq!(selected(&v), [2, 3, 4]);
    key(&mut v, "up", "", false, true);
    assert_eq!(selected(&v), [2, 3]);
    click(&mut v, row(8, None), true, false);
    key(&mut v, "down", "", true, true);
    assert_eq!(selected(&v), [2, 3, 8, 9]);
    key(&mut v, "char", "a", true, false);
    assert_eq!(v.selection_count(), 12);
    let header: String = v.render().lines[3]
        .iter()
        .map(|s| s.text.as_str())
        .collect();
    assert!(header.contains("12 selected"));
    key(&mut v, "escape", "", false, false);
    assert_eq!(selected(&v), [9]);
    key(&mut v, "command", "select_all", false, false);
    assert_eq!(v.selection_count(), 12);
    assert_eq!(v.session, before);
    assert!(v.history.is_empty());
}

#[test]
fn right_click_preserves_the_selection_and_plain_right_click_elsewhere_replaces_it() {
    let mut v = demo();
    click(&mut v, row(0, None), false, false);
    click(&mut v, row(3, None), false, true);
    menu(&mut v, row(1, None));
    assert_eq!(selected(&v), [0, 1, 2, 3]);
    assert_eq!(v.selection, 1);
    let text: String = v
        .render()
        .lines
        .iter()
        .flat_map(|l| l.iter().map(|s| s.text.as_str()))
        .collect();
    assert!(text.contains("4 waves selected"));
    for action in [
        MenuAction::Find,
        MenuAction::Copy,
        MenuAction::Rename,
        MenuAction::Expand,
    ] {
        assert!(
            !v.menu_items()
                .iter()
                .find(|i| i.action == action)
                .unwrap()
                .enabled
        );
    }
    assert_eq!(
        v.menu_items()
            .iter()
            .find(|i| i.action == MenuAction::Remove)
            .unwrap()
            .label,
        "Remove selection"
    );
    key(&mut v, "escape", "", false, false);
    assert_eq!(v.selection_count(), 4);
    menu(&mut v, row(7, None));
    assert_eq!(selected(&v), [7]);
}

#[test]
fn bulk_radix_color_and_analog_changes_are_atomic_and_show_mixed_settings() {
    let mut v = demo();
    if let Watch::Signal { radix, .. } = &mut v.session.watches[4] {
        *radix = Radix::Binary;
    }
    let before = v.session.clone();
    click(&mut v, row(4, None), false, false);
    click(&mut v, row(5, None), true, false);
    menu(&mut v, row(4, None));
    for action in [
        MenuAction::Radix(Radix::Hex),
        MenuAction::Radix(Radix::Binary),
    ] {
        let item = v
            .menu_items()
            .into_iter()
            .find(|i| i.action == action)
            .unwrap();
        assert!(item.mixed && !item.checked && item.enabled);
    }
    choose(&mut v, MenuAction::Radix(Radix::Unsigned));
    assert_eq!(v.history.len(), 1);
    assert_eq!(format(&v.session.watches[4]), Radix::Unsigned);
    assert_eq!(format(&v.session.watches[5]), Radix::Unsigned);
    assert_eq!(v.session.watches[3], before.watches[3]);
    menu(&mut v, row(5, None));
    assert!(
        v.menu_items()
            .into_iter()
            .find(|i| i.action == MenuAction::Radix(Radix::Unsigned))
            .unwrap()
            .checked
    );
    choose(&mut v, MenuAction::Colors);
    assert!(v.menu_items().iter().filter(|i| i.mixed).count() >= 2);
    choose(&mut v, MenuAction::Color(7));
    assert_eq!(color(&v.session.watches[4]), 7);
    assert_eq!(color(&v.session.watches[5]), 7);
    assert_eq!(v.history.len(), 2);
    menu(&mut v, row(4, None));
    choose(&mut v, MenuAction::Analog);
    assert!(matches!(
        v.session.watches[4],
        Watch::Signal { analog: true, .. }
    ));
    assert!(matches!(
        v.session.watches[5],
        Watch::Signal { analog: true, .. }
    ));
    let changed = v.session.clone();
    v.undo(false);
    assert!(matches!(
        v.session.watches[4],
        Watch::Signal { analog: false, .. }
    ));
    v.undo(true);
    assert_eq!(v.session, changed);
}

#[test]
fn no_op_bulk_edits_leave_undo_and_redo_intact() {
    let mut v = demo();
    click(&mut v, row(4, None), false, false);
    click(&mut v, row(5, None), true, false);
    key(&mut v, "char", "8", false, false);
    let changed = v.session.clone();
    key(&mut v, "char", "8", false, false);
    assert_eq!(v.history.len(), 1);
    v.undo(false);
    click(&mut v, row(4, None), false, false);
    click(&mut v, row(5, None), true, false);
    menu(&mut v, row(4, None));
    choose(&mut v, MenuAction::Radix(Radix::Hex));
    assert_eq!(v.redo.len(), 1);
    assert!(v.history.is_empty());
    v.undo(true);
    assert_eq!(v.session, changed);
}

#[test]
fn expanded_bits_take_part_in_ranges_and_keep_independent_sizes() {
    let mut v = demo();
    if let Watch::Signal { expanded, .. } = &mut v.session.watches[4] {
        *expanded = true;
    }
    click(&mut v, row(4, Some(31)), false, false);
    click(&mut v, row(4, Some(28)), false, true);
    assert_eq!(
        v.selected_rows(),
        [
            row(4, Some(31)),
            row(4, Some(30)),
            row(4, Some(29)),
            row(4, Some(28))
        ]
    );
    menu(&mut v, row(4, Some(29)));
    assert!(
        !v.menu_items()
            .iter()
            .find(|i| i.action == MenuAction::Radix(Radix::Hex))
            .unwrap()
            .enabled
    );
    choose(&mut v, MenuAction::Size(WaveSize::Small));
    for bit in 28..=31 {
        assert_eq!(v.session.watches[4].size(Some(bit)), WaveSize::Small);
    }
    assert_eq!(v.session.watches[4].size(Some(27)), WaveSize::Normal);
    assert_eq!(v.session.watches[4].size(None), WaveSize::Small);
    assert_eq!(v.history.len(), 1);
    key(&mut v, "char", "7", false, false);
    assert_eq!(color(&v.session.watches[4]), 6);
    assert_eq!(v.history.len(), 2);
    assert_eq!(v.selection_count(), 4);
    v.undo(false);
    v.undo(false);
    assert_eq!(v.session.watches[4].size(Some(31)), WaveSize::Normal);
}

#[test]
fn mixed_root_and_bit_selection_only_changes_compatible_number_formats() {
    let mut v = demo();
    if let Watch::Signal { expanded, .. } = &mut v.session.watches[4] {
        *expanded = true;
    }
    click(&mut v, row(4, Some(31)), false, false);
    click(&mut v, row(5, None), true, false);
    click(&mut v, row(11, None), true, false);
    let real = v.session.watches[11].clone();
    menu(&mut v, row(4, Some(31)));
    choose(&mut v, MenuAction::Radix(Radix::Unsigned));
    assert_eq!(format(&v.session.watches[5]), Radix::Unsigned);
    assert_eq!(format(&v.session.watches[4]), Radix::Hex);
    assert_eq!(v.session.watches[11], real);
    assert_eq!(v.selection_count(), 3);
    assert_eq!(v.history.len(), 1);
}

#[test]
fn delete_removes_noncontiguous_watches_once_and_undo_restores_the_entire_batch() {
    let mut v = demo();
    let before = v.session.clone();
    click(&mut v, row(1, None), false, false);
    click(&mut v, row(4, None), true, false);
    click(&mut v, row(9, None), true, false);
    menu(&mut v, row(4, None));
    choose(&mut v, MenuAction::Remove);
    let expected: Vec<_> = before
        .watches
        .iter()
        .enumerate()
        .filter(|(i, _)| ![1, 4, 9].contains(i))
        .map(|(_, w)| w.clone())
        .collect();
    assert_eq!(v.session.watches, expected);
    assert_eq!(v.history.len(), 1);
    let changed = v.session.clone();
    v.undo(false);
    assert_eq!(v.session, before);
    v.undo(true);
    assert_eq!(v.session, changed);
}

#[test]
fn deleting_expanded_bits_folds_each_bus_once_and_parent_removal_wins() {
    let mut v = demo();
    if let Watch::Signal { expanded, .. } = &mut v.session.watches[4] {
        *expanded = true;
    }
    let before = v.session.clone();
    click(&mut v, row(4, Some(31)), false, false);
    click(&mut v, row(4, Some(29)), false, true);
    key(&mut v, "delete", "", false, false);
    assert_eq!(v.session.watches.len(), 12);
    assert!(matches!(
        v.session.watches[4],
        Watch::Signal {
            expanded: false,
            ..
        }
    ));
    assert_eq!(v.history.len(), 1);
    v.undo(false);
    assert_eq!(v.session, before);
    click(&mut v, row(4, None), false, false);
    click(&mut v, row(4, Some(29)), false, true);
    key(&mut v, "delete", "", false, false);
    assert_eq!(v.session.watches.len(), 11);
    assert_eq!(v.session.watches[4], before.watches[5]);
    v.undo(false);
    assert_eq!(v.session, before);
}

#[test]
fn ranges_skip_collapsed_contents_and_group_focused_menus_still_edit_selected_signals() {
    let mut v = demo();
    v.session.watches.insert(
        2,
        Watch::Group {
            name: "Hidden".into(),
            collapsed: true,
        },
    );
    v.session.watches.insert(
        6,
        Watch::Group {
            name: "Visible".into(),
            collapsed: false,
        },
    );
    v.session.watches.insert(7, Watch::Divider);
    click(&mut v, row(1, None), false, false);
    click(&mut v, row(8, None), false, true);
    assert_eq!(selected(&v), [1, 2, 6, 7, 8]);
    let before = v.session.clone();
    menu(&mut v, row(6, None));
    choose(&mut v, MenuAction::Radix(Radix::Signed));
    assert_eq!(format(&v.session.watches[1]), Radix::Signed);
    assert_eq!(format(&v.session.watches[8]), Radix::Signed);
    assert_eq!(&v.session.watches[2..8], &before.watches[2..8]);
    key(&mut v, "delete", "", false, false);
    assert_eq!(v.session.watches.len(), 10);
}

#[test]
fn time_pan_and_range_zoom_keep_multiselection_and_name_modifier_clicks_keep_cursors() {
    let mut v = demo();
    click(&mut v, row(0, None), false, false);
    click(&mut v, row(3, None), false, true);
    let selection = v.selected_rows();
    let l = v.layout();
    let time = v.session.cursor;
    let y = l.body_y + v.rows()[5].line;
    for kind in ["down", "up"] {
        v.event(Event {
            kind: kind.into(),
            view_column: l.wave_x - 3,
            view_row: y,
            ctrl: true,
            ..Event::default()
        });
    }
    assert_eq!(v.session.cursor, time);
    assert_eq!(v.selection_count(), 5);
    click(&mut v, row(5, None), true, false);
    assert_eq!(v.selected_rows(), selection);
    for control in [true, false] {
        for (kind, x) in [
            ("down", l.wave_x + 20),
            ("drag", l.wave_x + 40),
            ("up", l.wave_x + 40),
        ] {
            v.event(Event {
                kind: kind.into(),
                view_column: x,
                view_row: l.body_y,
                control,
                shift: !control,
                ..Event::default()
            });
        }
        assert_eq!(v.selected_rows(), selection);
        assert!(v.drag.is_none());
    }
}

#[test]
fn inserting_reordering_restoring_and_browser_adding_clear_stale_row_indices() {
    let mut v = demo();
    click(&mut v, row(2, None), false, false);
    click(&mut v, row(5, None), false, true);
    key(&mut v, "char", "d", false, false);
    assert!(v.wave_selection.is_none());
    assert_eq!(v.selection_count(), 1);
    key(&mut v, "char", "a", true, false);
    v.event(Event {
        code: "up".into(),
        alt: true,
        ..Event::default()
    });
    assert!(v.wave_selection.is_none());
    key(&mut v, "char", "a", true, false);
    v.restore(v.session.clone()).unwrap();
    assert!(v.wave_selection.is_none());
    key(&mut v, "char", "a", true, false);
    v.add_signal(0);
    assert!(v.wave_selection.is_none());
    assert_eq!(v.selected_rows().len(), 1);
}

#[test]
fn select_all_handles_every_expanded_row_without_duplicate_parent_color_edits() {
    let mut v = Viewer::new("wide.vcd".into(), None);
    let mut source = String::from("$scope module top $end\n");
    for i in 0..256 {
        source += &format!("$var wire 64 s{i} bus{i} [63:0] $end\n");
    }
    source += "$upscope $end\n$enddefinitions $end\n#0\n";
    for i in 0..256 {
        source += &format!("b0 s{i}\n");
    }
    v.ready(vcd::parse(source.as_bytes()).unwrap());
    v.session.watches = v
        .wave
        .signals
        .iter()
        .map(|s| {
            let mut watch = Watch::signal(s.path.clone(), 0);
            if let Watch::Signal { expanded, .. } = &mut watch {
                *expanded = true;
            }
            watch
        })
        .collect();
    v.focus = Focus::Waves;
    key(&mut v, "char", "a", true, false);
    assert_eq!(v.selection_count(), 16_640);
    assert_eq!(v.selected_rows().len(), 16_640);
    key(&mut v, "char", "8", false, false);
    assert!(v.session.watches.iter().all(|w| color(w) == 7));
    assert_eq!(v.history.len(), 1);
    assert_eq!(v.selection_count(), 16_640);
    key(&mut v, "delete", "", false, false);
    assert!(v.session.watches.is_empty());
    v.undo(false);
    assert_eq!(v.session.watches.len(), 256);
}

#[test]
fn browser_menus_for_hidden_or_new_signals_never_edit_an_unrelated_selection() {
    let mut v = demo();
    v.session.watches.insert(
        4,
        Watch::Group {
            name: "Collapsed".into(),
            collapsed: true,
        },
    );
    click(&mut v, row(0, None), false, false);
    click(&mut v, row(2, None), false, true);
    v.filter = "wdata".into();
    v.rebuild_browser();
    let before = v.session.clone();
    v.event(Event {
        kind: "contextMenu".into(),
        view_column: 6,
        view_row: v.layout().body_y,
        ..Event::default()
    });
    assert_eq!(v.selected_rows(), [row(6, None)]);
    choose(&mut v, MenuAction::Radix(Radix::Signed));
    assert_eq!(format(&v.session.watches[6]), Radix::Signed);
    assert_eq!(&v.session.watches[..6], &before.watches[..6]);
    assert!(matches!(
        v.session.watches[4],
        Watch::Group {
            collapsed: true,
            ..
        }
    ));
    assert_eq!(v.history.len(), 1);
    v.undo(false);
    assert_eq!(v.session, before);

    key(&mut v, "char", "a", true, false);
    v.filter = "shift_reg".into();
    v.rebuild_browser();
    v.event(Event {
        kind: "contextMenu".into(),
        view_column: 6,
        view_row: v.layout().body_y,
        ..Event::default()
    });
    choose(&mut v, MenuAction::Size(WaveSize::Normal));
    assert_eq!(v.session.watches.len(), 14);
    assert_eq!(v.selection_count(), 1);
    assert_eq!(v.session.watches[13].size(None), WaveSize::Normal);
    assert_eq!(&v.session.watches[..4], &before.watches[..4]);
    assert_eq!(v.history.len(), 1);
    v.undo(false);
    assert_eq!(v.session, before);
}
