//! THROWAWAY SPIKE (Task 8): time-stretch quality harness for OpenMixer.
//!
//! Fully OFFLINE. Reads/writes WAV files only; never opens an audio device
//! and never plays anything.
//!
//! Usage:
//!   stretch <in.wav> <ratio> <out.wav> [engine]   stretch one file (ratio = tempo, 1.08 = 8% faster)
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

pub fn read_wav_mono(path: &Path) -> (Vec<f32>, u32) {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    let ch = spec.channels as usize;
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.unwrap() as f32 / scale).collect()
        }
    };
    let mono = raw.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect();
    (mono, spec.sample_rate)
}

pub fn write_wav_mono(path: &Path, data: &[f32], sr: u32) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).ok();
    }
    let spec = hound::WavSpec {
        channels: 1,
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
        Some(inp) if args.len() >= 3 => {
            let ratio: f64 = args[1].parse().expect("ratio");
            let name = args.get(3).map(|s| s.as_str()).unwrap_or("rubberband-rt");
            let eng = engines::all()
                .into_iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("unknown engine {name}; try `stretch engines`"));
            let (x, sr) = read_wav_mono(Path::new(inp));
            assert_eq!(sr, SR, "harness assumes 44.1 kHz mono");
            let y = (eng.run)(&x, ratio);
            write_wav_mono(Path::new(&args[2]), &y, sr);
            println!("{name}: {} -> {} samples", x.len(), y.len());
        }
        _ => eprintln!("usage: stretch <in.wav> <ratio> <out.wav> [engine] | gen | eval | engines"),
    }
}
