use vtcode_waveform_viewer::{
    controls::{Column, ControlDrag, ScrollPane},
    menus::MenuAction,
    model::Radix,
    vcd,
    viewer::{Event, Focus, Session, Viewer, Watch},
};

fn demo() -> Viewer {
    let mut v = Viewer::new("demo.vcd".into(), None);
    v.ready(vcd::parse(include_bytes!("../examples/demo.vcd")).unwrap());
    v.session.browser = true;
    v.width = 180;
    v.height = 32;
    v.session.cursor = 241;
    v.session.alt_cursor = Some(421);
    v.session.snap = false;
    // As after a real resize: whole cells for this width.
    v.clamp_window();
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
fn key(v: &mut Viewer, code: &str, text: &str) {
    v.event(Event {
        code: code.into(),
        text: text.into(),
        ..Event::default()
    });
}
fn menu(v: &mut Viewer, action: MenuAction) {
    let index = v
        .menu_items()
        .iter()
        .position(|i| i.action == action && i.enabled)
        .unwrap();
    v.context_menu.as_mut().unwrap().selected = index;
    key(v, "enter", "");
}
fn text(v: &Viewer) -> String {
    v.render()
        .lines
        .iter()
        .map(|line| line.iter().map(|s| s.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn open_signal(v: &mut Viewer) {
    let l = v.layout();
    let row = v.rows().into_iter().find(|r| r.watch == 4).unwrap();
    pointer(v, "contextMenu", l.browser + 6, l.body_y + row.line);
}

#[test]
fn both_column_dividers_preview_commit_once_and_support_undo_redo() {
    for column in [Column::Names, Column::Values] {
        let mut v = demo();
        v.event(Event {
            code: "command".into(),
            text: "select_all".into(),
            ..Event::default()
        });
        let before = v.session.clone();
        let selected = v.selected_rows();
        let l = v.layout();
        let x = if column == Column::Names {
            l.browser + l.name - 1
        } else {
            l.wave_x - 1
        };
        pointer(&mut v, "down", x, 3);
        pointer(&mut v, "drag", x + 12, 4);
        assert!(v.history.is_empty());
        assert!(matches!(v.control_drag, Some(ControlDrag::Column { .. })));
        assert_eq!(v.selected_rows(), selected);
        assert_eq!(v.session.cursor, before.cursor);
        assert_eq!(v.session.alt_cursor, before.alt_cursor);
        pointer(&mut v, "up", x + 12, 4);
        let changed = v.session.clone();
        assert_eq!(v.history.len(), 1);
        assert!(v.control_drag.is_none());
        assert_ne!(changed, before);
        assert_eq!(
            if column == Column::Names {
                v.layout().name
            } else {
                v.layout().value
            },
            if column == Column::Names {
                l.name + 12
            } else {
                l.value + 12
            }
        );
        v.undo(false);
        assert_eq!(v.session, before);
        v.undo(true);
        assert_eq!(v.session, changed);
    }
}

#[test]
fn cancelling_columns_restores_widths_and_selection_without_changing_history() {
    for cancel in ["escape", "cancel", "resize"] {
        let mut v = demo();
        key(&mut v, "command", "select_all");
        let selected = v.selected_rows();
        let before = v.session.clone();
        let x = v.layout().wave_x - 1;
        pointer(&mut v, "down", x, 3);
        pointer(&mut v, "drag", x + 20, 3);
        if cancel == "escape" {
            key(&mut v, "escape", "");
        } else {
            v.event(Event {
                kind: cancel.into(),
                ..Event::default()
            });
        }
        assert_eq!(v.session, before);
        assert_eq!(v.selected_rows(), selected);
        assert!(v.history.is_empty() && v.control_drag.is_none());
    }
    let mut v = demo();
    v.checkpoint();
    v.session.name_width = 40;
    v.undo(false);
    let x = v.layout().wave_x - 1;
    pointer(&mut v, "down", x, 3);
    pointer(&mut v, "up", x, 3);
    assert_eq!(v.redo.len(), 1);
    assert!(v.history.is_empty());
}

#[test]
fn header_menu_fits_columns_resets_them_and_keeps_selection() {
    let mut v = demo();
    if let Watch::Signal { label, .. } = &mut v.session.watches[0] {
        *label = "A long descriptive clock name".into();
    }
    key(&mut v, "command", "select_all");
    let selected = v.selected_rows();
    let before = v.session.clone();
    let x = v.layout().browser + 7;
    pointer(&mut v, "contextMenu", x, 3);
    assert!(text(&v).contains("Signals & values"));
    menu(&mut v, MenuAction::FitNames);
    assert!(v.session.name_width > 28);
    assert_eq!(v.selected_rows(), selected);
    pointer(&mut v, "contextMenu", x, 3);
    menu(&mut v, MenuAction::FitValues);
    assert!(v.session.value_width >= 11);
    pointer(&mut v, "contextMenu", x, 3);
    menu(&mut v, MenuAction::ResetColumns);
    assert_eq!((v.session.name_width, v.session.value_width), (28, 23));
    assert_eq!(v.session.watches, before.watches);
    let history = v.history.len();
    pointer(&mut v, "contextMenu", x, 3);
    menu(&mut v, MenuAction::ResetColumns);
    assert_eq!(v.history.len(), history);
}

#[test]
fn value_auto_fit_respects_full_formatted_values_and_saved_widths_migrate() {
    let mut v = Viewer::new("wide.vcd".into(), None);
    v.ready(vcd::parse(b"$scope module top $end $var wire 128 ! payload [127:0] $end $upscope $end $enddefinitions $end #0 b11111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111 ! #100").unwrap());
    v.width = 180;
    v.clamp_window();
    pointer(&mut v, "contextMenu", 44, 3);
    menu(&mut v, MenuAction::FitValues);
    assert!(
        v.layout().value >= 41,
        "32 hex digits plus separators must fit"
    );
    assert!(text(&v).contains("ffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff"));
    let saved: Session = serde_json::from_str(&serde_json::to_string(&v.session).unwrap()).unwrap();
    v.restore(saved.clone()).unwrap();
    assert_eq!(v.session, saved);
    let mut old = serde_json::to_value(&saved).unwrap();
    old.as_object_mut().unwrap().remove("value_width");
    v.restore(serde_json::from_value(old.clone()).unwrap())
        .unwrap();
    assert_eq!(v.session.value_width, 23);
    for (value, expected) in [(0, 7), (65535, 64)] {
        old["value_width"] = value.into();
        v.restore(serde_json::from_value(old.clone()).unwrap())
            .unwrap();
        assert_eq!(v.session.value_width, expected);
    }
}

#[test]
fn name_auto_fit_reserves_room_for_the_bus_format_badge() {
    let mut v = demo();
    let label = "Payload for the primary transaction";
    if let Watch::Signal { label: name, .. } = &mut v.session.watches[4] {
        *name = label.into();
    }
    let x = v.layout().browser + 5;
    pointer(&mut v, "contextMenu", x, 3);
    menu(&mut v, MenuAction::FitNames);
    assert!(text(&v).contains(label));
    assert!(text(&v).contains("HEX"));
}

#[test]
fn cursor_handles_are_separate_at_coincident_times_and_viewport_edges() {
    let mut v = demo();
    v.fit();
    for time in [0, 241, v.wave.end] {
        v.session.cursor = time;
        v.session.alt_cursor = Some(time);
        let handles = v.cursor_handles();
        assert_eq!(handles.len(), 2);
        assert!(
            handles[0].x + handles[0].width <= handles[1].x
                || handles[1].x + handles[1].width <= handles[0].x
        );
        assert!(
            handles
                .iter()
                .all(|h| h.x >= v.layout().wave_x && h.x + h.width <= v.width)
        );
        assert!(text(&v).contains("[A]") && text(&v).contains("[B]"));
    }
    for width in 1..80 {
        v.width = width;
        assert!(v.cursor_handles().iter().all(|h| h.x + h.width <= width));
        v.render();
    }
}

#[test]
fn cursor_b_can_be_dragged_without_alt_and_without_changing_a_or_selection() {
    let mut v = demo();
    key(&mut v, "command", "select_all");
    let selected = v.selected_rows();
    let before = v.session.clone();
    let handle = v.cursor_handles().into_iter().find(|h| h.alt).unwrap();
    let x = handle.x + 1;
    let expected = v.time_at(handle.column + 10);
    pointer(&mut v, "down", x, 3);
    pointer(&mut v, "drag", x + 10, 6);
    pointer(&mut v, "up", x + 10, 6);
    assert_eq!(v.session.alt_cursor, Some(expected));
    assert_eq!(v.session.cursor, before.cursor);
    assert_eq!(v.session.start, before.start);
    assert_eq!(v.selected_rows(), selected);
    assert!(v.history.is_empty() && v.control_drag.is_none());
}

#[test]
fn clicking_handles_never_rounds_their_time_and_escape_restores_a_drag() {
    let mut v = demo();
    key(&mut v, "command", "select_all");
    let before = v.session.clone();
    for alt in [false, true] {
        let handle = v
            .cursor_handles()
            .into_iter()
            .find(|h| h.alt == alt)
            .unwrap();
        let x = handle.x + 1;
        pointer(&mut v, "down", x, 3);
        pointer(&mut v, "up", x, 3);
        assert_eq!(v.session, before);
        pointer(&mut v, "down", x, 3);
        pointer(&mut v, "drag", x + 8, 3);
        assert_ne!(v.session, before);
        key(&mut v, "escape", "");
        assert_eq!(v.session, before);
        assert_eq!(v.selection_count(), 12);
    }
}

#[test]
fn alt_cursor_drag_keeps_its_target_when_alt_is_released_mid_gesture() {
    let mut v = demo();
    let a = v.session.cursor;
    let l = v.layout();
    v.event(Event {
        kind: "down".into(),
        view_column: l.wave_x + 20,
        view_row: 4,
        alt: true,
        ..Event::default()
    });
    pointer(&mut v, "drag", l.wave_x + 30, 4);
    pointer(&mut v, "up", l.wave_x + 30, 4);
    assert_eq!(v.session.cursor, a);
    assert_eq!(v.session.alt_cursor, Some(v.time_at(l.wave_x + 30)));
}

#[test]
fn snap_readout_is_clickable_and_has_exact_hover_feedback() {
    let mut v = demo();
    for alt in [None, Some(421)] {
        v.session.alt_cursor = alt;
        let before = v.session.clone();
        let (x, _) = v.snap_control().unwrap();
        pointer(&mut v, "move", x + 2, 2);
        assert!(text(&v).contains("Edge snapping · Click to toggle"));
        pointer(&mut v, "down", x + 2, 2);
        pointer(&mut v, "up", x + 2, 2);
        assert_eq!(v.session.snap, !before.snap);
        assert_eq!(v.session.cursor, before.cursor);
        assert_eq!(v.session.alt_cursor, before.alt_cursor);
        key(&mut v, "char", "s");
        assert_eq!(v.session, before);
    }
    v.width = 30;
    assert!(v.snap_control().is_none());
}

#[test]
fn hierarchy_and_signal_scrollbars_drag_page_and_cancel_without_touching_selection() {
    for pane in [ScrollPane::Browser, ScrollPane::Waves] {
        let mut v = demo();
        v.height = 16;
        key(&mut v, "command", "select_all");
        let selected = v.selected_rows();
        let before = v.session.clone();
        let l = v.layout();
        let x = if pane == ScrollPane::Browser {
            l.browser - 1
        } else {
            l.wave_x - 1
        };
        pointer(&mut v, "down", x, l.body_y);
        assert!(matches!(v.control_drag, Some(ControlDrag::Scroll { .. })));
        pointer(&mut v, "drag", x, l.body_y + l.body_height - 1);
        assert!(
            if pane == ScrollPane::Browser {
                v.browser_scroll
            } else {
                v.scroll
            } > 0
        );
        key(&mut v, "escape", "");
        assert_eq!((v.scroll, v.browser_scroll), (0, 0));
        assert_eq!(v.selected_rows(), selected);
        pointer(&mut v, "down", x, l.body_y + l.body_height - 1);
        pointer(&mut v, "up", x, l.body_y + l.body_height - 1);
        assert_eq!(
            if pane == ScrollPane::Browser {
                v.browser_scroll
            } else {
                v.scroll
            },
            usize::from(l.body_height - 1)
        );
        assert_eq!(v.session, before);
        assert!(v.history.is_empty() && v.control_drag.is_none());
    }
}

#[test]
fn thumb_click_without_drag_preserves_scroll_and_resize_clamps_the_browser() {
    let mut v = demo();
    v.height = 16;
    v.browser_scroll = 1;
    let l = v.layout();
    pointer(&mut v, "down", l.browser - 1, l.body_y);
    pointer(&mut v, "up", l.browser - 1, l.body_y);
    assert_eq!(v.browser_scroll, 1);
    v.browser_scroll = 12;
    v.event(Event {
        kind: "resize".into(),
        height: 100,
        width: 180,
        ..Event::default()
    });
    assert_eq!(v.browser_scroll, 0);
}

#[test]
fn menu_navigation_stays_visible_and_supports_type_to_select_and_color_shortcuts() {
    let mut v = demo();
    v.height = 18;
    let footer: Vec<_> = text(&v).lines().skip(16).map(str::to_owned).collect();
    open_signal(&mut v);
    let l = v.menu_layout();
    assert!(l.panel.y + l.panel.height <= v.height - 2);
    assert_eq!(
        text(&v).lines().skip(16).collect::<Vec<_>>(),
        footer.iter().map(String::as_str).collect::<Vec<_>>()
    );
    v.event(Event {
        direction: 1,
        ..Event::default()
    });
    let l = v.menu_layout();
    assert!((l.scroll..l.scroll + l.visible).contains(&v.context_menu.as_ref().unwrap().selected));
    key(&mut v, "char", "s");
    assert_eq!(
        v.menu_items()[v.context_menu.as_ref().unwrap().selected].action,
        MenuAction::Radix(Radix::Signed)
    );
    key(&mut v, "enter", "");
    assert!(matches!(
        v.session.watches[4],
        Watch::Signal {
            radix: Radix::Signed,
            ..
        }
    ));
    open_signal(&mut v);
    menu(&mut v, MenuAction::Colors);
    key(&mut v, "char", "8");
    assert!(matches!(
        v.session.watches[4],
        Watch::Signal { color: 7, .. }
    ));
    assert!(v.context_menu.is_none());
}

#[test]
fn advertised_menu_shortcuts_copy_find_rename_expand_and_delete_the_target() {
    let mut v = demo();
    open_signal(&mut v);
    v.event(Event {
        code: "char".into(),
        text: "c".into(),
        control: true,
        ..Event::default()
    });
    assert!(v.copy_requested.take().is_some() && v.context_menu.is_none());
    open_signal(&mut v);
    key(&mut v, "char", "f");
    assert_eq!(
        v.prompt,
        Some(vtcode_waveform_viewer::viewer::Prompt::FindValue)
    );
    key(&mut v, "escape", "");
    open_signal(&mut v);
    key(&mut v, "function", "2");
    assert_eq!(
        v.prompt,
        Some(vtcode_waveform_viewer::viewer::Prompt::Rename)
    );
    key(&mut v, "escape", "");
    open_signal(&mut v);
    key(&mut v, "char", "e");
    assert!(matches!(
        v.session.watches[4],
        Watch::Signal { expanded: true, .. }
    ));
    open_signal(&mut v);
    key(&mut v, "delete", "");
    assert_eq!(v.session.watches.len(), 11);
    assert!(v.context_menu.is_none());
}

#[test]
fn menu_scrollbar_drag_reaches_lower_actions_and_keeps_enter_target_visible() {
    let mut v = demo();
    v.height = 18;
    open_signal(&mut v);
    let l = v.menu_layout();
    let x = l.panel.x + l.panel.width - 1;
    pointer(&mut v, "down", x, l.items.y);
    pointer(&mut v, "drag", x, l.items.y + l.items.height - 1);
    pointer(&mut v, "up", x, l.items.y + l.items.height - 1);
    let l = v.menu_layout();
    assert_eq!(l.scroll + l.visible, v.menu_items().len());
    assert!((l.scroll..l.scroll + l.visible).contains(&v.context_menu.as_ref().unwrap().selected));
    assert!(text(&v).contains("Remove"));
    key(&mut v, "end", "");
    key(&mut v, "enter", "");
    assert_eq!(v.session.watches.len(), 11);
}

#[test]
fn format_badges_and_adaptive_values_do_not_overlap_or_require_more_rows() {
    let mut v = demo();
    v.selection = 11;
    v.ensure_selection();
    let rows = v.rows();
    let rendered = text(&v);
    assert!(rendered.contains("HEX") && rendered.contains("ANLG"));
    assert!(rendered.contains("address[31:0]") && rendered.contains("fifo_count[3:0]"));
    for width in [1, 12, 40, 64, 90, 180, 512] {
        v.width = width;
        v.session.value_width = 64;
        let l = v.layout();
        assert!(l.wave_x <= width);
        if width >= 48 {
            assert!(l.wave_width >= 16);
        }
        let frame = v.render();
        assert_eq!((frame.width, frame.height), (width, 32));
        assert_eq!(v.rows(), rows);
    }
    assert_eq!(v.focus, Focus::Waves);
}
