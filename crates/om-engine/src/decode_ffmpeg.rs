//! Optional decode fallback for formats symphonia cannot read. Runs the `ffmpeg` executable as a
//! subprocess that writes raw interleaved stereo `f32le` PCM to stdout; nothing is linked, so
//! there is no build dependency and no licensing entanglement. Control thread only (it blocks).
//!
//! Safety of the subprocess: arguments are passed individually (never through a shell); the input
//! is given as `file:<path>` so a path can neither be read as an option nor as a URL (ffmpeg would
//! otherwise open `http://...`), and only the `file` protocol is whitelisted; stdout is capped at
//! [`MAX_OUTPUT_BYTES`] and the run at [`TIMEOUT`], after which the child is killed.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Most decoded PCM accepted (matches the sampler's cap: 64 MiB of stereo `f32`).
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
/// Wall-clock limit for one decode.
pub const TIMEOUT: Duration = Duration::from_secs(60);
/// How much of ffmpeg's stderr is kept for an error message.
const MAX_STDERR_BYTES: usize = 4096;
const POLL: Duration = Duration::from_millis(5);

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("the ffmpeg executable was not found (set OPENMIXER_FFMPEG to its path)")]
    FfmpegMissing,
    #[error("ffmpeg could not decode the file: {0}")]
    FfmpegFailed(String),
}

/// Decoded audio: interleaved stereo `f32` at `sample_rate`.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// The ffmpeg to run: `OPENMIXER_FFMPEG` when set and non-empty, else `ffmpeg` from `PATH`.
fn binary() -> OsString {
    match std::env::var_os("OPENMIXER_FFMPEG") {
        Some(v) if !v.is_empty() => v,
        _ => OsString::from("ffmpeg"),
    }
}

/// True when the ffmpeg executable can be started here.
pub fn ffmpeg_available() -> bool {
    Command::new(binary())
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Decodes `path` with ffmpeg to stereo `f32` at `target_rate`.
pub fn decode_with_ffmpeg(path: &Path, target_rate: u32) -> Result<DecodedAudio, DecodeError> {
    run(&binary(), path, target_rate, MAX_OUTPUT_BYTES, TIMEOUT)
}

/// As [`decode_with_ffmpeg`] with a caller-chosen output cap (decks hold longer audio than samples).
pub fn decode_with_ffmpeg_capped(
    path: &Path,
    target_rate: u32,
    max_bytes: usize,
) -> Result<DecodedAudio, DecodeError> {
    run(&binary(), path, target_rate, max_bytes, TIMEOUT)
}

fn run(
    bin: &OsStr,
    path: &Path,
    target_rate: u32,
    max_bytes: usize,
    timeout: Duration,
) -> Result<DecodedAudio, DecodeError> {
    if target_rate == 0 {
        return Err(DecodeError::FfmpegFailed("target rate is zero".into()));
    }
    let mut input = OsString::from("file:");
    input.push(path);
    let mut child = Command::new(bin)
        .args(["-nostdin", "-v", "error", "-protocol_whitelist", "file"])
        .arg("-i")
        .arg(&input)
        .args(["-vn", "-f", "f32le", "-ac", "2", "-ar"])
        .arg(target_rate.to_string())
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => DecodeError::FfmpegMissing,
            _ => DecodeError::FfmpegFailed(format!("cannot start ffmpeg: {e}")),
        })?;

    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let out_thread = thread::spawn(move || read_capped(&mut stdout, max_bytes));
    let err_thread = thread::spawn(move || read_capped(&mut stderr, MAX_STDERR_BYTES).0);

    let start = Instant::now();
    let mut problem: Option<String> = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(e) => {
                problem = Some(format!("waiting for ffmpeg: {e}"));
                break None;
            }
        }
        if start.elapsed() >= timeout {
            problem = Some(format!("timed out after {} s", timeout.as_secs()));
            break None;
        }
        // When the reader stops at the cap it drops the pipe, so ffmpeg ends on a broken pipe (or
        // at the timeout at the latest).
        thread::sleep(POLL);
    };
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(problem) = problem {
        // The reader threads are left to end on their own: a grandchild of the killed process
        // could still hold the pipes open, and we must not wait for it.
        return Err(DecodeError::FfmpegFailed(problem));
    }
    let (bytes, over_cap) = out_thread.join().unwrap_or((Vec::new(), false));
    let stderr_text = err_thread.join().unwrap_or_default();
    let stderr_text = String::from_utf8_lossy(&stderr_text).trim().to_owned();

    if over_cap {
        return Err(DecodeError::FfmpegFailed(format!(
            "decoded audio exceeds {} MiB",
            max_bytes / (1024 * 1024)
        )));
    }
    match status {
        Some(s) if s.success() => {}
        _ => {
            let why = if stderr_text.is_empty() {
                "ffmpeg exited with an error".to_owned()
            } else {
                stderr_text
            };
            return Err(DecodeError::FfmpegFailed(why));
        }
    }
    if bytes.is_empty() {
        return Err(DecodeError::FfmpegFailed("the file has no audio".into()));
    }
    let samples = bytes
        .chunks_exact(8) // whole stereo frames only
        .flat_map(|f| {
            [
                f32::from_le_bytes([f[0], f[1], f[2], f[3]]),
                f32::from_le_bytes([f[4], f[5], f[6], f[7]]),
            ]
        })
        .collect();
    Ok(DecodedAudio {
        samples,
        sample_rate: target_rate,
    })
}

/// Reads up to `cap` bytes; the flag is true when the stream held more than that.
fn read_capped(r: &mut impl Read, cap: usize) -> (Vec<u8>, bool) {
    let mut buf = Vec::new();
    let _ = r.take(cap as u64 + 1).read_to_end(&mut buf);
    let over = buf.len() > cap;
    buf.truncate(cap);
    (buf, over)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in "ffmpeg": a shell script run with the real argument list.
    fn fake(dir: &Path, body: &str) -> OsString {
        let p = dir.join("fake-ffmpeg");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.into_os_string()
    }

    #[test]
    fn nonzero_exit_carries_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake(dir.path(), "echo 'bad header' >&2; exit 1");
        let err = run(&bin, Path::new("x.mp3"), 48_000, 1024, TIMEOUT).unwrap_err();
        assert!(
            matches!(&err, DecodeError::FfmpegFailed(m) if m == "bad header"),
            "{err:?}"
        );
    }

    #[test]
    fn output_over_the_cap_is_rejected_and_the_child_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake(dir.path(), "yes");
        let err = run(
            &bin,
            Path::new("x.mp3"),
            48_000,
            4096,
            Duration::from_secs(20),
        )
        .unwrap_err();
        assert!(
            matches!(&err, DecodeError::FfmpegFailed(m) if m.contains("exceeds")),
            "{err:?}"
        );
    }

    #[test]
    fn a_hung_child_is_killed_at_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake(dir.path(), "sleep 30");
        let t = Instant::now();
        let err = run(
            &bin,
            Path::new("x.mp3"),
            48_000,
            1024,
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert!(
            matches!(&err, DecodeError::FfmpegFailed(m) if m.contains("timed out")),
            "{err:?}"
        );
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn arguments_are_separate_and_the_path_is_neutralised() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("args.txt");
        let body = format!(
            "for a in \"$@\"; do echo \"$a\" >> '{}'; done",
            log.display()
        );
        let bin = fake(dir.path(), &body);
        // No output, so this errors; only the recorded arguments matter.
        let _ = run(&bin, Path::new("-evil; rm -rf x"), 44_100, 1024, TIMEOUT);
        let args: Vec<String> = std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        let i = args.iter().position(|a| a == "-i").unwrap();
        assert_eq!(args[i + 1], "file:-evil; rm -rf x");
        assert!(args.contains(&"44100".to_owned()));
    }

    #[test]
    fn whole_stereo_frames_decode_to_samples() {
        let dir = tempfile::tempdir().unwrap();
        // 1.0f32 then -1.0f32 little endian, as one frame.
        let bin = fake(
            dir.path(),
            "printf '\\000\\000\\200\\077\\000\\000\\200\\277'",
        );
        let got = run(&bin, Path::new("x.mp3"), 48_000, 1024, TIMEOUT).unwrap();
        assert_eq!(got.samples, vec![1.0, -1.0]);
    }
}
