//! Port of tests/unit/MidiLearnConcurrencySlowTest.cpp: a MIDI thread streams while the control
//! thread arms, cancels and edits the map. Bounded (400 rounds), so it runs in the normal suite.

mod support;

use om_midi::{control_change, note_on, LearnRequest, Mapping, MidiMapper};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use support::RecordingTarget;

#[test]
fn arm_cancel_and_edit_while_messages_stream() {
    let mapper = MidiMapper::new();
    let target = Arc::new(RecordingTarget::default());
    mapper.set_target(target.clone());
    let learned = Arc::new(AtomicUsize::new(0));
    let sink = learned.clone();
    mapper.set_on_learned(move |_: &Mapping| {
        sink.fetch_add(1, Ordering::Relaxed);
    });
    mapper
        .add_binding(Mapping {
            number: 1,
            target: "seed".into(),
            ..Mapping::default()
        })
        .unwrap();

    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            let mut i: usize = 0;
            while !stop.load(Ordering::SeqCst) {
                mapper.handle_message(&control_change(1, 1 + (i % 4) as u8, (i % 128) as u8));
                mapper.handle_message(&note_on(1, 40 + (i % 8) as u8, 100));
                i += 1;
            }
        });

        for round in 0..400usize {
            mapper.arm_learn(LearnRequest::new("learned"));
            if round % 3 == 0 {
                mapper.cancel_learn();
            }
            let target_id = format!("extra{}", round % 5);
            mapper
                .add_binding(Mapping {
                    number: 20 + (round % 5) as i32,
                    target: target_id.clone(),
                    ..Mapping::default()
                })
                .unwrap();
            if round % 4 == 0 {
                mapper.remove_bindings_for(&target_id);
            }
            mapper.get_bindings();
            mapper.set_outputs(Vec::new());
            if round % 50 == 0 {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        stop.store(true, Ordering::SeqCst);
    });

    assert!(learned.load(Ordering::Relaxed) > 0);
    assert!(target.count("seed") + target.count("learned") + target.count("extra0") > 0);
    assert!(!mapper.get_bindings().is_empty());
}
