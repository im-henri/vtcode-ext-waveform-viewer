//! Native preview of exactly the same model, input handling and frames as Wasm.
use crossterm::{
    cursor,
    event::{self, Event as TerminalEvent, KeyCode, KeyModifiers, MouseEventKind},
    execute, terminal,
};
use std::io::{self, Write};
use vtcode_waveform_viewer::{
    vcd,
    viewer::{Event, Viewer, WaveSize},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .iter()
        .skip(1)
        .find(|s| !s.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "examples/demo.vcd".into());
    let bytes = std::fs::read(&path)?;
    let wave = if path.ends_with(".fst") {
        vtcode_waveform_viewer::fst::parse(bytes)?
    } else {
        vcd::parse(&bytes)?
    };
    let mut viewer = Viewer::new(path, None);
    viewer.ready(wave);
    if let Ok(value) = std::env::var("WAVE_BROWSER") {
        viewer.session.browser = value != "0";
    }
    if let Ok(value) = std::env::var("WAVE_INSPECTOR") {
        viewer.session.inspector = value != "0";
    }
    if let Ok(size) = std::env::var("WAVE_SIZE") {
        let size = match size.to_lowercase().as_str() {
            "small" => WaveSize::Small,
            "normal" => WaveSize::Normal,
            _ => return Err("WAVE_SIZE must be small or normal".into()),
        };
        for watch in &mut viewer.session.watches {
            watch.set_size(None, size);
        }
    }
    if args
        .iter()
        .any(|s| s == "--text" || s == "--html" || s == "--json")
    {
        viewer.width = std::env::var("WAVE_WIDTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(150);
        viewer.height = std::env::var("WAVE_HEIGHT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(38);
        viewer.session.cursor = 240;
        viewer.session.alt_cursor = Some(420);
        if let Ok(width) = std::env::var("WAVE_VALUE_WIDTH") {
            viewer.session.value_width = width.parse()?;
        }
        if let Ok(cursor) = std::env::var("WAVE_CURSOR") {
            viewer.session.cursor = cursor.parse()?;
        }
        if let Ok(cursor) = std::env::var("WAVE_CURSOR_B") {
            viewer.session.alt_cursor = Some(cursor.parse()?);
        }
        if let Ok(start) = std::env::var("WAVE_START") {
            viewer.session.start = start.parse()?;
        }
        if let Ok(span) = std::env::var("WAVE_SPAN") {
            viewer.session.span = span.parse()?;
        }
        viewer.clamp_window();
        if std::env::var("WAVE_DETAIL").is_ok() {
            viewer.action("detail");
        }
        if let Ok(selection) = std::env::var("WAVE_SELECT") {
            for (position, index) in selection.split(',').enumerate() {
                let index: usize = index.trim().parse()?;
                let row = viewer
                    .rows()
                    .into_iter()
                    .find(|row| row.watch == index && row.bit.is_none())
                    .ok_or("Unknown WAVE_SELECT row")?;
                let l = viewer.layout();
                for kind in ["down", "up"] {
                    viewer.event(Event {
                        kind: kind.into(),
                        control: position > 0,
                        view_column: l.browser + 7,
                        view_row: l.body_y + row.line,
                        ..Event::default()
                    });
                }
            }
        }
        if let Ok(name) = std::env::var("WAVE_DRAG_SIGNAL") {
            viewer.session.browser = true;
            viewer.focus = vtcode_waveform_viewer::viewer::Focus::Browser;
            viewer.expanded_scopes.extend(0..viewer.wave.scopes.len());
            viewer.rebuild_browser();
            let node = viewer.browser_nodes.iter().position(|node| {
                matches!(node, vtcode_waveform_viewer::viewer::BrowserNode::Signal(id, _)
                    if viewer.wave.signals[*id].name == name || viewer.wave.signals[*id].path == name)
            }).ok_or("Unknown WAVE_DRAG_SIGNAL")?;
            let l = viewer.layout();
            viewer.browser_scroll =
                node.saturating_sub(usize::from(l.body_height.saturating_sub(1)));
            viewer.event(Event {
                kind: "down".into(),
                view_column: 6,
                view_row: l.body_y + (node - viewer.browser_scroll) as u16,
                ..Event::default()
            });
            viewer.event(Event {
                kind: "drag".into(),
                view_column: std::env::var("WAVE_DRAG_X")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(l.wave_x + 8),
                view_row: std::env::var("WAVE_DRAG_Y")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(l.body_y + 4),
                ..Event::default()
            });
        }
        if let (Ok(x), Ok(y)) = (std::env::var("WAVE_HOVER_X"), std::env::var("WAVE_HOVER_Y")) {
            viewer.event(Event {
                kind: "move".into(),
                view_column: x.parse()?,
                view_row: y.parse()?,
                ..Event::default()
            });
        }
        if let Ok(name) = std::env::var("WAVE_HOVER_SIGNAL") {
            let watch = viewer.session.watches.iter().position(|watch| matches!(watch, vtcode_waveform_viewer::viewer::Watch::Signal { path, .. } if viewer.wave.signals[viewer.signal_index[path]].name == name || *path == name)).ok_or("Unknown WAVE_HOVER_SIGNAL")?;
            viewer.selection = watch;
            viewer.ensure_selection();
            let row = viewer
                .rows()
                .into_iter()
                .find(|row| row.watch == watch && row.bit.is_none())
                .unwrap();
            let l = viewer.layout();
            let region = std::env::var("WAVE_HOVER_REGION").unwrap_or_default();
            let x = match region.as_str() {
                "name" => l.browser + 3,
                "value" => l.wave_x.saturating_sub(3),
                _ => std::env::var("WAVE_HOVER_TIME")
                    .ok()
                    .and_then(|time| time.parse().ok())
                    .and_then(|time| viewer.column_at(time))
                    .unwrap_or(l.wave_x + l.wave_width / 2),
            };
            viewer.event(Event {
                kind: "move".into(),
                view_column: x,
                view_row: l.body_y + row.line + row.height / 2 - viewer.scroll as u16,
                ..Event::default()
            });
        }
        if let (Ok(start), Ok(end)) = (
            std::env::var("WAVE_RANGE_START"),
            std::env::var("WAVE_RANGE_END"),
        ) {
            let first = viewer
                .column_at(start.parse()?)
                .ok_or("Range start outside viewport")?;
            let last = viewer
                .column_at(end.parse()?)
                .ok_or("Range end outside viewport")?;
            for (kind, x) in [("down", first), ("drag", last)] {
                viewer.event(Event {
                    kind: kind.into(),
                    shift: true,
                    view_column: x,
                    view_row: viewer.layout().body_y + 2,
                    ..Event::default()
                });
            }
        }
        if std::env::var("WAVE_HELP").is_ok() {
            viewer.action("help");
            viewer.help_scroll = std::env::var("WAVE_HELP_SCROLL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
        if let Ok(name) = std::env::var("WAVE_MENU_SIGNAL") {
            viewer.selection = viewer
                .session
                .watches
                .iter()
                .position(|watch| {
                    matches!(watch, vtcode_waveform_viewer::viewer::Watch::Signal { path, .. }
                    if viewer.wave.signals[viewer.signal_index[path]].name == name || *path == name)
                })
                .ok_or("Unknown WAVE_MENU_SIGNAL")?;
            viewer.ensure_selection();
            viewer.event(Event {
                code: "function".into(),
                text: "10".into(),
                shift: true,
                ..Event::default()
            });
        }
        if std::env::var("WAVE_COLUMN_MENU").is_ok() {
            viewer.event(Event {
                kind: "contextMenu".into(),
                view_column: viewer.layout().browser + 7,
                view_row: 3,
                ..Event::default()
            });
        }
        if let Ok(name) = std::env::var("WAVE_MAIN_MENU") {
            let menu = vtcode_waveform_viewer::workbench::MainMenu::ALL
                .into_iter()
                .find(|menu| menu.name().eq_ignore_ascii_case(&name))
                .ok_or("Unknown WAVE_MAIN_MENU")?;
            viewer.open_main_menu(menu);
        }
        if let Ok(query) = std::env::var("WAVE_QUERY") {
            use vtcode_waveform_viewer::viewer::Prompt;
            viewer.open_prompt(match query.as_str() {
                "group" => Prompt::Group,
                "marker" => Prompt::Marker,
                "goto" => Prompt::Goto,
                "goto_b" => Prompt::GotoB,
                "filter" => Prompt::Filter,
                "rename" => Prompt::Rename,
                "import" => Prompt::Import,
                _ => Prompt::FindValue,
            });
            if let Ok(text) = std::env::var("WAVE_QUERY_TEXT") {
                viewer.event(Event {
                    code: "paste".into(),
                    text,
                    ..Event::default()
                });
            }
            if std::env::var("WAVE_QUERY_SUBMIT").is_ok() {
                viewer.event(Event {
                    code: "enter".into(),
                    ..Event::default()
                });
            }
        }
        let frame = viewer.render();
        if args.iter().any(|s| s == "--json") {
            println!("{}", serde_json::to_string(&frame)?);
        } else if args.iter().any(|s| s == "--html") {
            println!(
                "<!doctype html><html><meta charset='utf-8'><title>VTCode Waveform Viewer</title><style>body{{margin:24px;background:#14171c;color:#d4d4d4}}pre{{font:14px/18px 'Cascadia Mono','Consolas',monospace;display:inline-block;background:#1e1e1e;padding:14px;box-shadow:0 12px 40px #0006;border:1px solid #34383f;border-radius:6px}}.muted{{color:#9098a3}}.accent{{color:#69b8ff}}.error{{color:#f48771}}.warning{{color:#e8c468}}.success,.chart1{{color:#6acb91}}.chart2{{color:#6ab8ff}}.chart3{{color:#e8c468}}.chart4{{color:#c297eb}}.chart5{{color:#ef91b5}}.chart6{{color:#68ccc8}}.chart7{{color:#eaaa79}}.chart8{{color:#b8c57d}}.bg-header,.bg-panel{{background:#252a31}}.bg-selection{{background:#303e51}}.bg-cursor{{background:#69b8ff;color:#14171c}}.bg-match{{background:#685525}}</style><pre>"
            );
            for line in frame.lines {
                for span in line {
                    print!(
                        "<span class='{} bg-{}'{}>{}</span>",
                        span.role,
                        span.background,
                        if span.bold {
                            " style='font-weight:bold'"
                        } else {
                            ""
                        },
                        html(&span.text)
                    );
                }
                println!();
            }
            println!("</pre></html>");
        } else {
            for line in frame.lines {
                println!(
                    "{}",
                    line.iter().map(|s| s.text.as_str()).collect::<String>()
                );
            }
        }
        return Ok(());
    }
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    execute!(
        io::stdout(),
        terminal::EnterAlternateScreen,
        event::EnableMouseCapture,
        cursor::Hide
    )?;
    let mut last_click: Option<(event::MouseButton, u16, u16, std::time::Instant, u8)> = None;
    loop {
        let (width, height) = terminal::size()?;
        viewer.width = width;
        viewer.height = height;
        draw(&viewer)?;
        let mut input = Event::default();
        if (viewer.search.is_some() || viewer.edge_search.is_some() || viewer.needs_drag_tick())
            && !event::poll(std::time::Duration::from_millis(16))?
        {
            viewer.search_step();
            viewer.drag_tick();
            continue;
        }
        match event::read()? {
            TerminalEvent::Key(key) => {
                if key.kind == event::KeyEventKind::Release {
                    continue;
                }
                if key.code == KeyCode::Char('q') && viewer.prompt.is_none()
                    || key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    break;
                }
                input.shift = key.modifiers.contains(KeyModifiers::SHIFT);
                input.control = key.modifiers.contains(KeyModifiers::CONTROL);
                input.alt = key.modifiers.contains(KeyModifiers::ALT);
                let (code, text) = match key.code {
                    KeyCode::Char(ch) => ("char", ch.to_string()),
                    KeyCode::F(n) => ("function", n.to_string()),
                    KeyCode::Left => ("left", String::new()),
                    KeyCode::Right => ("right", String::new()),
                    KeyCode::Up => ("up", String::new()),
                    KeyCode::Down => ("down", String::new()),
                    KeyCode::Enter => ("enter", String::new()),
                    KeyCode::Esc => ("escape", String::new()),
                    KeyCode::Delete => ("delete", String::new()),
                    KeyCode::Backspace => ("backspace", String::new()),
                    KeyCode::Tab => ("tab", String::new()),
                    KeyCode::BackTab => ("backTab", String::new()),
                    KeyCode::Home => ("home", String::new()),
                    KeyCode::End => ("end", String::new()),
                    KeyCode::PageUp => ("pageUp", String::new()),
                    KeyCode::PageDown => ("pageDown", String::new()),
                    _ => continue,
                };
                input.code = code.into();
                input.text = text;
            }
            TerminalEvent::Mouse(mouse) => {
                input.view_column = mouse.column;
                input.view_row = mouse.row;
                input.control = mouse.modifiers.contains(KeyModifiers::CONTROL);
                input.shift = mouse.modifiers.contains(KeyModifiers::SHIFT);
                input.alt = mouse.modifiers.contains(KeyModifiers::ALT);
                let button = match mouse.kind {
                    MouseEventKind::Down(button)
                    | MouseEventKind::Up(button)
                    | MouseEventKind::Drag(button) => Some(button),
                    _ => None,
                };
                if let Some(button) = button {
                    input.button = match button {
                        event::MouseButton::Left => "left",
                        event::MouseButton::Middle => "middle",
                        event::MouseButton::Right => "right",
                    }
                    .into();
                    if matches!(mouse.kind, MouseEventKind::Down(_)) {
                        let now = std::time::Instant::now();
                        input.click_count = last_click
                            .filter(|&(b, x, y, at, count)| {
                                b == button
                                    && x == mouse.column
                                    && y == mouse.row
                                    && now.duration_since(at)
                                        <= std::time::Duration::from_millis(400)
                                    && count < 3
                            })
                            .map_or(1, |(_, _, _, _, count)| count + 1);
                        last_click =
                            Some((button, mouse.column, mouse.row, now, input.click_count));
                    } else if matches!(mouse.kind, MouseEventKind::Drag(_)) {
                        last_click = None;
                    }
                }
                match mouse.kind {
                    MouseEventKind::Down(event::MouseButton::Left | event::MouseButton::Middle) => {
                        input.kind = "down".into()
                    }
                    MouseEventKind::Down(event::MouseButton::Right) => {
                        input.kind = "contextMenu".into()
                    }
                    MouseEventKind::Up(event::MouseButton::Left | event::MouseButton::Middle) => {
                        input.kind = "up".into()
                    }
                    MouseEventKind::Drag(event::MouseButton::Left | event::MouseButton::Middle) => {
                        input.kind = "drag".into()
                    }
                    MouseEventKind::Up(_) | MouseEventKind::Drag(_) => {
                        continue;
                    }
                    MouseEventKind::Moved => input.kind = "move".into(),
                    MouseEventKind::ScrollUp => input.direction = -1,
                    MouseEventKind::ScrollDown => input.direction = 1,
                    MouseEventKind::ScrollLeft => {
                        input.direction = -1;
                        input.axis = "horizontal".into();
                    }
                    MouseEventKind::ScrollRight => {
                        input.direction = 1;
                        input.axis = "horizontal".into();
                    }
                }
            }
            TerminalEvent::Resize(width, height) => {
                input.width = width;
                input.height = height;
                input.kind = "resize".into();
            }
            TerminalEvent::Paste(text) => {
                input.code = "paste".into();
                input.text = text;
            }
            _ => continue,
        }
        viewer.event(input);
        if std::mem::take(&mut viewer.reload_requested) {
            let result = std::fs::read(&viewer.path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    if viewer.path.to_ascii_lowercase().ends_with(".fst") {
                        vtcode_waveform_viewer::fst::parse(bytes)
                    } else {
                        vcd::parse(&bytes)
                    }
                });
            match result {
                Ok(wave) => {
                    viewer.pending_session = Some(viewer.session.clone());
                    viewer.ready(wave);
                }
                Err(error) => viewer.fail(error),
            }
        }
        if viewer.save_requested {
            viewer.save_requested = false;
            std::fs::write(
                "waveform-session.json",
                serde_json::to_string_pretty(&viewer.session)?,
            )?;
            viewer.status = "Saved waveform-session.json".into();
        }
    }
    Ok(())
}
fn html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn draw(viewer: &Viewer) -> io::Result<()> {
    let mut stdout = io::stdout();
    let frame = viewer.render();
    execute!(stdout, cursor::MoveTo(0, 0), cursor::Hide)?;
    for (y, line) in frame.lines.iter().enumerate() {
        execute!(stdout, cursor::MoveTo(0, y as u16))?;
        for span in line {
            let color = match span.role.as_str() {
                "accent" | "chart2" => "106;184;255",
                "muted" => "144;152;163",
                "success" | "chart1" => "106;203;145",
                "warning" | "chart3" => "232;196;104",
                "error" => "244;135;113",
                "chart4" => "194;151;235",
                "chart5" => "239;145;181",
                "chart6" => "104;204;200",
                "chart7" => "234;170;121",
                "chart8" => "184;197;125",
                _ => "212;212;212",
            };
            let background = match span.background.as_str() {
                "panel" | "header" => "37;42;49",
                "selection" => "48;62;81",
                "cursor" => "55;95;130",
                "match" => "104;85;37",
                _ => "30;30;30",
            };
            write!(
                stdout,
                "\x1b[0;38;2;{color};48;2;{background}m{}{}",
                if span.bold { "\x1b[1m" } else { "" },
                span.text
            )?;
        }
    }
    write!(stdout, "\x1b[0m")?;
    if let Some((x, y)) = frame.cursor {
        execute!(stdout, cursor::MoveTo(x, y), cursor::Show)?;
    }
    stdout.flush()
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            event::DisableMouseCapture,
            terminal::LeaveAlternateScreen,
            cursor::Show
        );
    }
}
