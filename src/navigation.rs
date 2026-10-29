use crate::model::{Radix, Value};
use crate::render::display_value;
use crate::viewer::{BrowserNode, Hover, OverviewDrag, Viewer, Watch, WaveRow};

pub(crate) const HELP_LINES: &[&str] = &[
    "NAVIGATION   Left / Right, n / N      Previous / next transition",
    "             u / U, j / J            Rising / falling edge; Shift reverses",
    "             Up / Down, PgUp / PgDn  Select signal",
    "             Tab                    Switch hierarchy / waves",
    "TIME         + / -                  Zoom around A; 0 fits the recording",
    "             i / Detail             Resolve edges and bus values around A",
    "             z / Shift+Z            Fit A-B / restore previous time view",
    "             v / Inspect            Toggle the docked value inspector",
    "             Hover + i              Zoom into the pointer cell; repeat to drill in",
    "             Double-click activity  Resolve a compressed cell",
    "             Zoom pauses at the terminal detail limit; - again continues",
    "             Overview: shaded cells contain multiple values, not a level",
    "             Values stay at A; pointer time/value appears in the inspector",
    "             At an edge: Before -> After; the timestamp uses After",
    "             Shift+Left / Right     Pan left / right",
    "             Ctrl+G                 Go to ticks or e.g. 125 ns",
    "             Home / End             Beginning / end; z zooms between A-B",
    "CURSORS      Click / drag           Cursor A; middle or Alt sets B",
    "             Alt+n / Alt+Shift+n    Next / previous B edge; Shift+B removes B",
    "             Alt+click edge button  Step B without moving A",
    "             Click A / B time       Enter exact time; x exchanges cursors",
    "             m / M, [ / ]           Add / remove marker; previous / next",
    "             s                      Toggle edge snapping",
    "SIGNALS      / or Signals           Inline path search; Enter adds / expands",
    "             Up / Down in search    Choose a matching signal; Esc restores filter",
    "             Shift+Enter on scope   Add descendants (up to 256 watches)",
    "             Drag from All signals  Preview and place; Esc cancels",
    "             Alt+Up / Down          Reorder; drag watched name also moves",
    "             Right-click, Shift+F10 Signal format, size, color and actions",
    "SELECTION    Ctrl+click names       Toggle individual waves",
    "             Shift+click / Up, Down Select a range; Ctrl+Shift adds to it",
    "             Ctrl+A / Esc           Select all visible waves / clear range",
    "             Right-click selection  Apply format, color, size or remove all",
    "             r / a / e              Radix / analog / bus expansion",
    "             1-8, F2, Delete        Color / rename / remove or fold",
    "             g / d                  Insert a group / divider",
    "             h                      Small / Normal size of selected waves",
    "             b / `                  Toggle hierarchy / full signal paths",
    "SEARCH       f or Ctrl+H             Find value in the selected signal",
    "             F3 / Shift+F3          Repeat forward / backward",
    "             0x / 0b override radix; x / z find four-state values",
    "SESSION      Ctrl+S                 Save view; source dump stays read-only",
    "             : / top menu bar       File, Signal, View, Waveform, Cursors, Tools",
    "             Ctrl+Z / Ctrl+Y        Undo / redo signal-list edits",
    "             Ctrl+C / Ctrl+E        Copy value / portable session JSON",
    "             Ctrl+V                 Import pasted session JSON; Enter applies",
    "             F5                     Reload simulation and keep the view",
    "MOUSE        Shift+wheel            Pan; up = left, down = right",
    "             Horizontal wheel       Pan; Ctrl+wheel zooms at the pointer",
    "             Wheel on labels        Scroll signals; wheel on waves pans",
    "             Shift+drag waves       Preview a range; release to zoom",
    "             Ctrl+drag waves        Pan; drag the overview to navigate",
    "             Hover trace / value    Inspect exact value; Ctrl+C copies it",
    "             Drag column dividers   Resize names / values; Esc cancels",
    "             Right-click headers    Fit names / values; reset column widths",
    "             Drag A / B handles     Adjust either cursor; Esc restores time",
    "             Click snap indicator   Toggle snapping to signal edges",
    "             Drag scrollbar thumbs  Navigate hierarchy / watched signals",
];

pub(crate) struct HelpLayout {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub visible: usize,
}

impl Viewer {
    pub(crate) fn help_layout(&self) -> HelpLayout {
        let width = self.width.min(512).saturating_sub(4).min(96);
        let height = self.height.min(200).saturating_sub(4).min(40);
        HelpLayout {
            x: self.width.min(512).saturating_sub(width) / 2,
            y: self.height.min(200).saturating_sub(height) / 2,
            width,
            height,
            visible: usize::from(height.saturating_sub(4)),
        }
    }

    pub(crate) fn scroll_help(&mut self, amount: isize) {
        self.help_scroll = self.help_scroll.saturating_add_signed(amount).min(
            self.help_lines()
                .len()
                .saturating_sub(self.help_layout().visible.max(1)),
        );
    }

    pub(crate) fn help_lines(&self) -> Vec<String> {
        let width = usize::from(self.help_layout().width.saturating_sub(4)).max(1);
        let indent = if width >= 75 { 13 } else { 2 }.min(width.saturating_sub(1));
        let mut sources = Vec::new();
        for source in HELP_LINES {
            if width >= 75 {
                sources.push((*source).to_string());
            } else {
                let heading = source[..13].trim();
                if !heading.is_empty() {
                    sources.push(heading.to_string());
                }
                let content = source[13..].trim();
                let content = if let Some((shortcut, description)) = content.split_once("  ") {
                    format!("{}: {}", shortcut.trim(), description.trim())
                } else {
                    content.to_string()
                };
                sources.push(format!("  {content}"));
            }
        }
        let mut lines = Vec::new();
        for source in sources {
            if source.len() <= width {
                lines.push(source);
                continue;
            }
            let mut line = if source.starts_with(' ') {
                " ".repeat(indent)
            } else {
                String::new()
            };
            for word in source.split_whitespace() {
                let gap = usize::from(!line.trim().is_empty());
                if line.len() + gap + word.len() > width && !line.trim().is_empty() {
                    lines.push(line);
                    line = " ".repeat(indent);
                } else if gap > 0 {
                    line.push(' ');
                }
                for ch in word.chars() {
                    if line.len() == width {
                        lines.push(line);
                        line = " ".repeat(indent);
                    }
                    line.push(ch);
                }
            }
            lines.push(line);
        }
        lines
    }

    pub(crate) fn row_at(&self, y: u16) -> Option<WaveRow> {
        let l = self.layout();
        if y < l.body_y || y >= l.body_y + l.body_height {
            return None;
        }
        let line = self.scroll + usize::from(y - l.body_y);
        self.rows()
            .into_iter()
            .find(|r| line >= usize::from(r.line) && line < usize::from(r.line + r.height))
    }

    pub(crate) fn hover_time(&self, hover: Hover) -> u64 {
        if hover.x >= self.layout().wave_x {
            self.time_at(hover.x)
        } else {
            self.session.cursor
        }
    }

    pub(crate) fn hover_activity(&self, hover: Hover) -> Option<String> {
        if hover.x < self.layout().wave_x {
            return None;
        }
        let row = hover.row?;
        let id = self.signal_id(row.watch)?;
        let Watch::Signal { bit, .. } = self.session.watches[row.watch] else {
            return None;
        };
        let trace = &self.wave.traces[self.wave.signals[id].trace];
        let (start, end) = self.cell_range(hover.x);
        let first = trace.index_at(start);
        let last = trace.index_at(end);
        let activity = trace.activity(first, last, row.bit.or(bit));
        if !activity.compressed() {
            return None;
        }
        Some(if activity == crate::sampling::Activity::Unresolved {
            "░ Bit unresolved here · i inspects this cell".into()
        } else {
            format!(
                "▒ {} in ({}, {}] · i inspects this cell",
                if row.bit.or(bit).is_some() {
                    "Multiple bit changes".into()
                } else {
                    format!("{} changes", last - first)
                },
                self.wave.timescale.format(start),
                self.wave.timescale.format(end)
            )
        })
    }

    pub(crate) fn hover_transition(&self, hover: Hover) -> Option<(u64, Value, Value)> {
        let row = hover.row?;
        let id = self.signal_id(row.watch)?;
        let Watch::Signal { bit, .. } = self.session.watches[row.watch] else {
            return None;
        };
        let bit = row.bit.or(bit);
        let trace = &self.wave.traces[self.wave.signals[id].trace];
        let mut time = self.hover_time(hover);
        if hover.x >= self.layout().wave_x {
            let (start, end) = self.cell_range(hover.x);
            let first = trace.index_at(start);
            let last = trace.index_at(end);
            match trace.activity(first, last, bit) {
                crate::sampling::Activity::Mixed | crate::sampling::Activity::Unresolved => {
                    return None;
                }
                crate::sampling::Activity::Edge => {
                    time = (first..last).find_map(|i| {
                        trace
                            .transition_index(trace.changes[i].time, bit)
                            .map(|_| trace.changes[i].time)
                    })?;
                }
                _ => {}
            }
        }
        let (before, after) = self.transition_values(row.watch, row.bit, time)?;
        Some((time, before, after))
    }

    pub(crate) fn full_watch_label(&self, row: WaveRow) -> String {
        let Some(Watch::Signal { path, bit, .. }) = self.session.watches.get(row.watch) else {
            return String::new();
        };
        let Some(signal) = self.signal_id(row.watch).map(|id| &self.wave.signals[id]) else {
            return path.clone();
        };
        if let Some(bit) = row.bit.or(*bit) {
            let suffix = if signal.width > 1 {
                format!("[{}:{}]", signal.msb, signal.lsb)
            } else {
                format!("[{}]", signal.msb)
            };
            return format!(
                "{}[{}]",
                path.strip_suffix(&suffix).unwrap_or(path),
                signal.bit_name(bit)
            );
        }
        path.clone()
    }

    pub(crate) fn overview_time(&self, x: u16) -> u64 {
        let l = self.layout();
        let width = l.wave_width.saturating_sub(1).max(1);
        let offset = x.saturating_sub(l.wave_x).min(width);
        self.wave.start.saturating_add(
            (u128::from(self.wave.end.saturating_sub(self.wave.start)) * u128::from(offset)
                / u128::from(width)) as u64,
        )
    }

    pub(crate) fn overview_column(&self, time: u64) -> u16 {
        let l = self.layout();
        let duration = self.wave.end.saturating_sub(self.wave.start).max(1);
        l.wave_x
            + (u128::from(time.saturating_sub(self.wave.start).min(duration))
                * u128::from(l.wave_width.saturating_sub(1))
                / u128::from(duration)) as u16
    }

    pub(crate) fn begin_overview_drag(&mut self, x: u16) {
        let time = self.overview_time(x);
        let anchor = if time >= self.session.start
            && time <= self.session.start.saturating_add(self.session.span)
        {
            time - self.session.start
        } else {
            self.session.span / 2
        };
        self.overview_drag = Some(OverviewDrag {
            start: self.session.start,
            anchor,
        });
        self.update_overview_drag(x);
    }

    pub(crate) fn update_overview_drag(&mut self, x: u16) {
        if let Some(drag) = self.overview_drag {
            self.session.start = self.overview_time(x).saturating_sub(drag.anchor);
            self.clamp_window();
        }
    }

    pub(crate) fn cancel_timeline_drag(&mut self) -> bool {
        let controls = self.cancel_control();
        let overview = self.overview_drag.take();
        if let Some(drag) = overview {
            self.session.start = drag.start;
            self.clamp_window();
        }
        self.drag.take().is_some() || overview.is_some() || controls
    }

    pub(crate) fn range_hint(&self) -> Option<String> {
        let drag = self
            .drag
            .filter(|d| d.zoom && !d.pan && !d.split && d.watch.is_none())?;
        Some(format!(
            "Zoom range: {} → {} · Δ {} · Release to zoom · Esc cancels",
            self.wave.timescale.format(drag.time.min(drag.end)),
            self.wave.timescale.format(drag.time.max(drag.end)),
            self.wave.timescale.format(drag.time.abs_diff(drag.end)),
        ))
    }

    pub(crate) fn hover_hint(&self) -> Option<String> {
        if let Some(hint) = self.control_hint() {
            return Some(hint);
        }
        let hover = self.hover?;
        let l = self.layout();
        if hover.y == 0 {
            return self
                .main_menus()
                .into_iter()
                .find(|b| hover.x >= b.x && hover.x < b.x + b.width)
                .map(|b| format!("{} menu · Click to open · ←/→ changes menu", b.menu.name()));
        }
        if hover.y == 1 {
            return self
                .toolbar()
                .into_iter()
                .find(|b| hover.x >= b.x && hover.x < b.x + b.width)
                .map(|b| {
                    if b.enabled {
                        b.hint.into()
                    } else {
                        match b.action {
                            "zoom_in" => "At maximum zoom · Use − or 0 to zoom out".into(),
                            "zoom_out" => {
                                "The entire recording is visible · Use + or Ctrl+wheel to zoom in"
                                    .into()
                            }
                            "zoom_back" => "No previous time view · Zoom or select a range first".into(),
                            "fit_cursors" => "Set B with Alt+click or Shift+B to fit the A–B range".into(),
                            "radix" | "analog" if self.selected().is_some_and(|(_, _, bit)| bit.is_some()) => "Bits use binary values · Select the parent bus to change its format".into(),
                            _ => "Select a signal to use this control".into(),
                        }
                    }
                });
        }
        if hover.x < l.browser {
            if matches!(hover.y, 3 | 4) {
                return Some(if hover.y == 3 && hover.x >= l.browser.saturating_sub(3) {
                    "Close the signal browser · b reopens it".into()
                } else if !self.filter.is_empty()
                    && hover.y == 4
                    && hover.x >= l.browser.saturating_sub(3)
                {
                    "Clear the signal filter".into()
                } else {
                    "Filter signal paths · / or Ctrl+F".into()
                });
            }
            if hover.y >= l.body_y && hover.y < l.body_y + l.body_height {
                let index = self.browser_scroll + usize::from(hover.y - l.body_y);
                return self.browser_nodes.get(index).map(|node| match node {
                    BrowserNode::Scope(id, _) => format!(
                        "{} · Enter expands · Shift+Enter adds descendants",
                        self.wave.scopes[*id].path
                    ),
                    BrowserNode::Signal(id, _) => {
                        let signal = &self.wave.signals[*id];
                        format!(
                            "{} · {} bits · {} · Drag to place or move",
                            signal.path, signal.width, signal.kind
                        )
                    }
                });
            }
        }
        if self.height >= 8
            && (self.height.min(200) - 4..=self.height.min(200) - 3).contains(&hover.y)
            && hover.x >= l.wave_x
        {
            return Some(format!(
                "Overview @ {} · Drag the highlighted viewport to pan · Click outside to center",
                self.wave.timescale.format(self.overview_time(hover.x))
            ));
        }
        let row = hover.row?;
        let Watch::Signal {
            path,
            radix,
            bit,
            analog,
            ..
        } = self.session.watches.get(row.watch)?
        else {
            return None;
        };
        let signal = self
            .signal_index
            .get(path)
            .map(|id| &self.wave.signals[*id])?;
        let effective_bit = row.bit.or(*bit);
        let radix = if effective_bit.is_some() {
            Radix::Binary
        } else {
            *radix
        };
        let name = self.full_watch_label(row);
        if hover.x < l.browser + l.name {
            return Some(format!(
                "{name} · {} bits · {} · {} · {} · Ctrl/Shift+click selects · Drag reorders",
                if effective_bit.is_some() {
                    1
                } else {
                    signal.width
                },
                signal.kind,
                if *analog && effective_bit.is_none() {
                    "ANALOG".into()
                } else {
                    radix.label().to_uppercase()
                },
                self.session.watches[row.watch].size(row.bit).label()
            ));
        }
        let time = self.hover_time(hover);
        let value = self.watch_value(row.watch, row.bit, time);
        if let Some((edge, before, after)) = self.hover_transition(hover) {
            return Some(format!(
                "Edge @ {} · Before {} → After {} · At edge = After · Ctrl+C copies After",
                self.wave.timescale.format(edge),
                display_value(&before, radix),
                display_value(&after, radix)
            ));
        }
        Some(format!(
            "{} @ {} · {name} · {}{} · Ctrl+C copies value",
            if hover.x >= l.wave_x {
                "Pointer"
            } else {
                "Cursor A"
            },
            self.wave.timescale.format(time),
            value
                .as_ref()
                .map(|value| display_value(value, radix))
                .unwrap_or_else(|| "No recorded value".into()),
            self.hover_activity(hover)
                .map(|hint| format!(" · {hint}"))
                .unwrap_or_default()
        ))
    }
}

pub(crate) fn scrollbar(total: usize, scroll: usize, visible: u16) -> Option<(u16, u16)> {
    let visible = usize::from(visible);
    if visible == 0 || total <= visible {
        return None;
    }
    let size = (visible * visible / total).max(1);
    let start = scroll.min(total - visible) * (visible - size) / (total - visible);
    Some((start as u16, size as u16))
}
