use serde::{Deserialize, Serialize};

pub const MAX_SIGNALS: usize = 40_000;
pub const MAX_WIDTH: usize = 16_384;
pub const MAX_DATA_BYTES: usize = 48 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bits(String),
    Real(f64),
    Text(String),
}

impl Value {
    pub fn bits(raw: &str, width: usize) -> Result<Self, String> {
        if width == 0 || width > MAX_WIDTH || raw.is_empty() || raw.len() > width {
            return Err(format!(
                "Invalid {width}-bit value (maximum width is {MAX_WIDTH})"
            ));
        }
        let mut bits = raw.to_ascii_lowercase();
        if !bits.bytes().all(|b| matches!(b, b'0' | b'1' | b'x' | b'z')) {
            return Err("Invalid four-state value".into());
        }
        // IEEE VCD: abbreviated X/Z vectors extend with X/Z; numeric vectors zero extend.
        if bits.len() < width {
            let fill = match bits.as_bytes()[0] {
                b'x' => 'x',
                b'z' => 'z',
                _ => '0',
            };
            bits = format!("{}{}", fill.to_string().repeat(width - bits.len()), bits);
        }
        Ok(Self::Bits(bits))
    }

    pub fn bit(&self, index: u32) -> Self {
        Self::Bits(self.bit_logic(index).to_string())
    }
    pub fn bit_logic(&self, index: u32) -> char {
        match self {
            Self::Bits(bits) => bits
                .as_bytes()
                .get(bits.len().wrapping_sub(index as usize + 1))
                .copied()
                .map(char::from)
                .unwrap_or('x'),
            _ => 'x',
        }
    }

    pub fn size(&self) -> usize {
        64 + match self {
            Self::Bits(s) | Self::Text(s) => s.len(),
            Self::Real(_) => 0,
        }
    }

    pub fn number(&self, signed: bool) -> Option<f64> {
        match self {
            Self::Real(n) => n.is_finite().then_some(*n),
            Self::Bits(s) if s.len() <= 128 && s.bytes().all(|b| b == b'0' || b == b'1') => {
                let n = u128::from_str_radix(s, 2).ok()?;
                if signed && s.starts_with('1') {
                    Some(sign_extend(n, s.len()) as f64)
                } else {
                    Some(n as f64)
                }
            }
            _ => None,
        }
    }

    pub fn format(&self, radix: Radix) -> String {
        match self {
            Self::Real(n) if *n != 0.0 && (n.abs() < 0.0001 || n.abs() >= 1e9) => {
                format!("{n:.5e}")
            }
            Self::Real(n) => format!("{n:.5}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string(),
            Self::Text(s) => s.clone(),
            Self::Bits(bits) => match radix {
                Radix::Binary => bits.clone(),
                Radix::Hex | Radix::Octal => {
                    let group = if radix == Radix::Hex { 4 } else { 3 };
                    let mut out = Vec::new();
                    for digits in bits.as_bytes().rchunks(group) {
                        let character = if digits.contains(&b'x') {
                            'x'
                        } else if digits.contains(&b'z') {
                            'z'
                        } else {
                            let n = digits
                                .iter()
                                .fold(0u32, |n, digit| (n << 1) | u32::from(*digit == b'1'));
                            char::from_digit(n, 1 << group).unwrap_or('?')
                        };
                        out.push(character);
                    }
                    out.into_iter().rev().collect()
                }
                Radix::Unsigned | Radix::Signed => {
                    if bits.contains('x') {
                        return "X".into();
                    }
                    if bits.contains('z') {
                        return "Z".into();
                    }
                    if bits.len() > 128 {
                        return format!("0x{}", self.format(Radix::Hex));
                    }
                    let n = u128::from_str_radix(bits, 2).unwrap_or_default();
                    if radix == Radix::Signed && bits.starts_with('1') {
                        sign_extend(n, bits.len()).to_string()
                    } else {
                        n.to_string()
                    }
                }
                Radix::Ascii => {
                    let mut bytes = Vec::new();
                    for digits in bits.as_bytes().rchunks(8) {
                        bytes.push(if digits.iter().any(|b| *b == b'x' || *b == b'z') {
                            '?'
                        } else {
                            let n = digits
                                .iter()
                                .fold(0u8, |n, b| (n << 1) | u8::from(*b == b'1'));
                            if n.is_ascii_graphic() || n == b' ' {
                                char::from(n)
                            } else {
                                '·'
                            }
                        });
                    }
                    bytes.into_iter().rev().collect()
                }
            },
        }
    }

    pub fn logic(&self) -> char {
        match self {
            Self::Bits(s) if s.contains('x') => 'x',
            Self::Bits(s) if s.contains('z') => 'z',
            Self::Bits(s) if s == "0" => '0',
            Self::Bits(s) if s == "1" => '1',
            _ => 'b',
        }
    }
}

fn sign_extend(number: u128, width: usize) -> i128 {
    let extended = if width == 128 {
        number
    } else {
        number | (u128::MAX << width)
    };
    extended as i128
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Radix {
    Binary,
    #[default]
    Hex,
    Octal,
    Unsigned,
    Signed,
    Ascii,
}
impl Radix {
    pub fn next(self) -> Self {
        match self {
            Self::Hex => Self::Unsigned,
            Self::Unsigned => Self::Signed,
            Self::Signed => Self::Binary,
            Self::Binary => Self::Octal,
            Self::Octal => Self::Ascii,
            Self::Ascii => Self::Hex,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Binary => "bin",
            Self::Hex => "hex",
            Self::Octal => "oct",
            Self::Unsigned => "dec",
            Self::Signed => "signed",
            Self::Ascii => "ascii",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Change {
    pub time: u64,
    pub value: Value,
}
#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub changes: Vec<Change>,
}
impl Trace {
    pub fn index_at(&self, time: u64) -> usize {
        self.changes.partition_point(|change| change.time <= time)
    }
    pub fn value_at(&self, time: u64) -> Option<&Value> {
        self.index_at(time)
            .checked_sub(1)
            .and_then(|i| self.changes.get(i))
            .map(|c| &c.value)
    }
}

#[derive(Clone, Debug)]
pub struct Signal {
    pub name: String,
    pub path: String,
    pub scope: usize,
    pub width: u32,
    pub kind: String,
    pub trace: usize,
    pub msb: i32,
    pub lsb: i32,
}
impl Signal {
    pub fn bit_name(&self, offset: u32) -> i32 {
        if self.msb >= self.lsb {
            self.lsb.saturating_add(offset as i32)
        } else {
            self.lsb.saturating_sub(offset as i32)
        }
    }
}
#[derive(Clone, Debug)]
pub struct Scope {
    pub name: String,
    pub path: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub signals: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Waveform {
    pub scopes: Vec<Scope>,
    pub signals: Vec<Signal>,
    pub traces: Vec<Trace>,
    pub timescale: Timescale,
    pub start: u64,
    pub end: u64,
    pub date: String,
    pub version: String,
    pub format: &'static str,
    pub data_bytes: usize,
    /// Remaining shared allowance, supplied by the loader before decoding.
    pub data_limit: usize,
    pub change_count: usize,
}
impl Default for Waveform {
    fn default() -> Self {
        Self {
            scopes: vec![Scope {
                name: "Design".into(),
                path: String::new(),
                parent: None,
                children: vec![],
                signals: vec![],
            }],
            signals: vec![],
            traces: vec![],
            timescale: Timescale::default(),
            start: 0,
            end: 0,
            date: String::new(),
            version: String::new(),
            format: "VCD",
            data_bytes: 0,
            data_limit: MAX_DATA_BYTES,
            change_count: 0,
        }
    }
}
impl Waveform {
    pub fn account_metadata(&mut self, bytes: usize) -> Result<(), String> {
        self.data_bytes = self.data_bytes.saturating_add(bytes);
        if self.data_bytes > self.data_limit.min(MAX_DATA_BYTES) {
            Err(memory_error(self.data_limit))
        } else {
            Ok(())
        }
    }
    pub fn add_scope(&mut self, parent: usize, name: &str) -> Result<usize, String> {
        if self.scopes.len() >= MAX_SIGNALS || name.len() > 4096 {
            return Err("Hierarchy exceeds the viewer's limit".into());
        }
        let path = if self.scopes[parent].path.is_empty() {
            name.into()
        } else {
            format!("{}.{}", self.scopes[parent].path, name)
        };
        if path.len() > 8192 {
            return Err("Hierarchy is too deeply nested".into());
        }
        self.account_metadata(path.len() + name.len() + 128)?;
        let id = self.scopes.len();
        self.scopes.push(Scope {
            name: name.into(),
            path,
            parent: Some(parent),
            children: vec![],
            signals: vec![],
        });
        self.scopes[parent].children.push(id);
        Ok(id)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn add_signal(
        &mut self,
        scope: usize,
        name: String,
        width: u32,
        kind: String,
        trace: usize,
        range: Option<(i32, i32)>,
    ) -> Result<usize, String> {
        if self.signals.len() >= MAX_SIGNALS
            || width == 0
            || width as usize > MAX_WIDTH
            || name.len() > 4096
        {
            return Err(format!(
                "Signal limit exceeded (max {MAX_SIGNALS} signals, {MAX_WIDTH} bits)"
            ));
        }
        if trace >= MAX_SIGNALS {
            return Err("Too many unique signals".into());
        }
        while self.traces.len() <= trace {
            self.traces.push(Trace::default());
        }
        let path_name = match range {
            Some((_, lsb)) if width == 1 => format!("{name}[{lsb}]"),
            Some((msb, lsb)) => format!("{name}[{msb}:{lsb}]"),
            None => name.clone(),
        };
        let path = if self.scopes[scope].path.is_empty() {
            path_name
        } else {
            format!("{}.{}", self.scopes[scope].path, path_name)
        };
        self.account_metadata(path.len() + name.len() + kind.len() + 128)?;
        let (msb, lsb) = range.unwrap_or((width as i32 - 1, 0));
        let id = self.signals.len();
        self.signals.push(Signal {
            name,
            path,
            scope,
            width,
            kind,
            trace,
            msb,
            lsb,
        });
        self.scopes[scope].signals.push(id);
        Ok(id)
    }
    pub fn push(&mut self, trace: usize, time: u64, value: Value) -> Result<(), String> {
        let limit = self.data_limit.min(MAX_DATA_BYTES);
        let changes = &mut self
            .traces
            .get_mut(trace)
            .ok_or("Unknown signal identifier")?
            .changes;
        if let Some(last) = changes.last_mut() {
            if time < last.time {
                return Err("Waveform timestamps are out of order".into());
            }
            if last.value == value {
                return Ok(());
            }
            if time == last.time {
                self.data_bytes = self.data_bytes.saturating_sub(last.value.size()) + value.size();
                if self.data_bytes > limit {
                    return Err(memory_error(limit));
                }
                last.value = value;
                return Ok(());
            }
        }
        if self.data_bytes.saturating_add(value.size()) > limit {
            return Err(memory_error(limit));
        }
        self.data_bytes += value.size();
        changes.push(Change { time, value });
        self.change_count += 1;
        self.end = self.end.max(time);
        Ok(())
    }
}

fn memory_error(limit: usize) -> String {
    if limit < MAX_DATA_BYTES {
        "Open waveforms exceed the shared memory budget; close another waveform and press F5 to retry".into()
    } else {
        "Waveform data or metadata exceeds 48 MiB; reduce the dump's scope or duration".into()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Timescale {
    pub factor: u32,
    pub exponent: i32,
}
impl Default for Timescale {
    fn default() -> Self {
        Self {
            factor: 1,
            exponent: -9,
        }
    }
}
impl Timescale {
    pub fn parse(text: &str) -> Result<Self, String> {
        let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let digits = clean
            .find(|c: char| !c.is_ascii_digit())
            .ok_or("Timescale needs a unit")?;
        let factor = clean[..digits]
            .parse::<u32>()
            .map_err(|_| "Invalid timescale")?;
        if !matches!(factor, 1 | 10 | 100) {
            return Err("Timescale multiplier must be 1, 10 or 100".into());
        }
        let exponent = unit_exponent(&clean[digits..]).ok_or("Unknown timescale unit")?;
        Ok(Self { factor, exponent })
    }
    pub fn label(self) -> String {
        format!("{} {}", self.factor, unit_name(self.exponent))
    }
    pub fn format(self, ticks: u64) -> String {
        // Integer arithmetic keeps cursor labels exact beyond f64's 53-bit precision.
        let value = u128::from(ticks) * u128::from(self.factor);
        let mut exponent = self.exponent;
        let mut divisor = 1u128;
        while exponent < 0 && value >= divisor * 1000 {
            divisor *= 1000;
            exponent += 3;
        }
        if divisor == 1 {
            format!("{value} {}", unit_name(exponent))
        } else {
            let whole = value / divisor;
            let remainder = value % divisor;
            if remainder == 0 {
                format!("{whole} {}", unit_name(exponent))
            } else {
                let places = divisor.ilog10() as usize;
                let decimal = format!("{remainder:0places$}");
                format!(
                    "{whole}.{} {}",
                    decimal.trim_end_matches('0'),
                    unit_name(exponent)
                )
            }
        }
    }
    pub fn parse_time(self, input: &str) -> Result<u64, String> {
        let clean = input.trim().replace(['_', ','], "");
        let split = clean
            .find(|c: char| c.is_alphabetic())
            .unwrap_or(clean.len());
        let number = clean[..split].trim();
        let unit = clean[split..].trim();
        if unit.is_empty() {
            return number
                .parse()
                .map_err(|_| "Enter nonnegative ticks or a time such as 125 ns".into());
        }
        let exponent = unit_exponent(unit).ok_or("Use s, ms, us, ns, ps or fs")?;
        let mut parts = number.split('.');
        let whole: u128 = parts
            .next()
            .unwrap_or("")
            .parse()
            .map_err(|_| "Invalid time")?;
        let fractional = parts.next().unwrap_or("");
        if parts.next().is_some()
            || fractional.len() > 18
            || !fractional.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("Invalid fractional time".into());
        }
        let denominator = 10u128.pow(fractional.len() as u32);
        let numerator = whole
            .checked_mul(denominator)
            .and_then(|n| n.checked_add(fractional.parse::<u128>().unwrap_or(0)))
            .ok_or("Time overflow")?;
        let shift = exponent - self.exponent;
        let (top, bottom) = if shift >= 0 {
            (
                numerator
                    .checked_mul(10u128.pow(shift as u32))
                    .ok_or("Time overflow")?,
                denominator * u128::from(self.factor),
            )
        } else {
            (
                numerator,
                denominator
                    .checked_mul(10u128.pow((-shift) as u32))
                    .and_then(|n| n.checked_mul(u128::from(self.factor)))
                    .ok_or("Time overflow")?,
            )
        };
        if top % bottom != 0 {
            return Err("Time is finer than the dump's resolution".into());
        }
        u64::try_from(top / bottom).map_err(|_| "Time overflow".into())
    }
}
pub fn unit_exponent(unit: &str) -> Option<i32> {
    match unit {
        "s" => Some(0),
        "ms" => Some(-3),
        "us" | "µs" => Some(-6),
        "ns" => Some(-9),
        "ps" => Some(-12),
        "fs" => Some(-15),
        _ => None,
    }
}
pub fn unit_name(exp: i32) -> &'static str {
    match exp {
        0 => "s",
        -3 => "ms",
        -6 => "us",
        -9 => "ns",
        -12 => "ps",
        -15 => "fs",
        _ => "ticks",
    }
}
