use vtcode_waveform_viewer::{
    menus::{MenuAction, MenuTarget},
    model::Radix,
    render::display_value,
    vcd,
    viewer::{Event, Focus, Prompt, Viewer, Watch, WaveSize},
    workbench::MainMenu,
};

fn viewer() -> Viewer {
    let mut v = Viewer::new("demo.vcd".into(), None);
    v.ready(vcd::parse(include_bytes!("../examples/demo.vcd")).unwrap());
    v.width = 160;
    v.height = 44;
    // As after a real resize: whole cells for this width.
    v.clamp_window();
    v
}
fn lines(v: &Viewer) -> Vec<String> {
    v.render()
        .lines
        .iter()
        .map(|line| line.iter().map(|span| span.text.as_str()).collect())
        .collect()
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
fn pointer(v: &mut Viewer, kind: &str, x: u16, y: u16) {
    v.event(Event {
        kind: kind.into(),
        view_column: x,
        view_row: y,
        ..Event::default()
    });
}

#[test]
fn workbench_opens_with_readable_traces_and_a_large_waveform_pane() {
    let v = viewer();
    let l = v.layout();
    assert_eq!((l.browser, v.inspector_height()), (0, 0));
    assert!(l.wave_width > v.width / 2);
    assert_eq!(v.rows()[0].height, 2);
    assert_eq!(v.rows()[4].height, 3);
    assert!(v.session.watches.iter().all(|watch| matches!(
        watch,
        Watch::Signal {
            color: 0,
            size: WaveSize::Normal,
            ..
        }
    )));
    let frame = lines(&v);
    for menu in MainMenu::ALL {
        assert!(frame[0].contains(menu.name()));
    }
    assert!(frame.iter().any(|line| line.contains("Recording 0 ns")));
    assert!(
        v.render()
            .pointer_buttons
            .iter()
            .any(|button| button == "middle")
    );
}

#[test]
fn initial_aliases_are_deduplicated_but_can_be_added_explicitly() {
    let mut v = Viewer::new("aliases.vcd".into(), None);
    v.ready(vcd::parse(b"$var wire 1 ! clk $end $var wire 1 ! clk_alias $end $var wire 1 # ready $end $enddefinitions $end #0 0! 1# #1000").unwrap());
    assert_eq!(v.wave.signals.len(), 3);
    assert_eq!(v.session.watches.len(), 2);
    v.add_signal(1);
    assert_eq!(v.session.watches.len(), 3);
}

#[test]
fn command_menus_navigate_and_create_one_undoable_edit() {
    let mut v = viewer();
    let original = v.session.clone();
    let button = v
        .main_menus()
        .into_iter()
        .find(|b| b.menu == MainMenu::Signal)
        .unwrap();
    pointer(&mut v, "down", button.x + 1, 0);
    pointer(&mut v, "up", button.x + 1, 0);
    key(&mut v, "right");
    assert!(matches!(
        v.context_menu.as_ref().unwrap().target,
        MenuTarget::Main(MainMenu::View)
    ));
    assert!(
        !v.menu_items()
            .iter()
            .find(|item| item.action == MenuAction::Command("fit_cursors"))
            .unwrap()
            .enabled
    );
    key(&mut v, "left");
    let at = v
        .menu_items()
        .iter()
        .position(|item| item.action == MenuAction::Command("group"))
        .unwrap();
    let menu = v.menu_layout();
    pointer(&mut v, "down", menu.items.x + 2, menu.items.y + at as u16);
    assert_eq!(v.prompt, Some(Prompt::Group));
    paste(&mut v, "CPU control");
    key(&mut v, "enter");
    assert!(matches!(&v.session.watches[0], Watch::Group { name, .. } if name == "CPU control"));
    assert_eq!(v.history.len(), 1);
    v.undo(false);
    assert_eq!(v.session, original);
    v.width = 48;
    pointer(&mut v, "down", 2, 0);
    assert!(matches!(
        v.context_menu.as_ref().unwrap().target,
        MenuTarget::Main(MainMenu::All)
    ));
}

#[test]
fn inline_signal_search_browses_cancels_and_adds_with_one_enter() {
    let mut v = viewer();
    let original = v.session.clone();
    v.action("add_signals");
    paste(&mut v, "data");
    assert_eq!(v.dialog_layout().input.y, 4);
    assert!(v.layout().browser > 0);
    key(&mut v, "down");
    assert_eq!(v.browser_selection, 1);
    key(&mut v, "escape");
    assert_eq!(v.session, original);
    assert!(v.filter.is_empty());
    v.action("add_signals");
    paste(&mut v, "shift_reg");
    key(&mut v, "enter");
    assert!(v.prompt.is_none());
    assert_eq!(v.focus, Focus::Waves);
    assert_eq!(v.session.watches.len(), original.watches.len() + 1);
    assert!(
        matches!(&v.session.watches[v.selection], Watch::Signal { path, .. } if path.contains("shift_reg"))
    );
    assert_eq!(v.history.len(), 1);
}

#[test]
fn middle_button_drags_b_and_editing_b_never_changes_a() {
    let mut v = viewer();
    v.session.cursor = 11;
    v.session.snap = false;
    let l = v.layout();
    let span = v.session.span;
    for (kind, dx) in [("down", 10), ("drag", 20), ("up", 30)] {
        v.event(Event {
            kind: kind.into(),
            button: "middle".into(),
            view_column: l.wave_x + dx,
            view_row: l.body_y,
            shift: true,
            control: true,
            ..Event::default()
        });
    }
    assert_eq!(v.session.cursor, 11);
    assert_eq!(v.session.alt_cursor, Some(v.time_at(l.wave_x + 30)));
    assert_eq!(v.session.span, span);
    let readout = &lines(&v)[2];
    let x = readout[..readout.find("[B ").unwrap()].chars().count() as u16 + 1;
    pointer(&mut v, "down", x, 2);
    assert_eq!(v.prompt, Some(Prompt::GotoB));
    paste(&mut v, "31 ns");
    key(&mut v, "enter");
    assert_eq!((v.session.cursor, v.session.alt_cursor), (11, Some(31)));
}

#[test]
fn double_clicking_a_compressed_cell_resolves_it_without_moving_b() {
    let mut v = Viewer::new("pulse.vcd".into(), None);
    v.ready(vcd::parse(b"$timescale 1 ns $end $var wire 1 ! clk $end $enddefinitions $end #0 0! #11 1! #12 0! #1000").unwrap());
    v.width = 120;
    v.height = 32;
    v.session.snap = false;
    v.session.alt_cursor = Some(31);
    let l = v.layout();
    let x = v.column_at(12).unwrap();
    pointer(&mut v, "down", x, l.body_y);
    pointer(&mut v, "up", x, l.body_y);
    let a = v.session.cursor;
    assert!(lines(&v)[usize::from(l.body_y)].contains('▒'));
    v.event(Event {
        kind: "down".into(),
        button: "left".into(),
        click_count: 2,
        view_column: x,
        view_row: l.body_y,
        ..Event::default()
    });
    assert_eq!(v.cell_ticks(), 1);
    assert_eq!((v.session.cursor, v.session.alt_cursor), (a, Some(31)));
    assert!(!lines(&v)[usize::from(l.body_y)].contains('▒'));
}

#[test]
fn detail_zoom_makes_bus_values_readable_as_well_as_digital_edges() {
    let mut v = viewer();
    v.session.cursor = 240;
    v.action("detail");
    let l = v.layout();
    let row = v.rows().into_iter().find(|row| row.watch == 5).unwrap();
    let value = display_value(&v.watch_value(5, None, 240).unwrap(), Radix::Hex);
    let plot: String = lines(&v)[usize::from(l.body_y + row.line + 1)]
        .chars()
        .skip(usize::from(l.wave_x))
        .collect();
    assert!(plot.contains(&value), "{value}: {plot}");
    assert!(lines(&v)[0].contains("Exact"));
    assert!(v.session.span < 200);
}
