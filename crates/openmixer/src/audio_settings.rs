//! The audio settings model: port of `src/ui/JuceAudioSettingsModel.cpp` over the om-audio output
//! backends, plus the device restore that `AppPreferencesTargets` asks for at start-up.
//!
//! [`OutputSettings`] owns the open device and its callback. A change (device type, output device,
//! rate, buffer) stops the device, takes the callback back, opens the new setup and starts the
//! callback again. If the new setup fails the previous one is reopened, as the C++ setup call
//! leaves the device unchanged on error. Every output channel of the device is opened (up to
//! [`MAX_OUTPUTS`]), so the master and cue pair choice happens in the routing, not in the device.
//!
//! Tests use [`MockBackend`] only. [`CpalBackend`] is the real one, used by the binary.

use crate::app_targets::AudioDeviceControl;
use om_audio::{
    default_output_name, hosts, output_devices_on, AudioCallback, AudioDevice, AudioError,
    CpalAudioDevice, LatencyStats, MockAudioDevice, OutputDeviceInfo, OutputPairs, OutputRequest,
    OutputRouter,
};
use om_ui::settings::{AudioSettingsModel, AudioStats, SettingsResult};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Outputs the device is opened with (C++ `JuceAudioSettingsModel::kMaxOutputs`).
pub const MAX_OUTPUTS: u16 = 8;

/// Enumerates and opens output devices. Listing must not open anything.
pub trait OutputBackend: Send {
    /// Device types (hosts), the default first.
    fn hosts(&self) -> Vec<String>;
    /// The default output device of `host`, if it has one.
    fn default_device(&self, host: &str) -> Option<String>;
    fn devices(&self, host: &str) -> Result<Vec<OutputDeviceInfo>, AudioError>;
    /// Opens the device described by `request`, routing through `pairs`. Does not start it.
    fn open(
        &self,
        request: &OutputRequest,
        pairs: &OutputPairs,
    ) -> Result<Box<dyn AudioDevice>, AudioError>;
}

/// The cpal backend the binary uses.
pub struct CpalBackend;

impl OutputBackend for CpalBackend {
    fn hosts(&self) -> Vec<String> {
        hosts()
    }

    fn default_device(&self, host: &str) -> Option<String> {
        default_output_name(Some(host))
    }

    fn devices(&self, host: &str) -> Result<Vec<OutputDeviceInfo>, AudioError> {
        output_devices_on(Some(host))
    }

    fn open(
        &self,
        request: &OutputRequest,
        pairs: &OutputPairs,
    ) -> Result<Box<dyn AudioDevice>, AudioError> {
        let device = CpalAudioDevice::open_with_pairs(request, pairs.clone())?;
        Ok(Box::new(device))
    }
}

/// A fixed set of devices with no hardware. Tests drive [`OutputSettings`] over it.
pub struct MockBackend {
    pub host: String,
    pub devices: Vec<OutputDeviceInfo>,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self {
            host: "Mock".to_owned(),
            devices: vec![
                OutputDeviceInfo {
                    name: "Mock Output".to_owned(),
                    max_channels: 4,
                    default_sample_rate: 48_000,
                    sample_rates: vec![44_100, 48_000, 96_000],
                    buffer_sizes: vec![128, 256, 512, 1024],
                },
                OutputDeviceInfo {
                    name: "Mock Stereo".to_owned(),
                    max_channels: 2,
                    default_sample_rate: 44_100,
                    sample_rates: vec![44_100, 48_000],
                    buffer_sizes: vec![256, 512],
                },
            ],
        }
    }
}

impl OutputBackend for MockBackend {
    fn hosts(&self) -> Vec<String> {
        vec![self.host.clone()]
    }

    fn default_device(&self, host: &str) -> Option<String> {
        (host == self.host)
            .then(|| self.devices.first().map(|d| d.name.clone()))
            .flatten()
    }

    fn devices(&self, host: &str) -> Result<Vec<OutputDeviceInfo>, AudioError> {
        if host == self.host {
            Ok(self.devices.clone())
        } else {
            Err(AudioError::NoDevice)
        }
    }

    fn open(
        &self,
        request: &OutputRequest,
        _pairs: &OutputPairs,
    ) -> Result<Box<dyn AudioDevice>, AudioError> {
        if request.host.as_deref().is_some_and(|h| h != self.host) {
            return Err(AudioError::NoDevice);
        }
        let info = self
            .devices
            .iter()
            .find(|d| request.device.as_deref().is_none_or(|n| n == d.name))
            .ok_or(AudioError::NoDevice)?;
        let rate = request.sample_rate.unwrap_or(info.default_sample_rate);
        if !info.sample_rates.contains(&rate) {
            return Err(AudioError::Unsupported(format!("{rate} Hz")));
        }
        let buffer = request.buffer_size.unwrap_or(512);
        if !info.buffer_sizes.contains(&buffer) {
            return Err(AudioError::Unsupported(format!("{buffer} samples")));
        }
        let channels = request.channels.unwrap_or(info.max_channels);
        if channels == 0 || channels > info.max_channels {
            return Err(AudioError::Unsupported(format!("{channels} channels")));
        }
        Ok(Box::new(Paced {
            inner: MockAudioDevice::new(f64::from(rate), buffer as usize, 0, usize::from(channels)),
            name: info.name.clone(),
        }))
    }
}

/// The mock, paced in real time into memory when started. Nothing reaches a sound card.
struct Paced {
    inner: MockAudioDevice,
    name: String,
}

impl AudioDevice for Paced {
    fn start(&mut self, callback: Box<dyn AudioCallback>) -> Result<(), AudioError> {
        self.inner.start(callback)?;
        self.inner.start_realtime();
        Ok(())
    }

    fn stop(&mut self) -> Option<Box<dyn AudioCallback>> {
        self.inner.stop_realtime();
        self.inner.stop()
    }

    fn is_running(&self) -> bool {
        self.inner.is_running()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn sample_rate(&self) -> f64 {
        self.inner.sample_rate()
    }

    fn buffer_size(&self) -> usize {
        self.inner.buffer_size()
    }

    fn latency_monitor(&self) -> &om_audio::LatencyMonitor {
        self.inner.latency_monitor()
    }
}

/// The setup the open device was made with, fully resolved (no defaults left).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Current {
    pub host: String,
    pub device: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub channels: u16,
}

impl Current {
    /// The stored form (`AudioPrefs::device_state`).
    pub fn encode(&self) -> String {
        json!({
            "host": self.host,
            "device": self.device,
            "sampleRate": self.sample_rate,
            "bufferSize": self.buffer_size,
            "channels": self.channels,
        })
        .to_string()
    }

    /// Parses the stored form. `None` for anything else, including an empty string.
    pub fn decode(state: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(state).ok()?;
        let num = |key: &str| v.get(key).and_then(Value::as_u64);
        let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(Self {
            host: text("host")?,
            device: text("device")?,
            sample_rate: u32::try_from(num("sampleRate")?).ok()?,
            buffer_size: u32::try_from(num("bufferSize")?).ok()?,
            channels: u16::try_from(num("channels")?).ok()?,
        })
    }

    fn request(&self) -> OutputRequest {
        OutputRequest {
            host: Some(self.host.clone()),
            device: Some(self.device.clone()),
            sample_rate: Some(self.sample_rate),
            buffer_size: Some(self.buffer_size),
            channels: Some(self.channels),
        }
    }
}

/// The open output and everything the settings page reads from it.
pub struct OutputSettings {
    backend: Box<dyn OutputBackend>,
    pairs: OutputPairs,
    device: Option<Box<dyn AudioDevice>>,
    current: Option<Current>,
    /// Devices of the selected host, refreshed when the host changes and by [`Self::refresh`].
    listing: Vec<OutputDeviceInfo>,
}

impl OutputSettings {
    /// Creates the model with no device open. `pairs` is the routing handle the device owner
    /// reads; it survives device changes because each device is opened through it.
    pub fn new(backend: Box<dyn OutputBackend>) -> Self {
        Self {
            backend,
            pairs: OutputRouter::new().1,
            device: None,
            current: None,
            listing: Vec::new(),
        }
    }

    /// The routing handle shared by every device this model opens.
    pub fn pairs(&self) -> OutputPairs {
        self.pairs.clone()
    }

    pub fn device(&self) -> Option<&dyn AudioDevice> {
        self.device.as_deref()
    }

    pub fn current(&self) -> Option<&Current> {
        self.current.as_ref()
    }

    /// Re-reads the device list of the selected host (called when the settings panel opens).
    pub fn refresh(&mut self) {
        let host = self.selected_host();
        self.listing = self.backend.devices(&host).unwrap_or_default();
    }

    /// Opens the stored setup when `saved` decodes and opens, else the default output. Returns
    /// the error of the default open when both fail.
    pub fn start_up(&mut self, saved: &str) -> Result<(), String> {
        if let Some(stored) = Current::decode(saved) {
            if self.change(stored.request()).is_ok() {
                return Ok(());
            }
        }
        let host = self.selected_host();
        let device = self.backend.default_device(&host);
        let request = self.request_for(&host, device, None, None);
        self.change(request)
    }

    /// Starts the callback on the open device. Fails when no device is open.
    pub fn start(&mut self, callback: Box<dyn AudioCallback>) -> Result<(), String> {
        let device = self.device.as_mut().ok_or("no audio device is open")?;
        device.start(callback).map_err(|e| e.to_string())
    }

    /// Stops the device and hands the callback back, for the owner to release or restart.
    pub fn stop(&mut self) -> Option<Box<dyn AudioCallback>> {
        self.device.as_mut().and_then(|d| d.stop())
    }

    /// The latency and load readout for the settings page. Zero sample rate when nothing is open.
    pub fn stats(&self) -> AudioStats {
        match self.device.as_deref() {
            None => AudioStats::default(),
            Some(device) => to_stats(&device.latency_stats()),
        }
    }

    /// Output channels of the open device; 0 when none is open.
    pub fn output_channels(&self) -> usize {
        self.current.as_ref().map_or(0, |c| usize::from(c.channels))
    }

    /// The host the settings name, the open one or else the backend default.
    fn selected_host(&self) -> String {
        match &self.current {
            Some(c) => c.host.clone(),
            None => self.backend.hosts().into_iter().next().unwrap_or_default(),
        }
    }

    fn info_for(&self, name: &str) -> Option<&OutputDeviceInfo> {
        self.listing.iter().find(|d| d.name == name)
    }

    /// Request for `host` and `device` (the default device when `None`), with the rate and buffer
    /// asked for or the device defaults. Every output of the device is requested.
    fn request_for(
        &self,
        host: &str,
        device: Option<String>,
        rate: Option<u32>,
        buffer: Option<u32>,
    ) -> OutputRequest {
        let channels = device
            .as_deref()
            .and_then(|name| self.info_for(name))
            .map(|info| info.max_channels.min(MAX_OUTPUTS));
        OutputRequest {
            host: Some(host.to_owned()),
            device,
            sample_rate: rate,
            buffer_size: buffer,
            channels,
        }
    }

    /// Stops the open device, opens `request` and restarts the callback there. On failure the
    /// previous setup is reopened and the error is returned.
    fn change(&mut self, request: OutputRequest) -> Result<(), String> {
        let host = request.host.clone().unwrap_or_default();
        if self.current.as_ref().map(|c| c.host.as_str()) != Some(host.as_str()) {
            self.listing = self.backend.devices(&host).unwrap_or_default();
        }
        let previous = self.current.clone();
        let callback = self.stop();
        self.device = None;
        self.current = None;
        match self.open(&request, callback) {
            Ok(()) => Ok(()),
            Err((error, callback)) => {
                let restored = match previous {
                    Some(previous) => self.open(&previous.request(), callback),
                    None => Err((String::new(), callback)),
                };
                match restored {
                    Ok(()) => Err(error),
                    Err((again, callback)) => {
                        if let Some(mut callback) = callback {
                            callback.release();
                        }
                        Err(format!(
                            "{error}; restoring the previous setup failed: {again}"
                        ))
                    }
                }
            }
        }
    }

    /// Opens `request` and, when a callback is given, starts it. On failure the callback comes back.
    fn open(
        &mut self,
        request: &OutputRequest,
        callback: Option<Box<dyn AudioCallback>>,
    ) -> Result<(), (String, Option<Box<dyn AudioCallback>>)> {
        let mut device = match self.backend.open(request, &self.pairs) {
            Ok(device) => device,
            Err(error) => return Err((error.to_string(), callback)),
        };
        if let Some(callback) = callback {
            if let Err(error) = device.start(callback) {
                return Err((error.to_string(), None));
            }
        }
        self.current = Some(Current {
            host: request.host.clone().unwrap_or_default(),
            device: device.name(),
            sample_rate: device.sample_rate().round() as u32,
            buffer_size: u32::try_from(device.buffer_size()).unwrap_or(u32::MAX),
            channels: request.channels.unwrap_or(0),
        });
        self.device = Some(device);
        Ok(())
    }
}

fn to_stats(s: &LatencyStats) -> AudioStats {
    AudioStats {
        sample_rate: s.sample_rate,
        buffer_size: s.buffer_size,
        output_latency_ms: s.output_latency_ms,
        round_trip_latency_ms: s.round_trip_latency_ms,
        cpu_load: s.cpu_load,
        max_callback_us: s.max_callback_us,
        budget_us: s.budget_us,
        xrun_count: s.xrun_count,
    }
}

impl AudioDeviceControl for OutputSettings {
    fn current_state(&self) -> String {
        self.current
            .as_ref()
            .map(Current::encode)
            .unwrap_or_default()
    }

    fn initialise(&mut self, state: &str) -> Result<(), String> {
        let stored = Current::decode(state).ok_or("unreadable audio device state")?;
        self.change(stored.request())
    }
}

/// The settings page's view of an [`OutputSettings`] shared with the window and the readout.
#[derive(Clone)]
pub struct SharedOutput(pub Arc<Mutex<OutputSettings>>);

impl SharedOutput {
    pub fn new(settings: OutputSettings) -> Self {
        Self(Arc::new(Mutex::new(settings)))
    }

    pub fn lock(&self) -> MutexGuard<'_, OutputSettings> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl AudioSettingsModel for SharedOutput {
    fn device_types(&self) -> Vec<String> {
        let settings = self.lock();
        settings.backend.hosts()
    }

    fn device_type(&self) -> String {
        self.lock().selected_host()
    }

    fn set_device_type(&mut self, device_type: &str) -> SettingsResult<()> {
        let mut settings = self.lock();
        if !settings.backend.hosts().iter().any(|h| h == device_type) {
            return Err(format!("unknown device type '{device_type}'"));
        }
        let request = settings.request_for(device_type, None, None, None);
        settings.change(request)
    }

    fn output_devices(&self) -> Vec<String> {
        self.lock().listing.iter().map(|d| d.name.clone()).collect()
    }

    fn output_device(&self) -> String {
        self.lock()
            .current
            .as_ref()
            .map(|c| c.device.clone())
            .unwrap_or_default()
    }

    fn set_output_device(&mut self, name: &str) -> SettingsResult<()> {
        let mut settings = self.lock();
        let host = settings.selected_host();
        let request = settings.request_for(&host, Some(name.to_owned()), None, None);
        settings.change(request)
    }

    fn sample_rates(&self) -> Vec<u32> {
        let settings = self.lock();
        let name = settings.current.as_ref().map(|c| c.device.clone());
        name.and_then(|n| settings.info_for(&n).map(|i| i.sample_rates.clone()))
            .unwrap_or_default()
    }

    fn sample_rate(&self) -> u32 {
        self.lock().current.as_ref().map_or(0, |c| c.sample_rate)
    }

    fn set_sample_rate(&mut self, rate: u32) -> SettingsResult<()> {
        let mut settings = self.lock();
        let current = settings.current.clone().ok_or("no audio device is open")?;
        let request = settings.request_for(
            &current.host,
            Some(current.device),
            Some(rate),
            Some(current.buffer_size),
        );
        settings.change(request)
    }

    fn buffer_sizes(&self) -> Vec<u32> {
        let settings = self.lock();
        let name = settings.current.as_ref().map(|c| c.device.clone());
        name.and_then(|n| settings.info_for(&n).map(|i| i.buffer_sizes.clone()))
            .unwrap_or_default()
    }

    fn buffer_size(&self) -> u32 {
        self.lock().current.as_ref().map_or(0, |c| c.buffer_size)
    }

    fn set_buffer_size(&mut self, samples: u32) -> SettingsResult<()> {
        let mut settings = self.lock();
        let current = settings.current.clone().ok_or("no audio device is open")?;
        let request = settings.request_for(
            &current.host,
            Some(current.device),
            Some(current.sample_rate),
            Some(samples),
        );
        settings.change(request)
    }

    fn output_channels(&self) -> usize {
        self.lock().output_channels()
    }

    fn state_string(&self) -> String {
        self.lock().current_state()
    }
}
