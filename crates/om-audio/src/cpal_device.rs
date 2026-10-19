//! Real output device through cpal (the JuceAudioDevice analogue). Opens an output device, runs
//! the callback from the cpal data callback and records callback timing and xruns.
//!
//! Output routing: the callback is wrapped in an [`OutputRouter`], so master and cue land on the
//! chosen stereo pairs (see [`CpalAudioDevice::output_pairs`]). A 1-channel device gets the
//! router's mono path.
//!
//! The device generates no sound itself. Nothing here writes audio other than what the callback
//! renders; tests open a device only with a callback that writes zeros.

use crate::device::{AudioDevice, AudioError};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, OutputCallbackInfo, Stream, StreamConfig};
use om_engine::{AudioCallback, LatencyMonitor, OutputPairs, OutputRouter};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Instant;

/// Largest device channel count the callback can address without allocating.
pub const MAX_DEVICE_CHANNELS: usize = 16;
/// Block size used when the device leaves the buffer size to the backend.
pub const DEFAULT_BLOCK: usize = 512;

/// Summary of one output device, for the settings UI.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputDeviceInfo {
    pub name: String,
    pub max_channels: u16,
    pub default_sample_rate: u32,
}

/// What to open. `None` fields take the device default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputRequest {
    /// Device name as listed by [`output_devices`]. `None` uses the default output.
    pub device: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_size: Option<u32>,
    pub channels: Option<u16>,
}

/// Lists the host's output devices. Devices that cannot report a config are left out.
pub fn output_devices() -> Result<Vec<OutputDeviceInfo>, AudioError> {
    let host = cpal::default_host();
    let devices = host
        .output_devices()
        .map_err(|e| AudioError::Stream(e.to_string()))?;
    let mut out = Vec::new();
    for device in devices {
        let Some(info) = describe(&device) else {
            continue;
        };
        out.push(info);
    }
    Ok(out)
}

fn describe(device: &cpal::Device) -> Option<OutputDeviceInfo> {
    let name = device.description().ok()?.name().to_owned();
    let default = device.default_output_config().ok()?;
    let max_channels = device
        .supported_output_configs()
        .ok()?
        .map(|range| range.channels())
        .max()
        .unwrap_or(default.channels());
    Some(OutputDeviceInfo {
        name,
        max_channels,
        default_sample_rate: default.sample_rate(),
    })
}

#[derive(Default)]
struct Counters {
    xruns: AtomicU64,
    stream_errors: AtomicU64,
}

pub struct CpalAudioDevice {
    device: cpal::Device,
    name: String,
    config: StreamConfig,
    block: usize,
    pairs: OutputPairs,
    monitor: Arc<LatencyMonitor>,
    counters: Arc<Counters>,
    stream: Option<Stream>,
    /// Receives the engine back from the stream's render state when the stream is dropped.
    handback: Option<rtrb::Consumer<Box<dyn AudioCallback>>>,
}

impl CpalAudioDevice {
    /// Opens the requested output on the default host. Fails when no device matches, when the
    /// device has no f32 configuration for the request, or when it has more than
    /// [`MAX_DEVICE_CHANNELS`] channels. Does not start the stream.
    pub fn open(request: &OutputRequest) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = pick_device(&host, request.device.as_deref())?;
        let name = device
            .description()
            .map_err(|e| AudioError::Stream(e.to_string()))?
            .name()
            .to_owned();
        let default = device
            .default_output_config()
            .map_err(|e| AudioError::Unsupported(e.to_string()))?;
        let channels = request.channels.unwrap_or(default.channels());
        if channels == 0 || channels as usize > MAX_DEVICE_CHANNELS {
            return Err(AudioError::Unsupported(format!("{channels} channels")));
        }
        let rate = request.sample_rate.unwrap_or(default.sample_rate());
        let range = device
            .supported_output_configs()
            .map_err(|e| AudioError::Unsupported(e.to_string()))?
            .find(|r| {
                r.channels() == channels
                    && r.sample_format() == cpal::SampleFormat::F32
                    && (r.min_sample_rate()..=r.max_sample_rate()).contains(&rate)
            })
            .ok_or_else(|| {
                AudioError::Unsupported(format!("no f32 config for {channels} ch at {rate} Hz"))
            })?;
        let supported = range.with_sample_rate(rate);
        let (buffer_size, block) = match (request.buffer_size, supported.buffer_size()) {
            (Some(frames), cpal::SupportedBufferSize::Range { min, max })
                if (*min..=*max).contains(&frames) =>
            {
                (BufferSize::Fixed(frames), frames as usize)
            }
            (Some(frames), _) => {
                tracing::warn!(
                    frames,
                    "buffer size not supported, using the device default"
                );
                (BufferSize::Default, DEFAULT_BLOCK)
            }
            (None, _) => (BufferSize::Default, DEFAULT_BLOCK),
        };
        let config = StreamConfig {
            channels,
            sample_rate: rate,
            buffer_size,
        };
        let (_, pairs) = OutputRouter::new();
        Ok(Self {
            device,
            name,
            config,
            block,
            pairs,
            monitor: Arc::new(LatencyMonitor::default()),
            counters: Arc::new(Counters::default()),
            stream: None,
            handback: None,
        })
    }

    /// Control handle for the master and cue pairs. Settings survive a stop/start.
    pub fn output_pairs(&self) -> OutputPairs {
        self.pairs.clone()
    }

    /// Xruns reported by the backend (cpal `ErrorKind::Xrun`).
    pub fn xrun_count(&self) -> u64 {
        self.counters.xruns.load(Relaxed)
    }

    /// Other backend stream errors since the device was opened.
    pub fn stream_error_count(&self) -> u64 {
        self.counters.stream_errors.load(Relaxed)
    }
}

fn pick_device(host: &cpal::Host, name: Option<&str>) -> Result<cpal::Device, AudioError> {
    match name {
        None => host.default_output_device().ok_or(AudioError::NoDevice),
        Some(wanted) => host
            .output_devices()
            .map_err(|e| AudioError::Stream(e.to_string()))?
            .find(|d| d.description().is_ok_and(|desc| desc.name() == wanted))
            .ok_or(AudioError::NoDevice),
    }
}

/// Everything the cpal data callback needs. Owned by the stream closure. Dropping it (on stop)
/// hands the engine back through `handback` instead of releasing it, so `stop` can return it.
struct Render {
    router: OutputRouter,
    engine: Box<dyn AudioCallback>,
    scratch: Vec<Vec<f32>>,
    block: usize,
    channels: usize,
    monitor: Arc<LatencyMonitor>,
    handback: rtrb::Producer<Box<dyn AudioCallback>>,
    sample_rate: f64,
    /// Frame count the monitor's budget was last configured for.
    configured_frames: usize,
}

/// Stand-in left in a dropped `Render`. Zero-sized, so boxing it does not allocate.
struct Idle;

impl AudioCallback for Idle {
    fn process(&mut self, _: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for out in outputs.iter_mut() {
            out.fill(0.0);
        }
    }
}

impl Render {
    /// Audio thread. Renders `data` (interleaved) in blocks of at most `self.block` frames.
    fn run(&mut self, data: &mut [f32]) {
        let started = Instant::now();
        let channels = self.channels;
        let frames = data.len() / channels;
        if frames > 0 && frames != self.configured_frames {
            // With the backend's default buffer size the real callback size is only known here.
            self.monitor.configure(self.sample_rate, frames);
            self.configured_frames = frames;
        }
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(self.block);
            {
                let Render {
                    router,
                    engine,
                    scratch,
                    ..
                } = self;
                let mut outs: [&mut [f32]; MAX_DEVICE_CHANNELS] =
                    std::array::from_fn(|_| <&mut [f32]>::default());
                for (slot, buf) in outs.iter_mut().zip(scratch.iter_mut()) {
                    *slot = &mut buf[..n];
                }
                router.process(engine.as_mut(), &mut outs[..channels], n);
            }
            for (c, buf) in self.scratch.iter().enumerate() {
                for (i, sample) in buf[..n].iter().enumerate() {
                    data[(done + i) * channels + c] = *sample;
                }
            }
            done += n;
        }
        let elapsed_us = started.elapsed().as_secs_f64() * 1.0e6;
        self.monitor.record_callback(elapsed_us, frames);
    }
}

impl Drop for Render {
    fn drop(&mut self) {
        // Hands the engine to the device; the push and the placeholder do not allocate.
        let engine = std::mem::replace(&mut self.engine, Box::new(Idle));
        let _ = self.handback.push(engine);
    }
}

impl AudioDevice for CpalAudioDevice {
    fn start(&mut self, mut callback: Box<dyn AudioCallback>) -> Result<(), AudioError> {
        if self.stream.is_some() {
            return Err(AudioError::AlreadyRunning);
        }
        let sample_rate = f64::from(self.config.sample_rate);
        let channels = self.config.channels as usize;
        callback.prepare(sample_rate, self.block);
        let mut router = OutputRouter::with_pairs(&self.pairs);
        router.prepare(self.block);
        self.monitor.reset();
        self.monitor.configure(sample_rate, self.block);

        let (handback_tx, handback_rx) = rtrb::RingBuffer::new(1);
        let mut render = Render {
            router,
            engine: callback,
            scratch: vec![vec![0.0; self.block]; channels],
            block: self.block,
            channels,
            monitor: Arc::clone(&self.monitor),
            handback: handback_tx,
            sample_rate,
            configured_frames: self.block,
        };
        let counters = Arc::clone(&self.counters);
        let stream = self
            .device
            .build_output_stream::<f32, _, _>(
                self.config,
                move |data: &mut [f32], _: &OutputCallbackInfo| render.run(data),
                move |err: cpal::Error| match err.kind() {
                    ErrorKind::Xrun => {
                        counters.xruns.fetch_add(1, Relaxed);
                    }
                    _ => {
                        counters.stream_errors.fetch_add(1, Relaxed);
                        tracing::warn!(error = %err, "audio stream error");
                    }
                },
                None,
            )
            .map_err(|e| AudioError::Stream(e.to_string()))?;
        stream
            .play()
            .map_err(|e| AudioError::Stream(e.to_string()))?;
        self.stream = Some(stream);
        self.handback = Some(handback_rx);
        Ok(())
    }

    fn stop(&mut self) -> Option<Box<dyn AudioCallback>> {
        // Dropping the stream stops the backend and drops its render state, which hands the engine
        // back. `None` only if the backend kept the closure alive.
        let stream = self.stream.take()?;
        let _ = stream.pause();
        drop(stream);
        self.handback.take()?.pop().ok()
    }

    fn is_running(&self) -> bool {
        self.stream.is_some()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn sample_rate(&self) -> f64 {
        f64::from(self.config.sample_rate)
    }

    fn buffer_size(&self) -> usize {
        // The configured size, which becomes the backend's real callback size after the first callback.
        self.monitor.stats().buffer_size
    }

    fn latency_monitor(&self) -> &LatencyMonitor {
        &self.monitor
    }
}

impl Drop for CpalAudioDevice {
    fn drop(&mut self) {
        // The device is going away, so nothing will start the callback again: release it.
        if let Some(mut callback) = self.stop() {
            callback.release();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes channel c as the constant c + 1, so routing is visible in the interleaved output.
    struct Constants;

    impl AudioCallback for Constants {
        fn process(&mut self, _: &[&[f32]], outputs: &mut [&mut [f32]]) {
            for (c, out) in outputs.iter_mut().enumerate() {
                out.fill((c + 1) as f32);
            }
        }
    }

    fn render(channels: usize, block: usize, pairs: &OutputPairs, frames: usize) -> Vec<f32> {
        let mut router = OutputRouter::with_pairs(pairs);
        router.prepare(block);
        let mut render = Render {
            router,
            engine: Box::new(Constants),
            scratch: vec![vec![0.0; block]; channels],
            block,
            channels,
            monitor: Arc::new(LatencyMonitor::default()),
            handback: rtrb::RingBuffer::new(1).0,
            sample_rate: 48_000.0,
            configured_frames: block,
        };
        let mut data = vec![-9.0_f32; frames * channels];
        render.run(&mut data);
        data
    }

    #[test]
    fn interleaves_master_and_cue_pairs_without_a_device() {
        let (_, pairs) = OutputRouter::new();
        // 4 channels, 100 frames in 32-frame blocks: the last block is short.
        let data = render(4, 32, &pairs, 100);
        for frame in data.chunks(4) {
            assert_eq!(frame, [1.0, 2.0, 3.0, 4.0]);
        }
    }

    #[test]
    fn budget_follows_the_frames_the_backend_delivers() {
        let (_, pairs) = OutputRouter::new();
        let mut router = OutputRouter::with_pairs(&pairs);
        router.prepare(512);
        let monitor = Arc::new(LatencyMonitor::default());
        // `start` assumes the 512-frame default when the backend picks the buffer size.
        monitor.configure(48_000.0, 512);
        let (handback, _) = rtrb::RingBuffer::new(1);
        let mut render = Render {
            router,
            engine: Box::new(Constants),
            scratch: vec![vec![0.0; 512]; 2],
            block: 512,
            channels: 2,
            monitor: Arc::clone(&monitor),
            handback,
            sample_rate: 48_000.0,
            configured_frames: 512,
        };
        // The backend now delivers 256-frame callbacks.
        let mut data = vec![0.0_f32; 256 * 2];
        render.run(&mut data);
        let stats = monitor.stats();
        assert_eq!(stats.buffer_size, 256);
        assert!((stats.budget_us - 1.0e6 * 256.0 / 48_000.0).abs() < 1e-6);
    }

    #[test]
    fn two_channel_output_carries_master_only() {
        let (_, pairs) = OutputRouter::new();
        let data = render(2, 64, &pairs, 10);
        for frame in data.chunks(2) {
            assert_eq!(frame, [1.0, 2.0]);
        }
    }

    #[test]
    fn engine_is_handed_back_when_the_render_state_drops() {
        struct Flag(Arc<std::sync::atomic::AtomicBool>);
        impl AudioCallback for Flag {
            fn process(&mut self, _: &[&[f32]], outputs: &mut [&mut [f32]]) {
                for out in outputs.iter_mut() {
                    out.fill(0.0);
                }
            }
            fn release(&mut self) {
                self.0.store(true, Relaxed);
            }
        }
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (_, pairs) = OutputRouter::new();
        let mut router = OutputRouter::with_pairs(&pairs);
        router.prepare(16);
        let (handback_tx, mut handback_rx) = rtrb::RingBuffer::new(1);
        let render = Render {
            router,
            engine: Box::new(Flag(Arc::clone(&released))),
            scratch: vec![vec![0.0; 16]; 2],
            block: 16,
            channels: 2,
            monitor: Arc::new(LatencyMonitor::default()),
            handback: handback_tx,
            sample_rate: 48_000.0,
            configured_frames: 16,
        };
        drop(render);
        assert!(
            !released.load(Relaxed),
            "dropping the render state does not release"
        );
        let mut engine = handback_rx.pop().expect("the engine is handed back");
        engine.release();
        assert!(released.load(Relaxed));
    }
}
