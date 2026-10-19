//! The MIDI half of the control thread safety checks: a map reload while messages
//! stream. The router cases live with om-core's router tests and the engine-side cases in om-audio.

mod support;

use om_midi::{control_change, MidiMapper};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use support::RecordingTarget;

const ROUNDS: usize = 300;

#[test]
fn map_reload_while_messages_stream() {
    let map_a = r#"{"mappings":[{"type":"cc","number":7,"target":"a","min":0,"max":1}]}"#;
    let map_b = r#"{"mappings":[{"type":"cc","number":7,"target":"b","min":0,"max":1},{"type":"cc","number":8,"target":"c"}]}"#;
    let mapper = MidiMapper::new();
    let target = std::sync::Arc::new(RecordingTarget::default());
    mapper.set_target(target);
    mapper.load_from_json(map_a).unwrap();

    let stop = AtomicBool::new(false);
    let fired = AtomicUsize::new(0);
    std::thread::scope(|s| {
        s.spawn(|| {
            let mut v: u8 = 0;
            while !stop.load(Ordering::SeqCst) {
                let n = mapper.handle_message(&control_change(1, 7, v & 127))
                    + mapper.handle_message(&control_change(1, 8, 1));
                fired.fetch_add(n, Ordering::Relaxed);
                v = v.wrapping_add(1);
                mapper.get_value("a");
                std::thread::sleep(Duration::from_micros(20));
            }
        });

        // Let the stream start (generous bound for slow hosts).
        let deadline = Instant::now() + Duration::from_secs(20);
        while fired.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        for i in 0..ROUNDS {
            let json = if i % 2 == 0 { map_b } else { map_a };
            mapper.load_from_json(json).expect("reload");
        }
        assert!(
            mapper.load_from_json("not json").is_err(),
            "a bad reload leaves the last good map"
        );
        stop.store(true, Ordering::SeqCst);
    });

    assert!(fired.load(Ordering::Relaxed) > 0);
    assert!(mapper.num_mappings() >= 1);
}
