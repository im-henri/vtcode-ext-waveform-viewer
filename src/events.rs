use crate::viewer::{
    BrowserNode, Drag, Event, Focus, Hover, Marker, Prompt, Session, Viewer, Watch,
};
use crate::{menus::MenuAction, model::Radix, viewer::WaveSize};
use unicode_width::UnicodeWidthStr;

pub struct Button {
    pub x: u16,
    pub width: u16,
    pub label: String,
    pub action: &'static str,
    pub hint: &'static str,
    pub active: bool,
    pub enabled: bool,
}

impl Viewer {
    /// Rendering and hit testing share the same adaptive toolbar geometry.
    pub fn toolbar(&self) -> Vec<Button> {
        let definitions = [
            (
                "Signals",
                "add_signals",
                "Find and add signals · /",
                self.layout().browser > 0,
                0,
            ),
            ("Fit", "fit", "Fit the entire recording · 0", false, 0),
            (
                "Detail",
                "detail",
                "Resolve the selected signal around cursor A · i",
                false,
                60,
            ),
            (
                "+",
                "zoom_in",
                "Zoom in around cursor A · + / Ctrl+wheel",
                false,
                0,
            ),
            (
                "−",
                "zoom_out",
                "Zoom out around cursor A · − / Ctrl+wheel",
                false,
                0,
            ),
            (
                "Back",
                "zoom_back",
                "Restore the previous zoom or range · Shift+Z",
                false,
                100,
            ),
            (
                "A–B",
                "fit_cursors",
                "Fit the time range between A and B · z",
                false,
                100,
            ),
            (
                "‹",
                "previous",
                "Previous transition · ← · Alt+click steps B",
                false,
                0,
            ),
            (
                "›",
                "next",
                "Next transition · → · Alt+click steps B",
                false,
                0,
            ),
            (
                "Find",
                "find",
                "Find a value in the selected signal · f",
                false,
                0,
            ),
            ("Group", "group", "Insert a signal group · g", false, 125),
            (
                "Mark",
                "marker",
                "Place a named marker at cursor A · m",
                false,
                100,
            ),
            (
                "Tree",
                "browser",
                "Show or hide the signal hierarchy · b",
                self.layout().browser > 0,
                60,
            ),
            (
                "Inspect",
                "inspector",
                "Show or hide the docked value inspector · v",
                self.inspector_height() > 0,
                100,
            ),
            (
                "Save",
                "save",
                "Save signals, cursors, markers and zoom · Ctrl+S",
                false,
                110,
            ),
            (
                "Reload",
                "reload",
                "Reload this recording and keep the view · F5",
                false,
                135,
            ),
        ];
        let width = self.width.min(512);
        let duration = self.wave.end.saturating_sub(self.wave.start).max(1);
        let mut buttons = Vec::new();
        let mut x: u16 = 1;
        for (label, action, hint, active, minimum) in definitions {
            if matches!(action, "group" | "marker" | "browser" | "save" | "reload") {
                continue;
            }
            if action == "inspector" && self.height < 30 {
                continue;
            }
            let label = match action {
                "previous" if width >= 120 => "‹Edge",
                "next" if width >= 120 => "Edge›",
                _ => label,
            };
            let group_gap =
                u16::from(matches!(action, "previous" | "find" | "browser" | "save")) * 2;
            let size = label.width() as u16 + 2;
            if width < minimum || x + group_gap + size + 5 > width {
                continue;
            }
            x += group_gap;
            buttons.push(Button {
                x,
                width: size,
                label: format!("[{label}]"),
                action,
                hint,
                active,
                enabled: self.loading.is_none()
                    && match action {
                        "previous" | "next" | "find" | "detail" => self.selected().is_some(),
                        "zoom_in" => self.session.span > 1,
                        "zoom_out" => self.session.span < duration,
                        "zoom_back" => !self.view_history.is_empty(),
                        "fit_cursors" => self.session.alt_cursor.is_some(),
                        _ => true,
                    },
            });
            x += size + 1;
        }
        if width >= 5 {
            buttons.push(Button {
                x: width - 4,
                width: 3,
                label: "[?]".into(),
                action: "help",
                hint: "Keyboard and mouse reference · ?",
                active: self.help,
                enabled: true,
            });
        }
        buttons
    }

    pub fn event(&mut self, mut event: Event) {
        if event.button == "middle" {
            event.alt = true;
            event.shift = false;
            event.control = false;
            event.ctrl = false;
        }
        // Passive inspection must not dismiss errors, change selection, or enter undo history.
        if self.prompt.is_none() && event.kind != "move" && event.kind != "leave" {
            self.error = false;
            if !(event.control && event.text.eq_ignore_ascii_case("c")
                || event.code == "command" && event.text == "copy"
                || event.code == "char" && event.text == "i")
            {
                self.hover = None;
            }
        }
        if event.width > 0 {
            self.width = event.width;
        }
        if event.height > 0 {
            self.height = event.height;
        }
        if event.kind == "resize" {
            let dragging = self.cancel_timeline_drag();
            let signal = self.signal_drag.take().is_some();
            if dragging || signal {
                self.status = "Drag cancelled".into();
            }
            if self.help {
                self.scroll_help(0);
            }
            if self.context_menu.is_some() {
                self.reveal_menu_selection();
            }
            self.ensure_selection();
            self.browser_scroll = self.browser_scroll.min(
                self.browser_nodes
                    .len()
                    .saturating_sub(usize::from(self.layout().body_height)),
            );
            return;
        }
        if self.prompt.is_some() {
            self.dialog_event(&event);
            return;
        }
        if event.kind == "contextMenu" {
            self.open_context_menu(&event);
            return;
        }
        if self.context_menu.is_some() {
            self.menu_event(&event);
            return;
        }
        if event.code == "function" && event.text == "10" && event.shift {
            self.open_keyboard_menu();
            return;
        }
        if event.direction != 0 {
            if self.control_drag.is_some() {
                return;
            }
            self.scroll_event(&event);
            return;
        }
        if !event.kind.is_empty() {
            self.pointer(&event);
            return;
        }
        let cancelled_drag = self.cancel_timeline_drag();
        if cancelled_drag {
            self.status = "Drag cancelled".into();
        }
        let cancelled_signal = self.signal_drag.take().is_some();
        if cancelled_signal {
            self.status = "Drag cancelled".into();
        }
        if event.code == "escape" {
            if cancelled_drag || cancelled_signal {
                return;
            }
            if self.prompt.take().is_some() {
                self.input.clear();
                return;
            }
            if self.help {
                self.help = false;
                return;
            }
            if self.search.take().is_some() || self.edge_search.take().is_some() {
                self.status = "Search cancelled".into();
                return;
            }
            if self.loading.take().is_some() {
                self.cancelled = true;
                self.status = "Loading cancelled · F5 retries".into();
                return;
            }
            self.drag = None;
            if self.wave_selection.is_some() {
                self.clear_wave_selection();
                self.status = "Waveform selected · Right-click for display options".into();
            }
            return;
        }
        if self.help {
            let page = self.help_layout().visible.max(1) as isize;
            match event.code.as_str() {
                "up" => self.scroll_help(-1),
                "down" => self.scroll_help(1),
                "pageUp" => self.scroll_help(-page),
                "pageDown" => self.scroll_help(page),
                "home" => self.help_scroll = 0,
                "end" => self.scroll_help(isize::MAX),
                _ => self.help = false,
            }
            return;
        }
        if event.code == "function" && event.text == "5" {
            self.reload_requested = true;
            return;
        }
        if self.loading.is_some() {
            return;
        }
        if event.code == "command" {
            match event.text.as_str() {
                "save" => self.save_requested = true,
                "find" => self.open_prompt(Prompt::Filter),
                "find_replace" => self.open_prompt(Prompt::FindValue),
                "goto_line" => self.open_prompt(Prompt::Goto),
                "undo" => self.undo(false),
                "redo" => self.undo(true),
                "find_next" => self.start_search(false),
                "find_prev" => self.start_search(true),
                "copy" => self.copy_value(),
                "paste" => self.open_prompt(Prompt::Import),
                "select_all" if self.focus == Focus::Waves => self.select_all_waves(),
                "documentChanged" => {}
                _ => {}
            }
            return;
        }
        if (event.control || event.ctrl)
            && !(event.shift
                && matches!(event.code.as_str(), "up" | "down" | "pageUp" | "pageDown"))
        {
            match event.text.to_ascii_lowercase().as_str() {
                "0" => self.fit(),
                "f" => self.open_prompt(Prompt::Filter),
                "h" => self.open_prompt(Prompt::FindValue),
                "g" => self.open_prompt(Prompt::Goto),
                "s" => self.save_requested = true,
                "z" => self.undo(event.shift),
                "y" => self.undo(true),
                "c" if event.shift => self.export_session(),
                "c" => self.copy_value(),
                "e" => self.export_session(),
                "v" => self.open_prompt(Prompt::Import),
                "a" if self.focus == Focus::Waves => self.select_all_waves(),
                _ => {}
            }
            return;
        }
        if event.code == "tab" || event.code == "backTab" {
            if self.layout().browser > 0 {
                self.focus = if self.focus == Focus::Browser {
                    Focus::Waves
                } else {
                    Focus::Browser
                };
            }
            return;
        }
        if event.code == "function" {
            match event.text.as_str() {
                "2" => self.open_prompt(Prompt::Rename),
                "3" => self.start_search(event.shift),
                _ => {}
            }
            return;
        }
        if event.text == "?" {
            self.help = true;
            self.help_scroll = 0;
            return;
        }
        if event.text == "/" {
            self.open_prompt(Prompt::Filter);
            return;
        }
        if self.focus == Focus::Browser {
            let count = self.browser_nodes.len();
            match event.code.as_str() {
                "up" => self.browser_selection = self.browser_selection.saturating_sub(1),
                "down" => {
                    self.browser_selection =
                        (self.browser_selection + 1).min(count.saturating_sub(1))
                }
                "home" => self.browser_selection = 0,
                "end" => self.browser_selection = count.saturating_sub(1),
                "pageUp" => self.browser_selection = self.browser_selection.saturating_sub(10),
                "pageDown" => {
                    self.browser_selection =
                        (self.browser_selection + 10).min(count.saturating_sub(1))
                }
                "enter" => self.browser_activate(event.shift),
                "right" => {
                    if let Some(BrowserNode::Scope(scope, _)) =
                        self.browser_nodes.get(self.browser_selection)
                    {
                        self.expanded_scopes.insert(*scope);
                        self.rebuild_browser();
                    }
                }
                "left" => {
                    if let Some(BrowserNode::Scope(scope, _)) =
                        self.browser_nodes.get(self.browser_selection)
                    {
                        self.expanded_scopes.remove(scope);
                        self.rebuild_browser();
                    }
                }
                "char" if event.text == " " => self.browser_activate(event.shift),
                _ => self.char_action(&event),
            }
            let visible = self.layout().body_height as usize;
            if self.browser_selection < self.browser_scroll {
                self.browser_scroll = self.browser_selection;
            }
            if self.browser_selection >= self.browser_scroll + visible {
                self.browser_scroll = (self.browser_selection + 1).saturating_sub(visible);
            }
            return;
        }
        match event.code.as_str() {
            "up" | "down" if event.alt => {
                if self.selected_bit.is_none() && !self.session.watches.is_empty() {
                    let next = if event.code == "up" {
                        self.selection.saturating_sub(1)
                    } else {
                        (self.selection + 1).min(self.session.watches.len() - 1)
                    };
                    if next != self.selection {
                        self.clear_wave_selection();
                        self.checkpoint();
                        self.session.watches.swap(next, self.selection);
                        self.selection = next;
                    }
                    self.ensure_selection();
                }
            }
            "up" | "down" | "pageUp" | "pageDown" => {
                let rows = self.rows();
                if !rows.is_empty() {
                    let current = rows
                        .iter()
                        .position(|r| r.watch == self.selection && r.bit == self.selected_bit)
                        .unwrap_or(0);
                    let distance = if event.code.starts_with("page") {
                        (self.layout().body_height / 2).max(1) as usize
                    } else {
                        1
                    };
                    let next = if matches!(event.code.as_str(), "up" | "pageUp") {
                        current.saturating_sub(distance)
                    } else {
                        (current + distance).min(rows.len() - 1)
                    };
                    self.select_wave(rows[next].into(), event.shift, event.control || event.ctrl);
                    self.ensure_selection();
                }
            }
            "left" | "right" if event.shift => self.pan(event.code == "right"),
            "left" | "right" => self.edge(event.code == "left", event.alt, None),
            "home" => {
                self.session.cursor = self.wave.start;
                self.session.start = self.wave.start;
            }
            "end" => {
                self.session.cursor = self.wave.end;
                self.session.start = self.wave.end.saturating_sub(self.session.span);
                self.clamp_window();
            }
            "delete" | "backspace" => self.remove_selected(),
            "enter" => self.toggle_expand(),
            "char" => self.char_action(&event),
            _ => {}
        }
    }

    fn char_action(&mut self, event: &Event) {
        match event.text.as_str() {
            "+" | "=" => self.zoom(true, self.session.cursor),
            "-" | "_" => self.zoom(false, self.session.cursor),
            "0" => self.fit(),
            "i" => self.inspect_detail(),
            "v" => self.action("inspector"),
            ":" => self.open_main_menu(crate::workbench::MainMenu::All),
            "x" => self.action("swap_cursors"),
            "n" | "N" => self.edge(event.shift || event.text == "N", event.alt, None),
            "p" => self.edge(true, event.alt, None),
            "u" => self.edge(event.shift, event.alt, Some('1')),
            "j" => self.edge(event.shift, event.alt, Some('0')),
            "r" | "R" => self.action("radix"),
            "a" => self.action("analog"),
            "h" => self.action("height"),
            "e" | " " => self.toggle_expand(),
            "b" => self.action("browser"),
            "B" => {
                self.session.alt_cursor = if event.shift && self.session.alt_cursor.is_some() {
                    None
                } else {
                    Some(self.session.cursor)
                }
            }
            "f" => self.open_prompt(Prompt::FindValue),
            "g" => self.open_prompt(Prompt::Group),
            "m" => self.open_prompt(Prompt::Marker),
            "M" => {
                self.checkpoint();
                self.session
                    .markers
                    .retain(|m| m.time != self.session.cursor);
            }
            "[" | "]" => {
                let current = self.session.cursor;
                let target = if event.text == "[" {
                    self.session
                        .markers
                        .iter()
                        .filter(|m| m.time < current)
                        .map(|m| m.time)
                        .max()
                } else {
                    self.session
                        .markers
                        .iter()
                        .filter(|m| m.time > current)
                        .map(|m| m.time)
                        .min()
                };
                if let Some(time) = target {
                    self.session.cursor = time;
                    self.reveal_cursor();
                }
            }
            "s" => {
                self.toggle_snap();
            }
            "z" if event.shift => self.zoom_back(),
            "z" => self.fit_cursors(),
            "Z" => self.zoom_back(),
            "d" => {
                if self.session.watches.len() < crate::viewer::MAX_WATCHES {
                    self.checkpoint();
                    self.session.watches.insert(
                        self.selection.min(self.session.watches.len()),
                        Watch::Divider,
                    );
                    self.clear_wave_selection();
                    self.selected_bit = None;
                    self.ensure_selection();
                }
            }
            "`" => self.session.full_names = !self.session.full_names,
            number if number.len() == 1 && matches!(number.as_bytes()[0], b'1'..=b'8') => {
                self.apply_selection_edit(MenuAction::Color(number.as_bytes()[0] - b'1'), true);
            }
            _ => {}
        }
    }
    pub fn action(&mut self, action: &str) {
        match action {
            "menu_file" => self.open_main_menu(crate::workbench::MainMenu::File),
            "menu_signal" => self.open_main_menu(crate::workbench::MainMenu::Signal),
            "menu_view" => self.open_main_menu(crate::workbench::MainMenu::View),
            "menu_waveform" => self.open_main_menu(crate::workbench::MainMenu::Waveform),
            "menu_cursors" => self.open_main_menu(crate::workbench::MainMenu::Cursors),
            "menu_tools" => self.open_main_menu(crate::workbench::MainMenu::Tools),
            "add_signals" => {
                self.open_prompt(Prompt::Filter);
                self.session.browser = true;
                self.focus = Focus::Browser;
            }
            "goto" => self.open_prompt(Prompt::Goto),
            "goto_b" => self.open_prompt(Prompt::GotoB),
            "b_at_a" => self.session.alt_cursor = Some(self.session.cursor),
            "clear_b" => self.session.alt_cursor = None,
            "swap_cursors" => {
                if let Some(b) = self.session.alt_cursor.replace(self.session.cursor) {
                    self.session.cursor = b;
                    self.status = "Cursors A and B exchanged · The value pane now follows A".into();
                } else {
                    self.session.alt_cursor = None;
                    self.status = "Set cursor B before exchanging A and B".into();
                }
            }
            "previous_b" => self.edge(true, true, None),
            "next_b" => self.edge(false, true, None),
            "rising" => self.edge(false, false, Some('1')),
            "falling" => self.edge(false, false, Some('0')),
            "snap" => self.toggle_snap(),
            "export" => self.export_session(),
            "import" => self.open_prompt(Prompt::Import),
            "undo" => self.undo(false),
            "redo" => self.undo(true),
            "remove" => self.remove_selected(),
            "expand" => self.toggle_expand(),
            "full_paths" => self.session.full_names = !self.session.full_names,
            "fit_names" => self.fit_columns(true, false, false),
            "fit_values" => self.fit_columns(false, true, false),
            "fit_columns" => self.fit_columns(true, true, false),
            "reset_columns" => self.fit_columns(false, false, true),
            "signal_display" => self.open_keyboard_menu(),
            "readable_heights" => {
                let changes = self.session.watches.iter().any(|watch| matches!(watch, Watch::Signal { size, bit_size, bit_sizes, .. } if *size != WaveSize::Normal || *bit_size != WaveSize::Normal || !bit_sizes.is_empty()));
                if changes {
                    self.checkpoint();
                }
                for watch in &mut self.session.watches {
                    if let Watch::Signal {
                        size,
                        bit_size,
                        bit_sizes,
                        ..
                    } = watch
                    {
                        *size = WaveSize::Normal;
                        *bit_size = WaveSize::Normal;
                        bit_sizes.clear();
                    }
                }
                self.ensure_selection();
                self.status = if changes {
                    "Normal wave heights applied · Ctrl+Z restores previous sizes"
                } else {
                    "All wave heights are already Normal"
                }
                .into();
            }
            "remove_marker" => {
                self.checkpoint();
                self.session
                    .markers
                    .retain(|marker| marker.time != self.session.cursor);
            }
            "divider" => {
                if self.session.watches.len() < crate::viewer::MAX_WATCHES {
                    self.checkpoint();
                    self.session.watches.insert(
                        self.selection.min(self.session.watches.len()),
                        Watch::Divider,
                    );
                    self.clear_wave_selection();
                    self.selected_bit = None;
                    self.ensure_selection();
                }
            }
            "fit" => self.fit(),
            "detail" => self.detail_at_cursor(),
            "zoom_back" => self.zoom_back(),
            "fit_cursors" => self.fit_cursors(),
            "inspector" => {
                self.session.inspector = !self.session.inspector;
                self.ensure_selection();
                self.status = if self.session.inspector {
                    "Inspector enabled · Docks in views at least 80 × 30"
                } else {
                    "Inspector hidden · Hover shows a value popup · v restores the dock"
                }
                .into();
            }
            "zoom_in" => self.zoom(true, self.session.cursor),
            "zoom_out" => self.zoom(false, self.session.cursor),
            "previous" => self.edge(true, false, None),
            "next" => self.edge(false, false, None),
            "radix" => {
                if let Some(key) = self.selected_edit_key(MenuAction::Radix(Radix::Hex))
                    && let Watch::Signal { radix, .. } = self.session.watches[key.watch]
                {
                    self.apply_selection_edit(MenuAction::Radix(radix.next()), true);
                }
            }
            "analog" => {
                if let Some(key) = self.selected_edit_key(MenuAction::Analog)
                    && let Watch::Signal { analog, .. } = self.session.watches[key.watch]
                {
                    self.apply_selection_edit(
                        if analog {
                            MenuAction::Digital
                        } else {
                            MenuAction::Analog
                        },
                        true,
                    );
                }
            }
            "height" => {
                if let Some(key) = self.selected_edit_key(MenuAction::Size(WaveSize::Normal)) {
                    let size = self.session.watches[key.watch].size(key.bit).next();
                    self.apply_selection_edit(MenuAction::Size(size), true);
                }
            }
            "find" => self.open_prompt(Prompt::FindValue),
            "group" => self.open_prompt(Prompt::Group),
            "marker" => self.open_prompt(Prompt::Marker),
            "save" => self.save_requested = true,
            "reload" => self.reload_requested = true,
            "browser" => {
                self.session.browser = self.layout().browser == 0;
                self.focus = if self.session.browser && self.width < 100 {
                    Focus::Browser
                } else {
                    Focus::Waves
                };
            }
            "help" => {
                self.help = !self.help;
                self.help_scroll = 0;
            }
            _ => {}
        }
    }
    pub(crate) fn toggle_expand(&mut self) {
        self.clear_wave_selection();
        self.checkpoint();
        if let Some(item) = self.session.watches.get_mut(self.selection) {
            match item {
                Watch::Signal { expanded, .. } => *expanded = !*expanded,
                Watch::Group { collapsed, .. } => *collapsed = !*collapsed,
                _ => {}
            }
        }
        self.selected_bit = None;
        self.ensure_selection();
    }
    pub(crate) fn remove_selected(&mut self) {
        self.delete_selected_waves();
    }
    pub(crate) fn copy_value(&mut self) {
        let (watch, bit, time) = self
            .hover
            .and_then(|h| h.row.map(|r| (r.watch, r.bit, self.hover_time(h))))
            .unwrap_or((self.selection, self.selected_bit, self.session.cursor));
        if let Some(value) = self.watch_value(watch, bit, time) {
            let radix = match self.session.watches[watch] {
                Watch::Signal {
                    radix,
                    bit: signal_bit,
                    ..
                } if bit.or(signal_bit).is_none() => radix,
                _ => crate::model::Radix::Binary,
            };
            self.copy_requested = Some(value.format(radix));
            self.status = if self.transition_values(watch, bit, time).is_some()
                || self
                    .hover
                    .is_some_and(|hover| self.hover_transition(hover).is_some())
            {
                "After value copied".into()
            } else {
                "Value copied".into()
            };
        }
    }
    fn export_session(&mut self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.session) {
            self.copy_requested = Some(json);
            self.status = "Session JSON copied · paste into a file to share".into();
        }
    }
    pub(crate) fn commit_prompt(&mut self, prompt: Prompt) -> Result<(), ()> {
        let input = self.input.trim().to_string();
        match prompt {
            Prompt::Filter => {
                self.status = format!(
                    "{} matching signals · Enter adds · Shift+Enter adds a scope",
                    self.browser_nodes.len()
                );
            }
            Prompt::FindValue => {
                self.search_query = input;
                self.start_search(false);
            }
            Prompt::Goto | Prompt::GotoB => match self.wave.timescale.parse_time(&input) {
                Ok(time) if time >= self.wave.start && time <= self.wave.end => {
                    if prompt == Prompt::GotoB {
                        self.session.alt_cursor = Some(time);
                    } else {
                        self.session.cursor = time;
                    }
                    self.reveal_time(time);
                }
                Ok(_) => {
                    self.fail("Time is outside the waveform");
                    return Err(());
                }
                Err(error) => {
                    self.fail(error);
                    return Err(());
                }
            },
            Prompt::Group => {
                if input.is_empty() || input.len() > 512 {
                    self.fail("Enter a group name of 1–512 bytes");
                    return Err(());
                }
                if self.session.watches.len() >= crate::viewer::MAX_WATCHES {
                    self.fail("Watch list is full");
                    return Err(());
                }
                self.checkpoint();
                self.session.watches.insert(
                    self.selection.min(self.session.watches.len()),
                    Watch::Group {
                        name: input,
                        collapsed: false,
                    },
                );
                self.selected_bit = None;
                self.clear_wave_selection();
                self.ensure_selection();
                self.status = "Group created · Ctrl+Z to undo".into();
            }
            Prompt::Rename => {
                if input.len() > 512 {
                    self.fail("Name is too long");
                    return Err(());
                }
                self.checkpoint();
                if let Some(watch) = self.session.watches.get_mut(self.selection) {
                    match watch {
                        Watch::Signal { label, .. } => *label = input,
                        Watch::Group { name, .. } => *name = input,
                        _ => {}
                    }
                }
            }
            Prompt::Marker => {
                if self.session.markers.len() >= 32 {
                    self.fail("At most 32 markers");
                    return Err(());
                }
                if input.len() > 512 {
                    self.fail("Marker name is too long");
                    return Err(());
                }
                self.checkpoint();
                let name = if input.is_empty() {
                    format!("M{}", self.session.markers.len() + 1)
                } else {
                    input
                };
                self.session.markers.push(Marker {
                    time: self.session.cursor,
                    name,
                });
                self.session.markers.sort_by_key(|m| m.time);
            }
            Prompt::Import => match serde_json::from_str::<Session>(&input) {
                Ok(session) => {
                    let before = self.session.clone();
                    if let Err(error) = self.restore(session) {
                        self.fail(error);
                        return Err(());
                    }
                    self.checkpoint_session(before);
                }
                Err(error) => {
                    self.fail(format!("Invalid session JSON: {error}"));
                    return Err(());
                }
            },
        }
        Ok(())
    }
    fn scroll_event(&mut self, event: &Event) {
        if self.help {
            self.scroll_help(if event.direction < 0 { -3 } else { 3 });
            return;
        }
        if self.prompt.is_some() || self.loading.is_some() {
            return;
        }
        let l = self.layout();
        let horizontal = event.shift || event.axis == "horizontal";
        if self.signal_drag.as_ref().is_some_and(|d| d.active)
            && event.view_column >= l.browser
            && !horizontal
            && !event.control
            && !event.ctrl
        {
            if let Some(drag) = &mut self.signal_drag {
                drag.pointer = (event.view_column, event.view_row);
            }
            self.scroll_drag(event.direction);
            return;
        }
        if event.control || event.ctrl {
            let anchor = self.time_at(event.view_column.max(l.wave_x));
            self.zoom(event.direction < 0, anchor);
        } else if horizontal || event.view_column >= l.wave_x {
            self.pan(event.direction > 0);
        } else if event.view_column < l.browser {
            let max = self
                .browser_nodes
                .len()
                .saturating_sub(l.body_height as usize);
            self.browser_scroll = if event.direction < 0 {
                self.browser_scroll.saturating_sub(3)
            } else {
                (self.browser_scroll + 3).min(max)
            };
        } else {
            let rows = self.rows();
            let max = rows
                .last()
                .map(|r| usize::from(r.line + r.height))
                .unwrap_or(0)
                .saturating_sub(l.body_height as usize);
            self.scroll = if event.direction < 0 {
                self.scroll.saturating_sub(3)
            } else {
                (self.scroll + 3).min(max)
            };
        }
    }

    fn begin_wave_drag(&mut self, event: &Event, watch: Option<usize>, split: bool) {
        let x = event.view_column;
        let pan = event.control || event.ctrl;
        self.drag = Some(Drag {
            x,
            start: self.session.start,
            span: self.session.span,
            time: self.time_at(x),
            end: self.time_at(x),
            end_x: x,
            zoom: event.shift,
            pan,
            watch,
            split,
            alt: event.alt,
        });
        if x >= self.layout().wave_x && !event.shift && !pan {
            self.set_cursor(x, event.alt);
        }
    }

    fn pointer(&mut self, event: &Event) {
        if event.kind == "cancel" {
            let signal = self.signal_drag.take().is_some();
            if self.cancel_timeline_drag() || signal {
                self.status = "Drag cancelled".into();
            }
            self.hover = None;
            return;
        }
        if event.kind == "leave" {
            self.hover = None;
            if let Some(drag) = &mut self.signal_drag {
                drag.target = None;
                drag.ticks = 0;
            }
            return;
        }
        if self.help {
            if event.kind == "down" {
                self.help = false;
            }
            return;
        }
        if self.prompt.is_some() || self.loading.is_some() {
            return;
        }
        let l = self.layout();
        let x = event.view_column;
        let y = event.view_row;
        if self.control_drag.is_some() && matches!(event.kind.as_str(), "drag" | "up" | "move") {
            self.update_control(event);
            return;
        }
        if self.overview_drag.is_some() && matches!(event.kind.as_str(), "drag" | "up" | "move") {
            self.update_overview_drag(x);
            if event.kind == "up" {
                self.overview_drag = None;
            }
            return;
        }
        if self.signal_drag.is_some() && matches!(event.kind.as_str(), "drag" | "up" | "move") {
            self.update_signal_drag(x, y, event.pointer_inside != Some(false));
            if event.kind == "up" {
                let drag = self.signal_drag.as_ref().unwrap();
                if !drag.active && drag.from_browser && x < l.browser {
                    self.signal_drag = None;
                    self.browser_activate(false);
                } else {
                    self.finish_signal_drag();
                }
            }
            return;
        }
        if event.kind == "move" {
            let row = (x >= l.browser && x < self.width.min(512))
                .then(|| self.row_at(y))
                .flatten()
                .filter(|r| matches!(self.session.watches[r.watch], Watch::Signal { .. }));
            self.hover = Some(Hover { x, y, row });
            return;
        }
        if event.kind == "down" {
            self.signal_drag = None;
            self.cancel_timeline_drag();
            if event.button == "middle" {
                if x >= l.wave_x
                    && x < l.wave_x + l.wave_width
                    && (y == 4 || y >= l.body_y && y < l.body_y + l.body_height)
                {
                    self.focus = Focus::Waves;
                    self.begin_wave_drag(event, None, false);
                }
                return;
            }
            if event.click_count == 2
                && x >= l.wave_x
                && let Some(row) = self.row_at(y)
            {
                let hover = Hover {
                    x,
                    y,
                    row: Some(row),
                };
                if self.hover_activity(hover).is_some() {
                    self.select_wave(row.into(), false, false);
                    self.hover = Some(hover);
                    self.inspect_detail();
                    return;
                }
            }
            if y == 0 {
                if let Some(menu) = self
                    .main_menus()
                    .into_iter()
                    .find(|b| x >= b.x && x < b.x + b.width)
                    .map(|b| b.menu)
                {
                    self.open_main_menu(menu);
                }
                return;
            }
            if self.begin_control(event) {
                return;
            }
            if y == 1 {
                for button in self.toolbar() {
                    if x >= button.x && x < button.x + button.width {
                        if button.enabled {
                            match button.action {
                                "previous" => self.edge(true, event.alt, None),
                                "next" => self.edge(false, event.alt, None),
                                _ => self.action(button.action),
                            }
                        }
                        return;
                    }
                }
                return;
            }
            if x < l.browser && (y == 3 || y == 4) {
                if y == 3 && x >= l.browser.saturating_sub(3) {
                    self.session.browser = false;
                    self.focus = Focus::Waves;
                } else if y == 4 && !self.filter.is_empty() && x >= l.browser.saturating_sub(3) {
                    self.filter.clear();
                    self.browser_selection = 0;
                    self.browser_scroll = 0;
                    self.rebuild_browser();
                    self.focus = Focus::Browser;
                    self.status = "Filter cleared · Drag a signal to place it".into();
                } else {
                    self.open_prompt(Prompt::Filter);
                }
                return;
            }
            if self.height >= 8
                && (self.height.min(200).saturating_sub(4)..=self.height.min(200).saturating_sub(3))
                    .contains(&y)
                && x >= l.wave_x
                && x < l.wave_x + l.wave_width
            {
                self.begin_overview_drag(x);
                return;
            }
            if y == 4 && x >= l.wave_x && x < l.wave_x + l.wave_width {
                self.focus = Focus::Waves;
                self.begin_wave_drag(event, None, false);
                return;
            }
            if y < l.body_y || y >= l.body_y + l.body_height {
                return;
            }
            if x < l.browser {
                self.focus = Focus::Browser;
                self.browser_selection = self.browser_scroll + usize::from(y - l.body_y);
                if let Some(BrowserNode::Signal(id, _)) =
                    self.browser_nodes.get(self.browser_selection)
                {
                    self.start_signal_drag(*id, None, x, y);
                } else {
                    self.browser_activate(event.shift);
                }
                return;
            }
            self.focus = Focus::Waves;
            let row_line = self.scroll + usize::from(y - l.body_y);
            let row = self
                .rows()
                .into_iter()
                .find(|r| row_line >= r.line as usize && row_line < (r.line + r.height) as usize);
            let split = x == l.browser + l.name.saturating_sub(1);
            let control = event.control || event.ctrl;
            if let Some(row) = row {
                if x < l.wave_x && !split {
                    self.select_wave(row.into(), event.shift, control);
                    if event.shift || control {
                        self.ensure_selection();
                        return;
                    }
                } else if !split && !event.shift && !control {
                    self.select_wave(row.into(), false, false);
                }
                if x < l.browser + 3 && row.bit.is_none() && !event.shift && !control {
                    self.toggle_expand();
                    return;
                }
            }
            if x < l.wave_x && !split && row.is_none() {
                return;
            }
            if x < l.wave_x
                && !split
                && !event.control
                && let Some(id) = self.signal_id(self.selection)
            {
                self.start_signal_drag(id, Some(self.selection), x, y);
                return;
            }
            self.begin_wave_drag(
                event,
                (x < l.wave_x && !split).then_some(self.selection),
                split,
            );
        } else if matches!(event.kind.as_str(), "drag" | "up")
            && let Some(drag) = self.drag
        {
            let end = self.time_at(x);
            if let Some(active) = &mut self.drag {
                active.end = end;
                active.end_x = x.clamp(l.wave_x, l.wave_x + l.wave_width.saturating_sub(1));
            }
            if drag.split {
                self.session.name_width = x.saturating_sub(l.browser).clamp(12, 80);
            } else if drag.pan {
                let shift = ((u128::from(x.abs_diff(drag.x)) * u128::from(drag.span))
                    / u128::from(l.wave_width.max(1))) as u64;
                self.session.start = if x >= drag.x {
                    drag.start.saturating_sub(shift)
                } else {
                    drag.start.saturating_add(shift)
                };
                self.clamp_window();
            } else if drag.watch.is_none() && !drag.zoom {
                self.set_cursor(x, drag.alt);
            }
        }
        if event.kind == "up"
            && let Some(drag) = self.drag.take()
        {
            if drag.zoom
                && !drag.pan
                && !drag.split
                && drag.watch.is_none()
                && drag.x.abs_diff(x) >= 2
            {
                let end = self.time_at(x);
                self.remember_view();
                self.session.start = end.min(drag.time);
                self.session.span = end.abs_diff(drag.time).max(1);
                self.clamp_window();
            } else if let Some(watch) = drag.watch {
                let line = self.scroll + usize::from(y.saturating_sub(l.body_y));
                if let Some(target) = self
                    .rows()
                    .iter()
                    .find(|r| line >= r.line as usize && line < (r.line + r.height) as usize)
                    .copied()
                    && target.watch != watch
                    && watch < self.session.watches.len()
                {
                    self.checkpoint();
                    let item = self.session.watches.remove(watch);
                    self.session
                        .watches
                        .insert(target.watch.min(self.session.watches.len()), item);
                    self.selection = target.watch.min(self.session.watches.len() - 1);
                    self.selected_bit = None;
                    self.clear_wave_selection();
                }
            }
        }
    }
}
