//! Integer time scales. A terminal cell always spans a whole number of ticks
//! and the window starts on a cell boundary, so a transition time maps to one
//! column for every row. When the cell size also divides every transition
//! time ("exact"), each interval is drawn with exactly its length in cells:
//! a regular clock stays regular and simultaneous edges share a column.
//! Scales coarser than the exact limit are previews and say so on screen.
use crate::model::Trace;
use crate::viewer::{Viewer, Watch};

pub fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// The largest tick count dividing every change time of a trace (measured
/// from `origin`). Zero means the trace never changes after the origin.
pub fn trace_grid(trace: &Trace, origin: u64) -> u64 {
    let mut grid = 0;
    for change in &trace.changes {
        grid = gcd(grid, change.time.saturating_sub(origin));
        if grid == 1 {
            break;
        }
    }
    grid
}

/// Sorted divisors of `n`. Trial division is bounded; a cofactor left above
/// the bound is treated as prime, which can only omit some divisors.
pub fn divisors(n: u64) -> Vec<u64> {
    if n == 0 {
        return vec![];
    }
    let mut factors: Vec<(u64, u32)> = vec![];
    let mut rest = n;
    let mut p = 2u64;
    while p.saturating_mul(p) <= rest && p <= 1_000_000 {
        if rest.is_multiple_of(p) {
            let mut count = 0;
            while rest.is_multiple_of(p) {
                rest /= p;
                count += 1;
            }
            factors.push((p, count));
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if rest > 1 {
        factors.push((rest, 1));
    }
    let mut out = vec![1u64];
    for (p, count) in factors {
        let len = out.len();
        let mut power = 1u64;
        for _ in 0..count {
            power = power.saturating_mul(p);
            for i in 0..len {
                out.push(out[i].saturating_mul(power));
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

impl Viewer {
    /// Columns between the first and last cell boundary of the wave area.
    pub fn wave_columns(&self) -> u64 {
        u64::from(self.layout().wave_width.saturating_sub(1).max(1))
    }
    /// Ticks per terminal cell for the current window (always a whole number).
    pub fn cell_ticks(&self) -> u64 {
        self.session.span.div_ceil(self.wave_columns()).max(1)
    }
    /// First visible time, aligned to a cell boundary.
    pub fn view_start(&self) -> u64 {
        let s = self.cell_ticks();
        let origin = self.wave.start;
        origin + (self.session.start.saturating_sub(origin) / s) * s
    }
    /// Last cell boundary of the window (may lie past the end of the recording).
    pub fn view_end(&self) -> u64 {
        self.view_start()
            .saturating_add(self.cell_ticks().saturating_mul(self.wave_columns()))
    }

    pub(crate) fn compute_grids(&mut self) {
        let origin = self.wave.start;
        self.grids = self
            .wave
            .traces
            .iter()
            .map(|trace| trace_grid(trace, origin))
            .collect();
    }
    pub(crate) fn grid_of(&self, trace: usize) -> u64 {
        self.grids.get(trace).copied().unwrap_or_else(|| {
            self.wave
                .traces
                .get(trace)
                .map(|t| trace_grid(t, self.wave.start))
                .unwrap_or(0)
        })
    }
    /// The exact limit: the coarsest cell size that still lands every watched
    /// transition on a cell boundary. None when nothing watched ever changes.
    pub fn exact_limit(&self) -> Option<u64> {
        let mut grid = 0;
        for watch in &self.session.watches {
            if let Watch::Signal { path, .. } = watch
                && let Some(&id) = self.signal_index.get(path)
            {
                grid = gcd(grid, self.grid_of(self.wave.signals[id].trace));
                if grid == 1 {
                    break;
                }
            }
        }
        (grid > 0).then_some(grid)
    }
    /// True when the current cell size is coarser than the exact limit.
    pub fn preview_scale(&self) -> bool {
        self.exact_limit()
            .is_some_and(|limit| self.cell_ticks() > limit)
    }
    /// The coarsest cell size that shows the whole recording.
    pub fn fit_ticks(&self) -> u64 {
        let duration = self.wave.end.saturating_sub(self.wave.start).max(1);
        self.snap_ticks(duration.div_ceil(self.wave_columns()))
    }
    /// Round a desired cell size up to one that is exact when possible.
    /// Coarser than the exact limit, any whole number of ticks is a preview.
    pub(crate) fn snap_ticks(&self, raw: u64) -> u64 {
        let raw = raw.max(1);
        match self.exact_limit() {
            Some(limit) if raw <= limit => divisors(limit)
                .into_iter()
                .find(|d| *d >= raw)
                .unwrap_or(limit),
            _ => raw,
        }
    }
    /// The coarsest exact cell size no larger than `raw` (at least one tick).
    pub(crate) fn snap_ticks_down(&self, raw: u64) -> u64 {
        let raw = raw.max(1);
        match self.exact_limit() {
            Some(limit) => divisors(limit)
                .into_iter()
                .rev()
                .find(|d| *d <= raw)
                .unwrap_or(1),
            None => raw,
        }
    }
    /// Open on the exact limit when it still shows a useful share of the
    /// recording; otherwise show everything as a labelled preview.
    pub(crate) fn initial_view(&mut self) {
        self.fit();
        let Some(limit) = self.exact_limit() else {
            return;
        };
        let duration = self.wave.end.saturating_sub(self.wave.start).max(1);
        if self.preview_scale()
            && u128::from(limit) * u128::from(self.wave_columns()) * 4 >= u128::from(duration)
        {
            self.session.start = self.wave.start;
            self.session.span = limit.saturating_mul(self.wave_columns());
            self.clamp_window();
        }
    }
    /// Next finer (inward) or coarser cell size on the zoom ladder.
    pub(crate) fn zoom_step(&self, inward: bool) -> u64 {
        let s = self.cell_ticks();
        let Some(limit) = self.exact_limit() else {
            return if inward {
                (s / 2).max(1)
            } else {
                s.saturating_mul(2)
            };
        };
        if inward {
            if s > limit {
                // Leaving the preview lands exactly on the limit.
                return if s / 2 <= limit { limit } else { s / 2 };
            }
            let exact = divisors(limit);
            exact
                .iter()
                .rev()
                .find(|d| **d <= s / 2)
                .or_else(|| exact.iter().rev().find(|d| **d < s))
                .copied()
                .unwrap_or(s)
        } else if s < limit {
            // Crossing the exact limit stops on it; the next step continues.
            divisors(limit)
                .into_iter()
                .find(|d| *d >= s.saturating_mul(2))
                .unwrap_or(limit)
        } else {
            s.saturating_mul(2)
        }
    }
    /// When an event changes the wave area's width (pane resize, column
    /// resize, browser toggle) without setting the time window itself, keep the
    /// cell size so an exact view stays exact; only the visible time changes.
    pub(crate) fn keep_cell_size(&mut self, (columns, cell, span): (u64, u64, u64)) {
        if self.wave_columns() == columns
            || self.session.span != span
            || self.layout().wave_width < 2
            || self.wave.signals.is_empty()
        {
            return;
        }
        self.session.span = cell.saturating_mul(self.wave_columns());
        let history = std::mem::take(&mut self.view_history);
        self.clamp_window();
        self.view_history = history;
    }
    /// Show at least [from, to]: the finest whole-cell scale whose aligned
    /// window still contains both ends.
    pub fn fit_range(&mut self, from: u64, to: u64) {
        let (from, to) = (from.min(to), from.max(to));
        let columns = self.wave_columns();
        let origin = self.wave.start;
        let mut raw = (to - from).max(1).div_ceil(columns);
        for _ in 0..64 {
            let s = self.snap_ticks(raw).min(self.fit_ticks());
            let start = origin + (from.saturating_sub(origin) / s) * s;
            if start.saturating_add(s.saturating_mul(columns)) >= to || s >= self.fit_ticks() {
                self.session.start = start;
                self.session.span = s.saturating_mul(columns);
                break;
            }
            raw = s + 1;
        }
        self.clamp_window();
    }
    /// Put the window on whole cells: integer ticks per cell, a start on a cell
    /// boundary, no zoom beyond the whole recording, and no empty scrolling.
    pub(crate) fn snap_window(&mut self) {
        if self.layout().wave_width < 2 {
            return;
        }
        let columns = self.wave_columns();
        let s = self
            .snap_ticks(self.session.span.div_ceil(columns))
            .min(self.fit_ticks());
        self.session.span = s.saturating_mul(columns);
        let origin = self.wave.start;
        let align = |time: u64| origin + (time.saturating_sub(origin) / s) * s;
        let last = if self.wave.end > origin.saturating_add(self.session.span) {
            // The latest start that still shows the final cell of the recording.
            align(self.wave.end - self.session.span + s - 1)
        } else {
            origin
        };
        self.session.start = align(self.session.start).clamp(origin, last.max(origin));
    }
    /// The resolution badge on the title row: its text, column and width.
    /// "Exact" means every transition sits on its own cell boundary.
    pub fn scale_badge(&self) -> Option<(String, u16, u16)> {
        use unicode_width::UnicodeWidthStr;
        let width = self.width.min(512);
        if width < 80 {
            return None;
        }
        let whole = self.view_start() <= self.wave.start && self.view_end() >= self.wave.end;
        let coverage = if whole {
            "all shown".to_string()
        } else {
            let duration = self.wave.end.saturating_sub(self.wave.start).max(1);
            let shown = self.view_end().min(self.wave.end) - self.view_start().min(self.wave.end);
            format!(
                "{}% shown",
                (u128::from(shown) * 100 / u128::from(duration)).clamp(1, 99)
            )
        };
        let text = format!(
            " {} · {} / cell · {coverage} ",
            if self.preview_scale() {
                "≈ Preview"
            } else {
                "Exact"
            },
            self.wave.timescale.format(self.cell_ticks())
        );
        let w = (text.width() as u16).min(width / 2);
        Some((text, width.saturating_sub(w + 1), w))
    }
    /// Zoom to the exact limit, keeping cursor A (or the window center) in place.
    pub fn exact_view(&mut self) {
        let Some(limit) = self.exact_limit() else {
            self.status = "Exact at every zoom · no watched signal changes".into();
            return;
        };
        if self.cell_ticks() == limit {
            self.status = format!(
                "Already at the exact limit · {} / cell",
                self.wave.timescale.format(limit)
            );
            return;
        }
        self.remember_view();
        let anchor = if self.column_at(self.session.cursor).is_some() {
            self.session.cursor
        } else {
            self.view_start() + (self.view_end() - self.view_start()) / 2
        };
        let column = self
            .column_at(anchor)
            .map(|x| u64::from(x - self.layout().wave_x))
            .unwrap_or(self.wave_columns() / 2);
        self.session.span = limit.saturating_mul(self.wave_columns());
        self.session.start = anchor.saturating_sub(column.saturating_mul(limit));
        self.clamp_window();
        self.status = format!(
            "Exact limit · {} / cell · the widest view where every edge sits on a cell boundary",
            self.wave.timescale.format(limit)
        );
    }
    /// Whether every transition this row draws in the window lies on a cell
    /// boundary. Bounded: very dense rows count as estimated.
    pub(crate) fn row_exact(&self, trace: &Trace, bit: Option<u32>) -> bool {
        let s = self.cell_ticks();
        if s == 1 || self.exact_limit().is_some_and(|limit| s <= limit) {
            return true;
        }
        let origin = self.wave.start;
        let start = self.view_start();
        let first = trace.index_at(start);
        let end = trace.index_at(self.view_end());
        if end.saturating_sub(first) > 4096 {
            return false;
        }
        for index in first.max(1)..end {
            let change = &trace.changes[index];
            if bit.is_some_and(|bit| {
                trace.changes[index - 1].value.bit_logic(bit) == change.value.bit_logic(bit)
            }) {
                continue;
            }
            if !(change.time.saturating_sub(origin)).is_multiple_of(s) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn divisors_are_sorted_and_complete() {
        assert_eq!(divisors(12), vec![1, 2, 3, 4, 6, 12]);
        assert_eq!(divisors(1), vec![1]);
        assert_eq!(divisors(97), vec![1, 97]);
        assert_eq!(divisors(10_000).len(), 25);
    }
}
