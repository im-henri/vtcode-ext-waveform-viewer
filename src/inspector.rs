//! A stable measurement surface, so inspecting data does not cover the traces.
use crate::model::{Radix, Value};
use crate::render::{Canvas, display_value, ellipsize};
use crate::viewer::{Viewer, Watch, WaveRow};
use unicode_width::UnicodeWidthStr;

impl Viewer {
    pub(crate) fn draw_inspector(&self, canvas: &mut Canvas) {
        let height = self.inspector_height();
        if height == 0 {
            return;
        }
        let y = canvas.height - 4 - height;
        for offset in 0..height {
            canvas.fill(0, y + offset, canvas.width, "text", "panel");
        }
        let row = self.hover.and_then(|h| h.row).or_else(|| {
            self.rows()
                .into_iter()
                .find(|row| row.watch == self.selection && row.bit == self.selected_bit)
        });
        let Some(row) = row.filter(|row| self.signal_id(row.watch).is_some()) else {
            canvas.text(
                1,
                y,
                canvas.width.saturating_sub(2),
                "INSPECTOR  ·  Select or hover a signal to inspect values and measurements",
                "muted",
                "header",
                true,
            );
            canvas.text(
                1,
                y + 1,
                canvas.width.saturating_sub(2),
                "Click a trace: A   Alt+click: B   ←/→: transition   v: hide inspector",
                "muted",
                "panel",
                false,
            );
            return;
        };
        let Watch::Signal { bit, radix, .. } = self.session.watches[row.watch] else {
            return;
        };
        let bit = row.bit.or(bit);
        let radix = if bit.is_some() { Radix::Binary } else { radix };
        let hover = self
            .hover
            .filter(|h| h.row == Some(row) && h.x >= self.layout().wave_x);
        let time = hover
            .map(|h| self.hover_time(h))
            .unwrap_or(self.session.cursor);
        let transition = self
            .hover
            .filter(|h| h.row == Some(row) && h.x >= self.layout().browser + self.layout().name)
            .and_then(|h| self.hover_transition(h))
            .or_else(|| {
                (hover.is_none())
                    .then(|| {
                        self.transition_values(row.watch, row.bit, time)
                            .map(|(before, after)| (time, before, after))
                    })
                    .flatten()
            });
        let source = if hover.is_some() { "Pointer" } else { "A" };
        let context = if let Some((edge, _, _)) = &transition {
            format!("{source} · Edge @ {}", self.wave.timescale.format(*edge))
        } else {
            format!("{source} @ {}", self.wave.timescale.format(time))
        };
        let id = self.signal_id(row.watch).unwrap();
        let signal = &self.wave.signals[id];
        let label = format!(
            "INSPECT  {}  ·  {} · {} {}",
            self.full_watch_label(row),
            radix.label().to_uppercase(),
            if bit.is_some() { 1 } else { signal.width },
            if bit.is_some() || signal.width == 1 {
                "bit"
            } else {
                "bits"
            }
        );
        let right = (context.width() as u16 + 2).min(canvas.width / 2);
        canvas.fill(0, y, canvas.width, "muted", "header");
        canvas.text(
            1,
            y,
            canvas.width.saturating_sub(right + 2),
            &ellipsize(&label, usize::from(canvas.width.saturating_sub(right + 2))),
            "text",
            "header",
            true,
        );
        canvas.text(
            canvas.width - right,
            y,
            right,
            &context,
            "accent",
            "header",
            true,
        );
        let width = canvas.width.saturating_sub(2);
        if let Some((edge, before, after)) = transition {
            for (offset, prefix, value, fallback) in [
                (1, "Before", before, "muted"),
                (2, "After ", after, "accent"),
            ] {
                let text = format!(
                    "{prefix} {}  {}",
                    radix.label().to_uppercase(),
                    display_value(&value, radix)
                );
                canvas.text(
                    1,
                    y + offset,
                    width,
                    &ellipsize(&text, usize::from(width)),
                    inspector_role(&value, fallback),
                    "panel",
                    false,
                );
            }
            let reference = self.reference_readout(row, radix);
            let text = format!(
                "Before: t < {} · After: t ≥ {} · {reference}",
                self.wave.timescale.format(edge),
                self.wave.timescale.format(edge)
            );
            canvas.text(
                1,
                y + 3,
                width,
                &ellipsize(&text, usize::from(width)),
                "muted",
                "panel",
                false,
            );
        } else {
            let value = self.inspector_value(row, time, radix);
            let text = format!(
                "{source} {}  ·  {}  {value}",
                self.wave.timescale.format(time),
                radix.label().to_uppercase()
            );
            let clipped = text.width() > usize::from(width);
            canvas.text(
                1,
                y + 1,
                width,
                &ellipsize(&text, usize::from(width)),
                "accent",
                "panel",
                false,
            );
            let reference = if hover.is_some() {
                format!(
                    "A {}: {} · {}",
                    self.wave.timescale.format(self.session.cursor),
                    self.inspector_value(row, self.session.cursor, radix),
                    self.reference_readout(row, radix)
                )
            } else {
                self.reference_readout(row, radix)
            };
            canvas.text(
                1,
                y + 2,
                width,
                &ellipsize(&reference, usize::from(width)),
                "warning",
                "panel",
                false,
            );
            let detail = hover
                .and_then(|hover| self.hover_activity(hover))
                .or_else(|| {
                    let (start, end) =
                        self.wave.traces[signal.trace].held_interval(time, bit, self.wave.end)?;
                    Some(format!(
                        "{} → {} · held {} · Ctrl+C copies {source} · v hides inspector",
                        self.wave.timescale.format(start),
                        self.wave.timescale.format(end),
                        self.wave.timescale.format(end.saturating_sub(start))
                    ))
                })
                .unwrap_or_else(|| {
                    "Held interval unresolved · Hover + i inspects a terminal cell".into()
                });
            let detail = if clipped {
                format!("… Ctrl+C copies the full value · {detail}")
            } else {
                detail
            };
            canvas.text(
                1,
                y + 3,
                width,
                &ellipsize(&detail, usize::from(width)),
                "muted",
                "panel",
                false,
            );
        }
    }

    fn inspector_value(&self, row: WaveRow, time: u64, radix: Radix) -> String {
        if let Some((before, after)) = self.transition_values(row.watch, row.bit, time) {
            format!(
                "{} → {}",
                display_value(&before, radix),
                display_value(&after, radix)
            )
        } else {
            self.watch_value(row.watch, row.bit, time)
                .map(|value| display_value(&value, radix))
                .unwrap_or_else(|| "No recorded value".into())
        }
    }

    fn reference_readout(&self, row: WaveRow, radix: Radix) -> String {
        self.session
            .alt_cursor
            .map(|b| {
                format!(
                    "B {}: {} · Δt(B−A) {}{}",
                    self.wave.timescale.format(b),
                    self.inspector_value(row, b, radix),
                    if b < self.session.cursor { "−" } else { "+" },
                    self.wave.timescale.format(b.abs_diff(self.session.cursor))
                )
            })
            .unwrap_or_else(|| {
                "B — · Alt+click sets B · A–B fits the measurement · Ctrl+C copies value".into()
            })
    }
}

fn inspector_role(value: &Value, fallback: &'static str) -> &'static str {
    match value.logic() {
        'x' => "error",
        'z' => "warning",
        _ => fallback,
    }
}
