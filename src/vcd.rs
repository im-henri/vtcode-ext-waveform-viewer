//! Incremental token parser. Neither line boundaries nor I/O chunk boundaries
//! have semantic meaning in VCD; a token and a directive can span many reads.
use crate::model::{Value, Waveform};
use std::collections::HashMap;

#[derive(Default)]
pub struct VcdParser {
    token: Vec<u8>,
    directive: Option<(String, Vec<String>)>,
    scope: usize,
    ids: HashMap<String, (usize, u32, String)>,
    pending: String,
    time: u64,
    pub header_done: bool,
    pub bytes: u64,
    pub tokens: u64,
}
impl VcdParser {
    pub fn feed(&mut self, data: &[u8], wave: &mut Waveform) -> Result<(), String> {
        let mut rest = data;
        while !rest.is_empty() {
            if self.token.is_empty() {
                let whitespace = rest
                    .iter()
                    .position(|byte| !byte.is_ascii_whitespace())
                    .unwrap_or(rest.len());
                self.bytes += whitespace as u64;
                rest = &rest[whitespace..];
                if rest.is_empty() {
                    break;
                }
            }
            let end = rest.iter().position(u8::is_ascii_whitespace);
            let length = end.unwrap_or(rest.len());
            if self.token.len() + length > 65_536 {
                return Err("VCD token exceeds 64 KiB".into());
            }
            self.bytes += (length + usize::from(end.is_some())) as u64;
            match end {
                Some(_) if self.token.is_empty() => {
                    // Most tokens fit inside a read. Parse directly from the
                    // source buffer; only boundary fragments need copying.
                    self.consume_bytes(&rest[..length], wave)?;
                }
                Some(_) => {
                    self.token.extend_from_slice(&rest[..length]);
                    self.flush(wave)?;
                }
                None => {
                    self.token.extend_from_slice(rest);
                    break;
                }
            }
            rest = &rest[length + 1..];
        }
        Ok(())
    }
    pub fn finish(&mut self, wave: &mut Waveform) -> Result<(), String> {
        if !self.token.is_empty() {
            self.flush(wave)?;
        }
        if self.directive.is_some() {
            return Err("Truncated VCD directive (missing $end)".into());
        }
        if !self.pending.is_empty() {
            return Err("Truncated VCD value change".into());
        }
        if !self.header_done {
            return Err("Not a complete VCD file: missing $enddefinitions $end".into());
        }
        if wave.signals.is_empty() {
            return Err("This VCD contains no signals".into());
        }
        Ok(())
    }
    fn flush(&mut self, wave: &mut Waveform) -> Result<(), String> {
        let bytes = std::mem::take(&mut self.token);
        let result = self.consume_bytes(&bytes, wave);
        self.token = bytes;
        self.token.clear();
        result
    }
    fn consume_bytes(&mut self, bytes: &[u8], wave: &mut Waveform) -> Result<(), String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "VCD contains invalid UTF-8")?;
        let text = if self.tokens == 0 {
            text.trim_start_matches('\u{feff}')
        } else {
            text
        };
        self.tokens += 1;
        self.consume(text, wave)
            .map_err(|e| format!("{e} (near byte {})", self.bytes))
    }
    fn consume(&mut self, token: &str, wave: &mut Waveform) -> Result<(), String> {
        if let Some((name, words)) = self.directive.as_mut() {
            if token == "$end" {
                let (name, words) = self.directive.take().unwrap();
                self.directive(&name, &words, wave)?;
            } else {
                // Comments/date/version can be huge; retain only a short useful preview.
                if matches!(name.as_str(), "$comment" | "$date" | "$version")
                    && (words.len() >= 256 || words.iter().map(String::len).sum::<usize>() > 2048)
                {
                    return Ok(());
                }
                if words.len() >= 1024 {
                    return Err("VCD directive is too long".into());
                }
                words.push(token.into());
            }
            return Ok(());
        }
        if !self.pending.is_empty() {
            let result = self.value(token, &self.pending, wave);
            self.pending.clear();
            return result;
        }
        if token.starts_with('$') {
            match token {
                "$dumpvars" | "$dumpall" | "$dumpon" | "$dumpoff" | "$end" => {}
                _ => self.directive = Some((token.into(), vec![])),
            }
        } else if let Some(time) = token.strip_prefix('#') {
            if !self.header_done {
                return Err("Timestamp before enddefinitions".into());
            }
            let time = time.parse::<u64>().map_err(|_| "Invalid VCD timestamp")?;
            if time < self.time {
                return Err("VCD timestamps are out of order".into());
            }
            self.time = time;
            wave.end = wave.end.max(time);
        } else if matches!(
            token.as_bytes().first(),
            Some(b'b' | b'B' | b'r' | b'R' | b's' | b'S')
        ) {
            // Retain capacity across bus/real/text changes instead of allocating
            // a temporary String for every value/identifier pair.
            self.pending.push_str(token);
        } else if matches!(
            token.as_bytes().first(),
            Some(b'0' | b'1' | b'x' | b'X' | b'z' | b'Z')
        ) {
            if token.len() < 2 {
                return Err("Missing VCD identifier".into());
            }
            self.value(&token[1..], &token[..1], wave)?;
        } else {
            return Err(format!("Unexpected VCD token {token:?}"));
        }
        Ok(())
    }
    fn directive(
        &mut self,
        name: &str,
        words: &[String],
        wave: &mut Waveform,
    ) -> Result<(), String> {
        if self.header_done
            && matches!(
                name,
                "$var" | "$scope" | "$upscope" | "$timescale" | "$enddefinitions"
            )
        {
            return Err("Declaration after enddefinitions".into());
        }
        match name {
            "$scope" => {
                if words.len() != 2 {
                    return Err("Malformed VCD scope".into());
                }
                self.scope = wave.add_scope(self.scope, &words[1])?;
            }
            "$upscope" => {
                self.scope = wave.scopes[self.scope]
                    .parent
                    .ok_or("Unbalanced VCD upscope")?;
            }
            "$timescale" => {
                wave.timescale = crate::model::Timescale::parse(&words.join(" "))?;
            }
            "$date" => wave.date = words.join(" "),
            "$version" => wave.version = words.join(" "),
            "$enddefinitions" => {
                if self.scope != 0 {
                    return Err("Unclosed VCD scope".into());
                }
                self.header_done = true;
            }
            "$var" => {
                if words.len() < 4 {
                    return Err("Malformed VCD variable".into());
                }
                let width: u32 = words[1].parse().map_err(|_| "Invalid VCD signal width")?;
                let kind = words[0].clone();
                let id = &words[2];
                if id.len() > 1024 {
                    return Err("VCD identifier exceeds 1 KiB".into());
                }
                let trace = if let Some((trace, old_width, old_kind)) = self.ids.get(id) {
                    if *old_width != width || ((*old_kind == "real") != (kind == "real")) {
                        return Err("VCD alias has an incompatible type".into());
                    }
                    *trace
                } else {
                    wave.account_metadata(id.len() + kind.len() + 96)?;
                    wave.traces.len()
                };
                let (name, range) = signal_name(&words[3..], width);
                wave.add_signal(self.scope, name, width, kind.clone(), trace, range)?;
                self.ids.insert(id.clone(), (trace, width, kind));
            }
            _ => {} // Vendor-specific declaration directives are intentionally ignored.
        }
        Ok(())
    }
    fn value(&self, id: &str, raw: &str, wave: &mut Waveform) -> Result<(), String> {
        if !self.header_done {
            return Err("Value change before enddefinitions".into());
        }
        let (trace, width, kind) = self
            .ids
            .get(id)
            .ok_or_else(|| format!("Unknown VCD identifier {id:?}"))?;
        let value = match raw.as_bytes()[0] {
            b'r' | b'R' => {
                if kind != "real" && kind != "realtime" {
                    return Err("Real value for a digital signal".into());
                }
                Value::Real(raw[1..].parse().map_err(|_| "Invalid real value")?)
            }
            b's' | b'S' => Value::Text(raw[1..].into()),
            b'b' | b'B' => Value::bits(&raw[1..], *width as usize)?,
            _ => Value::bits(raw, *width as usize)?,
        };
        wave.push(*trace, self.time, value)
    }
}

pub fn signal_name(words: &[String], width: u32) -> (String, Option<(i32, i32)>) {
    let joined = words.join("");
    // Escaped identifiers can contain brackets as part of the name. A separate
    // range token is unambiguous; otherwise only a trailing numeric range counts.
    if let Some(open) = joined.rfind('[')
        && joined.ends_with(']')
        && (!words[0].starts_with('\\') || words.len() > 1)
    {
        let range = &joined[open + 1..joined.len() - 1];
        let mut parts = range.split(':');
        if let Some(msb) = parts.next().and_then(|s| s.parse::<i32>().ok()) {
            let lsb = parts
                .next()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(msb);
            if parts.next().is_none() && msb.abs_diff(lsb).saturating_add(1) == width {
                return (joined[..open].into(), Some((msb, lsb)));
            }
        }
    }
    (joined, None)
}

pub fn parse(data: &[u8]) -> Result<Waveform, String> {
    let mut wave = Waveform::default();
    let mut parser = VcdParser::default();
    parser.feed(data, &mut wave)?;
    parser.finish(&mut wave)?;
    Ok(wave)
}
