//! The recorder's writer thread: drains the ring into WAV files.

use super::wav::{WavOut, HEADER_LEN};
use super::{part_path, RecordSettings, Shared, MIN_FREE_BYTES, WARN_FREE_BYTES};
use om_core::sync::LockExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) struct Context {
    pub shared: Arc<Shared>,
    pub settings: RecordSettings,
    pub folder: PathBuf,
    pub stem: String,
    pub rate: u32,
    pub first: Option<WavOut>,
    pub rx: rtrb::Consumer<f32>,
    pub stop: Arc<AtomicBool>,
    pub abort: Arc<AtomicBool>,
    pub back: Sender<rtrb::Consumer<f32>>,
}

#[derive(Default)]
pub(super) struct Outcome {
    pub files: Vec<PathBuf>,
    pub frames: u64,
    pub bytes: u64,
    pub error: Option<String>,
}

const SCRATCH: usize = 16_384;
const DISK_CHECK: Duration = Duration::from_secs(1);

pub(super) fn run(mut ctx: Context) -> Outcome {
    let mut out = Outcome::default();
    let result = drain_loop(&mut ctx, &mut out);
    if let Err(message) = result {
        out.error = Some(message.clone());
        *ctx.shared.error.lock_unpoisoned() = Some(message);
        ctx.shared.enabled.store(false, Relaxed);
        ctx.shared.state.store(2, Relaxed);
    }
    // Hand the consumer back so the next recording can reuse the ring.
    let _ = ctx.back.send(ctx.rx);
    out
}

fn drain_loop(ctx: &mut Context, out: &mut Outcome) -> Result<(), String> {
    let format = ctx.settings.format;
    let frame_bytes = format.frame_bytes();
    let max = ctx.settings.max_data_bytes.max(frame_bytes);
    let mut part = 1u32;
    let mut cur = ctx.first.take().ok_or("no output file")?;
    out.files.push(cur.path.clone());
    let mut scratch: Vec<f32> = Vec::with_capacity(SCRATCH);
    let mut closed_bytes = 0u64;
    let mut frames = 0u64;
    let started = Instant::now();
    let mut last_flush = started;
    let mut last_patch = started;
    let mut last_disk = started;

    let result = loop {
        if ctx.abort.load(Relaxed) {
            // A simulated crash: leave the file exactly as the last patch left it.
            return Ok(());
        }
        if ctx.settings.stall.as_ref().is_some_and(|s| s.load(Relaxed)) {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        let stopping = ctx.stop.load(Relaxed);
        let avail = ctx.rx.slots() & !1;
        if avail == 0 {
            if stopping {
                break Ok(());
            }
            std::thread::sleep(Duration::from_millis(3));
        } else {
            let n = avail.min(SCRATCH);
            scratch.clear();
            if let Ok(chunk) = ctx.rx.read_chunk(n) {
                let (a, b) = chunk.as_slices();
                scratch.extend_from_slice(a);
                scratch.extend_from_slice(b);
                chunk.commit_all();
            }
            let mut rest: &[f32] = &scratch;
            while !rest.is_empty() {
                let room = (max.saturating_sub(cur.data_bytes) / frame_bytes) as usize;
                if room == 0 {
                    // Seamless split: close this file, continue in the next with the same frames.
                    closed_bytes += HEADER_LEN + cur.data_bytes;
                    cur.finish().map_err(|e| e.to_string())?;
                    part += 1;
                    cur =
                        WavOut::create(&part_path(&ctx.folder, &ctx.stem, part), format, ctx.rate)
                            .map_err(|e| e.to_string())?;
                    out.files.push(cur.path.clone());
                    *ctx.shared.file.lock_unpoisoned() = Some(cur.path.clone());
                    continue;
                }
                let take = room.min(rest.len() / 2) * 2;
                cur.write(&rest[..take]).map_err(|e| e.to_string())?;
                frames += (take / 2) as u64;
                rest = &rest[take..];
            }
            ctx.shared.frames.store(frames, Relaxed);
            ctx.shared
                .bytes
                .store(closed_bytes + HEADER_LEN + cur.data_bytes, Relaxed);
        }
        let now = Instant::now();
        if now.duration_since(last_flush) >= ctx.settings.flush_interval {
            cur.flush().map_err(|e| e.to_string())?;
            last_flush = now;
        }
        if now.duration_since(last_patch) >= ctx.settings.header_interval {
            cur.patch().map_err(|e| e.to_string())?;
            last_patch = now;
        }
        if now.duration_since(last_disk) >= DISK_CHECK {
            last_disk = now;
            if let Some(free) = (ctx.settings.free_space)(&ctx.folder) {
                ctx.shared.low_disk.store(free < WARN_FREE_BYTES, Relaxed);
                if free < MIN_FREE_BYTES && ctx.settings.stop_on_low_disk {
                    break Err(format!(
                        "stopped: only {} MB of disk space left",
                        free >> 20
                    ));
                }
            }
        }
    };
    out.frames = frames;
    out.bytes = closed_bytes + HEADER_LEN + cur.data_bytes;
    // Finalise even when stopping because of an error, so the file stays playable.
    let fin = cur.finish().map_err(|e| e.to_string());
    result.and(fin)
}
