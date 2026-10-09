//! Pure Rust FST decoding; no simulator installation or native DLL is needed.
use crate::model::{MAX_SIGNALS, Timescale, Value, Waveform};
use fst_reader::{FstFilter, FstHierarchyEntry, FstReader, FstSignalValue, ReadSignalsError};
use std::io::Cursor;

pub const MAX_FST_BYTES: u64 = 8 * 1024 * 1024;

pub fn parse(bytes: Vec<u8>) -> Result<Waveform, String> {
    parse_with_budget(bytes, crate::model::MAX_DATA_BYTES)
}

pub fn parse_with_budget(bytes: Vec<u8>, budget: usize) -> Result<Waveform, String> {
    if bytes.len() as u64 > MAX_FST_BYTES {
        return Err("FST files are currently limited to 8 MiB; convert larger dumps to VCD".into());
    }
    let bytes = unwrap_gzip(bytes)?;
    preflight(&bytes)?;
    let mut reader =
        FstReader::open(Cursor::new(bytes)).map_err(|e| format!("Cannot open FST: {e}"))?;
    let header = reader.get_header();
    if header.var_count > MAX_SIGNALS as u64 || header.max_handle > MAX_SIGNALS as u64 {
        return Err(format!("FST exceeds {MAX_SIGNALS} signals"));
    }
    if !(-15..=0).contains(&header.timescale_exponent) {
        return Err("Unsupported FST timescale".into());
    }
    let exponent = i32::from(header.timescale_exponent);
    let base = exponent.div_euclid(3) * 3;
    let mut wave = Waveform {
        format: "FST",
        data_limit: budget.min(crate::model::MAX_DATA_BYTES),
        start: header.start_time,
        end: header.end_time,
        date: header.date,
        version: header.version,
        timescale: Timescale {
            factor: 10u32.pow((exponent - base) as u32),
            exponent: base,
        },
        ..Waveform::default()
    };
    let mut scope = 0;
    let mut error = None;
    let mut widths = vec![0; header.max_handle as usize];
    let mut strings = vec![false; header.max_handle as usize];
    reader
        .read_hierarchy(|entry| {
            if error.is_some() {
                return;
            }
            let result = match entry {
                FstHierarchyEntry::Scope { name, .. } => {
                    wave.add_scope(scope, &name).map(|id| scope = id)
                }
                FstHierarchyEntry::UpScope => wave.scopes[scope]
                    .parent
                    .ok_or_else(|| "Unbalanced FST scope".to_string())
                    .map(|id| scope = id),
                FstHierarchyEntry::Var {
                    name,
                    length,
                    handle,
                    tpe,
                    ..
                } => {
                    let index = handle.get_index();
                    if index >= widths.len() {
                        error = Some("Invalid FST handle".into());
                        return;
                    }
                    let width = length.max(1);
                    widths[index] = width;
                    strings[index] = tpe == fst_reader::FstVarType::GenericString;
                    let (name, range) = crate::vcd::signal_name(&[name], width);
                    let kind = if tpe.is_real() {
                        "real".into()
                    } else {
                        format!("{tpe:?}").to_ascii_lowercase()
                    };
                    wave.add_signal(scope, name, width, kind, index, range)
                        .map(|_| ())
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                error = Some(e);
            }
        })
        .map_err(|e| format!("Cannot read FST hierarchy: {e}"))?;
    if let Some(error) = error {
        return Err(error);
    }
    if scope != 0 {
        return Err("Unclosed FST scope".into());
    }
    if wave.signals.is_empty() {
        return Err("FST contains no signals".into());
    }
    reader
        .read_signals(
            &FstFilter::all(),
            |time, handle, value| -> Result<(), String> {
                let index = handle.get_index();
                let width = *widths.get(index).ok_or("Invalid FST handle")?;
                let value = match value {
                    FstSignalValue::Real(n) => Value::Real(n),
                    FstSignalValue::String(bytes) if strings[index] => {
                        Value::Text(String::from_utf8_lossy(bytes).into_owned())
                    }
                    FstSignalValue::String(bytes) => Value::bits(
                        std::str::from_utf8(bytes).map_err(|_| "Invalid FST digital value")?,
                        width as usize,
                    )?,
                };
                wave.push(index, time, value)
            },
        )
        .map_err(|e| match e {
            ReadSignalsError::CallbackError(e) => e,
            ReadSignalsError::ReadError(e) => format!("Cannot read FST values: {e}"),
        })?;
    Ok(wave)
}

fn be64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_be_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or("Truncated FST block")?
            .try_into()
            .map_err(|_| "Truncated FST block")?,
    ))
}
fn varint(bytes: &[u8], offset: &mut usize) -> Result<u64, String> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*offset).ok_or("Truncated FST integer")?;
        *offset += 1;
        if shift == 63 && byte > 1 {
            return Err("FST integer overflow".into());
        }
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err("Invalid FST integer".into())
}
fn bounded(value: u64, maximum: u64, what: &str) -> Result<(), String> {
    if value > maximum {
        Err(format!(
            "FST {what} exceeds the viewer's decoding limit; convert this dump to VCD"
        ))
    } else {
        Ok(())
    }
}
/// Check allocation sizes and section boundaries before calling a decoder
/// intended for trusted simulator output. This also turns common truncation and
/// malformed-header cases into normal errors rather than Wasm traps.
pub fn preflight(bytes: &[u8]) -> Result<(), String> {
    const BLOCK_LIMIT: u64 = 16 * 1024 * 1024;
    let mut offset = 0usize;
    let mut header = false;
    let mut hierarchy = false;
    let mut geometry = false;
    while offset < bytes.len() {
        let kind = bytes[offset];
        let length = be64(bytes, offset + 1)?;
        if kind == 255 && length == 0 {
            break;
        }
        if length < 8 {
            return Err("Invalid FST block length".into());
        }
        let end = offset
            .checked_add(1)
            .and_then(|n| n.checked_add(usize::try_from(length).ok()?))
            .ok_or("FST block length overflow")?;
        if end > bytes.len() {
            return Err("Truncated FST block".into());
        }
        let section = &bytes[offset + 1..end];
        match kind {
            0 => {
                if header || length != 329 {
                    return Err("Invalid or duplicate FST header".into());
                }
                if be64(section, 8)? > be64(section, 16)? {
                    return Err("FST end time precedes its start".into());
                }
                let float = section.get(24..32).ok_or("Truncated FST header")?;
                let needle = std::f64::consts::E;
                // The format's floating-point endian sentinel is Euler's constant.
                if float != needle.to_le_bytes() && float != needle.to_be_bytes() {
                    return Err("Invalid FST floating-point endian marker".into());
                }
                bounded(be64(section, 40)?, MAX_SIGNALS as u64, "scope count")?;
                bounded(be64(section, 48)?, MAX_SIGNALS as u64, "signal count")?;
                bounded(be64(section, 56)?, MAX_SIGNALS as u64, "handle count")?;
                header = true;
            }
            3 => {
                if geometry || length < 24 {
                    return Err("Invalid or duplicate FST geometry".into());
                }
                bounded(be64(section, 8)?, BLOCK_LIMIT, "geometry block")?;
                bounded(
                    be64(section, 16)?,
                    MAX_SIGNALS as u64,
                    "geometry handle count",
                )?;
                geometry = true;
            }
            4 | 6 | 7 => {
                if hierarchy || length < 16 {
                    return Err("Invalid or duplicate FST hierarchy".into());
                }
                bounded(be64(section, 8)?, BLOCK_LIMIT, "hierarchy block")?;
                hierarchy = true;
            }
            1 | 5 | 8 => {
                if length < 56 {
                    return Err("Invalid FST value-change block".into());
                }
                bounded(
                    be64(section, 24)?,
                    BLOCK_LIMIT,
                    "value-change working memory",
                )?;
                let footer = section.len() - 24;
                bounded(be64(section, footer)?, BLOCK_LIMIT, "time-table block")?;
                bounded(be64(section, footer + 16)?, 1_000_000, "time-table entries")?;
                if be64(section, footer + 8)? > length - 56 {
                    return Err("Invalid FST time-table length".into());
                }
                let mut frame = 32;
                bounded(
                    varint(section, &mut frame)?,
                    BLOCK_LIMIT,
                    "initial-value frame",
                )?;
                if varint(section, &mut frame)? > length {
                    return Err("Invalid FST initial-value length".into());
                }
                bounded(
                    varint(section, &mut frame)?,
                    MAX_SIGNALS as u64,
                    "frame handles",
                )?;
            }
            2 => {
                let mut at = 8;
                bounded(varint(section, &mut at)?, 1_000_000, "blackout entries")?;
            }
            255 => {}
            _ => return Err("Unknown FST block type".into()),
        }
        offset = end;
    }
    if !header || !geometry || !hierarchy {
        return Err("Incomplete FST: header, geometry or hierarchy is missing".into());
    }
    Ok(())
}
fn unwrap_gzip(bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    if bytes.first() != Some(&254) {
        return Ok(bytes);
    }
    let section = be64(&bytes, 1)?;
    let expected = be64(&bytes, 9)?;
    bounded(expected, MAX_FST_BYTES, "unwrapped file size")?;
    if section == 0 {
        return Err("FST compression has not finished".into());
    }
    let gzip = bytes.get(17..).ok_or("Truncated compressed FST")?;
    if gzip.len() < 18 || gzip[..3] != [0x1f, 0x8b, 8] || gzip[3] & 0xe0 != 0 {
        return Err("Invalid FST gzip wrapper".into());
    }
    let flags = gzip[3];
    let mut offset = 10;
    if flags & 4 != 0 {
        let extra: [u8; 2] = gzip
            .get(offset..offset + 2)
            .ok_or("Truncated gzip header")?
            .try_into()
            .map_err(|_| "Truncated gzip header")?;
        offset += 2 + usize::from(u16::from_le_bytes(extra));
    }
    for flag in [8, 16] {
        if flags & flag != 0 {
            let remaining = gzip.get(offset..).ok_or("Truncated gzip header")?;
            offset += remaining
                .iter()
                .position(|b| *b == 0)
                .ok_or("Unterminated gzip header")?
                + 1;
        }
    }
    if flags & 2 != 0 {
        offset += 2;
    }
    let footer = gzip.len() - 8;
    let compressed = gzip.get(offset..footer).ok_or("Truncated gzip data")?;
    let data =
        miniz_oxide::inflate::decompress_to_vec_with_limit(compressed, MAX_FST_BYTES as usize)
            .map_err(|_| "Invalid or oversized compressed FST")?;
    let crc = u32::from_le_bytes(
        gzip[footer..footer + 4]
            .try_into()
            .map_err(|_| "Invalid gzip trailer")?,
    );
    let size = u32::from_le_bytes(
        gzip[footer + 4..]
            .try_into()
            .map_err(|_| "Invalid gzip trailer")?,
    );
    if data.len() as u64 != expected || data.len() != size as usize || crc32fast::hash(&data) != crc
    {
        return Err("FST gzip checksum/size mismatch".into());
    }
    Ok(data)
}
