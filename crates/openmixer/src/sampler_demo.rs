//! `--demo-library`: eight short generated samples (kick, snare, hat, clap, two tones, a click, a
//! bass loop) so the sampler pads are loaded in screenshots. Written into a scratch folder, never
//! the user's bank. Device-free.

use om_engine::sampler::PadMode;
use om_ui::sample_bank::{self, PadEntry};
use std::io;
use std::path::Path;

const RATE: u32 = 44_100;

/// Writes the demo WAVs and a `sampler.json` for them into `dir` (created if needed).
pub fn write_demo_bank(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut pads: sample_bank::BankPads = Default::default();
    for (pad, (file, name, mode, samples)) in demo_pads().into_iter().enumerate() {
        write_wav_16(&dir.join(&file), &samples)?;
        pads[pad] = Some(PadEntry {
            file,
            name: name.to_owned(),
            gain: 0.9,
            pitch: 0.0,
            mode,
        });
    }
    std::fs::write(
        dir.join(sample_bank::BANK_FILE),
        sample_bank::to_json(&pads),
    )
}

type DemoPad = (String, &'static str, PadMode, Vec<f32>);

/// The eight demo pads: file name, pad name, mode and mono samples.
fn demo_pads() -> Vec<DemoPad> {
    let secs = |s: f64| (s * f64::from(RATE)) as usize;
    let mut noise = Lcg(0x9e37_79b9_7f4a_7c15);
    let kick: Vec<f32> = (0..secs(0.45))
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            let f = 50.0 + 90.0 * (-t * 25.0).exp();
            (0.9 * (-t * 7.0).exp() * (std::f64::consts::TAU * f * t).sin()) as f32
        })
        .collect();
    let snare: Vec<f32> = (0..secs(0.25))
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            (noise.next() * 0.6 * (-t * 22.0).exp()) as f32
        })
        .collect();
    let hat: Vec<f32> = (0..secs(0.08))
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            (noise.next() * 0.35 * (-t * 60.0).exp()) as f32
        })
        .collect();
    let clap: Vec<f32> = (0..secs(0.3))
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            let burst = ((t * 35.0).fract() < 0.5) as u8 as f64;
            (noise.next() * 0.5 * burst * (-t * 12.0).exp()) as f32
        })
        .collect();
    let tone = |freq: f64, secs_len: f64| -> Vec<f32> {
        (0..secs(secs_len))
            .map(|i| {
                let t = i as f64 / f64::from(RATE);
                let env = (t * 40.0).min(1.0) * (1.0 - t / secs_len).max(0.0);
                (0.4 * env * (std::f64::consts::TAU * freq * t).sin()) as f32
            })
            .collect()
    };
    let click: Vec<f32> = (0..secs(0.01))
        .map(|i| if i < 40 { 0.8 } else { 0.0 })
        .collect();
    let bass = tone(110.0, 1.0);
    vec![
        ("kick.wav".into(), "Kick", PadMode::OneShot, kick),
        ("snare.wav".into(), "Snare", PadMode::OneShot, snare),
        ("hat.wav".into(), "Hat", PadMode::OneShot, hat),
        ("clap.wav".into(), "Clap", PadMode::OneShot, clap),
        (
            "tone-a.wav".into(),
            "Tone A",
            PadMode::Gate,
            tone(440.0, 0.8),
        ),
        (
            "tone-e.wav".into(),
            "Tone E",
            PadMode::Gate,
            tone(659.3, 0.8),
        ),
        ("click.wav".into(), "Click", PadMode::OneShot, click),
        ("bass.wav".into(), "Bass", PadMode::Loop, bass),
    ]
}

/// A tiny deterministic noise source (no dependency, same output every run).
struct Lcg(u64);

impl Lcg {
    /// Uniform in -1..1.
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

/// Mono 16-bit PCM WAV at the demo rate.
fn write_wav_16(path: &Path, mono: &[f32]) -> io::Result<()> {
    let data_len = (mono.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &v in mono {
        let s = (v.clamp(-1.0, 1.0) * 32_767.0) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_bank_is_eight_loadable_files_and_a_valid_json() {
        let tmp = om_testing::fs::TempDir::new("demo-bank");
        write_demo_bank(tmp.path()).unwrap();
        let text = std::fs::read_to_string(tmp.path().join(sample_bank::BANK_FILE)).unwrap();
        let parsed = sample_bank::parse(&text).unwrap();
        assert!(parsed.issues.is_empty(), "{:?}", parsed.issues);
        for pad in 0..8 {
            let entry = parsed.pads[pad].as_ref().expect("every demo pad");
            let sample = om_engine::Sample::load(&tmp.path().join(&entry.file), 44_100.0).unwrap();
            assert!(sample.frames() > 0);
        }
    }
}
