//! Native waveform rendering/serialization benchmark, excluding parsing.
use std::time::Instant;
use vtcode_waveform_viewer::{render::Renderer, vcd, viewer::Viewer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut viewer = Viewer::new("demo.vcd".into(), None);
    viewer.ready(vcd::parse(include_bytes!("demo.vcd"))?);
    let mut rows = Vec::new();
    for reuse in [false, true] {
        let mut renderer = Renderer::default();
        for (width, height) in [(180, 30), (512, 80), (512, 160)] {
            viewer.width = width;
            let mut times = Vec::new();
            for index in 0..101 {
                viewer.height = height + index % 8;
                let start = Instant::now();
                let fresh;
                let frame = if reuse {
                    renderer.render(&viewer)
                } else {
                    fresh = viewer.render();
                    &fresh
                };
                let rendered = start.elapsed();
                let start = Instant::now();
                let bytes = serde_json::to_vec(frame)?;
                let serialized = start.elapsed();
                times.push((rendered, serialized, bytes.len()));
                std::hint::black_box(bytes);
            }
            times.sort_by_key(|(rendered, _, _)| *rendered);
            let (rendered, serialized, bytes) = times[50];
            println!(
                "{width}x{height} reuse={reuse}: render {:.3} ms, serialize {:.3} ms, {bytes} JSON bytes",
                rendered.as_secs_f64() * 1000.0,
                serialized.as_secs_f64() * 1000.0
            );
            rows.push(serde_json::json!({"width":width,"height":height,"reuse":reuse,"render_ms":rendered.as_secs_f64()*1000.0,"serialize_ms":serialized.as_secs_f64()*1000.0,"json_bytes":bytes}));
        }
    }
    if let Some(path) = std::env::args().nth(1) {
        std::fs::write(path, serde_json::to_vec_pretty(&rows)?)?;
    }
    Ok(())
}
