//! nWave-style command organization; every visible menu uses the same input model.
use crate::menus::{ContextMenu, MenuAction, MenuItem, MenuTarget};
use crate::viewer::{Event, Viewer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainMenu {
    All,
    File,
    Signal,
    View,
    Waveform,
    Cursors,
    Tools,
}
impl MainMenu {
    pub const ALL: [Self; 6] = [
        Self::File,
        Self::Signal,
        Self::View,
        Self::Waveform,
        Self::Cursors,
        Self::Tools,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::All => "Menu",
            Self::File => "File",
            Self::Signal => "Signal",
            Self::View => "View",
            Self::Waveform => "Waveform",
            Self::Cursors => "Cursors",
            Self::Tools => "Tools",
        }
    }
}
pub struct MainMenuButton {
    pub menu: MainMenu,
    pub x: u16,
    pub width: u16,
}

impl Viewer {
    pub fn main_menus(&self) -> Vec<MainMenuButton> {
        if self.width < 64 {
            return vec![MainMenuButton {
                menu: MainMenu::All,
                x: 1,
                width: 6.min(self.width.saturating_sub(1)),
            }];
        }
        let mut x = 1;
        MainMenu::ALL
            .into_iter()
            .map(|menu| {
                let width = menu.name().len() as u16 + 2;
                let button = MainMenuButton { menu, x, width };
                x += width + 1;
                button
            })
            .collect()
    }

    pub fn open_main_menu(&mut self, menu: MainMenu) {
        self.cancel_timeline_drag();
        self.signal_drag = None;
        self.hover = None;
        self.help = false;
        let x = self
            .main_menus()
            .iter()
            .find(|b| b.menu == menu)
            .map(|b| b.x)
            .unwrap_or(1);
        self.context_menu = Some(ContextMenu {
            target: MenuTarget::Main(menu),
            anchor: (x, 1),
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

    pub(crate) fn main_menu_event(&mut self, event: &Event) -> bool {
        let Some(MenuTarget::Main(current)) = self.context_menu.as_ref().map(|m| m.target) else {
            return false;
        };
        if matches!(event.kind.as_str(), "move" | "down")
            && event.view_row == 0
            && let Some(menu) = self
                .main_menus()
                .into_iter()
                .find(|b| event.view_column >= b.x && event.view_column < b.x + b.width)
                .map(|b| b.menu)
        {
            if menu != current {
                self.open_main_menu(menu);
            } else if event.kind == "down" {
                self.context_menu = None;
            }
            return true;
        }
        if matches!(event.code.as_str(), "left" | "right") && current != MainMenu::All {
            let at = MainMenu::ALL
                .iter()
                .position(|menu| *menu == current)
                .unwrap_or(0);
            let next = if event.code == "left" {
                (at + 5) % 6
            } else {
                (at + 1) % 6
            };
            self.open_main_menu(MainMenu::ALL[next]);
            return true;
        }
        false
    }

    pub(crate) fn main_menu_items(&self, menu: MainMenu) -> Vec<MenuItem> {
        let definitions: &[(&str, &str, &str)] = match menu {
            MainMenu::All => &[
                ("File", "", "menu_file"),
                ("Signal", "", "menu_signal"),
                ("View", "", "menu_view"),
                ("Waveform", "", "menu_waveform"),
                ("Cursors", "", "menu_cursors"),
                ("Tools", "", "menu_tools"),
            ],
            MainMenu::File => &[
                ("Save waveform view", "Ctrl+S", "save"),
                ("Export view as JSON", "Ctrl+E", "export"),
                ("Import waveform view…", "Ctrl+V", "import"),
                ("Reload simulation", "F5", "reload"),
            ],
            MainMenu::Signal => &[
                ("Add signals…", "/", "add_signals"),
                ("New group…", "g", "group"),
                ("Insert divider", "d", "divider"),
                ("Expand / collapse bus", "Enter", "expand"),
                ("Signal display…", "Shift+F10", "signal_display"),
                ("Remove selected signals", "Delete", "remove"),
                ("Undo signal edit", "Ctrl+Z", "undo"),
                ("Redo signal edit", "Ctrl+Y", "redo"),
            ],
            MainMenu::View => &[
                ("Fit entire recording", "0", "fit"),
                ("Resolve around A", "i", "detail"),
                ("Previous time view", "Shift+Z", "zoom_back"),
                ("Fit between A and B", "z", "fit_cursors"),
                ("Normal heights for all waves", "", "readable_heights"),
                ("Signal browser", "b", "browser"),
                ("Value inspector", "v", "inspector"),
                ("Full signal paths", "`", "full_paths"),
                ("Fit signal names", "", "fit_names"),
                ("Fit cursor values", "", "fit_values"),
                ("Reset column widths", "", "reset_columns"),
            ],
            MainMenu::Waveform => &[
                ("Find value…", "f", "find"),
                ("Previous transition", "Left", "previous"),
                ("Next transition", "Right", "next"),
                ("Next rising edge", "u", "rising"),
                ("Next falling edge", "j", "falling"),
                ("Number format / display…", "Shift+F10", "signal_display"),
                ("Small / normal height", "h", "height"),
            ],
            MainMenu::Cursors => &[
                ("Go to A time…", "Ctrl+G", "goto"),
                ("Go to B time…", "", "goto_b"),
                ("Set B at A", "", "b_at_a"),
                ("Clear B", "", "clear_b"),
                ("Swap A and B", "x", "swap_cursors"),
                ("Previous B transition", "Alt+Shift+N", "previous_b"),
                ("Next B transition", "Alt+N", "next_b"),
                ("Snap to transitions", "s", "snap"),
                ("Add marker…", "m", "marker"),
                ("Remove marker at A", "Shift+M", "remove_marker"),
            ],
            MainMenu::Tools => &[
                ("Fit all columns", "", "fit_columns"),
                ("Keyboard and mouse reference", "?", "help"),
            ],
        };
        definitions
            .iter()
            .map(|&(label, shortcut, command)| {
                let enabled = match command {
                    "zoom_back" => !self.view_history.is_empty(),
                    "fit_cursors" | "clear_b" | "swap_cursors" => self.session.alt_cursor.is_some(),
                    "previous" | "next" | "previous_b" | "next_b" | "rising" | "falling"
                    | "height" | "signal_display" | "find" | "detail" | "expand" | "remove" => {
                        self.selected().is_some()
                    }
                    "undo" => !self.history.is_empty(),
                    "redo" => !self.redo.is_empty(),
                    _ => true,
                };
                let checked = match command {
                    "browser" => self.layout().browser > 0,
                    "inspector" => self.session.inspector,
                    "full_paths" => self.session.full_names,
                    "snap" => self.session.snap,
                    _ => false,
                };
                MenuItem::new(
                    label,
                    shortcut,
                    MenuAction::Command(command),
                    enabled,
                    checked,
                )
            })
            .collect()
    }
}
