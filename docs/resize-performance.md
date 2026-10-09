# Split resizing measurements

Measured locally on Windows on 2026-10-09. Timings are development benchmarks,
not guarantees for every dump, terminal, or machine.

## VTCode integration

Two different VCD files are open in stacked editor groups. The test sends 48
splitter positions, draws the full workbench at each position, and drives the
production background scheduler. Release Wasm runs inside a debug VTCode host.
The terminal test backend includes UI drawing and terminal buffer comparison;
actual terminal output and simulator file generation are excluded.

| Terminal | Previous median draw | Updated median draw | Previous p95 | Updated p95 |
| --- | ---: | ---: | ---: | ---: |
| 180 × 60 | 6.61 ms | 1.23 ms | 7.99 ms | 2.81 ms |
| 512 × 160 | 52.35 ms | 7.53 ms | 55.97 ms | 9.16 ms |

After release, both panes reached their final dimensions in 11.14 ms and
57.39 ms respectively, compared with 77.69 ms and 244.54 ms before the changes.
The updated debug/test profiles optimize Ratatui's core/widgets and Unicode
dependencies. With the original dependency profiles, the new surface renderer
alone reduced the large-terminal draw to 28.70 ms. Release builds already
optimize these dependencies and benefit from the rendering and scheduling changes.

The 48-position timing test deliberately draws every sample to compare frame
cost. Separately, a regression test queues 241 drag samples and verifies that
VTCode applies the latest position in one update while preserving mouse release.
Further tests check modifier/key/wheel boundaries, continuous updates to both
panes, correct final geometry, and resizing 256 fully expanded 64-bit buses within
the UI callback's fuel budget.

Run from a VTCode checkout after refreshing the compiled fixture:

```powershell
cargo test --locked --bin vtcode waveform_wasm_split_resize_benchmark -- --ignored --nocapture --test-threads=1
```

## Changes behind the measurements

- VTCode coalesces already queued splitter drag positions without waiting for
  more input. Release, keys, modifiers, wheel events, and other gesture boundaries
  retain their order.
- Loaded panes apply a resize in the short input callback when earlier input has
  drained. Queued resize bursts coalesce independently for each pane. Rendering
  remains background work, avoiding an extra deferred apply turn per pane.
- Waveform rendering continues after the last updated pane, so continuous
  resizing cannot repeatedly choose the same dirty view and starve another one.
- One reusable canvas and span buffer serve all waveform panes. Every cell is
  repainted; parsed waveforms and view settings remain separate. Scratch storage
  is bounded by the 512 × 200 surface limit and released when the last view closes.
- VTCode paints structured surfaces directly, caches contrast-adjusted styles
  for the frame, and uses a fast path for ASCII and plot glyphs. General Unicode
  retains the previous grapheme and clipping behavior, checked against Paragraph.

## Native rendering

The release native rendering benchmark excludes loading and measures 101 frames
per case, changing the height by up to seven rows to exercise resizing. Reusing
the renderer reduced median drawing from approximately 0.237 to 0.113 ms at
180 × 30, 1.163 to 0.430 ms at 512 × 80, and 2.148 to 0.680 ms at 512 × 160.
JSON serialization is reported separately.

```powershell
cargo run --locked --release --example bench_render -- target/render.json
```

Native tests compare fresh and reused rendering across two different dumps,
Unicode, dialogs, shrinking/growing frames, and empty/tiny surfaces to detect
stale cells or data leaking between panes.
