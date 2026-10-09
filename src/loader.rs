use crate::{fst, model::Waveform, vcd::VcdParser};

pub trait Source {
    fn read(&mut self, offset: u64, count: usize) -> Result<Vec<u8>, String>;

    /// Fill reusable storage. Sources can override this to avoid allocating and
    /// copying a temporary Vec for every file/host read.
    fn read_into(&mut self, offset: u64, out: &mut [u8]) -> Result<usize, String> {
        let bytes = self.read(offset, out.len())?;
        if bytes.len() > out.len() {
            return Err("The waveform source returned too many bytes".into());
        }
        out[..bytes.len()].copy_from_slice(&bytes);
        Ok(bytes.len())
    }
}

pub const PARSE_CHUNK: usize = 16 * 1024;
pub const READ_AHEAD: usize = 128 * 1024;
pub struct Loader {
    pub size: u64,
    pub offset: u64,
    pub wave: Waveform,
    parser: VcdParser,
    fst: bool,
    bytes: Vec<u8>,
    buffer: Vec<u8>,
    buffered: usize,
    position: usize,
    pub done: bool,
}
impl Loader {
    pub fn new(size: u64, fst: bool) -> Result<Self, String> {
        if size == 0 {
            return Err("The waveform file is empty".into());
        }
        if fst && size > fst::MAX_FST_BYTES {
            return Err(
                "FST files are currently limited to 8 MiB; convert larger dumps to VCD".into(),
            );
        }
        Ok(Self {
            size,
            offset: 0,
            wave: Waveform::default(),
            parser: VcdParser::default(),
            fst,
            bytes: vec![],
            buffer: vec![],
            buffered: 0,
            position: 0,
            done: false,
        })
    }
    pub fn step(&mut self, source: &mut impl Source) -> Result<Option<Waveform>, String> {
        if self.done {
            return Ok(None);
        }
        if self.offset < self.size {
            if self.position == self.buffered {
                let count = (self.size - self.offset).min(READ_AHEAD as u64) as usize;
                self.buffer.resize(count, 0);
                self.buffered = source.read_into(self.offset, &mut self.buffer)?;
                self.position = 0;
                if self.buffered == 0 || self.buffered > count {
                    return Err("The waveform changed or could not be read; reopen the file".into());
                }
            }
            let count = (self.buffered - self.position).min(PARSE_CHUNK);
            let bytes = &self.buffer[self.position..self.position + count];
            self.position += count;
            self.offset += count as u64;
            if self.fst {
                self.bytes.extend_from_slice(bytes);
            } else {
                self.parser.feed(bytes, &mut self.wave)?;
            }
            return Ok(None);
        }
        self.done = true;
        if self.fst {
            Ok(Some(fst::parse_with_budget(
                std::mem::take(&mut self.bytes),
                self.wave.data_limit,
            )?))
        } else {
            self.parser.finish(&mut self.wave)?;
            Ok(Some(std::mem::take(&mut self.wave)))
        }
    }

    /// FST decompression gets a fresh worker turn and fuel budget after I/O.
    pub fn needs_decode_pass(&self) -> bool {
        self.fst && self.offset == self.size && !self.done
    }
}
