use vtcode_waveform_viewer::{
    loader::{Loader, PARSE_CHUNK, READ_AHEAD, Source},
    model::{Value, Waveform},
    vcd::{self, VcdParser},
};

const DEMO: &[u8] = include_bytes!("../examples/demo.vcd");

#[test]
fn borrowed_and_fragmented_tokens_agree_at_every_split_including_utf8() {
    let data = concat!(
        "\u{feff}$timescale 10 ps $end\r\n$scope module 核 $end\n",
        "$var wire 1 ! clk $end $var wire 8 # \\données [7:0] $end\n",
        "$var real 64 r analog $end $var string 8 s label $end\n",
        "$upscope $end $enddefinitions $end\n",
        "$dumpvars 0! b10 # r1.25 r sα s $end\n",
        "#7\t1! bXZ # R-2e-3 r Sβ s\r\n#10 b00110011 #"
    )
    .as_bytes();
    let mut expected = Waveform::default();
    let mut parser = VcdParser::default();
    parser.feed(data, &mut expected).unwrap();
    parser.finish(&mut expected).unwrap();
    assert_eq!(expected.change_count, 9);
    assert_eq!(
        expected.traces[1].changes[0].value,
        Value::Bits("00000010".into())
    );
    assert_eq!(
        expected.traces[1].changes[1].value,
        Value::Bits("xxxxxxxz".into())
    );
    assert_eq!(expected.traces[2].changes[1].value, Value::Real(-0.002));
    assert_eq!(expected.traces[3].changes[1].value, Value::Text("β".into()));
    let expected_tokens = parser.tokens;
    for split in 0..=data.len() {
        let mut wave = Waveform::default();
        let mut parser = VcdParser::default();
        parser.feed(&data[..split], &mut wave).unwrap();
        parser.feed(&[], &mut wave).unwrap();
        parser.feed(&data[split..], &mut wave).unwrap();
        parser.finish(&mut wave).unwrap();
        assert_eq!(parser.bytes, data.len() as u64, "split {split}");
        assert_eq!(parser.tokens, expected_tokens, "split {split}");
        assert_eq!(wave.change_count, expected.change_count);
        assert_eq!(wave.signals[1].path, expected.signals[1].path);
        for (actual, expected) in wave.traces.iter().zip(&expected.traces) {
            assert_eq!(actual.changes.len(), expected.changes.len());
            for (actual, expected) in actual.changes.iter().zip(&expected.changes) {
                assert_eq!(actual.time, expected.time, "split {split}");
                assert_eq!(actual.value, expected.value, "split {split}");
            }
        }
    }
}

#[test]
fn fast_and_fragmented_tokens_keep_limits_and_truncation_errors() {
    for length in [65_536, 65_537] {
        let mut data = b"$comment ".to_vec();
        data.extend(std::iter::repeat_n(b'a', length));
        data.extend_from_slice(b" $end\n");
        data.extend_from_slice(DEMO);
        for chunk_size in [1, 16_384, data.len()] {
            let mut wave = Waveform::default();
            let mut parser = VcdParser::default();
            let result = data
                .chunks(chunk_size)
                .try_for_each(|chunk| parser.feed(chunk, &mut wave));
            if length == 65_536 {
                result.unwrap();
                parser.finish(&mut wave).unwrap();
            } else {
                assert!(result.unwrap_err().contains("token exceeds"));
            }
        }
    }
    for body in [b"#1001 b101".as_slice(), b"#1001 s\xff s"] {
        let mut data = DEMO.to_vec();
        data.extend_from_slice(body);
        for chunk_size in [1, 7, data.len()] {
            let mut wave = Waveform::default();
            let mut parser = VcdParser::default();
            let result = data
                .chunks(chunk_size)
                .try_for_each(|chunk| parser.feed(chunk, &mut wave))
                .and_then(|()| parser.finish(&mut wave));
            let error = result.unwrap_err();
            assert!(
                error.contains("Truncated VCD value") || error.contains("invalid UTF-8"),
                "{error}"
            );
        }
    }
}

struct BufferedSource {
    bytes: Vec<u8>,
    reads: Vec<(u64, usize, usize)>,
    max_read: usize,
}
impl Source for BufferedSource {
    fn read(&mut self, _: u64, _: usize) -> Result<Vec<u8>, String> {
        panic!("the loader should fill reusable storage directly")
    }
    fn read_into(&mut self, offset: u64, out: &mut [u8]) -> Result<usize, String> {
        self.reads.push((offset, out.len(), out.as_ptr() as usize));
        let start = offset as usize;
        let count = out.len().min(self.max_read).min(self.bytes.len() - start);
        out[..count].copy_from_slice(&self.bytes[start..start + count]);
        Ok(count)
    }
}

#[test]
fn read_ahead_reuses_storage_but_parse_steps_and_progress_remain_bounded() {
    let mut bytes = b"$comment ".to_vec();
    bytes.resize(3 * READ_AHEAD, b' ');
    bytes.extend_from_slice(b" $end\n");
    bytes.extend_from_slice(DEMO);
    let mut source = BufferedSource {
        bytes,
        reads: vec![],
        max_read: usize::MAX,
    };
    let mut loader = Loader::new(source.bytes.len() as u64, false).unwrap();
    for step in 1..=READ_AHEAD / PARSE_CHUNK {
        assert!(loader.step(&mut source).unwrap().is_none());
        assert_eq!(loader.offset, (step * PARSE_CHUNK) as u64);
        assert_eq!(
            source.reads.len(),
            1,
            "parse from the buffered read before requesting more"
        );
    }
    let wave = loop {
        let before = loader.offset;
        let result = loader.step(&mut source).unwrap();
        assert!(loader.offset - before <= PARSE_CHUNK as u64);
        if let Some(wave) = result {
            break wave;
        }
    };
    assert_eq!(wave.change_count, vcd::parse(DEMO).unwrap().change_count);
    assert_eq!(source.reads.len(), source.bytes.len().div_ceil(READ_AHEAD));
    let first_pointer = source.reads[0].2;
    for (offset, requested, pointer) in &source.reads {
        assert!(*requested <= READ_AHEAD);
        assert_eq!(*pointer, first_pointer, "storage is reused between reads");
        assert_eq!(*offset % READ_AHEAD as u64, 0);
    }
    assert!(loader.done);
    assert!(loader.step(&mut source).unwrap().is_none());
}

#[test]
fn short_reads_preserve_tokens_and_fst_decode_waits_for_a_separate_step() {
    for (bytes, fst) in [
        (DEMO.to_vec(), false),
        (include_bytes!("fixtures/gcd.fst").to_vec(), true),
    ] {
        let mut source = BufferedSource {
            bytes,
            reads: vec![],
            max_read: 7,
        };
        let mut loader = Loader::new(source.bytes.len() as u64, fst).unwrap();
        while loader.offset < loader.size {
            assert!(!loader.needs_decode_pass());
            assert!(loader.step(&mut source).unwrap().is_none());
        }
        assert_eq!(loader.needs_decode_pass(), fst);
        let wave = loader.step(&mut source).unwrap().unwrap();
        assert!(wave.change_count > 0);
        assert!(!loader.needs_decode_pass());
        assert_eq!(source.reads.len(), source.bytes.len().div_ceil(7));
    }
}

#[test]
fn invalid_sources_cannot_overreport_or_silently_end_before_the_declared_size() {
    struct InvalidSource(usize);
    impl Source for InvalidSource {
        fn read(&mut self, _: u64, count: usize) -> Result<Vec<u8>, String> {
            Ok(vec![0; count + 1])
        }
        fn read_into(&mut self, _: u64, out: &mut [u8]) -> Result<usize, String> {
            Ok(if self.0 == 0 { 0 } else { out.len() + 1 })
        }
    }
    for count in [0, 1] {
        assert!(
            Loader::new(100, false)
                .unwrap()
                .step(&mut InvalidSource(count))
                .unwrap_err()
                .contains("changed")
        );
    }
    struct LegacySource;
    impl Source for LegacySource {
        fn read(&mut self, _: u64, count: usize) -> Result<Vec<u8>, String> {
            Ok(vec![0; count + 1])
        }
    }
    assert!(
        Loader::new(100, false)
            .unwrap()
            .step(&mut LegacySource)
            .unwrap_err()
            .contains("too many bytes")
    );
}
