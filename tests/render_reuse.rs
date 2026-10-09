use vtcode_waveform_viewer::{
    render::Renderer,
    vcd,
    viewer::{Event, Viewer},
};

#[test]
fn shared_render_storage_never_leaks_cells_styles_or_document_state_between_panes() {
    let mut renderer = Renderer::default();
    let mut first = Viewer::new("上段 波形.vcd".into(), Some(1));
    first.ready(vcd::parse(include_bytes!("../examples/demo.vcd")).unwrap());
    let mut second = Viewer::new("lower.vcd".into(), Some(2));
    second.ready(vcd::parse(b"$var wire 8 ! data $end $enddefinitions $end #0 bxxxxxxxx ! #10 bzzzzzzzz ! #20 b10101010 ! #100").unwrap());
    for (width, height) in [
        (512, 160),
        (180, 30),
        (512, 80),
        (40, 10),
        (0, 0),
        (90, 24),
        (2, 1),
        (600, 240),
    ] {
        for viewer in [&mut first, &mut second] {
            viewer.event(Event {
                kind: "resize".into(),
                width,
                height,
                ..Event::default()
            });
            // Include actual zero-sized renders, which input intentionally ignores.
            viewer.width = width;
            viewer.height = height;
            assert_eq!(
                serde_json::to_value(renderer.render(viewer)).unwrap(),
                serde_json::to_value(viewer.render()).unwrap()
            );
        }
    }
    for key in ["?", "?", "m", "escape", "g"] {
        first.event(Event {
            code: if key == "escape" { "escape" } else { "char" }.into(),
            text: key.into(),
            ..Event::default()
        });
        assert_eq!(
            serde_json::to_value(renderer.render(&first)).unwrap(),
            serde_json::to_value(first.render()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(renderer.render(&second)).unwrap(),
            serde_json::to_value(second.render()).unwrap()
        );
    }
}
