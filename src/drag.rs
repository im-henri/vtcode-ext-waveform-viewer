use crate::render::{Canvas, draw_panel, ellipsize};
use crate::viewer::{DisplayRow, MAX_WATCHES, SignalDrag, Viewer, Watch, WaveRow, WaveSize};
use unicode_width::UnicodeWidthStr;

impl Viewer {
    pub(crate) fn draw_drag_badge(&self, canvas: &mut Canvas) {
        let Some(drag) = self.signal_drag.as_ref().filter(|drag| drag.active) else {
            return;
        };
        if canvas.width < 8 || canvas.height < 3 {
            return;
        }
        let name = match &drag.watch {
            Watch::Signal { path, label, .. } => {
                if label.is_empty() {
                    path.rsplit('.').next().unwrap_or(path)
                } else {
                    label
                }
            }
            Watch::Group { name, .. } => name,
            Watch::Divider => "Divider",
        };
        let valid = drag.target.is_some();
        let prefix = if !valid {
            "× Cannot drop"
        } else if drag.source.is_some() {
            "↕ Move"
        } else {
            "+ Add"
        };
        let text = format!("{prefix}  {name}");
        let width = (text.width() as u16 + 4).min(54).min(canvas.width);
        let x = if drag.pointer.0.saturating_add(width + 2) <= canvas.width {
            drag.pointer.0 + 2
        } else {
            drag.pointer.0.saturating_sub(width + 1)
        }
        .min(canvas.width - width);
        let y = if drag.pointer.1.saturating_add(4) <= canvas.height {
            drag.pointer.1 + 1
        } else {
            drag.pointer.1.saturating_sub(3)
        }
        .min(canvas.height - 3);
        draw_panel(canvas, x, y, width, 3);
        let role = if valid { "accent" } else { "error" };
        for dy in 0..3 {
            for dx in 0..width {
                if (dy != 1 || dx == 0 || dx + 1 == width)
                    && let Some(cell) = canvas.get(x + dx, y + dy)
                {
                    cell.role = role;
                }
            }
        }
        canvas.text(
            x + 2,
            y + 1,
            width.saturating_sub(4),
            &ellipsize(&text, width.saturating_sub(4) as usize),
            role,
            "panel",
            true,
        );
    }

    pub fn rows(&self) -> Vec<WaveRow> {
        self.arrange_rows(false)
            .into_iter()
            .map(|r| r.row)
            .collect()
    }

    /// Project the drop without editing the session, history, or waveform data.
    pub fn display_rows(&self) -> Vec<DisplayRow> {
        self.arrange_rows(true)
    }

    fn drop_group(&self, target: usize) -> Option<usize> {
        self.session.watches[..target.min(self.session.watches.len())]
            .iter()
            .rposition(|w| matches!(w, Watch::Group { .. }))
    }

    pub(crate) fn preview_group(&self) -> Option<usize> {
        self.signal_drag
            .as_ref()
            .filter(|d| d.active)
            .and_then(|d| d.target)
            .and_then(|t| self.drop_group(t))
    }

    fn arrange_rows(&self, preview: bool) -> Vec<DisplayRow> {
        let drag = self.signal_drag.as_ref().filter(|d| {
            preview && d.active && d.target.is_some_and(|t| t <= self.session.watches.len())
        });
        let group = if preview { self.preview_group() } else { None };
        let mut order =
            Vec::with_capacity(self.session.watches.len() + usize::from(drag.is_some()));
        for index in 0..=self.session.watches.len() {
            if let Some(d) = drag
                && d.target == Some(index)
            {
                order.push((
                    d.source.unwrap_or(self.session.watches.len()),
                    &d.watch,
                    true,
                ));
            }
            if let Some(watch) = self.session.watches.get(index)
                && drag.is_none_or(|d| d.source != Some(index))
            {
                order.push((index, watch, false));
            }
        }
        let mut rows = vec![];
        let mut collapsed = false;
        let mut line: u16 = 0;
        for (watch, item, preview) in order {
            let id = match item {
                Watch::Signal { path, .. } => self.signal_index.get(path).copied(),
                _ => None,
            };
            let height = match item {
                Watch::Group {
                    collapsed: hidden, ..
                } => {
                    collapsed = *hidden && group != Some(watch);
                    1
                }
                _ if collapsed => continue,
                Watch::Divider => 1,
                Watch::Signal { analog, bit, .. } => {
                    if item.size(None) == WaveSize::Small {
                        if *analog && bit.is_none() { 2 } else { 1 }
                    } else if *analog
                        || bit.is_none()
                            && id.is_some_and(|id| {
                                self.wave.signals[id].width > 1
                                    || self.wave.signals[id].kind == "real"
                            })
                    {
                        3
                    } else {
                        2
                    }
                }
            };
            rows.push(DisplayRow {
                row: WaveRow {
                    watch,
                    bit: None,
                    line,
                    height,
                },
                preview,
            });
            line = line.saturating_add(height);
            if let Watch::Signal {
                expanded: true,
                bit: None,
                ..
            } = item
                && let Some(id) = id
            {
                for bit in (0..self.wave.signals[id].width.min(64)).rev() {
                    rows.push(DisplayRow {
                        row: WaveRow {
                            watch,
                            bit: Some(bit),
                            line,
                            height: if item.size(Some(bit)) == WaveSize::Small {
                                1
                            } else {
                                2
                            },
                        },
                        preview,
                    });
                    line = line.saturating_add(if item.size(Some(bit)) == WaveSize::Small {
                        1
                    } else {
                        2
                    });
                }
            }
        }
        rows
    }

    pub(crate) fn start_signal_drag(&mut self, id: usize, source: Option<usize>, x: u16, y: u16) {
        let source = source.or_else(|| self.session.watches.iter().position(
            |w| matches!(w, Watch::Signal { path, bit: None, .. } if path == &self.wave.signals[id].path)
        ));
        if source.is_none() && self.session.watches.len() >= MAX_WATCHES {
            self.fail("At most 256 watched signals/groups; remove some first");
            return;
        }
        let watch = source
            .map(|i| self.session.watches[i].clone())
            .unwrap_or_else(|| {
                let signal = &self.wave.signals[id];
                self.new_watch(signal, 0)
            });
        self.signal_drag = Some(SignalDrag {
            watch,
            source,
            from_browser: x < self.layout().browser,
            press: (x, y),
            pointer: (x, y),
            active: false,
            target: None,
            ticks: 0,
        });
    }

    pub(crate) fn update_signal_drag(&mut self, x: u16, y: u16, inside: bool) {
        let l = self.layout();
        let Some(drag) = &mut self.signal_drag else {
            return;
        };
        drag.pointer = (x, y);
        drag.active |= drag.press.0.abs_diff(x) >= 2 || drag.press.1 != y || !inside;
        if !drag.active {
            return;
        }
        if !inside
            || x < l.browser
            || x >= self.width.min(512)
            || y < l.body_y
            || y >= l.body_y + l.body_height
        {
            drag.target = None;
            drag.ticks = 0;
            return;
        }
        let line = self.scroll + usize::from(y - l.body_y);
        let rows = self.display_rows();
        // The pointer can rest inside the proposed row without chasing a target
        // that was shifted by that very preview. Expanded bits move as one bus.
        let target = if let Some(hit) = rows.iter().find(|r| {
            line >= usize::from(r.row.line) && line < usize::from(r.row.line + r.row.height)
        }) {
            if hit.preview {
                return;
            }
            let first = rows
                .iter()
                .find(|r| !r.preview && r.row.watch == hit.row.watch)
                .unwrap()
                .row;
            let last = rows
                .iter()
                .rev()
                .find(|r| !r.preview && r.row.watch == hit.row.watch)
                .unwrap()
                .row;
            let start = usize::from(first.line);
            let height = usize::from(last.line + last.height) - start;
            let after = if height == 1 {
                matches!(self.session.watches[hit.row.watch], Watch::Group { .. })
            } else {
                (line - start) * 2 >= height
            };
            hit.row.watch + usize::from(after)
        } else {
            self.session.watches.len()
        };
        self.signal_drag.as_mut().unwrap().target = Some(target);
    }

    pub(crate) fn scroll_drag(&mut self, direction: i32) {
        let max = self
            .display_rows()
            .last()
            .map(|r| usize::from(r.row.line + r.row.height))
            .unwrap_or(0)
            .saturating_sub(usize::from(self.layout().body_height));
        self.scroll = if direction < 0 {
            self.scroll.saturating_sub(3)
        } else {
            self.scroll.saturating_add(3).min(max)
        };
        if let Some(drag) = &self.signal_drag {
            let (x, y) = drag.pointer;
            self.update_signal_drag(x, y, true);
        }
    }

    fn drag_scroll_direction(&self) -> i32 {
        let Some(drag) = self
            .signal_drag
            .as_ref()
            .filter(|d| d.active && d.target.is_some())
        else {
            return 0;
        };
        let l = self.layout();
        if l.body_height < 4 {
            return 0;
        }
        if drag.pointer.1 < l.body_y + 2 && self.scroll > 0 {
            return -1;
        }
        let end = self
            .display_rows()
            .last()
            .map(|r| usize::from(r.row.line + r.row.height))
            .unwrap_or(0);
        if drag.pointer.1 >= l.body_y + l.body_height - 2
            && self.scroll + usize::from(l.body_height) < end
        {
            return 1;
        }
        0
    }

    pub fn needs_drag_tick(&self) -> bool {
        self.drag_scroll_direction() != 0
    }

    /// One small scroll step per 80 ms while held near a viewport edge.
    pub fn drag_tick(&mut self) -> bool {
        let direction = self.drag_scroll_direction();
        if direction == 0 {
            return false;
        }
        let drag = self.signal_drag.as_mut().unwrap();
        drag.ticks += 1;
        if drag.ticks < 5 {
            return false;
        }
        drag.ticks = 0;
        self.scroll = if direction < 0 {
            self.scroll.saturating_sub(1)
        } else {
            self.scroll + 1
        };
        let (x, y) = drag.pointer;
        self.update_signal_drag(x, y, true);
        true
    }

    pub(crate) fn finish_signal_drag(&mut self) {
        let Some(drag) = self.signal_drag.take() else {
            return;
        };
        let Some(target) = drag.target.filter(|_| drag.active) else {
            if drag.active {
                self.status = "Drag cancelled".into();
            }
            return;
        };
        let target = target.min(self.session.watches.len());
        let index = target.saturating_sub(usize::from(drag.source.is_some_and(|s| s < target)));
        let reveal = self.drop_group(target).is_some_and(|g| {
            matches!(
                self.session.watches[g],
                Watch::Group {
                    collapsed: true,
                    ..
                }
            )
        });
        if drag.source != Some(index) || reveal {
            self.checkpoint();
            if let Some(source) = drag.source {
                self.session.watches.remove(source);
            }
            self.session.watches.insert(index, drag.watch);
            if let Some(group) = self.drop_group(index)
                && let Watch::Group { collapsed, .. } = &mut self.session.watches[group]
            {
                *collapsed = false;
            }
        }
        self.selection = index;
        self.selected_bit = None;
        self.clear_wave_selection();
        self.ensure_selection();
        self.status = format!(
            "{} signal · Ctrl+Z undo",
            if drag.source.is_some() {
                "Moved"
            } else {
                "Added"
            }
        );
    }

    pub(crate) fn drag_hint(&self) -> Option<String> {
        let drag = self.signal_drag.as_ref().filter(|d| d.active)?;
        let name = match &drag.watch {
            Watch::Signal { path, label, .. } => {
                if label.is_empty() {
                    path
                } else {
                    label
                }
            }
            _ => return None,
        };
        let Some(target) = drag.target else {
            return Some(format!(
                "Drag {name} into Signals or the timeline · Esc cancels"
            ));
        };
        let next = self
            .session
            .watches
            .iter()
            .enumerate()
            .skip(target)
            .find(|(i, _)| Some(*i) != drag.source)
            .map(|(_, w)| match w {
                Watch::Signal { path, label, .. } => {
                    if label.is_empty() {
                        path.clone()
                    } else {
                        label.clone()
                    }
                }
                Watch::Group { name, .. } => name.clone(),
                Watch::Divider => "divider".into(),
            });
        Some(format!(
            "Release to {} {name} {} · Esc cancels",
            if drag.source.is_some() { "move" } else { "add" },
            next.map(|n| format!("before {n}"))
                .unwrap_or_else(|| "at the end".into())
        ))
    }
}
