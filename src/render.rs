use crate::model::{Radix, Value};
use crate::navigation::scrollbar;
use crate::sampling::{Activity, Sample};
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

#[derive(Default)]
struct Resolution {
    mixed: bool,
    unresolved: bool,
    unlabelled: bool,
    /// Rows whose transitions do not all land on cell boundaries.
    estimated: usize,
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
        canvas.fill(0, 0, width, "text", "header");
        canvas.fill(0, 1, width, "text", "header");
        for button in self.toolbar() {
            if matches!(button.action, "previous" | "find" | "browser" | "save") && button.x > 2 {
                canvas.character(button.x - 2, 1, '│', "muted");
            }
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
            (format!("[A {a}]  "), "accent"),
            (format!("[B {b}]  "), "warning"),
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
            if l.value < 11 { " @ A" } else { " Value @ A" },
            "text",
            "header",
            true,
        );
        if l.value >= 13 {
            canvas.text(
                l.browser + l.name,
                4,
                l.value.saturating_sub(1),
                "before→after",
                "muted",
                "",
                false,
            );
        }
        if l.browser > 0 {
            for y in 4..l.body_y + l.body_height {
                canvas.fill(0, y, l.browser, "muted", "panel");
            }
            canvas.text(
                1,
                3,
                l.browser.saturating_sub(5),
                "Hierarchy  / filter",
                if self.focus == Focus::Browser {
                    "accent"
                } else {
                    "text"
                },
                "header",
                true,
            );
            canvas.text(
                l.browser.saturating_sub(3),
                3,
                2,
                "×",
                "muted",
                "header",
                false,
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
        // Ticks sit on cell boundaries so labels and gridlines line up exactly
        // with the transitions drawn at the same times.
        let step = ruler_step(self.cell_ticks(), self.session.span, l.wave_width);
        let origin = self.wave.start;
        let first = origin
            + (self.view_start() - origin)
                .div_ceil(step)
                .saturating_mul(step);
        // Past the end of the recording there is nothing to measure: the ruler
        // ends with the recording's final time, right-aligned on its column.
        let right = self.view_end().min(self.wave.end);
        let start_label = self.wave.timescale.format(self.view_start());
        let end_label = self.wave.timescale.format(right);
        let right_x = self.column_at(right).unwrap_or(width.saturating_sub(1)) + 1;
        let end_x = right_x
            .min(width)
            .saturating_sub(end_label.width() as u16)
            .max(l.wave_x);
        let endpoints_fit =
            start_label.width() + end_label.width() + 4 <= usize::from(l.wave_width);
        let mut tick = first;
        for _ in 0..128 {
            if tick > right {
                break;
            }
            if let Some(x) = self.column_at(tick) {
                let label = self.wave.timescale.format(tick);
                // An incomplete ruler label looks like a different timestamp.
                if label.width() <= usize::from(width.saturating_sub(x))
                    && (!endpoints_fit
                        || x > l.wave_x + start_label.width() as u16 + 1
                            && x + label.width() as u16 + 1 < end_x)
                {
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
        if endpoints_fit {
            canvas.text(
                l.wave_x,
                4,
                start_label.width() as u16,
                &start_label,
                "text",
                "",
                false,
            );
            canvas.text(
                end_x,
                4,
                end_label.width() as u16,
                &end_label,
                "text",
                "",
                false,
            );
        }
        let rows = self.display_rows();
        let mut resolution = Resolution::default();
        for row in rows.iter().filter(|r| {
            r.row.line as usize + r.row.height as usize > self.scroll
                && (r.row.line as usize) < self.scroll + l.body_height as usize
        }) {
            self.draw_row(canvas, row.row, row.preview, &mut resolution);
        }
        self.shade_outside_recording(canvas);
        let badge = self.scale_badge();
        let badge_width = badge.as_ref().map(|(_, _, w)| *w + 1).unwrap_or(0);
        let title_end = width.saturating_sub(badge_width + 1);
        let mut meta_x = 1;
        for button in self.main_menus() {
            let active = self.context_menu.as_ref().is_some_and(|menu| matches!(menu.target, crate::menus::MenuTarget::Main(menu) if menu == button.menu));
            let hover = self.hover.is_some_and(|hover| {
                hover.y == 0 && hover.x >= button.x && hover.x < button.x + button.width
            });
            canvas.text(
                button.x,
                0,
                button.width,
                &format!(" {} ", button.menu.name()),
                if active { "foreground" } else { "accent" },
                if active || hover {
                    "selection"
                } else {
                    "header"
                },
                active,
            );
            meta_x = button.x + button.width + 1;
        }
        let name = self.path.rsplit(['/', '\\']).next().unwrap_or("Waveform");
        let metadata = if title_end.saturating_sub(meta_x) >= 45 {
            format!(
                "{name} · {} · {} signals · {} / tick",
                self.wave.format,
                self.wave.signals.len(),
                self.wave.timescale.label()
            )
        } else {
            format!("{} · {name}", self.wave.format)
        };
        canvas.text(
            meta_x,
            0,
            title_end.saturating_sub(meta_x),
            &ellipsize(&metadata, title_end.saturating_sub(meta_x) as usize),
            "muted",
            "header",
            false,
        );
        if let Some((text, x, w)) = badge {
            let hovered = self
                .hover
                .is_some_and(|h| h.y == 0 && h.x >= x && h.x < x + w);
            canvas.text(
                x,
                0,
                w,
                &ellipsize(&text, usize::from(w)),
                if self.preview_scale() {
                    "warning"
                } else {
                    "success"
                },
                if hovered { "selection" } else { "header" },
                true,
            );
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
            self.draw_inspector(canvas);
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
        let resolution_hint = if resolution.estimated > 0 {
            let limit = self
                .exact_limit()
                .map(|limit| self.wave.timescale.format(limit))
                .unwrap_or_default();
            if width >= 115 {
                format!(
                    "≈ Preview · tinted rows round edges to {} cells · ▒ changes too close to place{} · + or click the badge: exact ({limit} / cell)",
                    self.wave.timescale.format(self.cell_ticks()),
                    if resolution.unresolved {
                        " · ░ unresolved bit"
                    } else {
                        ""
                    }
                )
            } else {
                format!(
                    "≈ Preview · tinted rows are estimates{} · + zooms to exact ({limit} / cell)",
                    if resolution.unresolved {
                        " · ░ unresolved"
                    } else {
                        ""
                    }
                )
            }
        } else if resolution.unresolved {
            "░ unresolved: too many bus changes in one cell to check this bit · + zooms in".into()
        } else if resolution.unlabelled {
            "Exact · Some bus values are too narrow to label · hover shows them · + zooms in".into()
        } else {
            String::new()
        };
        let edge_hint = self.selected().and_then(|(_, _, bit)| {
            let (before, after) =
                self.transition_values(self.selection, bit, self.session.cursor)?;
            let radix = match self.session.watches[self.selection] {
                Watch::Signal { radix, .. } if bit.is_none() => radix,
                _ => Radix::Binary,
            };
            Some(format!(
                "{}A edge @ {} · Before {} → After {} · At the transition, A uses After",
                if resolution.estimated > 0 {
                    "≈ Preview (tinted rows are estimates) · "
                } else {
                    ""
                },
                self.wave.timescale.format(self.session.cursor),
                display_value(&before, radix),
                display_value(&after, radix)
            ))
        });
        let hint = if self.signal_drag.as_ref().is_some_and(|d| d.active) {
            "Drop anywhere in Signals or the timeline · Hold near an edge to scroll · Wheel scrolls signals · Esc cancels"
        } else if self.inspector_height() == 0
            && let Some(hint) = &edge_hint
        {
            hint
        } else if !resolution_hint.is_empty() {
            &resolution_hint
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
        if frame.pointer_buttons.is_empty() {
            frame.pointer_buttons.push("middle".into());
        }
    }

    fn draw_row(
        &self,
        canvas: &mut Canvas,
        row: WaveRow,
        preview: bool,
        resolution: &mut Resolution,
    ) {
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
        let mut estimated = false;
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
                let label_y = (relative_y
                    + i32::from(row.height / 2).saturating_sub(i32::from(row.height == 2)))
                .clamp(
                    i32::from(l.body_y),
                    i32::from(l.body_y + l.body_height.saturating_sub(1)),
                );
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
                        "text",
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
                    let time = self.session.cursor;
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
                        let label = if let Some((before, after)) =
                            self.transition_values(row.watch, row.bit, time)
                        {
                            transition_label(
                                &display_value(&before, radix),
                                &display_value(&after, radix),
                                usize::from(l.value.saturating_sub(2)),
                            )
                        } else {
                            ellipsize(
                                &display_value(&value, radix),
                                usize::from(l.value.saturating_sub(2)),
                            )
                        };
                        canvas.text(
                            l.browser + l.name + 1,
                            label_y as u16,
                            l.value.saturating_sub(2),
                            &label,
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
                    estimated = self.draw_trace(
                        canvas,
                        row,
                        relative_y,
                        id,
                        effective_bit,
                        *radix,
                        role,
                        *analog && effective_bit.is_none(),
                        resolution,
                    );
                    resolution.estimated += usize::from(estimated && !preview);
                }
            }
        }
        if estimated && !preview && !hovered && !selected {
            // A preview estimate: some transitions in this row fall inside a
            // cell, so their drawn position is rounded to the cell.
            for offset in 0..row.height {
                let y = relative_y + i32::from(offset);
                if y >= i32::from(l.body_y) && y < i32::from(l.body_y + l.body_height) {
                    for x in l.wave_x..canvas.width {
                        if let Some(cell) = canvas.get(x, y as u16)
                            && cell.background.is_empty()
                        {
                            cell.background = "hover";
                        }
                    }
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
        resolution: &mut Resolution,
    ) -> bool {
        let l = self.layout();
        let trace = &self.wave.traces[self.wave.signals[signal].trace];
        let estimated = !self.row_exact(trace, bit);
        let digital = bit.is_some()
            || self.wave.signals[signal].width == 1 && self.wave.signals[signal].kind != "real";
        let mut samples = self.samples(trace, bit);
        // An analog plot is a sampled curve either way; keep its shape.
        if estimated && !analog {
            summarize_crowded_edges(&mut samples);
        }
        for sample in &samples {
            resolution.mixed |= sample.activity == Activity::Mixed;
            resolution.unresolved |= sample.activity == Activity::Unresolved;
        }
        if !digital && !analog {
            if row.height == 1 {
                self.draw_compact_bus(canvas, y, signal, radix, role, &samples);
            } else {
                resolution.unlabelled |= self.draw_bus(canvas, y, signal, radix, role, &samples);
            }
            return estimated;
        }
        let mut range = (f64::INFINITY, f64::NEG_INFINITY);
        if analog {
            let first = trace.index_at(self.view_start()).saturating_sub(1);
            let end = trace.index_at(self.view_end());
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
        let mut previous_logic = 'x';
        let mut previous_position = None;
        for (dx, sample) in samples.iter().enumerate() {
            let x = l.wave_x + dx as u16;
            let index = sample.index;
            let value = index
                .checked_sub(1)
                .and_then(|index| trace.changes.get(index))
                .map(|c| &c.value);
            let bit_value = bit.and_then(|bit| value.map(|value| value.bit(bit)));
            let value = bit_value.as_ref().or(value);
            let logic = value.map(Value::logic).unwrap_or('·');
            let value_role = match logic {
                'x' => "error",
                'z' => "warning",
                _ => role,
            };
            let changed = sample.activity == Activity::Edge && logic != previous_logic;
            if let Some(glyph) = sample.activity.glyph() {
                for offset in 0..row.height {
                    draw(
                        canvas,
                        x,
                        offset,
                        glyph,
                        if sample.activity == Activity::Mixed {
                            role
                        } else {
                            "muted"
                        },
                    );
                }
                previous_logic = logic;
                previous_position = None;
                continue;
            }
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
                    draw(canvas, x, row.height / 2, logic, value_role);
                }
            } else if digital {
                if row.height == 1 {
                    let ch = if changed
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
                            '·' => '·',
                            _ => 'x',
                        }
                    };
                    draw(canvas, x, 0, ch, value_role);
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
                    '·' => {
                        draw(canvas, x, 0, '·', "muted");
                        draw(canvas, x, 1, '·', "muted");
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
            previous_logic = logic;
        }
        estimated
    }

    /// Cells entirely after the final recorded time hold no data. Blank them
    /// on the panel color so the end of the recording is unmistakable.
    fn shade_outside_recording(&self, canvas: &mut Canvas) {
        let l = self.layout();
        if self.view_end() <= self.wave.end || l.wave_width < 2 {
            return;
        }
        let Some(last) = self.column_at(self.wave.end) else {
            return;
        };
        for y in l.body_y..l.body_y + l.body_height {
            for x in last + 1..l.wave_x + l.wave_width {
                if let Some(cell) = canvas.get(x, y) {
                    cell.text = if x == last + 1 { '▏' } else { ' ' };
                    cell.role = "muted";
                    cell.background = "panel";
                    cell.protected = false;
                }
            }
        }
    }

    fn draw_bus(
        &self,
        canvas: &mut Canvas,
        y: i32,
        signal: usize,
        radix: Radix,
        role: &'static str,
        samples: &[Sample],
    ) -> bool {
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
        let mut previous = trace.index_at(self.view_start());
        let mut rail_role = role;
        for dx in 0..l.wave_width {
            let index = samples[dx as usize].index;
            let changed = index != previous;
            if dx == 0 || changed {
                rail_role = index
                    .checked_sub(1)
                    .and_then(|i| trace.changes.get(i))
                    .map(|c| match c.value.logic() {
                        'x' => "error",
                        'z' => "warning",
                        _ => role,
                    })
                    .unwrap_or("muted");
            }
            draw(canvas, l.wave_x + dx, 0, '─', rail_role);
            draw(canvas, l.wave_x + dx, 2, '─', rail_role);
            if changed {
                // A boundary occupies its own time column. Straight joins avoid
                // decorating neighboring values or suggesting an analog slope.
                draw(canvas, l.wave_x + dx, 0, '┬', rail_role);
                draw(canvas, l.wave_x + dx, 1, '│', rail_role);
                draw(canvas, l.wave_x + dx, 2, '┴', rail_role);
                runs.push((start, dx, previous));
                start = dx;
            }
            previous = index;
        }
        runs.push((start, l.wave_width, previous));
        for (dx, sample) in samples.iter().enumerate() {
            if let Some(glyph) = sample.activity.glyph() {
                for offset in 0..3 {
                    draw(
                        canvas,
                        l.wave_x + dx as u16,
                        offset,
                        glyph,
                        if sample.activity == Activity::Mixed {
                            role
                        } else {
                            "muted"
                        },
                    );
                }
            } else if sample.index == 0 {
                draw(canvas, l.wave_x + dx as u16, 0, '·', "muted");
                draw(canvas, l.wave_x + dx as u16, 2, '·', "muted");
            }
        }
        if y + 1 < i32::from(l.body_y) || y + 1 >= i32::from(l.body_y + l.body_height) {
            return false;
        }
        let mut unlabelled = false;
        for (start, end, index) in runs {
            let room = end.saturating_sub(start + 2);
            if room == 0 {
                unlabelled |= start > 0 && end < l.wave_width && end > start && index > 0;
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
            unlabelled |= size > usize::from(room) && start > 0 && end < l.wave_width;
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
        unlabelled
    }

    fn draw_compact_bus(
        &self,
        canvas: &mut Canvas,
        y: i32,
        signal: usize,
        radix: Radix,
        role: &'static str,
        samples: &[Sample],
    ) {
        let l = self.layout();
        if y < i32::from(l.body_y) || y >= i32::from(l.body_y + l.body_height) {
            return;
        }
        let y = y as u16;
        let trace = &self.wave.traces[self.wave.signals[signal].trace];
        let mut runs = vec![];
        let mut start = 0;
        let mut previous = trace.index_at(self.view_start());
        canvas.fill(l.wave_x, y, l.wave_width, "muted", "panel");
        for (dx, sample) in samples.iter().enumerate() {
            let index = sample.index;
            let dx = dx as u16;
            let value = index
                .checked_sub(1)
                .and_then(|i| trace.changes.get(i))
                .map(|c| &c.value);
            let rail = value.map(|v| value_role(v, role)).unwrap_or("muted");
            canvas.character(l.wave_x + dx, y, '─', rail);
            if index != previous {
                canvas.character(l.wave_x + dx, y, '┼', rail);
                runs.push((start, dx, previous));
                start = dx;
            }
            previous = index;
        }
        runs.push((start, l.wave_width, previous));
        for (start, end, index) in runs {
            let room = end.saturating_sub(start + 2);
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
            let x = l.wave_x + start + 1 + (room - size) / 2;
            canvas.text(x, y, size, &label, value_role(value, role), "panel", false);
        }
        for (dx, sample) in samples.iter().enumerate() {
            if let Some(glyph) = sample
                .activity
                .glyph()
                .or((sample.index == 0).then_some('·'))
            {
                canvas.character(l.wave_x + dx as u16, y, glyph, "muted");
            }
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
        let value = self.watch_value(row.watch, row.bit, time);
        let Some(Watch::Signal { radix, bit, .. }) = self.session.watches.get(row.watch) else {
            return;
        };
        let radix = if row.bit.or(*bit).is_some() {
            Radix::Binary
        } else {
            *radix
        };
        let label = value
            .as_ref()
            .map(|value| display_value(value, radix))
            .unwrap_or_else(|| "No recorded value".into());
        let transition = self.hover_transition(hover);
        let title = if let Some((edge, _, _)) = &transition {
            format!(
                "Edge @ {} · {}",
                self.wave.timescale.format(*edge),
                self.watch_label(row)
            )
        } else {
            format!(
                "{} @ {} · {}",
                if hover.x < l.wave_x {
                    "Cursor A"
                } else {
                    "Pointer"
                },
                self.wave.timescale.format(time),
                if hover.x < l.wave_x {
                    self.full_watch_label(row)
                } else {
                    self.watch_label(row)
                }
            )
        };
        let activity = self.hover_activity(hover);
        let detail = if transition.is_some() {
            format!(
                "{} @ {} · At edge = After",
                if hover.x < l.wave_x {
                    "Cursor A"
                } else {
                    "Pointer"
                },
                self.wave.timescale.format(time)
            )
        } else {
            self.signal_id(row.watch)
                .and_then(|id| {
                    let trace = &self.wave.traces[self.wave.signals[id].trace];
                    let (start, end) =
                        trace.held_interval(time, row.bit.or(*bit), self.wave.end)?;
                    Some(format!(
                        "{} → {}  ·  held {}",
                        self.wave.timescale.format(start),
                        self.wave.timescale.format(end),
                        self.wave.timescale.format(end.saturating_sub(start))
                    ))
                })
                .unwrap_or_else(|| {
                    if value.is_some() {
                        "Held interval unresolved at this scale".into()
                    } else {
                        "No sample has been recorded at this time".into()
                    }
                })
        };
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
        if self.inspector_height() > 0 || l.wave_width < 28 || l.body_height < 9 {
            return;
        }
        let readouts = if let Some((_, before, after)) = &transition {
            vec![
                (
                    format!(
                        "Before {}  {}",
                        radix.label().to_uppercase(),
                        display_value(before, radix)
                    ),
                    value_role(before, "muted"),
                ),
                (
                    format!(
                        "After  {}  {}",
                        radix.label().to_uppercase(),
                        display_value(after, radix)
                    ),
                    value_role(after, "accent"),
                ),
            ]
        } else {
            vec![(
                format!("{}  {label}", radix.label().to_uppercase()),
                value
                    .as_ref()
                    .map(|value| value_role(value, "accent"))
                    .unwrap_or("muted"),
            )]
        };
        let width = (title
            .width()
            .max(
                readouts
                    .iter()
                    .map(|(text, _)| text.width())
                    .max()
                    .unwrap_or(0),
            )
            .max(detail.width()) as u16
            + 4)
        .clamp(28, 66)
        .min(l.wave_width);
        let inner = width - 4;
        let max_lines = if transition.is_some() {
            (usize::from(l.body_height.saturating_sub(5)) / 2).clamp(1, 3)
        } else {
            3
        };
        let mut value_lines = Vec::new();
        let mut clipped = false;
        for (text, role) in readouts {
            let wrapped = wrap_value(&text, usize::from(inner), max_lines);
            clipped |= wrapped.iter().map(|line| line.width()).sum::<usize>() < text.width();
            value_lines.extend(wrapped.into_iter().map(|text| (text, role)));
        }
        let height =
            4 + value_lines.len() as u16 + u16::from(clipped) + u16::from(activity.is_some());
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
        for (i, (line, role)) in value_lines.iter().enumerate() {
            canvas.text(x + 2, y + 2 + i as u16, inner, line, role, "panel", false);
        }
        canvas.text(
            x + 2,
            y + height - 2 - u16::from(activity.is_some()),
            inner,
            &ellipsize(&detail, inner as usize),
            "muted",
            "panel",
            false,
        );
        if let Some(activity) = &activity {
            canvas.text(
                x + 2,
                y + height - 2,
                inner,
                &ellipsize(activity, inner as usize),
                "warning",
                "panel",
                false,
            );
        }
        if clipped {
            canvas.text(
                x + 2,
                y + height - 3 - u16::from(activity.is_some()),
                inner,
                if transition.is_some() {
                    "… Ctrl+C copies the full After value"
                } else {
                    "… Ctrl+C copies the full value"
                },
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
        // One row: the whole recording, with the visible window bracketed.
        let y = canvas.height - 3;
        canvas.fill(0, y, canvas.width, "muted", "panel");
        let label = format!(
            "Recording {} – {}",
            self.wave.timescale.format(self.wave.start),
            self.wave.timescale.format(self.wave.end)
        );
        canvas.text(
            l.browser + 1,
            y,
            (l.name + l.value).saturating_sub(2),
            &ellipsize(&label, (l.name + l.value).saturating_sub(2) as usize),
            "muted",
            "panel",
            false,
        );
        let first = self.overview_column(self.view_start());
        let last = self.overview_column(self.view_end());
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

/// Keep both sides and the direction visible when a transition pair is clipped.
fn transition_label(before: &str, after: &str, width: usize) -> String {
    let arrow = if width >= 7 { " → " } else { "→" };
    if before.width() + after.width() + arrow.width() <= width {
        return format!("{before}{arrow}{after}");
    }
    let room = width.saturating_sub(arrow.width());
    if room < 2 {
        return ellipsize("→", width);
    }
    let left = (room / 2).min(before.width());
    let right = (room - left).min(after.width());
    let left = room.saturating_sub(right).min(before.width());
    format!(
        "{}{arrow}{}",
        ellipsize(before, left),
        ellipsize(after, right)
    )
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

/// In an estimated row an edge's column is rounded, so changes less than two
/// cells apart would beat irregularly (a steady clock looking unstable).
/// Show such stretches as one uniform activity band instead; edges with
/// room around them stay as edges.
fn summarize_crowded_edges(samples: &mut [Sample]) {
    let busy: Vec<usize> = samples
        .iter()
        .enumerate()
        .filter(|(_, s)| s.activity != Activity::Stable)
        .map(|(i, _)| i)
        .collect();
    for pair in busy.windows(2) {
        if pair[1] - pair[0] <= 2 {
            for sample in &mut samples[pair[0]..=pair[1]] {
                if sample.activity != Activity::Unresolved {
                    sample.activity = Activity::Mixed;
                }
            }
        }
    }
}

/// A 1/2/5 tick step that is also a whole number of cells.
pub fn ruler_step(cell: u64, span: u64, width: u16) -> u64 {
    let mut step = tick_step(span, width);
    for _ in 0..12 {
        if step.is_multiple_of(cell) {
            return step;
        }
        step = tick_step(
            step.saturating_add(1)
                .saturating_mul(u64::from((width / 15).max(1))),
            width,
        );
    }
    cell.saturating_mul(tick_step(span / cell.max(1), width))
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
