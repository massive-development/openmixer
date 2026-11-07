//! Deck varispeed block cost (#241). Run with `cargo bench -p om-engine --bench varispeed`.
//!
//! One 256-frame stereo block at a 1.08 step through the whole deck render path (decoder thread
//! feeding the ring, as in the app), per quality and device rate. The block budget is
//! 256 / device rate: 5.80 ms at 44.1 kHz, 2.67 ms at 96 kHz. File rate equals device rate.

use divan::{black_box, Bencher};
use om_engine::deck::{Deck, VarispeedQuality};
use std::time::{Duration, Instant};

const BLOCK: usize = 256;

fn main() {
    divan::main();
}

fn write_wav(path: &std::path::Path, rate: u32, frames: usize) {
    let mut samples = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let t = n as f32 / rate as f32;
        let v = 0.4 * (t * 2.0 * std::f32::consts::PI * 997.0).sin()
            + 0.2 * (t * 2.0 * std::f32::consts::PI * 7_919.0).sin();
        samples.push(v);
        samples.push(v * 0.9);
    }
    om_testing::wav::write_float_wav(path, 2, rate, &samples);
}

#[divan::bench(args = [
    (VarispeedQuality::Linear, 44_100),
    (VarispeedQuality::Polynomial, 44_100),
    (VarispeedQuality::Sinc, 44_100),
    (VarispeedQuality::Linear, 96_000),
    (VarispeedQuality::Polynomial, 96_000),
    (VarispeedQuality::Sinc, 96_000),
])]
fn deck_block_at_1_08(bencher: Bencher, (quality, rate): (VarispeedQuality, u32)) {
    let dir = std::env::temp_dir().join(format!("om_bench_varispeed_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let wav = dir.join("tone.wav");
    let frames = rate as usize * 30;
    write_wav(&wav, rate, frames);

    let (deck, mut audio) = Deck::new(1 << 16);
    audio.set_varispeed_quality(quality);
    audio.prepare(f64::from(rate), BLOCK);
    deck.load(&wav).expect("fixture loads");
    // Loop most of the file so the bench never reaches the end.
    let _ = deck.set_loop(om_engine::quantizer::Loop {
        start: 0,
        end: frames as i64 - 1,
    });
    let (mut l, mut r) = (vec![0.0_f32; BLOCK], vec![0.0_f32; BLOCK]);
    let settle = Instant::now();
    while deck.buffered_frames() < 4096 && settle.elapsed() < Duration::from_secs(10) {
        audio.render_stereo(&mut l, &mut r);
        std::thread::sleep(Duration::from_millis(1));
    }
    deck.set_rate(1.08);
    deck.play();
    bencher
        .with_inputs(|| {
            // Untimed: keep the ring topped up so the block measures rendering, not starvation.
            while deck.buffered_frames() < BLOCK * 4 {
                std::thread::yield_now();
            }
        })
        .bench_local_values(|()| {
            audio.render_stereo(black_box(&mut l), black_box(&mut r));
        });
    drop(audio);
    drop(deck);
    let _ = std::fs::remove_dir_all(&dir);
}
