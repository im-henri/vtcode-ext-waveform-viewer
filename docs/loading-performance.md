# Loading measurements

Measured locally on Windows on 2026-10-09. These are reproducible development
benchmarks, not guarantees for every dump or machine.

## VTCode integration

Median of three loads using release Wasm inside a debug VTCode host, the
production idle scheduler, real file-backed document reads, and workbench
drawing into a 180 × 48 terminal test backend. File generation and runtime
initialization are excluded; opening, worker execution, loading, and drawing
the completed surface are included. The previous and optimized guests were
measured using the same updated host.

| Input | Previous guest | Optimized guest | Speedup |
| --- | ---: | ---: | ---: |
| Demo, 7,810 bytes / 415 changes | 40.37 ms | 46.07 ms | Small-file overhead dominates |
| Dense bus, 4,589,024 bytes / 200,000 changes | 1,048.51 ms | 538.19 ms | 1.95× |
| Metadata-heavy, 8,396,424 bytes / 415 changes | 1,833.76 ms | 247.46 ms | 7.41× |

Run in a VTCode checkout, after refreshing the compiled fixture:

```powershell
cargo test --locked --bin vtcode waveform_wasm_loading_benchmark -- --ignored --nocapture --test-threads=1
```

Regular integration tests also exercise idle loading, large-terminal loading
under continuous mouse input, cancellation, FST decoding, and the final value
of the dense bus. The tests check correctness rather than machine-specific
throughput thresholds.

## Native parser and streaming loader

Median of five release runs, with file generation and waveform destruction
outside the timer. Allocation counting is enabled in both builds. The streaming
source is in memory so these measurements isolate loader/parser costs.

| Input, streaming mode | Previous | Optimized |
| --- | ---: | ---: |
| Dense bus: elapsed | 23.26 ms | 18.25 ms |
| Dense bus: allocations/reallocations | 300,370 | 200,089 |
| Dense bus: source reads | 281 | 36 |
| Metadata-heavy: elapsed | 4.80 ms | 3.16 ms |
| Metadata-heavy: allocations/reallocations | 1,479 | 707 |
| Metadata-heavy: source reads | 513 | 65 |

Run from the extension directory:

```powershell
cargo run --locked --release --example bench_loading -- target/loading.json
```

## Changes behind the measurements

- Reuse a 128 KiB read-ahead buffer and parse it in bounded 16 KiB steps.
- Fill guest buffers directly through `read_document_into`, eliminating the
  temporary host read buffer and payload copy. Sequential file reads avoid
  redundant seeks while retaining file-change checks.
- Borrow complete VCD tokens from the input buffer and reuse storage for split
  tokens and pending bus/real/text values.
- Adapt each loading callback to the host's 6 ms work slice and fuel reserve,
  with an additional 1 MiB ceiling. Cheap input progresses faster; expensive
  parsing yields between small steps.
- Report progress when crossing a byte interval, including partial batches.
  Give the final FST decode a fresh worker turn after buffered I/O finishes.

This remains a bounded in-memory viewer. The existing waveform data budgets
and FST limits still apply; this change does not introduce a disk index or
lazy decoding of selected signals.
