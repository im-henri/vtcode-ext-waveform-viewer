//! Transient row selection and atomic edits of the selected waveforms.
use crate::menus::MenuAction;
use crate::viewer::{Focus, Viewer, Watch, WaveRow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowKey {
    pub watch: usize,
    pub bit: Option<u32>,
}
impl From<WaveRow> for RowKey {
    fn from(row: WaveRow) -> Self {
        Self {
            watch: row.watch,
            bit: row.bit,
        }
    }
}
impl RowKey {
    fn mask(self) -> u128 {
        match self.bit {
            None => 1,
            Some(bit) if bit < 64 => 1u128 << (bit + 1),
            _ => 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WaveSelection {
    // One parent row plus up to 64 expanded bits per watch. Membership stays
    // constant-time, even when all 16,640 possible rows are selected.
    masks: Vec<u128>,
}
impl WaveSelection {
    fn new(watches: usize) -> Self {
        Self {
            masks: vec![0; watches],
        }
    }
    fn contains(&self, key: RowKey) -> bool {
        self.masks
            .get(key.watch)
            .is_some_and(|mask| mask & key.mask() != 0)
    }
    fn insert(&mut self, key: RowKey) {
        if let Some(mask) = self.masks.get_mut(key.watch) {
            *mask |= key.mask();
        }
    }
    fn toggle(&mut self, key: RowKey) {
        if let Some(mask) = self.masks.get_mut(key.watch) {
            *mask ^= key.mask();
        }
    }
    fn len(&self) -> usize {
        self.masks
            .iter()
            .map(|mask| mask.count_ones() as usize)
            .sum()
    }
}

impl Viewer {
    pub fn selected_rows(&self) -> Vec<RowKey> {
        if self.wave_selection.is_none() {
            return self
                .session
                .watches
                .get(self.selection)
                .map(|_| RowKey {
                    watch: self.selection,
                    bit: self.selected_bit,
                })
                .into_iter()
                .collect();
        }
        self.rows()
            .into_iter()
            .filter(|row| self.is_row_selected(*row))
            .map(RowKey::from)
            .collect()
    }

    pub fn selection_count(&self) -> usize {
        self.wave_selection.as_ref().map_or_else(
            || usize::from(self.session.watches.get(self.selection).is_some()),
            WaveSelection::len,
        )
    }

    pub fn is_row_selected(&self, row: WaveRow) -> bool {
        self.wave_selection.as_ref().map_or(
            row.watch == self.selection && row.bit == self.selected_bit,
            |selection| selection.contains(row.into()),
        )
    }

    pub(crate) fn clear_wave_selection(&mut self) {
        self.wave_selection = None;
        self.selection_anchor = None;
    }

    pub(crate) fn retain_wave_selection(&mut self, rows: &[WaveRow]) {
        if let Some(selection) = &mut self.wave_selection {
            let mut visible = WaveSelection::new(self.session.watches.len());
            for row in rows {
                visible.insert((*row).into());
            }
            for (index, mask) in selection.masks.iter_mut().enumerate() {
                *mask &= visible.masks.get(index).copied().unwrap_or(0);
            }
        }
        if self
            .selection_anchor
            .is_some_and(|anchor| !rows.iter().any(|row| RowKey::from(*row) == anchor))
        {
            self.selection_anchor = None;
        }
    }

    pub(crate) fn select_wave(&mut self, key: RowKey, extend: bool, toggle: bool) {
        if !extend && !toggle {
            if self.session.watches.get(key.watch).is_none() {
                return;
            }
            let had_multiple = self.wave_selection.is_some();
            self.clear_wave_selection();
            self.selection = key.watch;
            self.selected_bit = key.bit;
            self.selection_anchor = Some(key);
            if had_multiple {
                self.status = "Waveform selected · Right-click for display options".into();
            }
            return;
        }
        let rows = self.rows();
        let Some(target) = rows.iter().position(|row| RowKey::from(*row) == key) else {
            return;
        };
        let current = RowKey {
            watch: self.selection,
            bit: self.selected_bit,
        };
        if extend {
            let anchor = self.selection_anchor.unwrap_or(current);
            let start = rows
                .iter()
                .position(|row| RowKey::from(*row) == anchor)
                .unwrap_or(target);
            let mut selection = if toggle {
                self.wave_selection.clone().unwrap_or_else(|| {
                    let mut selection = WaveSelection::new(self.session.watches.len());
                    selection.insert(current);
                    selection
                })
            } else {
                WaveSelection::new(self.session.watches.len())
            };
            for row in &rows[start.min(target)..=start.max(target)] {
                selection.insert((*row).into());
            }
            self.wave_selection = Some(selection);
            self.selection_anchor = Some(anchor);
        } else if toggle {
            let selection = self.wave_selection.get_or_insert_with(|| {
                let mut selection = WaveSelection::new(self.session.watches.len());
                selection.insert(current);
                selection
            });
            selection.toggle(key);
            self.selection_anchor = Some(key);
        }
        self.selection = key.watch;
        self.selected_bit = key.bit;
        if let Some(selection) = &self.wave_selection
            && !selection.contains(key)
            && let Some((_, row)) = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| selection.contains((**row).into()))
                .min_by_key(|(index, _)| index.abs_diff(target))
        {
            self.selection = row.watch;
            self.selected_bit = row.bit;
        }
        self.status = format!(
            "{} waves selected · Right-click for display options · Delete removes",
            self.selection_count()
        );
    }

    pub(crate) fn select_all_waves(&mut self) {
        let mut selection = WaveSelection::new(self.session.watches.len());
        for row in self.rows() {
            selection.insert(row.into());
        }
        self.wave_selection = Some(selection);
        self.selection_anchor = Some(RowKey {
            watch: self.selection,
            bit: self.selected_bit,
        });
        self.focus = Focus::Waves;
        self.status = format!(
            "{} waves selected · Right-click for display options · Delete removes",
            self.selection_count()
        );
    }

    pub(crate) fn row_accepts_edit(&self, key: RowKey, action: MenuAction) -> bool {
        let Some(Watch::Signal { bit, .. }) = self.session.watches.get(key.watch) else {
            return false;
        };
        let whole = key.bit.or(*bit).is_none();
        match action {
            MenuAction::Radix(_) => {
                whole
                    && self.signal_id(key.watch).is_some_and(|id| {
                        !matches!(self.wave.signals[id].kind.as_str(), "real" | "realtime")
                    })
            }
            MenuAction::Digital | MenuAction::Analog => whole,
            MenuAction::Color(_) | MenuAction::Size(_) => true,
            _ => false,
        }
    }

    pub(crate) fn row_matches_edit(&self, key: RowKey, action: MenuAction) -> bool {
        let Some(
            watch @ Watch::Signal {
                radix,
                analog,
                color,
                ..
            },
        ) = self.session.watches.get(key.watch)
        else {
            return false;
        };
        match action {
            MenuAction::Radix(value) => *radix == value && !*analog,
            MenuAction::Digital => !*analog,
            MenuAction::Analog => *analog,
            MenuAction::Color(value) => *color == value,
            MenuAction::Size(value) => watch.size(key.bit) == value,
            _ => false,
        }
    }

    pub(crate) fn selected_edit_key(&self, action: MenuAction) -> Option<RowKey> {
        let keys = self.selected_rows();
        let current = RowKey {
            watch: self.selection,
            bit: self.selected_bit,
        };
        keys.iter()
            .find(|key| **key == current && self.row_accepts_edit(**key, action))
            .or_else(|| keys.iter().find(|key| self.row_accepts_edit(**key, action)))
            .copied()
    }

    pub(crate) fn apply_selection_edit(&mut self, action: MenuAction, checkpoint: bool) -> usize {
        let mut seen = vec![false; self.session.watches.len()];
        let keys: Vec<_> = self
            .selected_rows()
            .into_iter()
            .filter_map(|mut key| {
                if matches!(action, MenuAction::Color(_)) {
                    key.bit = None;
                    if std::mem::replace(&mut seen[key.watch], true) {
                        return None;
                    }
                }
                (self.row_accepts_edit(key, action) && !self.row_matches_edit(key, action))
                    .then_some(key)
            })
            .collect();
        if keys.is_empty() {
            return 0;
        }
        if checkpoint {
            self.checkpoint();
        }
        for key in &keys {
            let watch = &mut self.session.watches[key.watch];
            if let MenuAction::Size(value) = action {
                watch.set_size(key.bit, value);
            } else if let Watch::Signal {
                radix,
                analog,
                color,
                ..
            } = watch
            {
                match action {
                    MenuAction::Radix(value) => {
                        *radix = value;
                        *analog = false;
                    }
                    MenuAction::Digital => *analog = false,
                    MenuAction::Analog => *analog = true,
                    MenuAction::Color(value) => *color = value,
                    _ => {}
                }
            }
        }
        self.hover = None;
        self.ensure_selection();
        self.status = format!("{} waveforms updated · Ctrl+Z to undo", keys.len());
        keys.len()
    }

    pub(crate) fn delete_selected_waves(&mut self) {
        let keys = self.selected_rows();
        if keys.is_empty() {
            return;
        }
        let mut remove = vec![false; self.session.watches.len()];
        let mut fold = vec![false; self.session.watches.len()];
        for key in keys {
            if key.bit.is_none() {
                remove[key.watch] = true;
            } else {
                fold[key.watch] = true;
            }
        }
        self.checkpoint();
        let removed = remove.iter().filter(|remove| **remove).count();
        let mut folded = 0;
        for (index, watch) in self.session.watches.iter_mut().enumerate() {
            if fold[index]
                && !remove[index]
                && let Watch::Signal { expanded, .. } = watch
            {
                *expanded = false;
                folded += 1;
            }
        }
        let next = remove
            .iter()
            .zip(&fold)
            .position(|(remove, fold)| *remove || *fold)
            .unwrap_or(self.selection);
        for index in (0..remove.len()).rev() {
            if remove[index] {
                self.session.watches.remove(index);
            }
        }
        self.clear_wave_selection();
        self.selection = next.min(self.session.watches.len().saturating_sub(1));
        self.selected_bit = None;
        self.ensure_selection();
        self.status = match (removed, folded) {
            (0, folded) => format!("Folded {folded} buses · Ctrl+Z to undo"),
            (removed, 0) => format!("Removed {removed} watched rows · Ctrl+Z to undo"),
            (removed, folded) => {
                format!("Removed {removed} watched rows · Folded {folded} buses · Ctrl+Z to undo")
            }
        };
    }
}
