//! Terminal cells represent intervals, not just a decimated stream of values.
use crate::model::{Trace, Value};
use crate::viewer::{Viewer, Watch};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Stable,
    Edge,
    Mixed,
    /// A projected bit could not be resolved within the bounded inspection budget.
    Unresolved,
}
impl Activity {
    pub fn compressed(self) -> bool {
        matches!(self, Self::Mixed | Self::Unresolved)
    }
    pub fn glyph(self) -> Option<char> {
        match self {
            Self::Mixed => Some('▒'),
            Self::Unresolved => Some('░'),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub index: usize,
    pub activity: Activity,
}

impl Trace {
    /// The final value at a timestamp is the after value. Initial samples and
    /// parent-bus updates that leave a projected bit unchanged are not edges.
    pub fn transition_index(&self, time: u64, bit: Option<u32>) -> Option<usize> {
        let index = self.index_at(time).checked_sub(1)?;
        if index == 0 || self.changes[index].time != time {
            return None;
        }
        if bit.is_some_and(|bit| {
            self.changes[index - 1].value.bit_logic(bit) == self.changes[index].value.bit_logic(bit)
        }) {
            return None;
        }
        Some(index)
    }
    /// Exact held interval, or None when a projected bit needs more work than
    /// an interactive tooltip may do. Parent bus updates are not bit edges.
    pub fn held_interval(&self, time: u64, bit: Option<u32>, end: u64) -> Option<(u64, u64)> {
        let index = self.index_at(time).checked_sub(1)?;
        let mut first = index;
        let mut last = index + 1;
        if let Some(bit) = bit {
            let logic = self.changes[index].value.bit_logic(bit);
            while first > 0 && self.changes[first - 1].value.bit_logic(bit) == logic {
                if index - first >= 64 {
                    return None;
                }
                first -= 1;
            }
            while last < self.changes.len() && self.changes[last].value.bit_logic(bit) == logic {
                if last - index > 64 {
                    return None;
                }
                last += 1;
            }
        }
        Some((
            self.changes[first].time,
            self.changes.get(last).map(|c| c.time).unwrap_or(end),
        ))
    }
    /// Changes in (previous sample, current sample], including pulses that return
    /// to their original value. Unrelated bus updates do not become bit edges.
    pub fn activity(&self, previous: usize, current: usize, bit: Option<u32>) -> Activity {
        let count = if let Some(bit) = bit {
            let mut count = 0;
            let mut old = previous
                .checked_sub(1)
                .map(|i| self.changes[i].value.bit_logic(bit));
            for change in self.changes[previous..current].iter().take(64) {
                let new = Some(change.value.bit_logic(bit));
                count += usize::from(old != new);
                if count > 1 {
                    return Activity::Mixed;
                }
                old = new;
            }
            if current - previous > 64 {
                return Activity::Unresolved;
            }
            count
        } else {
            current - previous
        };
        match count {
            0 => Activity::Stable,
            1 => Activity::Edge,
            _ => Activity::Mixed,
        }
    }
}

impl Viewer {
    pub(crate) fn remember_view(&mut self) {
        let window = (self.session.start, self.session.span);
        if !self.wave.signals.is_empty() && self.view_history.last() != Some(&window) {
            self.view_history.push(window);
            if self.view_history.len() > 64 {
                self.view_history.remove(0);
            }
        }
    }
    pub fn zoom_back(&mut self) {
        while let Some((start, span)) = self.view_history.pop() {
            if (start, span) == (self.session.start, self.session.span) {
                continue;
            }
            self.session.start = start;
            self.session.span = span;
            self.clamp_window();
            self.detail_stop = None;
            self.status =
                "Previous time view restored · Cursors preserved · Shift+Z goes back again".into();
            return;
        }
        self.status = "No previous time view".into();
    }
    pub fn fit_cursors(&mut self) {
        let Some(b) = self.session.alt_cursor else {
            self.status =
                "Set B with Alt+click or Shift+B, then use A–B to fit the measurement".into();
            return;
        };
        self.remember_view();
        self.session.start = b.min(self.session.cursor);
        self.session.span = b.abs_diff(self.session.cursor).max(1);
        self.clamp_window();
        self.status = "Fitted between A and B · Shift+Z restores the previous view".into();
    }
    pub(crate) fn transition_values(
        &self,
        watch: usize,
        bit: Option<u32>,
        time: u64,
    ) -> Option<(Value, Value)> {
        let id = self.signal_id(watch)?;
        let Watch::Signal { bit: own_bit, .. } = self.session.watches[watch] else {
            return None;
        };
        let bit = bit.or(own_bit);
        let trace = &self.wave.traces[self.wave.signals[id].trace];
        let index = trace.transition_index(time, bit)?;
        let project = |value: &Value| {
            bit.map(|bit| value.bit(bit))
                .unwrap_or_else(|| value.clone())
        };
        Some((
            project(&trace.changes[index - 1].value),
            project(&trace.changes[index].value),
        ))
    }
    pub(crate) fn inspect_detail(&mut self) {
        self.remember_view();
        if let Some(hover) = self
            .hover
            .filter(|hover| hover.row.is_some() && hover.x >= self.layout().wave_x)
        {
            let (start, end) = self.cell_range(hover.x);
            self.session.start = start;
            self.session.span = end.saturating_sub(start).max(1);
            self.clamp_window();
            // The host coalesces stationary pointer moves. Keep inspection alive
            // after zoom so another i works without a physical mouse movement.
            self.detail_stop = None;
            self.status =
                "Inspecting pointer cell · Cursors preserved · Hover + i drills down again".into();
        } else {
            self.detail_at_cursor();
        }
    }
    pub(crate) fn samples(&self, trace: &Trace, bit: Option<u32>) -> Vec<Sample> {
        let l = self.layout();
        let mut previous = trace.index_at(self.session.start);
        (0..l.wave_width)
            .map(|dx| {
                let index = trace.index_at(self.time_at(l.wave_x + dx));
                let mut activity = trace.activity(previous, index, bit);
                if l.wave_width == 1 && activity == Activity::Edge {
                    activity = Activity::Mixed;
                }
                previous = index;
                Sample { index, activity }
            })
            .collect()
    }

    /// A cell owns the right endpoint; this matches column_at's ceiling mapping.
    pub fn cell_range(&self, column: u16) -> (u64, u64) {
        let l = self.layout();
        if l.wave_width == 1 {
            return (self.session.start, self.time_at(l.wave_x));
        }
        let column = column.clamp(l.wave_x, l.wave_x + l.wave_width.saturating_sub(1));
        (
            self.time_at(column.saturating_sub(1).max(l.wave_x)),
            self.time_at(column),
        )
    }

    /// Reserve two columns for logic levels and enough space for bus values. The scan
    /// is bounded even for multi-million-transition traces; very dense regions
    /// conservatively use one tick. Fit and further zoom-out always stay available.
    pub fn detail_limit(&self) -> Option<u64> {
        let l = self.layout();
        let mut limit: Option<u64> = None;
        for row in self.rows().into_iter().filter(|row| {
            usize::from(row.line + row.height) > self.scroll
                && usize::from(row.line) < self.scroll + usize::from(l.body_height)
        }) {
            let Some(id) = self.signal_id(row.watch) else {
                continue;
            };
            let Watch::Signal {
                bit, radix, analog, ..
            } = self.session.watches[row.watch]
            else {
                continue;
            };
            let bit = row.bit.or(bit);
            let trace = &self.wave.traces[self.wave.signals[id].trace];
            let first = trace.index_at(self.session.start).saturating_sub(1);
            let end = (trace.index_at(self.session.start.saturating_add(self.session.span)) + 1)
                .min(trace.changes.len());
            let columns = if bit.is_some() || self.wave.signals[id].width == 1 || analog {
                2
            } else {
                trace.changes[first..end]
                    .iter()
                    .take(2048)
                    .map(|change| {
                        crate::render::display_value(&change.value, radix).width() as u64 + 4
                    })
                    .max()
                    .unwrap_or(4)
            };
            let mut gap = (end.saturating_sub(first) > 2048).then_some(1);
            let mut previous = None;
            for change in trace.changes[first..end].iter().take(2048) {
                if let Some((time, old)) = previous {
                    if bit.is_some_and(|bit| old == change.value.bit_logic(bit)) {
                        continue;
                    }
                    let interval = change.time.saturating_sub(time).max(1);
                    gap = Some(gap.map_or(interval, |gap: u64| gap.min(interval)));
                }
                previous = Some((
                    change.time,
                    bit.map(|bit| change.value.bit_logic(bit)).unwrap_or('b'),
                ));
            }
            if let Some(gap) = gap {
                let span = (gap.saturating_mul(u64::from(l.wave_width.saturating_sub(1)))
                    / columns)
                    .max(1);
                limit = Some(limit.map_or(span, |limit| limit.min(span)));
            }
        }
        limit
    }

    /// Inspect the selected signal near A, with room for both sides of an edge.
    pub fn detail_at_cursor(&mut self) {
        self.detail_stop = None;
        let Some((_, trace, bit)) = self.selected() else {
            self.fail("Select a signal to inspect at cursor A");
            return;
        };
        self.remember_view();
        let changes = &self.wave.traces[trace].changes;
        let index = self.wave.traces[trace].index_at(self.session.cursor);
        let first = index.saturating_sub(128);
        let end = (index + 128).min(changes.len());
        let mut edges = Vec::new();
        for i in first..end {
            if i == 0
                || bit.is_none_or(|bit| {
                    changes[i - 1].value.bit_logic(bit) != changes[i].value.bit_logic(bit)
                })
            {
                edges.push(changes[i].time);
            }
        }
        let at = edges.partition_point(|time| *time <= self.session.cursor);
        let nearby = &edges[at.saturating_sub(2)..(at + 2).min(edges.len())];
        let gap = nearby
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .min()
            .unwrap_or(1)
            .max(1);
        self.session.span =
            gap.saturating_mul(u64::from(self.layout().wave_width.saturating_sub(1))) / 4;
        self.session.span = self.session.span.max(1);
        if let Some(limit) = self.detail_limit() {
            self.session.span = self.session.span.min(limit);
        }
        self.session.start = self.session.cursor.saturating_sub(self.session.span / 2);
        self.clamp_window();
        self.status = "Detail around cursor A · ←/→ steps exact transitions · − zooms out".into();
    }

    pub(crate) fn cell_scale(&self) -> String {
        let columns = u64::from(self.layout().wave_width.saturating_sub(1).max(1));
        let ticks = self.session.span.div_ceil(columns);
        if self.session.span.is_multiple_of(columns) {
            format!("{} / cell", self.wave.timescale.format(ticks))
        } else {
            format!("≤ {} / cell", self.wave.timescale.format(ticks))
        }
    }
}
