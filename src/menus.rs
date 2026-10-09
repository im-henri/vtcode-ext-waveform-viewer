//! Signal-local commands, available at the signal rather than in the global toolbar.
use crate::dialogs::PopupRect;
use crate::model::Radix;
use crate::render::{Canvas, draw_panel, ellipsize};
use crate::selection::RowKey;
use crate::viewer::{
    BrowserNode, Event, Focus, MAX_WATCHES, PALETTE, Prompt, Viewer, Watch, WaveSize,
};

#[derive(Clone, Copy, Debug)]
pub enum MenuTarget {
    Watch { watch: usize, bit: Option<u32> },
    Browser { signal: usize },
    Columns,
}
#[derive(Clone, Debug)]
pub struct ContextMenu {
    pub target: MenuTarget,
    pub anchor: (u16, u16),
    pub selected: usize,
    pub scroll: usize,
    pub colors: bool,
    pub scroll_drag: Option<(u16, usize, u16)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Heading,
    Add,
    Radix(Radix),
    Digital,
    Analog,
    Size(WaveSize),
    Expand,
    Colors,
    Color(u8),
    Back,
    Find,
    Rename,
    Copy,
    Remove,
    FitNames,
    FitValues,
    ResetColumns,
}
pub struct MenuItem {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub action: MenuAction,
    pub enabled: bool,
    pub checked: bool,
    pub mixed: bool,
}
impl MenuItem {
    fn new(
        label: &'static str,
        shortcut: &'static str,
        action: MenuAction,
        enabled: bool,
        checked: bool,
    ) -> Self {
        Self {
            label,
            shortcut,
            action,
            enabled,
            checked,
            mixed: false,
        }
    }
    fn heading(label: &'static str) -> Self {
        Self::new(label, "", MenuAction::Heading, false, false)
    }
}
pub struct MenuLayout {
    pub panel: PopupRect,
    pub items: PopupRect,
    pub visible: usize,
    pub scroll: usize,
}

impl Viewer {
    fn menu_watch(&self, target: MenuTarget) -> Option<Watch> {
        match target {
            MenuTarget::Watch { watch, bit } => {
                let mut item = self.session.watches.get(watch)?.clone();
                let size = item.size(bit);
                if let Watch::Signal { bit: watch_bit, .. } = &mut item {
                    *watch_bit = bit.or(*watch_bit);
                }
                item.set_size(None, size);
                Some(item)
            }
            MenuTarget::Browser { signal } => Some(self.new_watch(
                self.wave.signals.get(signal)?,
                (self.session.watches.len() % PALETTE.len()) as u8,
            )),
            MenuTarget::Columns => None,
        }
    }

    pub fn menu_items(&self) -> Vec<MenuItem> {
        let Some(menu) = &self.context_menu else {
            return vec![];
        };
        if matches!(menu.target, MenuTarget::Columns) {
            return vec![
                MenuItem::heading("Column widths"),
                MenuItem::new("Fit signal names", "", MenuAction::FitNames, true, false),
                MenuItem::new("Fit values at A", "", MenuAction::FitValues, true, false),
                MenuItem::new(
                    "Reset column widths",
                    "",
                    MenuAction::ResetColumns,
                    true,
                    false,
                ),
            ];
        }
        let Some(mut watch) = self.menu_watch(menu.target) else {
            return vec![];
        };
        let watched = matches!(menu.target, MenuTarget::Watch { .. });
        let selected = if watched {
            self.selected_rows()
        } else {
            vec![]
        };
        let multiple = selected.len() > 1;
        if multiple
            && !matches!(watch, Watch::Signal { .. })
            && let Some(signal) = selected.iter().find_map(|key| {
                self.menu_watch(MenuTarget::Watch {
                    watch: key.watch,
                    bit: key.bit,
                })
                .filter(|watch| matches!(watch, Watch::Signal { .. }))
            })
        {
            watch = signal;
        }
        let can_add = watched || self.session.watches.len() < MAX_WATCHES;
        let mut items = vec![];
        if menu.colors {
            items.push(MenuItem::new(
                "‹ Back to signal",
                "←",
                MenuAction::Back,
                true,
                false,
            ));
            items.push(MenuItem::heading("Signal color"));
            for (color, label) in [
                "Green", "Blue", "Gold", "Purple", "Pink", "Teal", "Orange", "Olive",
            ]
            .into_iter()
            .enumerate()
            {
                items.push(MenuItem::new(label, ["1", "2", "3", "4", "5", "6", "7", "8"][color], MenuAction::Color(color as u8), can_add,
                    matches!(watch, Watch::Signal { color: current, .. } if current as usize == color)));
            }
            self.configure_selection_menu(&mut items, &selected, multiple);
            return items;
        }
        if !watched {
            items.push(MenuItem::new(
                "Add to waveform",
                "",
                MenuAction::Add,
                can_add,
                false,
            ));
        }
        if let Watch::Signal {
            radix,
            bit,
            analog,
            expanded,
            size,
            ref path,
            ..
        } = watch
        {
            let real = self.signal_index.get(path).is_some_and(|id| {
                matches!(self.wave.signals[*id].kind.as_str(), "real" | "realtime")
            });
            let bus = bit.is_none()
                && !real
                && self
                    .signal_index
                    .get(path)
                    .is_some_and(|id| self.wave.signals[*id].width > 1);
            items.push(MenuItem::heading("Number format"));
            for (label, format) in [
                ("Hexadecimal", Radix::Hex),
                ("Binary", Radix::Binary),
                ("Unsigned decimal", Radix::Unsigned),
                ("Signed decimal", Radix::Signed),
                ("Octal", Radix::Octal),
                ("ASCII", Radix::Ascii),
            ] {
                items.push(MenuItem::new(
                    label,
                    "",
                    MenuAction::Radix(format),
                    can_add && !real && bit.is_none(),
                    !real
                        && (bit.is_some() || !analog)
                        && if bit.is_some() {
                            format == Radix::Binary
                        } else {
                            radix == format
                        },
                ));
            }
            items.push(MenuItem::heading("Display"));
            items.push(MenuItem::new(
                "Digital waveform",
                "",
                MenuAction::Digital,
                can_add && bit.is_none(),
                bit.is_some() || !analog,
            ));
            items.push(MenuItem::new(
                "Analog plot",
                "",
                MenuAction::Analog,
                can_add && bit.is_none(),
                analog && bit.is_none(),
            ));
            items.push(MenuItem::heading("Waveform size"));
            for value in [WaveSize::Small, WaveSize::Normal] {
                items.push(MenuItem::new(
                    value.label(),
                    "",
                    MenuAction::Size(value),
                    can_add,
                    size == value,
                ));
            }
            items.push(MenuItem::heading("Signal"));
            items.push(MenuItem::new(
                if expanded {
                    "Collapse bus"
                } else {
                    "Expand bus"
                },
                "e",
                MenuAction::Expand,
                bus && can_add,
                expanded,
            ));
            items.push(MenuItem::new(
                "Color ›",
                "1–8",
                MenuAction::Colors,
                can_add,
                false,
            ));
            if watched {
                items.push(MenuItem::new(
                    "Find value…",
                    "f",
                    MenuAction::Find,
                    true,
                    false,
                ));
                items.push(MenuItem::new(
                    "Rename…",
                    "F2",
                    MenuAction::Rename,
                    bit.is_none(),
                    false,
                ));
            }
            items.push(MenuItem::new(
                "Copy value at A",
                "Ctrl+C",
                MenuAction::Copy,
                true,
                false,
            ));
        } else if let Watch::Group { collapsed, .. } = watch {
            items.push(MenuItem::new(
                if collapsed {
                    "Expand group"
                } else {
                    "Collapse group"
                },
                "Enter",
                MenuAction::Expand,
                true,
                false,
            ));
            items.push(MenuItem::new(
                "Rename…",
                "F2",
                MenuAction::Rename,
                true,
                false,
            ));
        }
        if watched {
            items.push(MenuItem::new(
                "Remove",
                "Delete",
                MenuAction::Remove,
                true,
                false,
            ));
        }
        self.configure_selection_menu(&mut items, &selected, multiple);
        items
    }

    fn configure_selection_menu(
        &self,
        items: &mut [MenuItem],
        selected: &[RowKey],
        multiple: bool,
    ) {
        if !multiple {
            return;
        }
        for item in items {
            match item.action {
                MenuAction::Radix(_)
                | MenuAction::Digital
                | MenuAction::Analog
                | MenuAction::Color(_)
                | MenuAction::Size(_) => {
                    let mut eligible = 0;
                    let mut matching = 0;
                    for key in selected {
                        if self.row_accepts_edit(*key, item.action) {
                            eligible += 1;
                            matching += usize::from(self.row_matches_edit(*key, item.action));
                        }
                    }
                    item.enabled = eligible > 0;
                    item.checked = eligible > 0 && matching == eligible;
                    item.mixed = matching > 0 && matching < eligible;
                }
                MenuAction::Expand | MenuAction::Find | MenuAction::Rename | MenuAction::Copy => {
                    item.enabled = false;
                    item.checked = false;
                }
                MenuAction::Remove => item.label = "Remove selection",
                _ => {}
            }
        }
    }

    pub fn menu_layout(&self) -> MenuLayout {
        let width = self.width.min(512);
        let height = self.height.min(200);
        let count = self.menu_items().len();
        let w = width.min(38);
        let h = if height >= 8 { height - 2 } else { height }.min((count + 4) as u16);
        let menu = self.context_menu.as_ref();
        let (ax, ay) = menu.map_or((0, 0), |m| m.anchor);
        let x = ax.saturating_add(1).min(width.saturating_sub(w));
        let bottom = if height >= 8 { height - 2 } else { height };
        let y = ay.min(bottom.saturating_sub(h));
        let visible = usize::from(h.saturating_sub(4));
        let scroll = menu
            .map_or(0, |m| m.scroll)
            .min(count.saturating_sub(visible));
        MenuLayout {
            panel: PopupRect {
                x,
                y,
                width: w,
                height: h,
            },
            items: PopupRect {
                x: x + u16::from(w > 2),
                y: y + 2.min(h),
                width: w.saturating_sub(2),
                height: visible as u16,
            },
            visible,
            scroll,
        }
    }

    pub(crate) fn open_context_menu(&mut self, event: &Event) {
        self.context_menu = None;
        if self.loading.is_some() || event.pointer_inside == Some(false) {
            return;
        }
        self.signal_drag = None;
        self.cancel_timeline_drag();
        self.help = false;
        self.hover = None;
        let l = self.layout();
        let (x, y) = (event.view_column, event.view_row);
        if x >= self.width.min(512) {
            return;
        }
        let target = if matches!(y, 3 | 4) && x >= l.browser && x < l.wave_x {
            MenuTarget::Columns
        } else if y < l.body_y || y >= l.body_y + l.body_height {
            return;
        } else if x < l.browser {
            let node = self.browser_scroll + usize::from(y - l.body_y);
            let Some(BrowserNode::Signal(signal, _)) = self.browser_nodes.get(node) else {
                return;
            };
            let signal = *signal;
            self.focus = Focus::Browser;
            self.browser_selection = node;
            self.session.watches.iter().position(|watch| matches!(watch, Watch::Signal { path, bit: None, .. } if *path == self.wave.signals[signal].path))
                .map_or(MenuTarget::Browser { signal }, |watch| MenuTarget::Watch { watch, bit: None })
        } else {
            let Some(row) = self.row_at(y) else { return };
            self.focus = Focus::Waves;
            MenuTarget::Watch {
                watch: row.watch,
                bit: row.bit,
            }
        };
        if let MenuTarget::Watch { watch, bit } = target {
            let key = RowKey { watch, bit };
            if self.selected_rows().contains(&key) {
                self.selection = watch;
                self.selected_bit = bit;
            } else {
                self.select_wave(key, false, false);
            }
        }
        self.context_menu = Some(ContextMenu {
            target,
            anchor: (x, y),
            selected: 0,
            scroll: 0,
            colors: false,
            scroll_drag: None,
        });
        let first = self
            .menu_items()
            .iter()
            .position(|item| item.enabled)
            .unwrap_or(0);
        if let Some(menu) = &mut self.context_menu {
            menu.selected = first;
        }
    }

    pub(crate) fn open_keyboard_menu(&mut self) {
        let l = self.layout();
        let event = if self.focus == Focus::Browser && l.browser > 0 {
            Event {
                view_column: 3.min(l.browser - 1),
                view_row: l.body_y
                    + self.browser_selection.saturating_sub(self.browser_scroll) as u16,
                ..Event::default()
            }
        } else {
            let Some(row) = self
                .rows()
                .into_iter()
                .find(|row| row.watch == self.selection && row.bit == self.selected_bit)
            else {
                return;
            };
            Event {
                view_column: l.browser + 4,
                view_row: l.body_y + (row.line as usize).saturating_sub(self.scroll) as u16,
                ..Event::default()
            }
        };
        self.open_context_menu(&event);
    }

    pub(crate) fn reveal_menu_selection(&mut self) {
        let l = self.menu_layout();
        if let Some(menu) = &mut self.context_menu {
            menu.scroll_drag = None;
            if menu.selected < l.scroll {
                menu.scroll = menu.selected;
            } else if menu.selected >= l.scroll + l.visible {
                menu.scroll = (menu.selected + 1).saturating_sub(l.visible);
            }
        }
    }

    pub(crate) fn menu_event(&mut self, event: &Event) {
        let items = self.menu_items();
        if event.kind == "cancel" {
            if let Some(menu) = &mut self.context_menu {
                menu.scroll_drag = None;
            }
            return;
        }
        if let Some((press, original, grip)) =
            self.context_menu.as_ref().and_then(|m| m.scroll_drag)
            && matches!(event.kind.as_str(), "drag" | "up" | "move")
        {
            let l = self.menu_layout();
            if let Some((_, size)) =
                crate::navigation::scrollbar(items.len(), l.scroll, l.visible as u16)
            {
                let travel = l.visible.saturating_sub(usize::from(size));
                let position =
                    usize::from(event.view_row.saturating_sub(l.items.y + grip)).min(travel);
                if let Some(menu) = &mut self.context_menu {
                    menu.scroll = if event.view_row == press {
                        original
                    } else {
                        position * items.len().saturating_sub(l.visible) / travel.max(1)
                    };
                    if event.kind == "up" {
                        menu.scroll_drag = None;
                    }
                }
                self.keep_menu_selection_visible(&items);
            }
            return;
        }
        if event.direction != 0 {
            let l = self.menu_layout();
            if let Some(menu) = &mut self.context_menu {
                menu.scroll = l
                    .scroll
                    .saturating_add_signed(if event.direction > 0 { 3 } else { -3 })
                    .min(items.len().saturating_sub(l.visible));
                if !(menu.scroll..menu.scroll + l.visible).contains(&menu.selected)
                    && let Some(index) = (menu.scroll..(menu.scroll + l.visible).min(items.len()))
                        .find(|index| items[*index].enabled)
                {
                    menu.selected = index;
                }
            }
            return;
        }
        if matches!(event.kind.as_str(), "move" | "down") {
            let l = self.menu_layout();
            let (x, y) = (event.view_column, event.view_row);
            if event.kind == "down" && !l.panel.contains(x, y) {
                self.context_menu = None;
                return;
            }
            if l.items.contains(x, y) {
                let index = l.scroll + usize::from(y - l.items.y);
                if let Some(item) = items.get(index).filter(|item| item.enabled) {
                    if let Some(menu) = &mut self.context_menu {
                        menu.selected = index;
                    }
                    if event.kind == "down" {
                        self.apply_menu_action(item.action);
                    }
                }
            }
            if event.kind == "down"
                && x == l.panel.x + l.panel.width.saturating_sub(1)
                && y >= l.items.y
                && y < l.items.y + l.items.height
                && let Some((start, size)) =
                    crate::navigation::scrollbar(items.len(), l.scroll, l.visible as u16)
            {
                if let Some(menu) = &mut self.context_menu {
                    let top = l.items.y + start;
                    if (top..top + size).contains(&y) {
                        menu.scroll_drag = Some((y, l.scroll, y - top));
                    } else {
                        menu.scroll = l
                            .scroll
                            .saturating_add_signed(if y < top {
                                -(l.visible as isize)
                            } else {
                                l.visible as isize
                            })
                            .min(items.len().saturating_sub(l.visible));
                    }
                }
                self.keep_menu_selection_visible(&items);
            }
            return;
        }
        if !event.kind.is_empty() {
            return;
        }
        let Some(menu) = &self.context_menu else {
            return;
        };
        let shortcut = if (event.control || event.ctrl) && event.text.eq_ignore_ascii_case("c")
            || event.code == "command" && event.text == "copy"
        {
            Some(MenuAction::Copy)
        } else if matches!(event.code.as_str(), "delete" | "backspace") {
            Some(MenuAction::Remove)
        } else if event.code == "function" && event.text == "2" {
            Some(MenuAction::Rename)
        } else if event.code == "char" && event.text == "e" {
            Some(MenuAction::Expand)
        } else if event.code == "char" && event.text == "f" {
            Some(MenuAction::Find)
        } else {
            None
        };
        if let Some(action) = shortcut
            && items
                .iter()
                .any(|item| item.action == action && item.enabled)
        {
            self.apply_menu_action(action);
            return;
        }
        if event.code == "char" && !(event.control || event.ctrl) && event.text.len() == 1 {
            let byte = event.text.as_bytes()[0];
            if (b'1'..=b'8').contains(&byte)
                && items.iter().any(|item| {
                    item.enabled && matches!(item.action, MenuAction::Colors | MenuAction::Color(_))
                })
            {
                self.apply_menu_action(MenuAction::Color(byte - b'1'));
                return;
            }
            if byte.is_ascii_alphabetic() {
                let start = menu.selected;
                if let Some(index) = (1..=items.len())
                    .map(|offset| (start + offset) % items.len())
                    .find(|index| {
                        items[*index].enabled
                            && items[*index]
                                .label
                                .to_ascii_lowercase()
                                .starts_with((byte as char).to_ascii_lowercase())
                    })
                {
                    self.context_menu.as_mut().unwrap().selected = index;
                    self.reveal_menu_selection();
                }
                return;
            }
        }
        if event.code == "escape" || event.code == "left" {
            if menu.colors {
                self.apply_menu_action(MenuAction::Back);
            } else {
                self.context_menu = None;
            }
            return;
        }
        if event.code == "enter" || event.code == "right" || event.text == " " {
            if let Some(item) = items.get(menu.selected).filter(|item| item.enabled) {
                self.apply_menu_action(item.action);
            }
            return;
        }
        if matches!(
            event.code.as_str(),
            "up" | "down" | "home" | "end" | "pageUp" | "pageDown" | "tab" | "backTab"
        ) {
            let enabled: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.enabled)
                .map(|(i, _)| i)
                .collect();
            if enabled.is_empty() {
                return;
            }
            let current = enabled
                .iter()
                .position(|i| *i == menu.selected)
                .unwrap_or(0);
            let next = match event.code.as_str() {
                "home" => 0,
                "end" => enabled.len() - 1,
                "pageUp" => current.saturating_sub(self.menu_layout().visible.max(1)),
                "pageDown" => (current + self.menu_layout().visible.max(1)).min(enabled.len() - 1),
                "up" | "backTab" => (current + enabled.len() - 1) % enabled.len(),
                "tab" if event.shift => (current + enabled.len() - 1) % enabled.len(),
                _ => (current + 1) % enabled.len(),
            };
            if let Some(menu) = &mut self.context_menu {
                menu.selected = enabled[next];
            }
            self.reveal_menu_selection();
        }
    }

    fn keep_menu_selection_visible(&mut self, items: &[MenuItem]) {
        let l = self.menu_layout();
        if let Some(menu) = &mut self.context_menu
            && !(l.scroll..l.scroll + l.visible).contains(&menu.selected)
            && let Some(index) = (l.scroll..(l.scroll + l.visible).min(items.len()))
                .find(|index| items[*index].enabled)
        {
            menu.selected = index;
        }
    }

    fn apply_menu_action(&mut self, action: MenuAction) {
        let Some(menu) = self.context_menu.clone() else {
            return;
        };
        if matches!(action, MenuAction::Colors | MenuAction::Back) {
            if let Some(menu) = &mut self.context_menu {
                menu.colors = action == MenuAction::Colors;
                menu.selected = 0;
                menu.scroll = 0;
            }
            return;
        }
        if matches!(
            action,
            MenuAction::Radix(_)
                | MenuAction::Digital
                | MenuAction::Analog
                | MenuAction::Color(_)
                | MenuAction::Size(_)
        ) && matches!(menu.target, MenuTarget::Watch { .. })
            && self
                .menu_items()
                .iter()
                .any(|item| item.action == action && item.checked)
        {
            self.context_menu = None;
            return;
        }
        self.context_menu = None;
        if matches!(menu.target, MenuTarget::Columns) {
            self.fit_columns(
                action == MenuAction::FitNames,
                action == MenuAction::FitValues,
                action == MenuAction::ResetColumns,
            );
            return;
        }
        if action == MenuAction::Copy {
            if let Some(Watch::Signal {
                path, bit, radix, ..
            }) = self.menu_watch(menu.target)
                && let Some(id) = self.signal_index.get(&path)
            {
                let value =
                    self.wave.traces[self.wave.signals[*id].trace].value_at(self.session.cursor);
                if let Some(value) = value {
                    self.copy_requested = Some(if let Some(bit) = bit {
                        value.bit(bit).format(Radix::Binary)
                    } else {
                        value.format(radix)
                    });
                    self.status = "Value at A copied".into();
                }
            }
            return;
        }
        let (index, bit, added) = match menu.target {
            MenuTarget::Watch { watch, bit } => (watch, bit, false),
            MenuTarget::Browser { .. } => {
                if self.session.watches.len() >= MAX_WATCHES {
                    self.fail("Watch list is full");
                    return;
                }
                let Some(watch) = self.menu_watch(menu.target) else {
                    return;
                };
                self.checkpoint();
                self.session.watches.push(watch);
                (self.session.watches.len() - 1, None, true)
            }
            MenuTarget::Columns => return,
        };
        self.selection = index;
        self.selected_bit = bit;
        self.focus = Focus::Waves;
        if added {
            self.clear_wave_selection();
            self.status = "Signal added · right-click for display options".into();
            // A browser action can append a signal after a collapsed group.
            if let Some(Watch::Group { collapsed, .. }) = self.session.watches[..index]
                .iter_mut()
                .rev()
                .find(|watch| matches!(watch, Watch::Group { .. }))
            {
                *collapsed = false;
            }
        }
        match action {
            MenuAction::Rename => self.open_prompt(Prompt::Rename),
            MenuAction::Find => self.open_prompt(Prompt::FindValue),
            MenuAction::Remove => self.remove_selected(),
            MenuAction::Expand if !added => self.toggle_expand(),
            MenuAction::Add => {
                self.status = "Signal added · right-click for display options".into()
            }
            MenuAction::Size(_)
            | MenuAction::Radix(_)
            | MenuAction::Digital
            | MenuAction::Analog
            | MenuAction::Color(_) => {
                self.apply_selection_edit(action, !added);
            }
            MenuAction::Expand => {
                if let Some(Watch::Signal { expanded, .. }) = self.session.watches.get_mut(index) {
                    *expanded = !*expanded;
                }
                self.status = "Signal display updated · Ctrl+Z to undo".into();
            }
            _ => {}
        }
        self.ensure_selection();
    }

    pub(crate) fn draw_context_menu(&self, canvas: &mut Canvas) {
        let Some(menu) = &self.context_menu else {
            return;
        };
        let l = self.menu_layout();
        // A one-cell shadow separates the menu from dense waveform traces.
        let bottom = if canvas.height >= 8 {
            canvas.height - 2
        } else {
            canvas.height
        };
        for y in l.panel.y + 1..(l.panel.y + l.panel.height + 1).min(bottom) {
            canvas.fill(l.panel.x + l.panel.width, y, 1, "muted", "header");
        }
        draw_panel(canvas, l.panel.x, l.panel.y, l.panel.width, l.panel.height);
        let title = if matches!(menu.target, MenuTarget::Columns) {
            "Signals & values".into()
        } else if matches!(menu.target, MenuTarget::Watch { .. }) && self.selection_count() > 1 {
            format!("{} waves selected", self.selection_count())
        } else {
            match self.menu_watch(menu.target) {
                Some(Watch::Signal {
                    path, label, bit, ..
                }) => {
                    let name = if label.is_empty() {
                        path.rsplit('.').next().unwrap_or(&path).to_owned()
                    } else {
                        label
                    };
                    if let Some(bit) = bit {
                        format!("{} [{bit}]", name.split('[').next().unwrap_or(&name))
                    } else {
                        name
                    }
                }
                Some(Watch::Group { name, .. }) => name,
                _ => "Divider".into(),
            }
        };
        canvas.text(
            l.panel.x + 2,
            l.panel.y + 1,
            l.panel.width.saturating_sub(4),
            &ellipsize(&title, l.panel.width.saturating_sub(4) as usize),
            "accent",
            "panel",
            true,
        );
        for (visible, (index, item)) in self
            .menu_items()
            .iter()
            .enumerate()
            .skip(l.scroll)
            .take(l.visible)
            .enumerate()
        {
            let y = l.items.y + visible as u16;
            let selected = menu.selected == index && item.enabled;
            let bg = if selected { "selection" } else { "panel" };
            canvas.fill(l.items.x, y, l.items.width, "text", bg);
            if item.action == MenuAction::Heading {
                canvas.text(
                    l.items.x + 1,
                    y,
                    l.items.width.saturating_sub(2),
                    &format!("─ {}", item.label),
                    "muted",
                    bg,
                    false,
                );
                continue;
            }
            let role = if !item.enabled {
                "muted"
            } else if let MenuAction::Color(color) = item.action {
                PALETTE[color as usize]
            } else if item.action == MenuAction::Remove {
                "error"
            } else if selected || item.checked {
                "accent"
            } else {
                "text"
            };
            canvas.text(
                l.items.x,
                y,
                2,
                if item.checked {
                    "✓ "
                } else if item.mixed {
                    "− "
                } else {
                    "  "
                },
                role,
                bg,
                false,
            );
            canvas.text(
                l.items.x + 2,
                y,
                l.items.width.saturating_sub(4),
                item.label,
                role,
                bg,
                selected,
            );
            if l.items.width >= 30 && !item.shortcut.is_empty() {
                canvas.text(
                    l.items.x
                        + l.items
                            .width
                            .saturating_sub(item.shortcut.chars().count() as u16 + 1),
                    y,
                    item.shortcut.chars().count() as u16,
                    item.shortcut,
                    "muted",
                    bg,
                    false,
                );
            }
        }
        let count = self.menu_items().len();
        if let Some((start, size)) = crate::navigation::scrollbar(count, l.scroll, l.visible as u16)
        {
            for y in l.items.y..l.items.y + l.items.height {
                let thumb = (l.items.y + start..l.items.y + start + size).contains(&y);
                canvas.character(
                    l.panel.x + l.panel.width.saturating_sub(1),
                    y,
                    if thumb { '┃' } else { '│' },
                    if thumb { "accent" } else { "muted" },
                );
            }
        }
        if l.panel.height >= 4 {
            let hint = if l.scroll > 0 || l.scroll + l.visible < count {
                format!(
                    "{}–{} / {count}  ↑↓ scroll · Enter apply",
                    l.scroll + 1,
                    (l.scroll + l.visible).min(count)
                )
            } else {
                "↑↓ select  Enter apply  Esc close".into()
            };
            canvas.text(
                l.panel.x + 2,
                l.panel.y + l.panel.height - 2,
                l.panel.width.saturating_sub(4),
                &hint,
                "muted",
                "panel",
                false,
            );
        }
    }
}
