//! Reproducible loading benchmark. Generation and destruction are not timed.
//! cargo run --locked --release --example bench_loading -- target/loading.json
use std::{
    alloc::{GlobalAlloc, Layout, System},
    fmt::Write,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use vtcode_waveform_viewer::{
    loader::{Loader, Source},
    vcd,
};

struct CountAllocations;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOCATOR: CountAllocations = CountAllocations;
unsafe impl GlobalAlloc for CountAllocations {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

struct Bytes<'a> {
    bytes: &'a [u8],
    reads: usize,
}
impl Source for Bytes<'_> {
    fn read(&mut self, offset: u64, count: usize) -> Result<Vec<u8>, String> {
        self.reads += 1;
        let start = offset as usize;
        Ok(self.bytes[start..(start + count).min(self.bytes.len())].to_vec())
    }
    fn read_into(&mut self, offset: u64, out: &mut [u8]) -> Result<usize, String> {
        self.reads += 1;
        let start = offset as usize;
        let count = out.len().min(self.bytes.len() - start);
        out[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }
}

#[derive(serde::Serialize)]
struct ResultRow {
    case: &'static str,
    mode: &'static str,
    bytes: usize,
    changes: usize,
    median_ms: f64,
    mib_per_second: f64,
    allocations: usize,
    source_reads: usize,
}

fn dense_vcd() -> Vec<u8> {
    let mut data = String::from(
        "$timescale 1 ns $end $scope module top $end $var wire 1 c clk $end $var wire 32 b data [31:0] $end $upscope $end $enddefinitions $end\n",
    );
    for time in 0..100_000u32 {
        writeln!(data, "#{time}\n{}c\nb{time:032b} b", time % 2).unwrap();
    }
    data.into_bytes()
}

fn measure(case: &'static str, bytes: &[u8], incremental: bool, expected: usize) -> ResultRow {
    let mut samples = Vec::new();
    for _ in 0..5 {
        let mut source = Bytes { bytes, reads: 0 };
        let before = ALLOCATIONS.load(Ordering::Relaxed);
        let started = Instant::now();
        let wave = if incremental {
            let mut loader = Loader::new(bytes.len() as u64, false).unwrap();
            loop {
                if let Some(wave) = loader.step(&mut source).unwrap() {
                    break wave;
                }
            }
        } else {
            vcd::parse(bytes).unwrap()
        };
        let elapsed = started.elapsed().as_secs_f64();
        let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
        let changes: usize = wave.traces.iter().map(|trace| trace.changes.len()).sum();
        assert_eq!(changes, expected);
        samples.push((elapsed, allocations, source.reads));
        std::hint::black_box(wave);
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (seconds, allocations, source_reads) = samples[2];
    ResultRow {
        case,
        mode: if incremental { "stream" } else { "parser" },
        bytes: bytes.len(),
        changes: expected,
        median_ms: seconds * 1000.0,
        mib_per_second: bytes.len() as f64 / (1024.0 * 1024.0) / seconds,
        allocations,
        source_reads,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dense = dense_vcd();
    let mut comments = b"$comment ".to_vec();
    comments.resize(8 * 1024 * 1024, b' ');
    comments.extend_from_slice(b" $end\n");
    comments.extend_from_slice(include_bytes!("demo.vcd"));
    let mut rows = Vec::new();
    for (case, bytes, changes) in [
        ("dense_bus", dense.as_slice(), 200_000),
        ("comments", comments.as_slice(), 415),
    ] {
        for incremental in [false, true] {
            let row = measure(case, bytes, incremental, changes);
            println!(
                "{} {}: {:.2} ms, {:.1} MiB/s, {} allocations, {} reads",
                row.case,
                row.mode,
                row.median_ms,
                row.mib_per_second,
                row.allocations,
                row.source_reads
            );
            rows.push(row);
        }
    }
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
    }
    Ok(())
}
