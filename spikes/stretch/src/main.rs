//! THROWAWAY SPIKE (Task 8): time-stretch quality harness for OpenMixer.
//!
//! Fully OFFLINE. Reads/writes WAV files only; never opens an audio device
//! and never plays anything.
//!
//! Usage:
//!   stretch <in.wav> <ratio> <out.wav> <engine>   stretch one file (mono or stereo; engine REQUIRED) (ratio = tempo, 1.08 = 8% faster)
//!   stretch gen                                   write synthetic test material to out/
//!   stretch eval                                  generate, stretch with every engine, measure, write out/ + results
//!   stretch engines                               list engine names
//!
//! "ratio" is the DJ tempo / playback-rate factor: output length = input length / ratio,
//! pitch preserved (keylock).

mod engines;
mod metrics;
mod synth;

use std::path::Path;

pub const SR: u32 = 44_100;

/// Read interleaved f32 and channel count (1 or 2 only).
pub fn read_wav(path: &Path) -> (Vec<f32>, usize, u32) {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    assert!(spec.channels == 1 || spec.channels == 2, "only mono or stereo WAV supported, got {} channels", spec.channels);
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.unwrap() as f32 / scale).collect()
        }
    };
    (raw, spec.channels as usize, spec.sample_rate)
}

pub fn write_wav(path: &Path, data: &[f32], channels: usize, sr: u32) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).ok();
    }
    let spec = hound::WavSpec {
        channels: channels as u16,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("create wav");
    for &s in data {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).unwrap();
    }
    w.finalize().unwrap();
}

pub fn write_wav_mono(path: &Path, data: &[f32], sr: u32) {
    write_wav(path, data, 1, sr)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("gen") => {
            let out = Path::new("out");
            write_wav_mono(&out.join("chord_orig.wav"), &synth::chord(8.0), SR);
            write_wav_mono(&out.join("drums_orig.wav"), &synth::drums(8.0), SR);
            println!("wrote out/chord_orig.wav out/drums_orig.wav");
        }
        Some("engines") => {
            for e in engines::all() {
                println!("{}: {}", e.name, e.desc);
            }
        }
        Some("eval") => metrics::eval_all(),
        Some("debug-pitch") => metrics::debug_pitch(),
        Some("debug-rb") => metrics::debug_rb_opts(),
        Some("debug-sine") => metrics::debug_sine(&args[1], args[2].parse().unwrap()),
        Some(inp) if args.len() >= 4 => {
            let ratio: f64 = args[1].parse().expect("ratio");
            let name = args[3].as_str();
            let eng = engines::all()
                .into_iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("unknown engine {name}; try `stretch engines`"));
            let (x, ch, sr) = read_wav(Path::new(inp));
            assert_eq!(sr, SR, "harness assumes 44.1 kHz");
            let y = (eng.run)(&x, ch, ratio);
            write_wav(Path::new(&args[2]), &y, ch, sr);
            println!("{name}: {} -> {} samples", x.len(), y.len());
        }
        _ => eprintln!("usage: stretch <in.wav> <ratio> <out.wav> <engine> (engine required; see `stretch engines`) | gen | eval | engines"),
    }
}
