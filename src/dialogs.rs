//! Modal queries share their geometry between rendering and mouse input.
use crate::render::{Canvas, draw_panel, ellipsize};
use crate::viewer::{Event, Focus, Prompt, Viewer};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Copy, Debug, Default)]
pub struct PopupRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}
impl PopupRect {
    pub fn contains(self, x: u16, y: u16) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DialogFocus {
    #[default]
    Input,
    Cancel,
    Submit,
}

#[derive(Default)]
pub struct DialogState {
    pub cursor: usize,
    pub select_all: bool,
    pub focus: DialogFocus,
    filter_before: String,
    browser_before: bool,
    selection_before: usize,
    scroll_before: usize,
    focus_before: Option<Focus>,
    status_before: String,
}
impl DialogState {
    pub(crate) fn new(viewer: &Viewer) -> Self {
        Self {
            filter_before: viewer.filter.clone(),
            browser_before: viewer.session.browser,
            selection_before: viewer.browser_selection,
            scroll_before: viewer.browser_scroll,
            focus_before: Some(viewer.focus),
            status_before: viewer.status.clone(),
            ..Self::default()
        }
    }
}

pub struct DialogLayout {
    pub panel: PopupRect,
    pub field: PopupRect,
    pub input: PopupRect,
    pub cancel: PopupRect,
    pub submit: PopupRect,
    pub close: PopupRect,
}

impl Viewer {
    pub fn dialog_layout(&self) -> DialogLayout {
        let width = self.width.min(512);
        let height = self.height.min(200);
        let w = if width >= 20 { width - 4 } else { width }.min(72);
        let h = if height >= 15 { height - 2 } else { height }.min(13);
        let x = (width - w) / 2;
        let y = (height - h) / 2;
        let inset = if w >= 8 { 3 } else { 0 };
        let field_y = y + 5.min(h.saturating_sub(5));
        let field_height = if h >= 8 { 3 } else { 1.min(h) };
        let field = PopupRect {
            x: x + inset,
            y: field_y,
            width: w.saturating_sub(inset * 2),
            height: field_height,
        };
        let input = PopupRect {
            x: field.x + u16::from(field.width > 2),
            y: field.y + u16::from(field.height == 3),
            width: field
                .width
                .saturating_sub(if field.width > 2 { 2 } else { 0 }),
            height: 1.min(h),
        };
        let buttons_y = y + h.saturating_sub(3);
        let submit_width = 12.min(w.saturating_sub(inset));
        let submit = PopupRect {
            x: x + w.saturating_sub(inset + submit_width),
            y: buttons_y,
            width: submit_width,
            height: 1.min(h),
        };
        let cancel_width = 10.min(submit.x.saturating_sub(x + inset + 1));
        let cancel = PopupRect {
            x: submit.x.saturating_sub(cancel_width + 1).max(x),
            y: buttons_y,
            width: cancel_width,
            height: 1.min(h),
        };
        let close = PopupRect {
            x: x + w.saturating_sub(4),
            y: y + u16::from(h > 2),
            width: 3.min(w),
            height: 1.min(h),
        };
        DialogLayout {
            panel: PopupRect {
                x,
                y,
                width: w,
                height: h,
            },
            field,
            input,
            cancel,
            submit,
            close,
        }
    }

    pub fn cancel_prompt(&mut self) {
        if self.prompt.take() == Some(Prompt::Filter) {
            self.filter = std::mem::take(&mut self.dialog.filter_before);
            self.rebuild_browser();
            self.session.browser = self.dialog.browser_before;
            self.browser_selection = self.dialog.selection_before;
            self.browser_scroll = self.dialog.scroll_before;
            if let Some(focus) = self.dialog.focus_before {
                self.focus = focus;
            }
        }
        self.status = std::mem::take(&mut self.dialog.status_before);
        self.input.clear();
        self.dialog = DialogState::default();
        self.error = false;
    }

    fn submit_dialog(&mut self) {
        let Some(prompt) = self.prompt else { return };
        if prompt == Prompt::FindValue && self.input.trim().is_empty() {
            self.fail("Enter the value to find");
        } else if self.commit_prompt(prompt).is_ok() {
            self.prompt = None;
            self.input.clear();
            self.dialog = DialogState::default();
            self.error = false;
        }
    }

    pub(crate) fn dialog_event(&mut self, event: &Event) {
        let Some(prompt) = self.prompt else { return };
        self.dialog.cursor = self.dialog.cursor.min(self.input.len());
        while !self.input.is_char_boundary(self.dialog.cursor) {
            self.dialog.cursor -= 1;
        }
        if event.kind == "down" || event.kind == "contextMenu" {
            let l = self.dialog_layout();
            let (x, y) = (event.view_column, event.view_row);
            if !l.panel.contains(x, y) || l.close.contains(x, y) || l.cancel.contains(x, y) {
                self.cancel_prompt();
            } else if event.kind == "down" && l.submit.contains(x, y) {
                self.submit_dialog();
            } else if event.kind == "down" && l.field.contains(x, y) {
                self.dialog.focus = DialogFocus::Input;
                self.dialog.select_all = false;
                let (start, _, _) = self.dialog_input_window(l.input.width);
                let mut column = 0;
                let mut position = start;
                for (offset, ch) in self.input[start..].char_indices() {
                    let size = ch.width().unwrap_or(1).max(1) as u16;
                    if column + size > x.saturating_sub(l.input.x) {
                        break;
                    }
                    column += size;
                    position = start + offset + ch.len_utf8();
                    if column >= l.input.width {
                        break;
                    }
                }
                self.dialog.cursor = position;
            }
            return;
        }
        // The host sends release/cancel events after toolbar clicks. They do not
        // dismiss the dialog or allow the underlying view to handle the gesture.
        if !event.kind.is_empty() || event.direction != 0 {
            return;
        }
        if event.code == "escape" {
            self.cancel_prompt();
            return;
        }
        if matches!(event.code.as_str(), "tab" | "backTab") {
            self.dialog.focus = match (self.dialog.focus, event.shift || event.code == "backTab") {
                (DialogFocus::Input, false) | (DialogFocus::Submit, true) => DialogFocus::Cancel,
                (DialogFocus::Cancel, false) | (DialogFocus::Input, true) => DialogFocus::Submit,
                _ => DialogFocus::Input,
            };
            return;
        }
        if event.code == "enter" || event.text == " " && self.dialog.focus != DialogFocus::Input {
            if self.dialog.focus == DialogFocus::Cancel {
                self.cancel_prompt();
            } else {
                self.submit_dialog();
            }
            return;
        }
        if self.dialog.focus != DialogFocus::Input {
            return;
        }
        let control = event.control || event.ctrl || event.code == "command";
        let key = if event.code == "command" {
            match event.text.as_str() {
                "select_all" => "a",
                "copy" => "c",
                "cut" => "x",
                _ => "",
            }
        } else {
            &event.text
        };
        if control && key.eq_ignore_ascii_case("a") {
            self.dialog.select_all = true;
            self.dialog.cursor = self.input.len();
            return;
        }
        if control
            && matches!(key.to_ascii_lowercase().as_str(), "c" | "x")
            && self.dialog.select_all
        {
            self.copy_requested = Some(self.input.clone());
            if key.eq_ignore_ascii_case("c") {
                return;
            }
            self.input.clear();
            self.dialog.cursor = 0;
            self.dialog.select_all = false;
        } else if matches!(event.code.as_str(), "left" | "right" | "home" | "end") {
            self.dialog.cursor = match event.code.as_str() {
                "home" => 0,
                "end" => self.input.len(),
                "left" if self.dialog.select_all => 0,
                "right" if self.dialog.select_all => self.input.len(),
                "left" => previous_boundary(&self.input, self.dialog.cursor),
                _ => next_boundary(&self.input, self.dialog.cursor),
            };
            self.dialog.select_all = false;
            return;
        } else if matches!(event.code.as_str(), "backspace" | "delete") {
            if self.dialog.select_all {
                self.input.clear();
                self.dialog.cursor = 0;
            } else if event.code == "backspace" {
                let previous = previous_boundary(&self.input, self.dialog.cursor);
                self.input.replace_range(previous..self.dialog.cursor, "");
                self.dialog.cursor = previous;
            } else {
                self.input.replace_range(
                    self.dialog.cursor..next_boundary(&self.input, self.dialog.cursor),
                    "",
                );
            }
            self.dialog.select_all = false;
        } else if event.code == "paste" || event.code == "char" && !control && !event.alt {
            let text = if prompt == Prompt::Import {
                event.text.clone()
            } else {
                event.text.replace(['\r', '\n', '\t'], " ")
            };
            let replace =
                self.dialog.select_all || prompt == Prompt::Import && event.code == "paste";
            let limit = if prompt == Prompt::Import {
                4 * 1024 * 1024
            } else {
                4096
            };
            if (if replace { 0 } else { self.input.len() }) + text.len() > limit {
                self.fail(if prompt == Prompt::Import {
                    "Session exceeds 4 MiB"
                } else {
                    "Text exceeds 4096 bytes"
                });
                return;
            }
            if replace {
                self.input.clear();
                self.dialog.cursor = 0;
            }
            self.input.insert_str(self.dialog.cursor, &text);
            self.dialog.cursor += text.len();
            self.dialog.select_all = false;
        } else {
            return;
        }
        self.error = false;
        if prompt == Prompt::Filter {
            self.filter = self.input.clone();
            self.browser_selection = 0;
            self.browser_scroll = 0;
            self.rebuild_browser();
            self.session.browser = true;
            self.focus = Focus::Browser;
        }
    }

    /// Bound rendering work even when the clipboard contains a multi-megabyte session.
    fn dialog_input_window(&self, width: u16) -> (usize, String, u16) {
        let mut cursor = self.dialog.cursor.min(self.input.len());
        while !self.input.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let mut start = cursor;
        let mut used = 0;
        for (offset, ch) in self.input[..cursor].char_indices().rev() {
            let size = ch.width().unwrap_or(1).max(1) as u16;
            if used + size > width.saturating_sub(1) {
                break;
            }
            used += size;
            start = offset;
        }
        let mut shown = String::new();
        let mut shown_width = 0;
        for ch in self.input[start..].chars() {
            let ch = if ch.is_control() { ' ' } else { ch };
            let size = ch.width().unwrap_or(0) as u16;
            if shown_width + size > width {
                break;
            }
            shown.push(ch);
            shown_width += size;
        }
        (start, shown, used)
    }

    pub(crate) fn draw_dialog(&self, canvas: &mut Canvas) -> Option<(u16, u16)> {
        let prompt = self.prompt?;
        let l = self.dialog_layout();
        for cell in &mut canvas.cells {
            cell.role = "muted";
            cell.background = "";
            cell.bold = false;
        }
        draw_panel(canvas, l.panel.x, l.panel.y, l.panel.width, l.panel.height);
        let x = l.panel.x + 3;
        let w = l.panel.width.saturating_sub(6);
        canvas.text(
            x,
            l.panel.y + 1,
            w.saturating_sub(3),
            prompt.label(),
            "accent",
            "panel",
            true,
        );
        canvas.text(
            l.close.x,
            l.close.y,
            l.close.width,
            " × ",
            "muted",
            "panel",
            false,
        );
        let context = match prompt {
            Prompt::Marker => format!(
                "Place a marker at A · {}",
                self.wave.timescale.format(self.session.cursor)
            ),
            Prompt::Group => "Organize signals into a named group.".into(),
            Prompt::Rename => "Set a display name. Leave empty to use the signal name.".into(),
            Prompt::Filter => "Search signal names and hierarchical paths.".into(),
            Prompt::FindValue => {
                "Search the selected signal. Prefix with 0x or 0b to override radix.".into()
            }
            Prompt::Goto => format!(
                "Recording: {} → {}",
                self.wave.timescale.format(self.wave.start),
                self.wave.timescale.format(self.wave.end)
            ),
            Prompt::Import => {
                "Paste a saved session to restore signals, markers and cursors.".into()
            }
        };
        if l.panel.height >= 11 {
            canvas.text(
                x,
                l.panel.y + 3,
                w,
                &ellipsize(&context, w as usize),
                "text",
                "panel",
                false,
            );
        }
        draw_panel(canvas, l.field.x, l.field.y, l.field.width, l.field.height);
        if self.dialog.focus == DialogFocus::Input && l.field.height == 3 {
            for dx in 1..l.field.width.saturating_sub(1) {
                canvas.character(
                    l.field.x + dx,
                    l.field.y,
                    '─',
                    if self.error { "error" } else { "accent" },
                );
                canvas.character(
                    l.field.x + dx,
                    l.field.y + 2,
                    '─',
                    if self.error { "error" } else { "accent" },
                );
            }
        }
        let (_, shown, caret) = self.dialog_input_window(l.input.width);
        let background = if self.dialog.select_all {
            "selection"
        } else {
            "header"
        };
        canvas.fill(l.input.x, l.input.y, l.input.width, "text", "header");
        let placeholder = match prompt {
            Prompt::Marker => format!("M{}", self.session.markers.len() + 1),
            Prompt::Group => "e.g. AXI write channel".into(),
            Prompt::Goto => "e.g. 125 ns or 1250 ticks".into(),
            Prompt::FindValue => "e.g. 0x34C8, 0b1010, x or z".into(),
            Prompt::Filter => "e.g. tb.dut.wdata".into(),
            Prompt::Import => "Ctrl+V to paste session JSON".into(),
            Prompt::Rename => "Signal display name".into(),
        };
        canvas.text(
            l.input.x,
            l.input.y,
            l.input.width,
            if self.input.is_empty() {
                &placeholder
            } else {
                &shown
            },
            if self.input.is_empty() {
                "muted"
            } else {
                "text"
            },
            background,
            false,
        );
        if l.panel.height >= 12 {
            let message = if self.error {
                self.status.clone()
            } else if prompt == Prompt::Import {
                format!("{} bytes · Clipboard contents stay local", self.input.len())
            } else {
                "Click outside or press Esc to cancel".into()
            };
            canvas.text(
                x,
                l.panel.y + 8,
                w,
                &ellipsize(&message, w as usize),
                if self.error { "error" } else { "muted" },
                "panel",
                false,
            );
        }
        for (rect, label, focused, primary) in [
            (
                l.cancel,
                " Cancel ",
                self.dialog.focus == DialogFocus::Cancel,
                false,
            ),
            (
                l.submit,
                match prompt {
                    Prompt::Filter => " Apply ",
                    Prompt::Import => " Import ",
                    Prompt::FindValue => " Find ",
                    Prompt::Goto => " Go to ",
                    Prompt::Rename => " Rename ",
                    _ => " Create ",
                },
                self.dialog.focus == DialogFocus::Submit,
                true,
            ),
        ] {
            let label = if focused {
                format!("[{}]", label.trim())
            } else {
                label.to_owned()
            };
            canvas.fill(
                rect.x,
                rect.y,
                rect.width,
                "text",
                if focused || primary {
                    "selection"
                } else {
                    "header"
                },
            );
            canvas.text(
                rect.x + rect.width.saturating_sub(label.width() as u16) / 2,
                rect.y,
                rect.width,
                &label,
                if primary || focused { "accent" } else { "text" },
                if focused || primary {
                    "selection"
                } else {
                    "header"
                },
                primary || focused,
            );
        }
        if l.panel.height >= 12 {
            canvas.text(
                x,
                l.panel.y + l.panel.height - 2,
                w,
                "Enter to apply   Tab to move focus   Esc to cancel",
                "muted",
                "panel",
                false,
            );
        }
        (self.dialog.focus == DialogFocus::Input && l.input.width > 0).then_some((
            l.input.x + caret.min(l.input.width.saturating_sub(1)),
            l.input.y,
        ))
    }
}

fn previous_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .char_indices()
        .next_back()
        .map_or(0, |(offset, _)| offset)
}
fn next_boundary(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .chars()
        .next()
        .map_or(cursor, |ch| cursor + ch.len_utf8())
}
