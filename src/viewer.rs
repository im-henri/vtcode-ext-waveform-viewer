use crate::model::{Radix, Value, Waveform};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const MAX_WATCHES: usize = 256;
pub const PALETTE: [&str; 8] = [
    "chart1", "chart2", "chart3", "chart4", "chart5", "chart6", "chart7", "chart8",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Watch {
    Signal {
        path: String,
        bit: Option<u32>,
        radix: Radix,
        color: u8,
        label: String,
        expanded: bool,
        analog: bool,
        #[serde(default)]
        size: WaveSize,
        #[serde(default, skip_serializing_if = "WaveSize::is_normal")]
        bit_size: WaveSize,
        #[serde(
            default,
            deserialize_with = "deserialize_bit_sizes",
            skip_serializing_if = "BTreeMap::is_empty"
        )]
        bit_sizes: BTreeMap<u32, WaveSize>,
    },
    Group {
        name: String,
        collapsed: bool,
    },
    Divider,
}
impl Watch {
    pub fn signal(path: String, color: u8) -> Self {
        Self::Signal {
            path,
            bit: None,
            radix: Radix::Hex,
            color,
            label: String::new(),
            expanded: false,
            analog: false,
            size: WaveSize::Normal,
            bit_size: WaveSize::Normal,
            bit_sizes: BTreeMap::new(),
        }
    }

    pub fn size(&self, bit: Option<u32>) -> WaveSize {
        match self {
            Self::Signal {
                size,
                bit_size,
                bit_sizes,
                ..
            } => bit
                .map(|bit| bit_sizes.get(&bit).copied().unwrap_or(*bit_size))
                .unwrap_or(*size),
            _ => WaveSize::Small,
        }
    }

    pub fn set_size(&mut self, bit: Option<u32>, value: WaveSize) {
        if let Self::Signal {
            size,
            bit_size,
            bit_sizes,
            ..
        } = self
        {
            if let Some(bit) = bit {
                if value == *bit_size {
                    bit_sizes.remove(&bit);
                } else {
                    bit_sizes.insert(bit, value);
                }
            } else {
                *size = value;
            }
        }
    }
}

fn deserialize_bit_sizes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<u32, WaveSize>, D::Error> {
    // Internally tagged enum buffering leaves JSON object keys as strings.
    BTreeMap::<String, WaveSize>::deserialize(deserializer)?
        .into_iter()
        .map(|(bit, size)| {
            bit.parse::<u32>()
                .map(|bit| (bit, size))
                .map_err(serde::de::Error::custom)
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Marker {
    pub time: u64,
    pub name: String,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum WaveSize {
    #[serde(alias = "Compact")]
    Small,
    #[default]
    #[serde(alias = "Roomy")]
    Normal,
}
impl WaveSize {
    fn is_normal(&self) -> bool {
        *self == Self::Normal
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Normal => "Normal",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Small => Self::Normal,
            Self::Normal => Self::Small,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Session {
    pub schema: u32,
    pub watches: Vec<Watch>,
    pub cursor: u64,
    pub alt_cursor: Option<u64>,
    pub start: u64,
    pub span: u64,
    pub markers: Vec<Marker>,
    pub browser: bool,
    pub full_names: bool,
    pub snap: bool,
    pub name_width: u16,
    pub value_width: u16,
    #[serde(default)]
    pub inspector: bool,
    /// Read the former global setting only to migrate older sessions.
    #[serde(rename = "density", skip_serializing)]
    pub legacy_density: Option<WaveSize>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            schema: 2,
            watches: vec![],
            cursor: 0,
            alt_cursor: None,
            start: 0,
            span: 1,
            markers: vec![],
            browser: false,
            full_names: false,
            snap: true,
            name_width: 28,
            value_width: 23,
            inspector: false,
            legacy_density: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Browser,
    Waves,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompt {
    Filter,
    FindValue,
    Goto,
    GotoB,
    Rename,
    Group,
    Marker,
    Import,
}
impl Prompt {
    pub fn label(self) -> &'static str {
        match self {
            Self::Filter => "Filter signals",
            Self::FindValue => "Find value",
            Self::Goto => "Go to time (A)",
            Self::GotoB => "Go to time (B)",
            Self::Rename => "Rename",
            Self::Group => "New group",
            Self::Marker => "Marker name",
            Self::Import => "Paste session JSON",
        }
    }
}
#[derive(Clone, Debug)]
pub enum BrowserNode {
    Scope(usize, usize),
    Signal(usize, usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveRow {
    pub watch: usize,
    pub bit: Option<u32>,
    pub line: u16,
    pub height: u16,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Event {
    pub code: String,
    pub text: String,
    pub kind: String,
    pub view_column: u16,
    pub view_row: u16,
    pub width: u16,
    pub height: u16,
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub direction: i32,
    pub axis: String,
    pub pointer_inside: Option<bool>,
    /// Optional host additions; legacy event payloads remain accepted.
    pub button: String,
    pub click_count: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub browser: u16,
    pub name: u16,
    pub value: u16,
    pub wave_x: u16,
    pub wave_width: u16,
    pub body_y: u16,
    pub body_height: u16,
}
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    pub x: u16,
    pub start: u64,
    pub span: u64,
    pub time: u64,
    pub end: u64,
    pub end_x: u16,
    pub zoom: bool,
    pub pan: bool,
    pub watch: Option<usize>,
    pub split: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct OverviewDrag {
    pub start: u64,
    pub anchor: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Hover {
    pub x: u16,
    pub y: u16,
    pub row: Option<WaveRow>,
}

#[derive(Clone, Debug)]
pub struct SignalDrag {
    pub watch: Watch,
    pub source: Option<usize>,
    pub from_browser: bool,
    pub press: (u16, u16),
    pub pointer: (u16, u16),
    pub active: bool,
    /// An insertion boundary in the original watch list, before removing the source.
    pub target: Option<usize>,
    pub ticks: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayRow {
    pub row: WaveRow,
    pub preview: bool,
}

pub struct Viewer {
    pub wave: Waveform,
    pub signal_index: HashMap<String, usize>,
    pub path: String,
    pub document: Option<u32>,
    pub session: Session,
    pub width: u16,
    pub height: u16,
    pub focus: Focus,
    pub selection: usize,
    pub selected_bit: Option<u32>,
    pub control_drag: Option<crate::controls::ControlDrag>,
    pub wave_selection: Option<crate::selection::WaveSelection>,
    pub selection_anchor: Option<crate::selection::RowKey>,
    pub scroll: usize,
    pub browser_selection: usize,
    pub browser_scroll: usize,
    pub expanded_scopes: HashSet<usize>,
    pub filter: String,
    pub browser_nodes: Vec<BrowserNode>,
    pub prompt: Option<Prompt>,
    pub dialog: crate::dialogs::DialogState,
    pub context_menu: Option<crate::menus::ContextMenu>,
    pub input: String,
    pub status: String,
    pub error: bool,
    pub loading: Option<(u64, u64)>,
    pub help: bool,
    pub help_scroll: usize,
    pub drag: Option<Drag>,
    pub overview_drag: Option<OverviewDrag>,
    pub signal_drag: Option<SignalDrag>,
    pub hover: Option<Hover>,
    pub(crate) detail_stop: Option<(u64, u64, u16)>,
    pub(crate) view_history: Vec<(u64, u64)>,
    pub history: Vec<Session>,
    pub redo: Vec<Session>,
    pub save_requested: bool,
    pub reload_requested: bool,
    pub copy_requested: Option<String>,
    pub search_query: String,
    pub search: Option<ValueSearch>,
    pub edge_search: Option<EdgeSearch>,
    pub pending_session: Option<Session>,
    pub cancelled: bool,
}
pub struct ValueSearch {
    pub trace: usize,
    pub bit: Option<u32>,
    pub next: usize,
    pub visited: usize,
    pub reverse: bool,
    pub query: String,
    pub radix: Radix,
}
pub struct EdgeSearch {
    pub trace: usize,
    pub bit: Option<u32>,
    pub next: Option<usize>,
    pub reverse: bool,
    pub alt: bool,
    pub polarity: Option<char>,
}

impl Viewer {
    pub fn new(path: String, document: Option<u32>) -> Self {
        let mut viewer = Self {
            wave: Waveform::default(),
            signal_index: HashMap::new(),
            path,
            document,
            session: Session::default(),
            width: 120,
            height: 32,
            focus: Focus::Waves,
            selection: 0,
            selected_bit: None,
            control_drag: None,
            wave_selection: None,
            selection_anchor: None,
            scroll: 0,
            browser_selection: 0,
            browser_scroll: 0,
            expanded_scopes: HashSet::from([0]),
            filter: String::new(),
            browser_nodes: vec![],
            prompt: None,
            dialog: crate::dialogs::DialogState::default(),
            context_menu: None,
            input: String::new(),
            status: "Loading waveform…  Esc cancels".into(),
            error: false,
            loading: None,
            help: false,
            help_scroll: 0,
            drag: None,
            overview_drag: None,
            signal_drag: None,
            hover: None,
            detail_stop: None,
            view_history: Vec::new(),
            history: vec![],
            redo: vec![],
            save_requested: false,
            reload_requested: false,
            copy_requested: None,
            search_query: String::new(),
            search: None,
            edge_search: None,
            pending_session: None,
            cancelled: false,
        };
        viewer.rebuild_browser();
        viewer
    }
    pub(crate) fn new_watch(&self, signal: &crate::model::Signal, color: u8) -> Watch {
        let mut watch = Watch::signal(signal.path.clone(), color);
        if let Watch::Signal { analog, .. } = &mut watch {
            *analog = matches!(signal.kind.as_str(), "real" | "realtime");
        }
        watch
    }

    pub fn ready(&mut self, wave: Waveform) {
        self.clear_wave_selection();
        self.hover = None;
        self.drag = None;
        self.overview_drag = None;
        self.control_drag = None;
        self.signal_drag = None;
        self.wave = wave;
        self.signal_index = self
            .wave
            .signals
            .iter()
            .enumerate()
            .map(|(i, s)| (s.path.clone(), i))
            .collect();
        self.loading = None;
        self.cancelled = false;
        for &scope in &self.wave.scopes[0].children {
            self.expanded_scopes.insert(scope);
            self.expanded_scopes
                .extend(self.wave.scopes[scope].children.iter().copied());
        }
        self.session.cursor = self.wave.start;
        self.session.watches.clear();
        self.fit();
        let restored = self
            .pending_session
            .take()
            .is_some_and(|session| self.restore(session).is_ok());
        if !restored {
            let mut traces = HashSet::new();
            for signal in self
                .wave
                .signals
                .iter()
                .filter(|signal| traces.insert(signal.trace))
                .take(12)
            {
                let watch = self.new_watch(signal, 0);
                self.session.watches.push(watch);
            }
        }
        self.rebuild_browser();
        self.status = format!(
            "{} signals · {} changes · {}  |  ? help",
            self.wave.signals.len(),
            self.wave.change_count,
            if restored {
                "session restored"
            } else {
                "ready"
            }
        );
        self.error = false;
        self.view_history.clear();
    }
    pub fn inspector_height(&self) -> u16 {
        if self.session.inspector && self.width >= 80 && self.height >= 30 {
            4
        } else {
            0
        }
    }
    pub fn layout(&self) -> Layout {
        let browser = if self.session.browser
            && (self.width >= 100 || self.focus == Focus::Browser && self.width >= 48)
        {
            (self.width / 5).clamp(16, 34)
        } else {
            0
        };
        let available = self.width.min(512).saturating_sub(browser);
        let name = self
            .session
            .name_width
            .min((available / 3).max(8))
            .min(available);
        let value = if available > 45 {
            self.session
                .value_width
                .min(available.saturating_sub(name + 16))
        } else {
            7.min(available.saturating_sub(name))
        };
        let wave_x = browser + name + value;
        Layout {
            browser,
            name,
            value,
            wave_x,
            wave_width: self.width.min(512).saturating_sub(wave_x),
            body_y: 5,
            body_height: self
                .height
                .min(200)
                .saturating_sub(9 + self.inspector_height()),
        }
    }
    pub fn fit(&mut self) {
        if (self.session.start, self.session.span)
            != (
                self.wave.start,
                self.wave.end.saturating_sub(self.wave.start).max(1),
            )
        {
            self.remember_view();
        }
        self.detail_stop = None;
        self.session.start = self.wave.start;
        self.session.span = self.wave.end.saturating_sub(self.wave.start).max(1);
        self.status =
            "Entire recording · Shaded cells show activity · Detail inspects cursor A".into();
    }
    pub fn clamp_window(&mut self) {
        let duration = self.wave.end.saturating_sub(self.wave.start).max(1);
        self.session.span = self.session.span.clamp(1, duration);
        self.session.start = self.session.start.clamp(
            self.wave.start,
            self.wave
                .end
                .saturating_sub(self.session.span)
                .max(self.wave.start),
        );
        if self.view_history.last() == Some(&(self.session.start, self.session.span)) {
            self.view_history.pop();
        }
    }
    pub fn time_at(&self, column: u16) -> u64 {
        let l = self.layout();
        if l.wave_width == 1 {
            return self
                .session
                .start
                .saturating_add(self.session.span)
                .min(self.wave.end);
        }
        let x = column
            .saturating_sub(l.wave_x)
            .min(l.wave_width.saturating_sub(1));
        self.session
            .start
            .saturating_add(
                ((u128::from(self.session.span) * u128::from(x))
                    / u128::from(l.wave_width.saturating_sub(1).max(1))) as u64,
            )
            .min(self.wave.end)
    }
    pub fn column_at(&self, time: u64) -> Option<u16> {
        let l = self.layout();
        if time < self.session.start || time > self.session.start.saturating_add(self.session.span)
        {
            return None;
        }
        Some(
            l.wave_x
                + ((u128::from(time - self.session.start)
                    * u128::from(l.wave_width.saturating_sub(1)))
                .div_ceil(u128::from(self.session.span.max(1)))) as u16,
        )
    }
    pub fn zoom(&mut self, inward: bool, anchor: u64) {
        self.remember_view();
        let old = self.session.span;
        let previous_stop = self.detail_stop.take();
        let continue_overview =
            !inward && previous_stop == Some((self.session.start, old, self.layout().wave_width));
        let mut new = if inward {
            old.div_ceil(2).max(1)
        } else {
            old.saturating_mul(2)
        };
        let limit = self.detail_limit();
        let mut stopped = false;
        if let Some(limit) = limit
            && !continue_overview
            && old < limit
            && new > limit
        {
            new = limit;
            stopped = true;
        }
        let duration = self.wave.end.saturating_sub(self.wave.start).max(1);
        new = new.min(duration);
        let relative = anchor.saturating_sub(self.session.start).min(old);
        self.session.span = new;
        self.session.start = anchor.saturating_sub(
            ((u128::from(relative) * u128::from(new)) / u128::from(old.max(1))) as u64,
        );
        self.clamp_window();
        if stopped || !inward && limit == Some(self.session.span) {
            self.detail_stop = Some((
                self.session.start,
                self.session.span,
                self.layout().wave_width,
            ));
            self.status =
                "Terminal detail limit · − again continues into activity overview · i inspects A"
                    .into();
        } else {
            self.status = format!(
                "Zoomed {} · Hover + i inspects a cell · Detail inspects cursor A",
                if inward { "in" } else { "out" }
            );
        }
    }
    pub fn pan(&mut self, right: bool) {
        let step = (self.session.span / 5).max(1);
        self.session.start = if right {
            self.session.start.saturating_add(step)
        } else {
            self.session.start.saturating_sub(step)
        };
        self.clamp_window();
    }
    pub fn reveal_cursor(&mut self) {
        self.reveal_time(self.session.cursor);
    }
    pub(crate) fn reveal_time(&mut self, time: u64) {
        if self.column_at(time).is_none() {
            self.remember_view();
            self.session.start = time.saturating_sub(self.session.span / 2);
            self.clamp_window();
        }
    }
    pub fn signal_id(&self, watch: usize) -> Option<usize> {
        let Watch::Signal { path, .. } = self.session.watches.get(watch)? else {
            return None;
        };
        self.signal_index.get(path).copied()
    }
    pub fn selected(&self) -> Option<(usize, usize, Option<u32>)> {
        let id = self.signal_id(self.selection)?;
        let Watch::Signal { bit, .. } = self.session.watches.get(self.selection)? else {
            return None;
        };
        Some((id, self.wave.signals[id].trace, self.selected_bit.or(*bit)))
    }
    pub fn watch_value(&self, watch: usize, bit: Option<u32>, time: u64) -> Option<Value> {
        let signal = self.signal_id(watch)?;
        let Watch::Signal { bit: own_bit, .. } = &self.session.watches[watch] else {
            return None;
        };
        let value = self.wave.traces[self.wave.signals[signal].trace].value_at(time)?;
        Some(if let Some(bit) = bit.or(*own_bit) {
            value.bit(bit)
        } else {
            value.clone()
        })
    }
    pub fn ensure_selection(&mut self) {
        let rows = self.rows();
        self.retain_wave_selection(&rows);
        if rows.is_empty() {
            self.selection = 0;
            self.scroll = 0;
            self.selected_bit = None;
            return;
        }
        let index = rows
            .iter()
            .position(|r| r.watch == self.selection && r.bit == self.selected_bit)
            .or_else(|| rows.iter().position(|row| self.is_row_selected(*row)))
            .unwrap_or(0);
        self.selection = rows[index].watch;
        self.selected_bit = rows[index].bit;
        let visible = usize::from(self.layout().body_height);
        let top = usize::from(rows[index].line);
        let bottom = top + usize::from(rows[index].height);
        if top < self.scroll {
            self.scroll = top;
        }
        if bottom > self.scroll + visible {
            self.scroll = bottom.saturating_sub(visible);
        }
        self.scroll = self.scroll.min(
            usize::from(rows.last().unwrap().line + rows.last().unwrap().height)
                .saturating_sub(visible),
        );
    }
    pub fn checkpoint(&mut self) {
        self.checkpoint_session(self.session.clone());
    }
    pub(crate) fn checkpoint_session(&mut self, session: Session) {
        if self.history.last() != Some(&session) {
            self.history.push(session);
        }
        if self.history.len() > 64 {
            self.history.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, redo: bool) {
        let session = if redo {
            self.redo.pop()
        } else {
            self.history.pop()
        };
        if let Some(session) = session {
            self.clear_wave_selection();
            let old = std::mem::replace(&mut self.session, session);
            if redo {
                self.history.push(old);
            } else {
                self.redo.push(old);
            }
            self.selection = self
                .selection
                .min(self.session.watches.len().saturating_sub(1));
            self.ensure_selection();
            self.status = if redo { "Redone" } else { "Undone" }.into();
        }
    }
    pub fn restore(&mut self, mut session: Session) -> Result<(), String> {
        if !matches!(session.schema, 1 | 2)
            || session.watches.len() > MAX_WATCHES
            || session.markers.len() > 32
        {
            return Err("Unsupported session version or too many signals/markers".into());
        }
        let mut missing = 0;
        session.watches.retain(|watch| match watch {
            Watch::Signal {
                path,
                bit,
                label,
                color,
                ..
            } => {
                let found = path.len() <= 8192
                    && label.len() <= 512
                    && *color < 8
                    && self
                        .signal_index
                        .get(path)
                        .is_some_and(|&id| bit.is_none_or(|bit| bit < self.wave.signals[id].width));
                if !found {
                    missing += 1;
                }
                found
            }
            Watch::Group { name, .. } => name.len() <= 512,
            Watch::Divider => true,
        });
        if session.markers.iter().any(|m| m.name.len() > 512) {
            return Err("Marker name is too long".into());
        }
        let legacy_size = session.legacy_density.take();
        let legacy_size = (session.schema == 1).then(|| legacy_size.unwrap_or(WaveSize::Small));
        session.schema = 2;
        for watch in &mut session.watches {
            if let Watch::Signal {
                path,
                bit,
                size,
                bit_size,
                bit_sizes,
                ..
            } = watch
            {
                let width = self.wave.signals[self.signal_index[path]].width.min(64);
                bit_sizes.retain(|bit, _| *bit < width);
                if let Some(value) = legacy_size {
                    // The old global density was a default, not an individual
                    // size choice. Give every single-bit trace square edges.
                    *size = if bit.is_some() || width == 1 {
                        WaveSize::Normal
                    } else {
                        value
                    };
                    *bit_size = WaveSize::Normal;
                }
            }
        }
        session.cursor = session.cursor.clamp(self.wave.start, self.wave.end);
        session.alt_cursor = session
            .alt_cursor
            .map(|t| t.clamp(self.wave.start, self.wave.end));
        session
            .markers
            .retain(|m| m.time >= self.wave.start && m.time <= self.wave.end);
        session.name_width = session.name_width.clamp(12, 80);
        session.value_width = session.value_width.clamp(7, 64);
        self.session = session;
        self.clear_wave_selection();
        self.clamp_window();
        self.selected_bit = None;
        self.ensure_selection();
        self.status = format!("Session loaded · {missing} unavailable signals skipped");
        self.error = false;
        Ok(())
    }
    pub fn rebuild_browser(&mut self) {
        let mut nodes = Vec::new();
        let filter = self.filter.to_lowercase();
        let filtering = !filter.is_empty();
        let mut stack = vec![(0usize, 0usize)];
        // Iterative traversal also handles deeply nested simulator hierarchies.
        while let Some((scope, depth)) = stack.pop() {
            if scope > 0 {
                nodes.push(BrowserNode::Scope(scope, depth.saturating_sub(1)));
            }
            if !filtering && !self.expanded_scopes.contains(&scope) {
                continue;
            }
            let data = &self.wave.scopes[scope];
            for &signal in &data.signals {
                if !filtering
                    || self.wave.signals[signal]
                        .path
                        .to_lowercase()
                        .contains(&filter)
                {
                    nodes.push(BrowserNode::Signal(signal, depth));
                }
            }
            for &child in data.children.iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        if filtering {
            nodes.retain(|n| matches!(n, BrowserNode::Signal(..)));
        }
        self.browser_nodes = nodes;
        self.browser_selection = self
            .browser_selection
            .min(self.browser_nodes.len().saturating_sub(1));
        self.browser_scroll = self.browser_scroll.min(self.browser_selection);
    }
    pub fn add_signal(&mut self, id: usize) {
        self.clear_wave_selection();
        let signal = &self.wave.signals[id];
        if let Some(index) = self.session.watches.iter().position(
            |w| matches!(w, Watch::Signal { path, bit: None, .. } if path == &signal.path),
        ) {
            self.selection = index;
            self.selected_bit = None;
            // Reveal a signal inside a collapsed group.
            if let Some(Watch::Group { collapsed, .. }) = self.session.watches[..index]
                .iter_mut()
                .rev()
                .find(|w| matches!(w, Watch::Group { .. }))
            {
                *collapsed = false;
            }
        } else {
            if self.session.watches.len() >= MAX_WATCHES {
                self.fail("At most 256 watched signals/groups; remove some first");
                return;
            }
            let watch = self.new_watch(signal, 0);
            self.session.watches.push(watch);
            self.selection = self.session.watches.len() - 1;
            self.selected_bit = None;
        }
        self.ensure_selection();
    }
    pub fn browser_activate(&mut self, recursive: bool) {
        match self.browser_nodes.get(self.browser_selection).cloned() {
            Some(BrowserNode::Scope(scope, _)) if recursive => {
                self.checkpoint();
                let mut stack = vec![scope];
                while let Some(scope) = stack.pop() {
                    let data = &self.wave.scopes[scope];
                    let signals = data.signals.clone();
                    stack.extend(data.children.iter().rev());
                    for id in signals {
                        if self.session.watches.len() >= MAX_WATCHES {
                            break;
                        }
                        self.add_signal(id);
                    }
                }
                self.status = "Scope signals added".into();
            }
            Some(BrowserNode::Scope(scope, _)) => {
                if !self.expanded_scopes.remove(&scope) {
                    self.expanded_scopes.insert(scope);
                }
                self.rebuild_browser();
            }
            Some(BrowserNode::Signal(id, _)) => {
                self.checkpoint();
                self.add_signal(id);
                self.status = format!("Added {}", self.wave.signals[id].path);
            }
            None => {}
        }
    }
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = error.into();
        self.error = true;
    }
    pub fn open_prompt(&mut self, prompt: Prompt) {
        self.cancel_timeline_drag();
        self.signal_drag = None;
        self.context_menu = None;
        self.hover = None;
        self.help = false;
        self.dialog = crate::dialogs::DialogState::new(self);
        self.prompt = Some(prompt);
        self.input = match prompt {
            Prompt::Goto => self.wave.timescale.format(self.session.cursor),
            Prompt::GotoB => self
                .wave
                .timescale
                .format(self.session.alt_cursor.unwrap_or(self.session.cursor)),
            Prompt::Filter => self.filter.clone(),
            Prompt::FindValue => self.search_query.clone(),
            Prompt::Rename => match self.session.watches.get(self.selection) {
                Some(Watch::Group { name, .. }) => name.clone(),
                Some(Watch::Signal { label, .. }) => label.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        };
        self.dialog.cursor = self.input.len();
        self.dialog.select_all = !self.input.is_empty();
        self.error = false;
        if prompt == Prompt::Filter {
            self.session.browser = true;
            self.focus = Focus::Browser;
        }
    }
    pub fn edge(&mut self, reverse: bool, alt: bool, polarity: Option<char>) {
        let current = if alt {
            self.session.alt_cursor.unwrap_or(self.session.cursor)
        } else {
            self.session.cursor
        };
        self.search = None;
        if let Some((signal, trace, bit)) = self.selected() {
            if polarity.is_some()
                && bit.is_none()
                && (self.wave.signals[signal].width > 1 || self.wave.signals[signal].kind == "real")
            {
                self.fail(
                    "Select a single-bit signal or expand a bus to navigate rising/falling edges",
                );
                return;
            }
            let changes = &self.wave.traces[trace].changes;
            let index = changes.partition_point(|c| {
                if reverse {
                    c.time < current
                } else {
                    c.time <= current
                }
            });
            let next = if reverse {
                index.checked_sub(1)
            } else {
                (index < changes.len()).then_some(index)
            };
            self.edge_search = Some(EdgeSearch {
                trace,
                bit,
                next,
                reverse,
                alt,
                polarity,
            });
            self.edge_step();
        } else {
            let time = if reverse {
                current.saturating_sub(1).max(self.wave.start)
            } else {
                current.saturating_add(1).min(self.wave.end)
            };
            if alt {
                self.session.alt_cursor = Some(time);
            } else {
                self.session.cursor = time;
                self.reveal_cursor();
            }
        }
    }
    pub fn edge_step(&mut self) {
        let Some(mut search) = self.edge_search.take() else {
            return;
        };
        let changes = &self.wave.traces[search.trace].changes;
        for _ in 0..1024 {
            let Some(i) = search.next else {
                self.status = "No more matching transitions".into();
                return;
            };
            let new = &changes[i].value;
            let previous = i.checked_sub(1).map(|p| &changes[p].value);
            let same = previous.is_some_and(|old| {
                if let Some(bit) = search.bit {
                    old.bit_logic(bit) == new.bit_logic(bit)
                } else {
                    old == new
                }
            });
            let logic = |v: &Value| {
                search
                    .bit
                    .map(|bit| v.bit_logic(bit))
                    .unwrap_or_else(|| v.logic())
            };
            let matches = !same
                && search.polarity.is_none_or(|p| {
                    logic(new) == p
                        && previous.is_some_and(|v| logic(v) == if p == '1' { '0' } else { '1' })
                });
            if matches {
                let time = changes[i].time;
                if search.alt {
                    self.session.alt_cursor = Some(time);
                } else {
                    self.session.cursor = time;
                    self.reveal_cursor();
                }
                self.status = format!(
                    "{} cursor · {}",
                    if search.alt { "B" } else { "A" },
                    self.wave.timescale.format(time)
                );
                return;
            }
            search.next = if search.reverse {
                i.checked_sub(1)
            } else {
                (i + 1 < changes.len()).then_some(i + 1)
            };
        }
        self.status = "Seeking transition…  Esc cancels".into();
        self.edge_search = Some(search);
    }
    pub fn set_cursor(&mut self, column: u16, alt: bool) {
        let mut time = self.time_at(column);
        if self.session.snap
            && let Some((_, trace, bit)) = self.selected()
        {
            let changes = &self.wave.traces[trace].changes;
            let index = changes.partition_point(|c| c.time < time);
            let (start, end) = self.cell_range(column);
            let compressed = self.wave.traces[trace]
                .activity(
                    self.wave.traces[trace].index_at(start),
                    self.wave.traces[trace].index_at(end),
                    bit,
                )
                .compressed();
            let mut nearest = None;
            for i in index.saturating_sub(64)..index.saturating_add(64).min(changes.len()) {
                if compressed {
                    break;
                }
                if let Some(bit) = bit
                    && i > 0
                    && changes[i].value.bit_logic(bit) == changes[i - 1].value.bit_logic(bit)
                {
                    continue;
                }
                let Some(edge_column) = self.column_at(changes[i].time) else {
                    continue;
                };
                if edge_column.abs_diff(column) > 2 {
                    continue;
                }
                let (edge_start, edge_end) = self.cell_range(edge_column);
                if self.wave.traces[trace]
                    .activity(
                        self.wave.traces[trace].index_at(edge_start),
                        self.wave.traces[trace].index_at(edge_end),
                        bit,
                    )
                    .compressed()
                {
                    continue;
                }
                if nearest.is_none_or(|t: u64| changes[i].time.abs_diff(time) < t.abs_diff(time)) {
                    nearest = Some(changes[i].time);
                }
            }
            if let Some(t) = nearest {
                time = t;
            }
        }
        if alt {
            self.session.alt_cursor = Some(time);
        } else {
            self.session.cursor = time;
        }
    }
    pub fn start_search(&mut self, reverse: bool) {
        self.edge_search = None;
        let Some((_, trace, bit)) = self.selected() else {
            self.fail("Select a signal to search");
            return;
        };
        if self.search_query.is_empty() {
            self.open_prompt(Prompt::FindValue);
            return;
        }
        let radix = match self.session.watches[self.selection] {
            Watch::Signal { radix, .. } => radix,
            _ => Radix::Hex,
        };
        let changes = &self.wave.traces[trace].changes;
        let next = if reverse {
            changes
                .partition_point(|c| c.time < self.session.cursor)
                .checked_sub(1)
                .unwrap_or(changes.len().saturating_sub(1))
        } else {
            changes.partition_point(|c| c.time <= self.session.cursor) % changes.len().max(1)
        };
        self.search = Some(ValueSearch {
            trace,
            bit,
            next,
            visited: 0,
            reverse,
            query: self.search_query.trim().to_ascii_lowercase(),
            radix,
        });
        self.status = "Searching values…  Esc cancels".into();
    }
    pub fn search_step(&mut self) {
        if self.edge_search.is_some() {
            self.edge_step();
            return;
        }
        let Some(mut search) = self.search.take() else {
            return;
        };
        let changes = &self.wave.traces[search.trace].changes;
        for _ in 0..1024 {
            if search.visited >= changes.len() {
                self.status = "Value not found".into();
                return;
            }
            let change = &changes[search.next];
            let value = search
                .bit
                .map(|bit| change.value.bit(bit))
                .unwrap_or_else(|| change.value.clone());
            let query = search
                .query
                .strip_prefix("0x")
                .or_else(|| search.query.strip_prefix("0b"))
                .unwrap_or(&search.query);
            let radix = if search.query.starts_with("0x") {
                Radix::Hex
            } else if search.query.starts_with("0b") {
                Radix::Binary
            } else {
                search.radix
            };
            let formatted = value.format(radix).to_ascii_lowercase();
            let normalized = |s: &str| {
                let clean = s.trim_start_matches('0');
                if clean.is_empty() {
                    "0".to_string()
                } else {
                    clean.to_string()
                }
            };
            let wildcard =
                query == "x" && value.logic() == 'x' || query == "z" && value.logic() == 'z';
            if wildcard
                || formatted == query
                || (radix != Radix::Ascii && normalized(&formatted) == normalized(query))
            {
                let time = change.time;
                self.session.cursor = time;
                self.reveal_cursor();
                self.status = format!(
                    "Found {} at {}",
                    value.format(radix),
                    self.wave.timescale.format(time)
                );
                self.error = false;
                return;
            }
            search.visited += 1;
            search.next = if search.reverse {
                search.next.checked_sub(1).unwrap_or(changes.len() - 1)
            } else {
                (search.next + 1) % changes.len()
            };
        }
        self.search = Some(search);
    }
}
