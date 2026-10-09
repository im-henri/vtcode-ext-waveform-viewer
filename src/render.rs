use crate::model::{Radix, Value};
use crate::navigation::scrollbar;
use crate::viewer::{BrowserNode, Focus, PALETTE, Viewer, Watch, WaveRow};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use vtcode_extension_api::editor::{Frame, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub text: char,
    pub role: &'static str,
    pub background: &'static str,
    pub bold: bool,
    pub protected: bool,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            text: ' ',
            role: "text",
            background: "",
            bold: false,
            protected: false,
        }
    }
}
pub struct Canvas {
    pub width: u16,
    pub height: u16,
    pub cells: Vec<Cell>,
}
impl Canvas {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); usize::from(width) * usize::from(height)],
        }
    }
    fn reset(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let length = usize::from(width) * usize::from(height);
        let existing = self.cells.len().min(length);
        self.cells[..existing].fill(Cell::default());
        self.cells.resize(length, Cell::default());
    }
    pub fn get(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.cells
            .get_mut(usize::from(y) * usize::from(self.width) + usize::from(x))
    }
    #[allow(clippy::too_many_arguments)] // Cell geometry and style are explicit at drawing sites.
    pub fn text(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        text: &str,
        role: &'static str,
        background: &'static str,
        bold: bool,
    ) {
        let mut column = x;
        for ch in text.chars() {
            if ch.is_control() {
                continue;
            }
            let size = ch.width().unwrap_or(0) as u16;
            if size == 0 || column.saturating_add(size) > x.saturating_add(width).min(self.width) {
                if size > 0 {
                    break;
                } else {
                    continue;
                }
            }
            if let Some(cell) = self.get(column, y) {
                *cell = Cell {
                    text: ch,
                    role,
                    background,
                    bold,
                    protected: true,
                };
            }
            if size == 2
                && let Some(cell) = self.get(column + 1, y)
            {
                *cell = Cell {
                    text: '\0',
                    role,
                    background,
                    bold,
                    protected: true,
                };
            }
            column += size;
        }
    }
    pub fn fill(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        role: &'static str,
        background: &'static str,
    ) {
        for column in x..x.saturating_add(width).min(self.width) {
            if let Some(cell) = self.get(column, y) {
                *cell = Cell {
                    role,
                    background,
                    ..Cell::default()
                };
            }
        }
    }
    pub fn character(&mut self, x: u16, y: u16, ch: char, role: &'static str) {
        if let Some(cell) = self.get(x, y) {
            cell.text = ch;
            cell.role = role;
            cell.protected = false;
        }
    }
    pub fn lines(&self) -> Vec<Vec<Span>> {
        let mut lines = Vec::new();
        self.write_lines(&mut lines);
        lines
    }
    fn write_lines(&self, lines: &mut Vec<Vec<Span>>) {
        lines.resize_with(usize::from(self.height), Vec::new);
        for (row, spans) in self
            .cells
            .chunks(usize::from(self.width.max(1)))
            .zip(lines.iter_mut())
        {
            let mut count: usize = 0;
            for cell in row {
                if cell.text == '\0' {
                    continue;
                }
                if let Some(last) = count.checked_sub(1).and_then(|index| spans.get_mut(index))
                    && last.role == cell.role
                    && last.background == cell.background
                    && last.bold == cell.bold
                {
                    last.text.push(cell.text);
                } else {
                    if count == spans.len() {
                        spans.push(Span::default());
                    }
                    let span = &mut spans[count];
                    span.text.clear();
                    span.text.push(cell.text);
                    span.role.clear();
                    span.role.push_str(cell.role);
                    span.background.clear();
                    span.background.push_str(cell.background);
                    span.bold = cell.bold;
                    span.underline = false;
                    count += 1;
                }
            }
            spans.truncate(count);
        }
        // Stay inside the host's span budget for pathological, alternating
        // X/Z patterns on very large terminals. Preserve text and backgrounds.
        if lines.iter().map(Vec::len).sum::<usize>() > 32_000 {
            for line in lines {
                let mut compact = Vec::<Span>::new();
                for span in line.drain(..) {
                    if let Some(last) = compact.last_mut()
                        && last.background == span.background
                    {
                        last.text.push_str(&span.text);
                    } else {
                        compact.push(Span {
                            role: "text".into(),
                            ..span
                        });
                    }
                }
                *line = compact;
            }
        }
    }
}

/// Scratch storage shared by successive pane renders. No waveform/session
/// state is cached: every cell is repainted, while allocation capacity is reused.
pub struct Renderer {
    canvas: Canvas,
    frame: Frame,
}
impl Default for Renderer {
    fn default() -> Self {
        Self {
            canvas: Canvas::new(0, 0),
            frame: Frame::default(),
        }
    }
}
impl Renderer {
    pub fn render(&mut self, viewer: &Viewer) -> &Frame {
        viewer.render_into(&mut self.canvas, &mut self.frame);
        &self.frame
    }
}

impl Viewer {
    pub fn render(&self) -> Frame {
        let mut renderer = Renderer::default();
        renderer.render(self);
        renderer.frame
    }
    fn render_into(&self, canvas: &mut Canvas, frame: &mut Frame) {
        let width = self.width.min(512);
        let height = self.height.min(200);
        canvas.reset(width, height);
        if width == 0 || height == 0 {
            *frame = Frame {
                width: self.width,
                height: self.height,
                ..Frame::default()
            };
            return;
        }
        let l = self.layout();
        let name = self.path.rsplit(['/', '\\']).next().unwrap_or("Waveform");
        canvas.fill(0, 0, width, "text", "header");
        let title = format!("∿  {name}");
        canvas.text(
            1,
            0,
            width.saturating_sub(2),
            &ellipsize(&title, width.saturating_sub(2) as usize),
            "accent",
            "header",
            true,
        );
        let meta_x = title.width().saturating_add(3).min(width as usize) as u16;
        canvas.text(
            meta_x,
            0,
            width.saturating_sub(meta_x + 1),
            &format!(
                "·  {}  ·  {} signals  ·  {} / tick",
                self.wave.format,
                self.wave.signals.len(),
                self.wave.timescale.label()
            ),
            "muted",
            "header",
            false,
        );
        for button in self.toolbar() {
            let hovered = self
                .hover
                .is_some_and(|h| h.y == 1 && h.x >= button.x && h.x < button.x + button.width);
            canvas.text(
                button.x,
                1,
                button.width,
                &button.label,
                if !button.enabled {
                    "muted"
                } else if button.active || hovered {
                    "accent"
                } else {
                    "text"
                },
                if hovered && button.enabled {
                    "selection"
                } else if button.active {
                    "header"
                } else {
                    ""
                },
                button.enabled && (button.active || hovered),
            );
        }
        let a = self.wave.timescale.format(self.session.cursor);
        let b = self
            .session
            .alt_cursor
            .map(|t| self.wave.timescale.format(t))
            .unwrap_or_else(|| "—".into());
        let delta = self
            .session
            .alt_cursor
            .map(|t| self.wave.timescale.format(t.abs_diff(self.session.cursor)))
            .unwrap_or_else(|| "—".into());
        let measurement = [
            (format!("A {a}   "), "accent"),
            (format!("B {b}   "), "warning"),
            (format!("Δ {delta}   "), "text"),
            (
                format!(
                    "{}snap {}",
                    self.window_readout(),
                    if self.session.snap { "on" } else { "off" }
                ),
                "muted",
            ),
        ];
        let mut measure_x = 1;
        for (text, role) in measurement {
            canvas.text(
                measure_x,
                2,
                width.saturating_sub(measure_x + 1),
                &text,
                role,
                "",
                false,
            );
            measure_x = measure_x.saturating_add(text.width() as u16);
        }
        canvas.fill(0, 3, width, "muted", "header");
        if let Some((x, w)) = self.snap_control() {
            let hovered = self
                .hover
                .is_some_and(|h| h.y == 2 && h.x >= x && h.x < x + w);
            canvas.text(
                x,
                2,
                w,
                if self.session.snap {
                    "snap on"
                } else {
                    "snap off"
                },
                if self.session.snap || hovered {
                    "accent"
                } else {
                    "muted"
                },
                if hovered { "selection" } else { "" },
                hovered,
            );
        }
        canvas.text(
            l.browser,
            3,
            l.name,
            &if self.selection_count() > 1 {
                let count = self.selection_count();
                if l.name >= 24 {
                    format!("  Signals · {count} selected")
                } else if l.name >= 15 {
                    format!(" {count} selected")
                } else {
                    format!(" {count} sel.")
                }
            } else {
                format!(
                    "  Signals · {}",
                    self.session
                        .watches
                        .iter()
                        .filter(|w| matches!(w, Watch::Signal { .. }))
                        .count()
                )
            },
            if self.focus == Focus::Waves {
                "accent"
            } else {
                "text"
            },
            "header",
            true,
        );
        canvas.text(
            l.browser + l.name,
            3,
            l.value,
            if l.value < 11 {
                " Value"
            } else if self
                .hover
                .is_some_and(|h| h.row.is_some() && h.x >= l.wave_x)
            {
                " Value @ t"
            } else {
                " Value @ A"
            },
            "text",
            "header",
            true,
        );
        if l.browser > 0 {
            for y in 4..l.body_y + l.body_height {
                canvas.fill(0, y, l.browser, "muted", "panel");
            }
            canvas.text(
                1,
                3,
                l.browser.saturating_sub(2),
                "Hierarchy  / filter",
                if self.focus == Focus::Browser {
                    "accent"
                } else {
                    "text"
                },
                "header",
                true,
            );
            let filter = if self.filter.is_empty() {
                "All signals".to_string()
            } else {
                format!("⌕ {}", self.filter)
            };
            canvas.text(
                1,
                4,
                l.browser
                    .saturating_sub(if self.filter.is_empty() { 2 } else { 5 }),
                &ellipsize(
                    &filter,
                    l.browser
                        .saturating_sub(if self.filter.is_empty() { 2 } else { 5 })
                        as usize,
                ),
                "muted",
                "panel",
                false,
            );
            if !self.filter.is_empty() {
                let hovered = self
                    .hover
                    .is_some_and(|h| h.y == 4 && h.x >= l.browser - 3 && h.x < l.browser);
                canvas.text(
                    l.browser - 3,
                    4,
                    2,
                    "× ",
                    if hovered { "accent" } else { "muted" },
                    if hovered { "selection" } else { "panel" },
                    hovered,
                );
            }
            for (i, node) in self
                .browser_nodes
                .iter()
                .enumerate()
                .skip(self.browser_scroll)
                .take(l.body_height as usize)
            {
                let y = l.body_y + (i - self.browser_scroll) as u16;
                let selected = i == self.browser_selection && self.focus == Focus::Browser;
                let hovered = self.hover.is_some_and(|h| h.x < l.browser && h.y == y);
                let bg = if selected || hovered {
                    "selection"
                } else {
                    "panel"
                };
                canvas.fill(0, y, l.browser, "muted", bg);
                let (depth, text, role) = match node {
                    BrowserNode::Scope(id, depth) => (
                        *depth,
                        format!(
                            "{} {}",
                            if self.expanded_scopes.contains(id) || !self.filter.is_empty() {
                                "▾"
                            } else {
                                "▸"
                            },
                            self.wave.scopes[*id].name
                        ),
                        "accent",
                    ),
                    BrowserNode::Signal(id, depth) => {
                        let signal = &self.wave.signals[*id];
                        let watched = self.session.watches.iter().any(
                            |w| matches!(w, Watch::Signal { path, .. } if path == &signal.path),
                        );
                        let label = if self.filter.is_empty() {
                            signal.name.clone()
                        } else {
                            signal.path.clone()
                        };
                        (
                            *depth,
                            format!(
                                "{} {}{}",
                                if watched { "✓" } else { "+" },
                                label,
                                if signal.width > 1 {
                                    format!(" [{}]", signal.width)
                                } else {
                                    String::new()
                                }
                            ),
                            if watched { "success" } else { "text" },
                        )
                    }
                };
                let x = 1 + (depth as u16).saturating_mul(2).min(l.browser / 3);
                canvas.text(
                    x,
                    y,
                    l.browser.saturating_sub(x + 1),
                    &ellipsize(&text, l.browser.saturating_sub(x + 1) as usize),
                    role,
                    bg,
                    false,
                );
            }
            if self.browser_nodes.is_empty() && !self.filter.is_empty() {
                canvas.text(
                    1,
                    l.body_y + 1,
                    l.browser.saturating_sub(2),
                    "No matching signals",
                    "text",
                    "panel",
                    false,
                );
                canvas.text(
                    1,
                    l.body_y + 3,
                    l.browser.saturating_sub(2),
                    "Clear × or / to edit",
                    "muted",
                    "panel",
                    false,
                );
            }
            for y in 3..height.saturating_sub(2) {
                canvas.character(l.browser - 1, y, '│', "muted");
            }
            if let Some((start, size)) =
                scrollbar(self.browser_nodes.len(), self.browser_scroll, l.body_height)
            {
                for y in l.body_y + start..l.body_y + start + size {
                    canvas.character(l.browser - 1, y, '┃', "accent");
                }
            }
        }
        // A 1/2/5 tick progression keeps rulers useful at every zoom level.
        let step = tick_step(self.session.span, l.wave_width);
        let first = self.session.start.div_ceil(step).saturating_mul(step);
        let right = self.session.start.saturating_add(self.session.span);
        let mut tick = first;
        for _ in 0..128 {
            if tick > right {
                break;
            }
            if let Some(x) = self.column_at(tick) {
                let label = self.wave.timescale.format(tick);
                // An incomplete ruler label looks like a different timestamp.
                if label.width() <= usize::from(width.saturating_sub(x)) {
                    canvas.text(x, 4, label.width() as u16, &label, "muted", "", false);
                }
                for y in l.body_y..l.body_y + l.body_height {
                    canvas.character(x, y, '┊', "muted");
                }
            }
            let next = tick.saturating_add(step);
            if next <= tick {
                break;
            }
            tick = next;
        }
        let rows = self.display_rows();
        for row in rows.iter().filter(|r| {
            r.row.line as usize + r.row.height as usize > self.scroll
                && (r.row.line as usize) < self.scroll + l.body_height as usize
        }) {
            self.draw_row(canvas, row.row, row.preview);
        }
        for y in 3..l.body_y + l.body_height {
            if l.name > 0 {
                let active = matches!(
                    self.control_drag,
                    Some(crate::controls::ControlDrag::Column {
                        column: crate::controls::Column::Names,
                        ..
                    })
                ) || self
                    .hover
                    .is_some_and(|h| h.x == l.browser + l.name - 1 && h.y >= 3);
                canvas.character(
                    l.browser + l.name - 1,
                    y,
                    if y == 3 && active { '↔' } else { '│' },
                    if active { "accent" } else { "muted" },
                );
            }
            if l.value > 0 {
                let active = matches!(
                    self.control_drag,
                    Some(crate::controls::ControlDrag::Column {
                        column: crate::controls::Column::Values,
                        ..
                    })
                ) || self
                    .hover
                    .is_some_and(|h| h.x == l.wave_x - 1 && matches!(h.y, 3 | 4));
                canvas.character(
                    l.wave_x - 1,
                    y,
                    if y == 3 && active { '↔' } else { '│' },
                    if active { "accent" } else { "muted" },
                );
            }
        }
        let total = rows
            .last()
            .map(|r| usize::from(r.row.line + r.row.height))
            .unwrap_or(0);
        if l.value > 0
            && let Some((start, size)) = scrollbar(total, self.scroll, l.body_height)
        {
            for y in l.body_y + start..l.body_y + start + size {
                canvas.character(l.wave_x - 1, y, '┃', "accent");
            }
        }
        if self.session.watches.is_empty() && rows.is_empty() && self.loading.is_none() {
            canvas.text(l.browser + 2, l.body_y + 2, width.saturating_sub(l.browser + 4), "Drag signals here from All signals.  / searches · Enter adds · Shift+Enter adds a scope", "muted", "", false);
        }
        if let Some((done, total)) = self.loading {
            let percentage = if total == 0 {
                0
            } else {
                (u128::from(done) * 100 / u128::from(total)) as u64
            };
            canvas.text(
                l.browser + 3,
                l.body_y + 2,
                width.saturating_sub(l.browser + 6),
                &format!("Reading waveform… {percentage}%   ({done} / {total} bytes)"),
                "accent",
                "",
                true,
            );
            canvas.text(
                l.browser + 3,
                l.body_y + 4,
                width.saturating_sub(l.browser + 6),
                "Work runs in the background.  Esc cancels.",
                "muted",
                "",
                false,
            );
        }
        // Markers and cursors are composited after traces, so they never disappear
        // behind a high level or a bus label.
        for marker in &self.session.markers {
            if let Some(x) = self.column_at(marker.time) {
                for y in l.body_y..l.body_y + l.body_height {
                    if let Some(cell) = canvas.get(x, y)
                        && !cell.protected
                        && matches!(cell.text, ' ' | '┊')
                    {
                        cell.text = '┆';
                        cell.role = "warning";
                    }
                }
                canvas.text(
                    x,
                    3,
                    width.saturating_sub(x),
                    &format!("◆{}", marker.name),
                    "warning",
                    "header",
                    true,
                );
            }
        }
        self.draw_range(canvas);
        for (time, role, label) in [
            (self.session.alt_cursor, "warning", "B"),
            (Some(self.session.cursor), "accent", "A"),
        ] {
            if let Some(time) = time
                && let Some(x) = self.column_at(time)
            {
                for y in l.body_y..l.body_y + l.body_height {
                    if let Some(cell) = canvas.get(x, y) {
                        cell.background = if label == "A" { "selection" } else { "match" };
                        // Preserve bus values and contours underneath the measurement guide.
                        if !cell.protected && matches!(cell.text, ' ' | '┊' | '┆') {
                            cell.text = '┆';
                            cell.role = role;
                        }
                    }
                }
            }
        }
        for handle in self.cursor_handles() {
            let hovered = self
                .hover
                .is_some_and(|h| h.y == 3 && h.x >= handle.x && h.x < handle.x + handle.width)
                || matches!(self.control_drag, Some(crate::controls::ControlDrag::Cursor { alt, .. }) if alt == handle.alt);
            canvas.text(
                handle.x,
                3,
                handle.width,
                if handle.width >= 3 {
                    if handle.alt { "[B]" } else { "[A]" }
                } else if handle.alt {
                    "B"
                } else {
                    "A"
                },
                if handle.alt { "warning" } else { "accent" },
                if hovered {
                    if handle.alt { "match" } else { "selection" }
                } else {
                    "header"
                },
                true,
            );
        }
        if height >= 8 {
            self.draw_overview(canvas);
        }
        if height >= 2 {
            canvas.fill(0, height - 2, width, "muted", "header");
            let status = self
                .control_hint()
                .or_else(|| self.drag_hint())
                .or_else(|| self.range_hint())
                .or_else(|| {
                    self.overview_drag
                        .map(|_| "Drag overview to pan · Esc restores the previous window".into())
                })
                .or_else(|| (!self.error).then(|| self.hover_hint()).flatten())
                .unwrap_or_else(|| self.status.clone());
            canvas.text(
                1,
                height - 2,
                width.saturating_sub(2),
                &ellipsize(&status, width.saturating_sub(2) as usize),
                if self.error { "error" } else { "text" },
                "header",
                false,
            );
        }
        let hint = if self.signal_drag.as_ref().is_some_and(|d| d.active) {
            "Drop anywhere in Signals or the timeline · Hold near an edge to scroll · Wheel scrolls signals · Esc cancels"
        } else if self.selection_count() > 1 {
            "Right-click selection: format, color & size   Delete removes   Ctrl+Z undo   Esc selects one   ? help"
        } else {
            "Ctrl/Shift+click names: select waves   Right-click: display   ←/→ edge   ± zoom   Shift+wheel pan   ? help"
        };
        canvas.text(
            1,
            height - 1,
            width.saturating_sub(2),
            hint,
            "muted",
            "",
            false,
        );
        if !self.help
            && self.prompt.is_none()
            && self.context_menu.is_none()
            && self.signal_drag.is_none()
            && self.drag.is_none()
            && self.overview_drag.is_none()
            && self.control_drag.is_none()
        {
            self.draw_hover(canvas);
        }
        self.draw_drag_badge(canvas);
        self.draw_context_menu(canvas);
        if self.help {
            self.draw_help(canvas);
        }
        frame.cursor = self.draw_dialog(canvas);
        canvas.write_lines(&mut frame.lines);
        frame.width = self.width;
        frame.height = self.height;
        frame.document = self.document;
        frame.title = Some(format!("{name} · Waveform"));
    }

    fn draw_row(&self, canvas: &mut Canvas, row: WaveRow, preview: bool) {
        let l = self.layout();
        let item = if preview {
            self.signal_drag.as_ref().map(|d| &d.watch)
        } else {
            self.session.watches.get(row.watch)
        };
        let Some(item) = item else {
            return;
        };
        let relative_y = i32::from(l.body_y) + i32::from(row.line) - self.scroll as i32;
        let selected = !preview && self.is_row_selected(row);
        let hovered = !preview
            && self.hover.is_some_and(|h| {
                h.row
                    .is_some_and(|r| r.watch == row.watch && r.bit == row.bit)
            });
        let bg = if selected || preview {
            "selection"
        } else if hovered {
            "panel"
        } else {
            ""
        };
        for offset in 0..row.height {
            let y = relative_y + i32::from(offset);
            if y >= i32::from(l.body_y) && y < i32::from(l.body_y + l.body_height) {
                canvas.fill(
                    l.browser,
                    y as u16,
                    if preview {
                        canvas.width.saturating_sub(l.browser)
                    } else {
                        l.name + l.value
                    },
                    "text",
                    bg,
                );
                if selected && l.name > 0 {
                    canvas.character(l.browser, y as u16, '▎', "accent");
                }
            }
        }
        let in_view = relative_y >= i32::from(l.body_y);
        match item {
            Watch::Group { name, collapsed } => {
                let collapsed = *collapsed && self.preview_group() != Some(row.watch);
                if in_view {
                    let group_bg = if selected { "selection" } else { "header" };
                    canvas.fill(
                        l.browser,
                        relative_y as u16,
                        self.width.saturating_sub(l.browser),
                        "accent",
                        group_bg,
                    );
                    canvas.text(
                        l.browser,
                        relative_y as u16,
                        self.width.saturating_sub(l.browser),
                        &format!("{} {name}", if collapsed { "▸" } else { "▾" }),
                        "accent",
                        group_bg,
                        true,
                    );
                }
            }
            Watch::Divider => {
                if in_view {
                    if selected {
                        canvas.fill(
                            l.browser,
                            relative_y as u16,
                            canvas.width.saturating_sub(l.browser),
                            "muted",
                            "selection",
                        );
                    }
                    for x in l.browser..canvas.width {
                        canvas.character(x, relative_y as u16, '─', "muted");
                    }
                }
            }
            Watch::Signal {
                path,
                bit,
                radix,
                color,
                expanded,
                analog,
                ..
            } => {
                let id = self.signal_index.get(path).copied();
                let role = PALETTE[usize::from(*color).min(7)];
                let effective_bit = row.bit.or(*bit);
                let bus = effective_bit.is_none()
                    && id.is_some_and(|id| {
                        self.wave.signals[id].width > 1 && self.wave.signals[id].kind != "real"
                    });
                let name = format!(
                    "{} {}",
                    if preview {
                        "↓"
                    } else if bus {
                        if *expanded { "▾" } else { "▸" }
                    } else {
                        " "
                    },
                    self.watch_label_for(item, row.bit)
                );
                let label_y = relative_y
                    + i32::from(row.height / 2).saturating_sub(i32::from(row.height == 2));
                if label_y >= i32::from(l.body_y) && label_y < i32::from(l.body_y + l.body_height) {
                    let badge = if l.name >= 23
                        && effective_bit.is_none()
                        && id.is_some_and(|id| self.wave.signals[id].width > 1)
                    {
                        Some(if *analog {
                            "ANLG"
                        } else {
                            match radix {
                                Radix::Hex => "HEX",
                                Radix::Binary => "BIN",
                                Radix::Unsigned => "DEC",
                                Radix::Signed => "SGN",
                                Radix::Octal => "OCT",
                                Radix::Ascii => "TEXT",
                            }
                        })
                    } else {
                        None
                    };
                    canvas.text(
                        l.browser + 1,
                        label_y as u16,
                        l.name.saturating_sub(if badge.is_some() { 7 } else { 2 }),
                        &ellipsize(
                            &name,
                            l.name.saturating_sub(if badge.is_some() { 7 } else { 2 }) as usize,
                        ),
                        role,
                        bg,
                        selected || preview,
                    );
                    if let Some(badge) = badge {
                        canvas.text(
                            l.browser + l.name - 5,
                            label_y as u16,
                            4,
                            badge,
                            "muted",
                            bg,
                            false,
                        );
                    }
                    let time = self
                        .hover
                        .filter(|h| h.row.is_some() && h.x >= l.wave_x)
                        .map(|h| self.hover_time(h))
                        .unwrap_or(self.session.cursor);
                    if let Some(value) = id
                        .and_then(|id| self.wave.traces[self.wave.signals[id].trace].value_at(time))
                    {
                        let value = effective_bit
                            .map(|bit| value.bit(bit))
                            .unwrap_or_else(|| value.clone());
                        let value_role = match value.logic() {
                            'x' => "error",
                            'z' => "warning",
                            _ => role,
                        };
                        let radix = if effective_bit.is_some() {
                            Radix::Binary
                        } else {
                            *radix
                        };
                        canvas.text(
                            l.browser + l.name + 1,
                            label_y as u16,
                            l.value.saturating_sub(2),
                            &ellipsize(
                                &display_value(&value, radix),
                                l.value.saturating_sub(2) as usize,
                            ),
                            value_role,
                            bg,
                            selected || preview,
                        );
                    } else {
                        canvas.text(
                            l.browser + l.name + 1,
                            label_y as u16,
                            l.value.saturating_sub(2),
                            "—",
                            "muted",
                            bg,
                            false,
                        );
                    }
                }
                if let Some(id) = id {
                    self.draw_trace(
                        canvas,
                        row,
                        relative_y,
                        id,
                        effective_bit,
                        *radix,
                        role,
                        *analog && effective_bit.is_none(),
                    );
                }
            }
        }
        if !preview && matches!(item, Watch::Signal { .. }) && (hovered || selected) {
            let trace_bg = if hovered || selected && self.selection_count() > 1 {
                "selection"
            } else {
                "panel"
            };
            for offset in 0..row.height {
                let y = relative_y + i32::from(offset);
                if y >= i32::from(l.body_y) && y < i32::from(l.body_y + l.body_height) {
                    for x in l.wave_x..canvas.width {
                        if let Some(cell) = canvas.get(x, y as u16) {
                            cell.background = trace_bg;
                        }
                    }
                }
            }
        }
        if preview {
            // Trace labels use their usual semantic colors. Keep the entire
            // proposed row on one continuous background, including bus text.
            for offset in 0..row.height {
                let y = relative_y + i32::from(offset);
                if y >= i32::from(l.body_y) && y < i32::from(l.body_y + l.body_height) {
                    for x in l.browser..canvas.width {
                        if let Some(cell) = canvas.get(x, y as u16) {
                            cell.background = "selection";
                        }
                    }
                    let (left, right) = if row.height == 1 {
                        ('▸', '◂')
                    } else if offset == 0 {
                        ('╭', '╮')
                    } else if offset + 1 == row.height {
                        ('╰', '╯')
                    } else {
                        ('│', '│')
                    };
                    canvas.character(l.browser, y as u16, left, "accent");
                    canvas.character(canvas.width.saturating_sub(1), y as u16, right, "accent");
                }
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn draw_trace(
        &self,
        canvas: &mut Canvas,
        row: WaveRow,
        y: i32,
        signal: usize,
        bit: Option<u32>,
        radix: Radix,
        role: &'static str,
        analog: bool,
    ) {
        let l = self.layout();
        let trace = &self.wave.traces[self.wave.signals[signal].trace];
        let digital = bit.is_some()
            || self.wave.signals[signal].width == 1 && self.wave.signals[signal].kind != "real";
        if !digital && !analog {
            if row.height == 1 {
                self.draw_compact_bus(canvas, y, signal, radix, role);
            } else {
                self.draw_bus(canvas, y, signal, radix, role);
            }
            return;
        }
        let mut range = (f64::INFINITY, f64::NEG_INFINITY);
        if analog {
            let first = trace.index_at(self.session.start).saturating_sub(1);
            let end = trace.index_at(self.session.start.saturating_add(self.session.span));
            // Sampling only visible columns makes analog rendering independent of
            // the total number of transitions in the dump.
            for x in 0..l.wave_width {
                let time = self.time_at(l.wave_x + x);
                if let Some(number) = trace
                    .value_at(time)
                    .and_then(|v| v.number(radix == Radix::Signed))
                {
                    range.0 = range.0.min(number);
                    range.1 = range.1.max(number);
                }
            }
            if let Some(number) = trace
                .changes
                .get(first.min(end))
                .and_then(|c| c.value.number(radix == Radix::Signed))
            {
                range.0 = range.0.min(number);
                range.1 = range.1.max(number);
            }
        }
        let draw = |canvas: &mut Canvas, x: u16, offset: u16, ch: char, role| {
            let y = y + i32::from(offset);
            if y >= i32::from(l.body_y) && y < i32::from(l.body_y + l.body_height) {
                canvas.character(x, y as u16, ch, role);
            }
        };
        let mut previous_index = trace.index_at(self.session.start);
        let mut previous_logic = 'x';
        let mut previous_position = None;
        for dx in 0..l.wave_width {
            let x = l.wave_x + dx;
            let time = self.time_at(x);
            let next_time = if dx + 1 < l.wave_width {
                self.time_at(x + 1)
            } else {
                time
            };
            let index = trace.index_at(time);
            let edge_index = trace.index_at(next_time);
            let value = index
                .checked_sub(1)
                .and_then(|index| trace.changes.get(index))
                .map(|c| &c.value);
            let bit_value = bit.and_then(|bit| value.map(|value| value.bit(bit)));
            let value = bit_value.as_ref().or(value);
            let logic = value.map(Value::logic).unwrap_or('x');
            let value_role = match logic {
                'x' => "error",
                'z' => "warning",
                _ => role,
            };
            let changed = index != previous_index && logic != previous_logic;
            // Dense pulses are marked, rather than aliased into a constant line.
            let dense = (edge_index.saturating_sub(index) > 1
                || index.saturating_sub(previous_index) > 1)
                && bit.is_none();
            if analog {
                if let Some(number) = value.and_then(|v| v.number(radix == Radix::Signed)) {
                    let position = if range.1 > range.0 {
                        ((number - range.0) / (range.1 - range.0) * f64::from(row.height - 1))
                            .round() as u16
                    } else {
                        row.height / 2
                    };
                    for offset in 0..row.height {
                        draw(
                            canvas,
                            x,
                            offset,
                            if offset == row.height - 1 - position {
                                '─'
                            } else {
                                ' '
                            },
                            role,
                        );
                    }
                    if let Some(previous) = previous_position
                        && position != previous
                    {
                        for level in position.min(previous)..position.max(previous) {
                            draw(canvas, x, row.height - 1 - level, '│', role);
                        }
                        draw(
                            canvas,
                            x,
                            row.height - 1 - previous,
                            if position > previous { '┘' } else { '┐' },
                            role,
                        );
                        draw(
                            canvas,
                            x,
                            row.height - 1 - position,
                            if position > previous { '┌' } else { '└' },
                            role,
                        );
                    }
                    previous_position = Some(position);
                } else {
                    draw(canvas, x, 1, 'x', value_role);
                }
            } else if digital {
                if row.height == 1 {
                    let ch = if dense {
                        '┆'
                    } else if changed
                        && matches!(logic, '0' | '1')
                        && matches!(previous_logic, '0' | '1')
                    {
                        '│'
                    } else {
                        // Cell-wide rails keep both levels aligned without the
                        // font-dependent spacing of underscores and overlines.
                        match logic {
                            '0' => '▁',
                            '1' => '▔',
                            'z' => '┄',
                            _ => 'x',
                        }
                    };
                    draw(canvas, x, 0, ch, value_role);
                    previous_index = index;
                    previous_logic = logic;
                    continue;
                }
                if dense {
                    draw(canvas, x, 0, '┆', value_role);
                    draw(canvas, x, 1, '┆', value_role);
                    previous_index = index;
                    previous_logic = logic;
                    continue;
                }
                match logic {
                    '0' => {
                        draw(canvas, x, 0, ' ', role);
                        draw(canvas, x, 1, '─', role);
                    }
                    '1' => {
                        draw(canvas, x, 0, '─', role);
                        draw(canvas, x, 1, ' ', role);
                    }
                    'z' => {
                        draw(canvas, x, 0, 'z', value_role);
                        draw(canvas, x, 1, '┄', value_role);
                    }
                    _ => {
                        draw(canvas, x, 0, 'x', value_role);
                        draw(canvas, x, 1, 'x', value_role);
                    }
                }
                if changed && matches!(logic, '0' | '1') && matches!(previous_logic, '0' | '1') {
                    draw(
                        canvas,
                        x,
                        0,
                        if logic == '1' { '┌' } else { '┐' },
                        value_role,
                    );
                    draw(
                        canvas,
                        x,
                        1,
                        if logic == '1' { '┘' } else { '└' },
                        value_role,
                    );
                }
            }
            previous_index = index;
            previous_logic = logic;
        }
    }

    fn draw_bus(
        &self,
        canvas: &mut Canvas,
        y: i32,
        signal: usize,
        radix: Radix,
        role: &'static str,
    ) {
        let l = self.layout();
        let trace = &self.wave.traces[self.wave.signals[signal].trace];
        let draw = |canvas: &mut Canvas, x, offset, ch, role| {
            let y = y + offset;
            if y >= i32::from(l.body_y) && y < i32::from(l.body_y + l.body_height) {
                canvas.character(x, y as u16, ch, role);
            }
        };
        // Work is bounded by screen columns, independent of transition count.
        // Sampling at the same times as hover makes transfer boundaries agree
        // with the value inspector, including changes inside a terminal cell.
        let mut runs = Vec::new();
        let mut start = 0;
        let mut previous = trace.index_at(self.session.start);
        let mut rail_role = "muted";
        let samples = (0..l.wave_width)
            .map(|dx| trace.index_at(self.time_at(l.wave_x + dx)))
            .collect::<Vec<_>>();
        let mut joins = Vec::new();
        let mut tapered = vec![false; l.wave_width as usize];
        for dx in 0..l.wave_width {
            let index = samples[dx as usize];
            let changed = index != previous;
            if dx == 0 || changed {
                rail_role = index
                    .checked_sub(1)
                    .and_then(|i| trace.changes.get(i))
                    .map(|c| match c.value.logic() {
                        'x' => "error",
                        'z' => "warning",
                        _ => "muted",
                    })
                    .unwrap_or("muted");
            }
            draw(canvas, l.wave_x + dx, 0, '─', rail_role);
            draw(canvas, l.wave_x + dx, 2, '─', rail_role);
            if changed {
                let dense = index.saturating_sub(previous) > 1;
                // A narrow neck joins the two bus envelopes. At sub-cell
                // density use a compact boundary instead of overlapping caps.
                let taper = !dense
                    && dx >= 2
                    && samples[dx as usize - 2] == previous
                    && samples
                        .get(dx as usize + 1)
                        .is_none_or(|next| *next == index);
                tapered[dx as usize] = taper;
                joins.push((dx, taper, dense, rail_role));
                runs.push((start, dx, previous));
                start = dx;
            }
            previous = index;
        }
        runs.push((start, l.wave_width, previous));
        for (dx, taper, dense, rail_role) in joins {
            let x = l.wave_x + dx;
            if taper {
                for (offset, left, right) in [(0, '╮', '╭'), (1, '├', '┤'), (2, '╯', '╰')]
                {
                    draw(canvas, x - 1, offset, left, rail_role);
                    draw(canvas, x, offset, right, rail_role);
                }
            } else {
                draw(canvas, x, 0, '┬', rail_role);
                draw(canvas, x, 1, if dense { '┆' } else { '│' }, rail_role);
                draw(canvas, x, 2, '┴', rail_role);
            }
        }
        if y + 1 < i32::from(l.body_y) || y + 1 >= i32::from(l.body_y + l.body_height) {
            return;
        }
        for (start, end, index) in runs {
            let inset = u16::from(tapered.get(end as usize).copied().unwrap_or(false));
            let room = end.saturating_sub(start + 2 + inset);
            if room == 0 {
                continue;
            }
            let Some(value) = index
                .checked_sub(1)
                .and_then(|i| trace.changes.get(i))
                .map(|c| &c.value)
            else {
                continue;
            };
            let label = display_value(value, radix);
            let size = label.width();
            // Labels are atomic: never clip, abbreviate, overlap a boundary, or
            // spill into the neighboring transfer. Narrow intervals stay empty.
            if size > 0 && size <= usize::from(room) {
                let x = l.wave_x + start + 1 + (room - size as u16) / 2;
                canvas.text(
                    x,
                    (y + 1) as u16,
                    size as u16,
                    &label,
                    value_role(value, role),
                    "",
                    false,
                );
            }
        }
    }

    fn draw_compact_bus(
        &self,
        canvas: &mut Canvas,
        y: i32,
        signal: usize,
        radix: Radix,
        role: &'static str,
    ) {
        let l = self.layout();
        if y < i32::from(l.body_y) || y >= i32::from(l.body_y + l.body_height) {
            return;
        }
        let y = y as u16;
        let trace = &self.wave.traces[self.wave.signals[signal].trace];
        let samples: Vec<_> = (0..l.wave_width)
            .map(|dx| trace.index_at(self.time_at(l.wave_x + dx)))
            .collect();
        let mut runs = vec![];
        let mut start = 0;
        let mut previous = trace.index_at(self.session.start);
        canvas.fill(l.wave_x, y, l.wave_width, "muted", "panel");
        for (dx, &index) in samples.iter().enumerate() {
            let dx = dx as u16;
            let value = index
                .checked_sub(1)
                .and_then(|i| trace.changes.get(i))
                .map(|c| &c.value);
            let rail = value.map(|v| value_role(v, "muted")).unwrap_or("muted");
            canvas.character(l.wave_x + dx, y, '═', rail);
            if index != previous {
                let dense = index.saturating_sub(previous) > 1;
                let taper = !dense
                    && dx >= 2
                    && samples[usize::from(dx - 2)] == previous
                    && samples
                        .get(usize::from(dx + 1))
                        .is_none_or(|next| *next == index);
                if taper {
                    canvas.character(l.wave_x + dx - 1, y, '>', rail);
                    canvas.character(l.wave_x + dx, y, '<', rail);
                } else {
                    canvas.character(l.wave_x + dx, y, if dense { '┆' } else { '│' }, rail);
                }
                runs.push((start, dx, previous, u16::from(taper)));
                start = dx;
            }
            previous = index;
        }
        runs.push((start, l.wave_width, previous, 0));
        for (start, end, index, cap) in runs {
            let room = end.saturating_sub(start + 4 + cap);
            let Some(value) = index
                .checked_sub(1)
                .and_then(|i| trace.changes.get(i))
                .map(|c| &c.value)
            else {
                continue;
            };
            let label = display_value(value, radix);
            let size = label.width() as u16;
            if size == 0 || size > room {
                continue;
            }
            let x = l.wave_x + start + 2 + (room - size) / 2;
            canvas.text(
                x - 1,
                y,
                size + 2,
                &format!(" {label} "),
                value_role(value, role),
                "panel",
                false,
            );
        }
    }

    fn watch_label(&self, row: WaveRow) -> String {
        self.session
            .watches
            .get(row.watch)
            .map(|w| self.watch_label_for(w, row.bit))
            .unwrap_or_default()
    }
    pub(crate) fn watch_row_label(&self, row: WaveRow) -> String {
        match self.session.watches.get(row.watch) {
            Some(Watch::Group { name, .. }) => name.clone(),
            _ => self.watch_label(row),
        }
    }

    fn watch_label_for(&self, watch: &Watch, row_bit: Option<u32>) -> String {
        let Watch::Signal {
            path, bit, label, ..
        } = watch
        else {
            return String::new();
        };
        let Some(&id) = self.signal_index.get(path) else {
            return path.clone();
        };
        let signal = &self.wave.signals[id];
        if let Some(bit) = row_bit.or(*bit) {
            return format!("{}[{}]", signal.name, signal.bit_name(bit));
        }
        if !label.is_empty() {
            return label.clone();
        }
        if self.session.full_names {
            return path.clone();
        }
        if signal.width > 1 && signal.kind != "real" {
            format!("{}[{}:{}]", signal.name, signal.msb, signal.lsb)
        } else {
            signal.name.clone()
        }
    }

    fn draw_hover(&self, canvas: &mut Canvas) {
        let Some(hover) = self.hover else {
            return;
        };
        let Some(row) = hover.row else {
            return;
        };
        let l = self.layout();
        if hover.x < l.browser + l.name {
            return;
        }
        let time = self.hover_time(hover);
        let Some(value) = self.watch_value(row.watch, row.bit, time) else {
            return;
        };
        let Some(Watch::Signal { radix, bit, .. }) = self.session.watches.get(row.watch) else {
            return;
        };
        let radix = if row.bit.or(*bit).is_some() {
            Radix::Binary
        } else {
            *radix
        };
        let label = display_value(&value, radix);
        let title = format!(
            "{}  ·  @ {}",
            if hover.x < l.wave_x {
                self.full_watch_label(row)
            } else {
                self.watch_label(row)
            },
            self.wave.timescale.format(time)
        );
        let detail = self
            .signal_id(row.watch)
            .and_then(|id| {
                let trace = &self.wave.traces[self.wave.signals[id].trace];
                let i = trace.index_at(time).checked_sub(1)?;
                let start = trace.changes[i].time;
                let end = trace
                    .changes
                    .get(i + 1)
                    .map(|c| c.time)
                    .unwrap_or(self.wave.end);
                Some(format!(
                    "{} → {}  ·  held {}",
                    self.wave.timescale.format(start),
                    self.wave.timescale.format(end),
                    self.wave.timescale.format(end.saturating_sub(start))
                ))
            })
            .unwrap_or_default();
        if hover.x >= l.wave_x {
            for y in l.body_y..l.body_y + l.body_height {
                if let Some(cell) = canvas.get(hover.x, y) {
                    if matches!(cell.background, "" | "panel") {
                        cell.background = "header";
                    }
                    if !cell.protected && matches!(cell.text, ' ' | '┊') {
                        cell.text = '┊';
                        cell.role = "text";
                    }
                }
            }
        }
        if l.wave_width < 28 || l.body_height < 9 {
            return;
        }
        let width = (title
            .width()
            .max(label.width() + radix.label().len() + 3)
            .max(detail.width()) as u16
            + 4)
        .clamp(28, 66)
        .min(l.wave_width);
        let inner = width - 4;
        let value_lines = wrap_value(
            &format!("{}  {label}", radix.label().to_uppercase()),
            inner as usize,
            3,
        );
        let clipped = value_lines.iter().map(|s| s.width()).sum::<usize>()
            < label.width() + radix.label().len() + 2;
        let height = 4 + value_lines.len() as u16 + u16::from(clipped);
        let row_top = i32::from(l.body_y) + i32::from(row.line) - self.scroll as i32;
        let below = (row_top + i32::from(row.height) + 1).max(i32::from(l.body_y)) as u16;
        let fits_below = below + height <= l.body_y + l.body_height;
        let fits_above = row_top >= i32::from(l.body_y + height + 1);
        let fits_right = hover.x.saturating_add(width + 2) < canvas.width;
        let fits_left = hover.x >= l.wave_x.saturating_add(width + 2);
        if !fits_below && !fits_above && !fits_right && !fits_left {
            return;
        }
        let y = if fits_below {
            below
        } else if fits_above {
            (row_top - i32::from(height) - 1).max(i32::from(l.body_y)) as u16
        } else {
            hover.y.min(l.body_y + l.body_height - height).max(l.body_y)
        };
        let x = if fits_right {
            hover.x + 2
        } else if fits_left {
            hover.x - width - 2
        } else {
            canvas.width - width
        }
        .max(l.wave_x);
        draw_panel(canvas, x, y, width, height);
        canvas.text(
            x + 2,
            y + 1,
            inner,
            &ellipsize(&title, inner as usize),
            "text",
            "panel",
            true,
        );
        for (i, line) in value_lines.iter().enumerate() {
            canvas.text(
                x + 2,
                y + 2 + i as u16,
                inner,
                line,
                value_role(&value, "accent"),
                "panel",
                false,
            );
        }
        canvas.text(
            x + 2,
            y + height - 2,
            inner,
            &ellipsize(&detail, inner as usize),
            "muted",
            "panel",
            false,
        );
        if clipped {
            canvas.text(
                x + 2,
                y + height - 3,
                inner,
                "… Ctrl+C copies the full value",
                "muted",
                "panel",
                false,
            );
        }
    }
    fn draw_range(&self, canvas: &mut Canvas) {
        let Some(drag) = self
            .drag
            .filter(|d| d.zoom && !d.pan && !d.split && d.watch.is_none())
        else {
            return;
        };
        let l = self.layout();
        let first = drag.x.min(drag.end_x).max(l.wave_x);
        let last = drag
            .x
            .max(drag.end_x)
            .min(l.wave_x + l.wave_width.saturating_sub(1));
        if first > last {
            return;
        }
        for y in l.body_y..l.body_y + l.body_height {
            for x in first..=last {
                if let Some(cell) = canvas.get(x, y) {
                    cell.background = "selection";
                    if (x == first || x == last)
                        && !cell.protected
                        && matches!(cell.text, ' ' | '┊' | '┆')
                    {
                        cell.text = '┆';
                        cell.role = "warning";
                    }
                }
            }
        }
        canvas.fill(first, 3, last - first + 1, "warning", "selection");
        canvas.text(first, 3, 1, "[", "warning", "selection", true);
        canvas.text(last, 3, 1, "]", "warning", "selection", true);
    }

    fn draw_overview(&self, canvas: &mut Canvas) {
        let l = self.layout();
        if l.wave_width == 0 {
            return;
        }
        let y = canvas.height - 3;
        canvas.text(
            l.browser + 1,
            y,
            (l.name + l.value).saturating_sub(1),
            "Overview · drag to pan",
            "muted",
            "",
            false,
        );
        let first = self.overview_column(self.session.start);
        let last = self.overview_column(self.session.start.saturating_add(self.session.span));
        for x in l.wave_x..l.wave_x + l.wave_width {
            let selected = x >= first && x <= last;
            canvas.text(
                x,
                y,
                1,
                if selected { "━" } else { "─" },
                if selected { "accent" } else { "muted" },
                if selected { "selection" } else { "panel" },
                false,
            );
        }
        canvas.text(
            first,
            y,
            1,
            if first == last { "┃" } else { "[" },
            "accent",
            "selection",
            true,
        );
        if first != last {
            canvas.text(last, y, 1, "]", "accent", "selection", true);
        }
        for marker in &self.session.markers {
            canvas.character(self.overview_column(marker.time), y, '┆', "warning");
        }
        if let Some(time) = self.session.alt_cursor {
            canvas.character(self.overview_column(time), y, 'B', "warning");
        }
        canvas.character(self.overview_column(self.session.cursor), y, 'A', "accent");
    }

    fn draw_help(&self, canvas: &mut Canvas) {
        let h = self.help_layout();
        if h.width < 8 || h.height < 5 {
            canvas.fill(0, 0, canvas.width, "text", "panel");
            canvas.text(
                0,
                0,
                canvas.width,
                "Help · Esc closes",
                "accent",
                "panel",
                true,
            );
            return;
        }
        draw_panel(canvas, h.x, h.y, h.width, h.height);
        canvas.text(
            h.x + 2,
            h.y + 1,
            h.width - 4,
            "WAVEFORM EXPLORER",
            "accent",
            "panel",
            true,
        );
        let lines = self.help_lines();
        let start = self.help_scroll.min(lines.len().saturating_sub(h.visible));
        for (offset, line) in lines.iter().skip(start).take(h.visible).enumerate() {
            canvas.text(
                h.x + 2,
                h.y + 2 + offset as u16,
                h.width - 4,
                line,
                "text",
                "panel",
                false,
            );
            if !line.starts_with(' ') {
                canvas.text(
                    h.x + 2,
                    h.y + 2 + offset as u16,
                    (h.width - 4).min(13),
                    line,
                    "accent",
                    "panel",
                    true,
                );
            }
        }
        let footer = format!(
            "{}–{} / {} · ↑/↓ PgUp/PgDn wheel · Esc closes",
            start + 1,
            (start + h.visible).min(lines.len()),
            lines.len()
        );
        canvas.text(
            h.x + 2,
            h.y + h.height - 2,
            h.width - 4,
            &ellipsize(&footer, usize::from(h.width - 4)),
            "muted",
            "panel",
            false,
        );
        if let Some((start, size)) = scrollbar(lines.len(), start, h.visible as u16) {
            for dy in start..start + size {
                canvas.character(h.x + h.width - 2, h.y + 2 + dy, '┃', "accent");
            }
        }
    }
}

pub(crate) fn draw_panel(canvas: &mut Canvas, x: u16, y: u16, width: u16, height: u16) {
    if width < 2 || height < 2 {
        return;
    }
    for dy in 0..height {
        canvas.fill(x, y + dy, width, "text", "panel");
        canvas.character(x, y + dy, '│', "muted");
        canvas.character(x + width - 1, y + dy, '│', "muted");
    }
    for dx in 1..width - 1 {
        canvas.character(x + dx, y, '─', "muted");
        canvas.character(x + dx, y + height - 1, '─', "muted");
    }
    for (dx, dy, ch) in [
        (0, 0, '╭'),
        (width - 1, 0, '╮'),
        (0, height - 1, '╰'),
        (width - 1, height - 1, '╯'),
    ] {
        canvas.character(x + dx, y + dy, ch, "muted");
    }
}

fn value_role(value: &Value, fallback: &'static str) -> &'static str {
    match value.logic() {
        'x' => "error",
        'z' => "warning",
        _ => fallback,
    }
}

/// Compact hex vectors and group long values without changing the data or search syntax.
pub fn display_value(value: &Value, radix: Radix) -> String {
    let raw = value.format(radix);
    if radix != Radix::Hex || !matches!(value, Value::Bits(_)) {
        return raw;
    }
    let raw = raw.trim_start_matches('0');
    if raw.is_empty() {
        return "0".into();
    }
    if raw.len() <= 4 {
        return raw.into();
    }
    let mut output = String::with_capacity(raw.len() + raw.len() / 4);
    for (i, ch) in raw.chars().enumerate() {
        if i > 0 && (raw.len() - i).is_multiple_of(4) {
            output.push('_');
        }
        output.push(ch);
    }
    output
}

pub(crate) fn ellipsize(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.into();
    }
    if width == 0 {
        return String::new();
    }
    let mut output = String::new();
    let mut used = 0;
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        let size = ch.width().unwrap_or(0);
        if used + size >= width {
            break;
        }
        output.push(ch);
        used += size;
    }
    output.push('…');
    output
}

fn wrap_value(text: &str, width: usize, count: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut line = String::new();
    let mut used = 0;
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        let size = ch.width().unwrap_or(0);
        if used + size > width {
            lines.push(std::mem::take(&mut line));
            if lines.len() == count {
                return lines;
            }
            used = 0;
        }
        line.push(ch);
        used += size;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn tick_step(span: u64, width: u16) -> u64 {
    let target = (span / u64::from((width / 15).max(1))).max(1);
    let power = 10u64.saturating_pow(target.ilog10());
    for scale in [1, 2, 5, 10] {
        let step = power.saturating_mul(scale);
        if step >= target {
            return step;
        }
    }
    u64::MAX
}
