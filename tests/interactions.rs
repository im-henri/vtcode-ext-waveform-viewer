use unicode_width::UnicodeWidthStr;
use vtcode_waveform_viewer::{
    dialogs::DialogFocus,
    menus::MenuAction,
    model::Radix,
    vcd,
    viewer::{BrowserNode, Event, Focus, Prompt, Viewer, Watch},
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
fn key(v: &mut Viewer, code: &str) {
    v.event(Event {
        code: code.into(),
        ..Event::default()
    });
}
fn paste(v: &mut Viewer, text: &str) {
    v.event(Event {
        code: "paste".into(),
        text: text.into(),
        ..Event::default()
    });
}
fn control(v: &mut Viewer, text: &str) {
    v.event(Event {
        code: "char".into(),
        text: text.into(),
        control: true,
        ..Event::default()
    });
}
fn pointer(v: &mut Viewer, kind: &str, x: u16, y: u16) {
    v.event(Event {
        kind: kind.into(),
        view_column: x,
        view_row: y,
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
fn signal(v: &Viewer, name: &str) -> usize {
    v.session.watches.iter().position(|watch| matches!(watch, Watch::Signal { path, .. } if v.wave.signals[v.signal_index[path]].name == name)).unwrap()
}
fn menu(v: &mut Viewer, watch: usize) {
    let row = v
        .rows()
        .into_iter()
        .find(|row| row.watch == watch && row.bit.is_none())
        .unwrap();
    pointer(
        v,
        "contextMenu",
        v.layout().wave_x + 10,
        v.layout().body_y + row.line - v.scroll as u16,
    );
    assert!(v.context_menu.is_some());
}
fn choose(v: &mut Viewer, action: MenuAction) {
    let index = v
        .menu_items()
        .iter()
        .position(|item| item.action == action && item.enabled)
        .unwrap();
    // Bring the command into view using the same wheel path as the UI.
    while index >= v.menu_layout().scroll + v.menu_layout().visible {
        v.event(Event {
            direction: 1,
            ..Event::default()
        });
    }
    while index < v.menu_layout().scroll {
        v.event(Event {
            direction: -1,
            ..Event::default()
        });
    }
    let l = v.menu_layout();
    pointer(
        v,
        "down",
        l.items.x + 4,
        l.items.y + (index - l.scroll) as u16,
    );
}

#[test]
fn menu_queries_are_centered_clickable_and_commit_one_edit() {
    for (action, prompt, input) in [
        ("group", Prompt::Group, "AXI control"),
        ("marker", Prompt::Marker, "Handshake"),
    ] {
        let mut v = demo();
        let original = v.session.clone();
        let category = if action == "group" {
            vtcode_waveform_viewer::workbench::MainMenu::Signal
        } else {
            vtcode_waveform_viewer::workbench::MainMenu::Cursors
        };
        let button = v
            .main_menus()
            .into_iter()
            .find(|b| b.menu == category)
            .unwrap();
        pointer(&mut v, "down", button.x + 1, 0);
        choose(&mut v, MenuAction::Command(action));
        pointer(&mut v, "up", 0, 0);
        pointer(&mut v, "cancel", 0, 0);
        assert_eq!(v.prompt, Some(prompt));
        let frame = v.render();
        let l = v.dialog_layout();
        assert!(l.panel.y > 1 && l.panel.y + l.panel.height < v.height - 1);
        assert_eq!(frame.cursor.unwrap().1, l.input.y);
        let text = lines(&v).join("\n");
        assert!(
            text.contains(prompt.label()) && text.contains("Cancel") && text.contains("Create")
        );
        paste(&mut v, input);
        pointer(&mut v, "down", l.submit.x + 2, l.submit.y);
        assert_eq!(v.prompt, None);
        assert_eq!(v.history.len(), 1);
        if prompt == Prompt::Group {
            assert!(matches!(&v.session.watches[0], Watch::Group { name, .. } if name == input));
        } else {
            assert_eq!(v.session.markers[0].name, input);
        }
        v.undo(false);
        assert_eq!(v.session, original);
    }
}

#[test]
fn escape_close_cancel_and_outside_click_dismiss_without_background_actions() {
    for how in 0..4 {
        let mut v = demo();
        let original = v.session.clone();
        v.open_prompt(Prompt::Group);
        paste(&mut v, "temporary");
        let l = v.dialog_layout();
        match how {
            0 => key(&mut v, "escape"),
            1 => pointer(&mut v, "down", l.close.x + 1, l.close.y),
            2 => pointer(&mut v, "down", l.cancel.x + 1, l.cancel.y),
            _ => pointer(&mut v, "down", 10, 1),
        }
        assert!(v.prompt.is_none());
        assert_eq!(v.session, original);
        assert!(v.history.is_empty() && v.input.is_empty());
    }
    let mut v = demo();
    v.filter = "clk".into();
    v.rebuild_browser();
    v.focus = Focus::Waves;
    v.session.browser = false;
    v.open_prompt(Prompt::Filter);
    paste(&mut v, "wdata");
    assert_eq!(v.filter, "wdata");
    assert!(v.session.browser);
    key(&mut v, "escape");
    assert_eq!(v.filter, "clk");
    assert!(!v.session.browser);
    assert_eq!(v.focus, Focus::Waves);
}

#[test]
fn text_editing_is_utf8_safe_with_caret_selection_and_keyboard_focus() {
    let mut v = demo();
    v.open_prompt(Prompt::Group);
    paste(&mut v, "AXI 数据");
    key(&mut v, "left");
    key(&mut v, "backspace");
    assert_eq!(v.input, "AXI 据");
    key(&mut v, "home");
    key(&mut v, "delete");
    assert_eq!(v.input, "XI 据");
    key(&mut v, "end");
    paste(&mut v, " valid");
    assert_eq!(v.input, "XI 据 valid");
    control(&mut v, "a");
    control(&mut v, "c");
    assert_eq!(v.copy_requested.as_deref(), Some("XI 据 valid"));
    paste(&mut v, "Replacement");
    assert_eq!(v.input, "Replacement");
    for command in ["select_all", "cut"] {
        v.event(Event {
            code: "command".into(),
            text: command.into(),
            ..Event::default()
        });
    }
    assert!(v.input.is_empty());
    assert_eq!(v.copy_requested.as_deref(), Some("Replacement"));
    paste(&mut v, "Replacement");
    key(&mut v, "home");
    paste(&mut v, "前 ");
    assert_eq!(v.input, "前 Replacement");
    key(&mut v, "tab");
    assert_eq!(v.dialog.focus, DialogFocus::Cancel);
    assert!(v.render().cursor.is_none());
    key(&mut v, "tab");
    assert_eq!(v.dialog.focus, DialogFocus::Submit);
    key(&mut v, "enter");
    assert!(matches!(&v.session.watches[0], Watch::Group { name, .. } if name == "前 Replacement"));
    v.open_prompt(Prompt::Rename);
    assert!(v.dialog.select_all);
    paste(&mut v, "Renamed");
    key(&mut v, "tab");
    key(&mut v, "enter");
    assert!(v.prompt.is_none());
    assert!(matches!(&v.session.watches[0], Watch::Group { name, .. } if name == "前 Replacement"));
}

#[test]
fn invalid_queries_stay_editable_and_show_errors_inside_the_dialog() {
    let mut v = demo();
    v.open_prompt(Prompt::Group);
    key(&mut v, "enter");
    assert!(v.error && v.prompt == Some(Prompt::Group));
    let l = v.dialog_layout();
    pointer(&mut v, "move", l.input.x, l.input.y);
    pointer(&mut v, "up", l.input.x, l.input.y);
    assert!(v.error);
    assert!(lines(&v)[(l.panel.y + 8) as usize].contains("Enter a group name"));
    paste(&mut v, "Control");
    assert!(!v.error);
    key(&mut v, "enter");
    v.open_prompt(Prompt::Goto);
    let original = v.session.clone();
    paste(&mut v, "999999 ns");
    key(&mut v, "enter");
    assert!(v.error && v.prompt.is_some());
    v.event(Event {
        direction: 1,
        shift: true,
        ..Event::default()
    });
    assert_eq!(v.session, original);
    control(&mut v, "a");
    paste(&mut v, "240 ns");
    key(&mut v, "enter");
    assert!(v.prompt.is_none());
    assert_eq!(v.session.cursor, 240);
}

#[test]
fn clicking_a_query_field_moves_the_caret_and_large_pastes_are_bounded() {
    let mut v = demo();
    v.open_prompt(Prompt::Marker);
    paste(&mut v, "Alpha");
    let l = v.dialog_layout();
    pointer(&mut v, "down", l.input.x + 2, l.input.y);
    paste(&mut v, "X");
    assert_eq!(v.input, "AlXpha");
    paste(&mut v, &"x".repeat(4097));
    assert!(v.error);
    assert_eq!(v.input, "AlXpha");
    key(&mut v, "escape");
    v.open_prompt(Prompt::Import);
    let session = v.session.clone();
    let json = format!(
        "{}{}",
        " ".repeat(1_000_000),
        serde_json::to_string(&session).unwrap()
    );
    paste(&mut v, &json);
    assert_eq!(v.input.len(), json.len());
    assert!(
        v.render()
            .lines
            .iter()
            .map(|line| line.iter().map(|s| s.text.len()).sum::<usize>())
            .sum::<usize>()
            < 20_000
    );
    key(&mut v, "enter");
    assert!(v.prompt.is_none());
    assert_eq!(v.session, session);
    v.undo(false);
    let history = v.history.clone();
    let redo = v.redo.clone();
    v.open_prompt(Prompt::Import);
    let mut invalid = session.clone();
    invalid.schema = 999;
    paste(&mut v, &serde_json::to_string(&invalid).unwrap());
    key(&mut v, "enter");
    assert!(v.error && v.prompt == Some(Prompt::Import));
    assert_eq!(v.history, history);
    assert_eq!(v.redo, redo, "failed imports preserve redo history");
}

#[test]
fn context_menu_targets_the_clicked_signal_and_sets_explicit_formats_undoably() {
    let mut v = demo();
    let watch = signal(&v, "wdata");
    for radix in [
        Radix::Hex,
        Radix::Binary,
        Radix::Unsigned,
        Radix::Signed,
        Radix::Octal,
        Radix::Ascii,
    ] {
        let before = v.session.clone();
        let history = v.history.len();
        v.selection = 0;
        menu(&mut v, watch);
        assert_eq!(v.selection, watch);
        assert_eq!(v.session, before);
        assert_eq!(v.history.len(), history);
        choose(&mut v, MenuAction::Radix(radix));
        assert!(v.context_menu.is_none());
        assert!(
            matches!(v.session.watches[watch], Watch::Signal { radix: actual, analog: false, .. } if actual == radix)
        );
        if v.session == before {
            assert_eq!(
                v.history.len(),
                history,
                "choosing the active format does not create an undo entry"
            );
        } else {
            assert_eq!(v.history.len(), history + 1);
            v.undo(false);
        }
        assert_eq!(v.session, before);
    }
    menu(&mut v, watch);
    choose(&mut v, MenuAction::Analog);
    assert!(matches!(
        v.session.watches[watch],
        Watch::Signal { analog: true, .. }
    ));
    menu(&mut v, watch);
    assert!(
        v.menu_items()
            .iter()
            .any(|item| item.action == MenuAction::Analog && item.checked)
    );
    choose(&mut v, MenuAction::Radix(Radix::Hex));
    assert!(matches!(
        v.session.watches[watch],
        Watch::Signal {
            analog: false,
            radix: Radix::Hex,
            ..
        }
    ));
}

#[test]
fn browser_menu_adds_a_configured_signal_in_one_edit_and_copy_does_not_add_it() {
    let mut v = demo();
    let watch = signal(&v, "wdata");
    let path = match v.session.watches.remove(watch) {
        Watch::Signal { path, .. } => path,
        _ => unreachable!(),
    };
    v.expanded_scopes.extend(0..v.wave.scopes.len());
    v.rebuild_browser();
    let node = v
        .browser_nodes
        .iter()
        .position(
            |node| matches!(node, BrowserNode::Signal(id, _) if v.wave.signals[*id].path == path),
        )
        .unwrap();
    let original = v.session.clone();
    let y = v.layout().body_y + node as u16;
    pointer(&mut v, "contextMenu", 8, y);
    assert!(v.context_menu.is_some());
    assert_eq!(v.session, original);
    choose(&mut v, MenuAction::Copy);
    assert!(v.copy_requested.is_some());
    assert_eq!(v.session, original);
    assert!(v.history.is_empty());
    pointer(&mut v, "contextMenu", 8, y);
    choose(&mut v, MenuAction::Analog);
    assert!(
        matches!(&v.session.watches[v.selection], Watch::Signal { path: actual, analog: true, .. } if *actual == path)
    );
    assert_eq!(v.history.len(), 1);
    v.undo(false);
    assert_eq!(v.session, original);
}

#[test]
fn context_menu_keyboard_colors_and_dismissal_do_not_leak_to_the_waveform() {
    let mut v = demo();
    let watch = signal(&v, "wdata");
    menu(&mut v, watch);
    let original = v.session.clone();
    choose(&mut v, MenuAction::Colors);
    assert!(v.context_menu.as_ref().unwrap().colors);
    key(&mut v, "escape");
    assert!(!v.context_menu.as_ref().unwrap().colors);
    choose(&mut v, MenuAction::Colors);
    choose(&mut v, MenuAction::Color(4));
    assert!(matches!(
        v.session.watches[watch],
        Watch::Signal { color: 4, .. }
    ));
    v.undo(false);
    assert_eq!(v.session, original);
    v.event(Event {
        code: "function".into(),
        text: "10".into(),
        shift: true,
        ..Event::default()
    });
    assert!(v.context_menu.is_some());
    key(&mut v, "end");
    assert_eq!(
        v.menu_items()[v.context_menu.as_ref().unwrap().selected].action,
        MenuAction::Remove
    );
    key(&mut v, "home");
    assert_eq!(
        v.menu_items()[v.context_menu.as_ref().unwrap().selected].action,
        MenuAction::Radix(Radix::Hex)
    );
    pointer(&mut v, "cancel", 0, 0);
    assert!(v.context_menu.is_some());
    pointer(&mut v, "down", 1, 1);
    assert!(v.context_menu.is_none());
    assert_eq!(v.session, original);
}

#[test]
fn expanded_bits_are_contiguous_and_menus_disable_bus_only_formats() {
    let mut v = demo();
    let watch = signal(&v, "wdata");
    if let Watch::Signal { expanded, .. } = &mut v.session.watches[watch] {
        *expanded = true;
    }
    v.selection = watch;
    v.ensure_selection();
    let rows = v.rows();
    for pair in rows.windows(2) {
        assert_eq!(pair[0].line + pair[0].height, pair[1].line);
    }
    let bit = rows
        .iter()
        .find(|row| row.watch == watch && row.bit == Some(31))
        .unwrap();
    let x = v.layout().browser + 8;
    let y = v.layout().body_y + bit.line - v.scroll as u16;
    pointer(&mut v, "contextMenu", x, y);
    let items = v.menu_items();
    assert!(
        items
            .iter()
            .any(|item| item.action == MenuAction::Radix(Radix::Binary) && item.checked)
    );
    assert!(
        items
            .iter()
            .any(|item| item.action == MenuAction::Radix(Radix::Hex) && !item.enabled)
    );
    assert!(
        items
            .iter()
            .any(|item| item.action == MenuAction::Analog && !item.enabled)
    );
    choose(&mut v, MenuAction::Copy);
    assert!(matches!(
        v.copy_requested.as_deref(),
        Some("0" | "1" | "x" | "z")
    ));
}

#[test]
fn popups_remain_within_tiny_resized_views_and_menu_scroll_reaches_every_action() {
    let mut v = demo();
    for (width, height) in [(1, 1), (16, 5), (40, 8), (80, 16), (150, 32), (512, 200)] {
        v.width = width;
        v.height = height;
        for prompt in [
            Prompt::Group,
            Prompt::Marker,
            Prompt::Rename,
            Prompt::Goto,
            Prompt::Import,
        ] {
            v.open_prompt(prompt);
            paste(&mut v, "A 数据");
            let frame = v.render();
            if let Some((x, y)) = frame.cursor {
                assert!(x < width && y < height);
            }
            for line in lines(&v) {
                assert!(line.width() <= width as usize);
            }
            key(&mut v, "escape");
        }
    }
    v.width = 80;
    v.height = 16;
    menu(&mut v, 0);
    let before = v.session.clone();
    key(&mut v, "end");
    let l = v.menu_layout();
    let selected = v.context_menu.as_ref().unwrap().selected;
    assert!(selected >= l.scroll && selected < l.scroll + l.visible);
    assert!(lines(&v).join("\n").contains("Remove"));
    v.event(Event {
        kind: "resize".into(),
        width: 16,
        height: 5,
        ..Event::default()
    });
    for line in lines(&v) {
        assert!(line.width() <= 16);
    }
    key(&mut v, "escape");
    // Resizing keeps the cell size; only the amount of visible time changes.
    let mut session = v.session.clone();
    (session.start, session.span) = (before.start, before.span);
    assert_eq!(session, before);
}

#[test]
fn drag_badge_tracks_the_pointer_indicates_invalid_drops_and_disappears() {
    let mut v = demo();
    let watch = signal(&v, "wdata");
    let row = v.rows().into_iter().find(|row| row.watch == watch).unwrap();
    let x = v.layout().browser + 6;
    let y = v.layout().body_y + row.line;
    pointer(&mut v, "down", x, y);
    let wave_x = v.layout().wave_x;
    pointer(&mut v, "drag", wave_x + 4, 8);
    let first = lines(&v);
    assert!(first[10].contains("Move  wdata[31:0]"));
    assert_eq!(
        v.signal_drag.as_ref().unwrap().pointer,
        (v.layout().wave_x + 4, 8)
    );
    pointer(&mut v, "drag", 7, 3);
    assert!(lines(&v)[5].contains("Cannot drop  wdata[31:0]"));
    let original = v.session.clone();
    key(&mut v, "escape");
    assert!(v.signal_drag.is_none());
    assert!(!lines(&v).join("\n").contains("Cannot drop"));
    assert_eq!(v.session, original);
    assert!(v.history.is_empty());
}
