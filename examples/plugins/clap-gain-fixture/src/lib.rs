//! A CLAP gain effect for the host tests: stereo in, stereo out, one parameter (`gain`, 0 to 2, default 0.5)
//! and a GUI stub that records nothing but answers every call. The host tests build it as a shared library.

use clack_extensions::audio_ports::{
    AudioPortFlags, AudioPortInfo, AudioPortInfoWriter, AudioPortType, PluginAudioPorts,
    PluginAudioPortsImpl,
};
use clack_extensions::gui::{
    GuiApiType, GuiConfiguration, GuiSize, PluginGui, PluginGuiImpl, Window,
};
use clack_extensions::latency::{PluginLatency, PluginLatencyImpl};
use clack_extensions::params::{
    ParamDisplayWriter, ParamInfo, ParamInfoFlags, ParamInfoWriter, PluginAudioProcessorParams,
    PluginMainThreadParams, PluginParams,
};
use clack_extensions::state::{PluginState, PluginStateImpl};
use clack_plugin::events::spaces::CoreEventSpace;
use clack_plugin::prelude::*;
use clack_plugin::stream::{InputStream, OutputStream};
use std::cell::Cell;
use std::ffi::CStr;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};

const PARAM_GAIN: u32 = 1;
/// Not 1.0, so a test can tell a processed block from a passed-through one.
const DEFAULT_GAIN: f32 = 0.5;
const LATENCY_FRAMES: u32 = 16;
/// `change-latency`: the latency the plugin switches to, and tells the host about, after activation.
#[cfg(feature = "change-latency")]
const CHANGED_LATENCY_FRAMES: u32 = 64;

pub struct GainFixture;

/// State the main thread and the audio processor share: the gain as `f32` bits.
pub struct Shared {
    gain: AtomicU32,
    latency: AtomicU32,
}

impl Shared {
    fn gain(&self) -> f32 {
        f32::from_bits(self.gain.load(Ordering::Relaxed))
    }

    fn set_gain(&self, value: f64) {
        let value = value.clamp(0.0, 2.0) as f32;
        self.gain.store(value.to_bits(), Ordering::Relaxed);
    }
}

impl PluginShared<'_> for Shared {}

impl Plugin for GainFixture {
    type AudioProcessor<'a> = GainProcessor<'a>;
    type Shared<'a> = Shared;
    type MainThread<'a> = GainMainThread<'a>;

    fn declare_extensions(builder: &mut PluginExtensions<Self>, _shared: Option<&Shared>) {
        builder
            .register::<PluginAudioPorts>()
            .register::<PluginParams>()
            .register::<PluginGui>()
            .register::<PluginLatency>()
            .register::<PluginState>();
    }
}

impl DefaultPluginFactory for GainFixture {
    fn get_descriptor() -> PluginDescriptor {
        use clack_plugin::plugin::features::{AUDIO_EFFECT, STEREO, UTILITY};
        PluginDescriptor::new("org.openmixer.test.clap-gain", "OpenMixer Test Gain")
            .with_vendor("OpenMixer")
            .with_version("1.0.0")
            .with_features([AUDIO_EFFECT, UTILITY, STEREO])
    }

    fn new_shared(_host: HostSharedHandle<'_>) -> Result<Shared, PluginError> {
        Ok(Shared {
            gain: AtomicU32::new(DEFAULT_GAIN.to_bits()),
            latency: AtomicU32::new(LATENCY_FRAMES),
        })
    }

    fn new_main_thread<'a>(
        host: HostMainThreadHandle<'a>,
        shared: &'a Shared,
    ) -> Result<GainMainThread<'a>, PluginError> {
        Ok(GainMainThread {
            host,
            shared,
            size: Cell::new((320, 200)),
        })
    }
}

pub struct GainMainThread<'a> {
    #[cfg_attr(not(feature = "change-latency"), allow(dead_code))]
    host: HostMainThreadHandle<'a>,
    shared: &'a Shared,
    /// The stub window's size; `set_size` changes it.
    size: Cell<(u32, u32)>,
}

impl<'a> PluginMainThread<'a, Shared> for GainMainThread<'a> {
    /// `change-latency`: switches to the new latency and signals it through the host's `latency`
    /// extension (asked for by `activate`).
    #[cfg(feature = "change-latency")]
    fn on_main_thread(&self) {
        use clack_extensions::latency::HostLatency;
        self.shared
            .latency
            .store(CHANGED_LATENCY_FRAMES, Ordering::Relaxed);
        if let Some(latency) = self.host.get_extension::<HostLatency>() {
            latency.changed(&self.host);
        }
    }
}

impl PluginAudioPortsImpl for GainMainThread<'_> {
    fn count(&self, _is_input: bool) -> u32 {
        1
    }

    fn get(&self, index: u32, is_input: bool, writer: &mut AudioPortInfoWriter) {
        if index != 0 {
            return;
        }
        writer.set(&AudioPortInfo {
            id: ClapId::new(0),
            name: if is_input { b"in" } else { b"out" },
            channel_count: 2,
            flags: AudioPortFlags::IS_MAIN,
            port_type: Some(AudioPortType::STEREO),
            in_place_pair: None,
        });
    }
}

impl PluginMainThreadParams for GainMainThread<'_> {
    fn count(&self) -> u32 {
        1
    }

    fn get_info(&self, param_index: u32, info: &mut ParamInfoWriter) {
        if param_index != 0 {
            return;
        }
        info.set(&ParamInfo {
            id: ClapId::new(PARAM_GAIN),
            flags: ParamInfoFlags::IS_AUTOMATABLE,
            cookie: Default::default(),
            name: b"Gain",
            module: b"",
            min_value: 0.0,
            max_value: 2.0,
            default_value: f64::from(DEFAULT_GAIN),
        });
    }

    fn get_value(&self, param_id: ClapId) -> Option<f64> {
        (param_id.get() == PARAM_GAIN).then(|| f64::from(self.shared.gain()))
    }

    fn value_to_text(
        &self,
        _param_id: ClapId,
        value: f64,
        writer: &mut ParamDisplayWriter,
    ) -> core::fmt::Result {
        write!(writer, "{value:.2}x")
    }

    fn text_to_value(&self, _param_id: ClapId, text: &CStr) -> Option<f64> {
        text.to_str().ok()?.trim_end_matches('x').parse().ok()
    }

    fn flush(
        &self,
        input_parameter_changes: &InputEvents,
        _output_parameter_changes: &mut OutputEvents,
    ) {
        apply_param_events(self.shared, input_parameter_changes);
    }
}

fn apply_param_events(shared: &Shared, events: &InputEvents) {
    for event in events {
        if let Some(CoreEventSpace::ParamValue(e)) = event.as_core_event() {
            if e.param_id() == Some(ClapId::new(PARAM_GAIN)) {
                shared.set_gain(e.value());
            }
        }
    }
}

/// State layout (version 1): the magic `GAIN`, a version byte, the gain as little-endian `f32`.
const STATE_MAGIC: &[u8; 4] = b"GAIN";
const STATE_VERSION: u8 = 1;

impl PluginStateImpl for GainMainThread<'_> {
    fn save(&self, output: &mut OutputStream) -> Result<(), PluginError> {
        use std::io::Write as _;
        let mut bytes = [0_u8; 9];
        bytes[..4].copy_from_slice(STATE_MAGIC);
        bytes[4] = STATE_VERSION;
        bytes[5..].copy_from_slice(&self.shared.gain().to_le_bytes());
        output.write_all(&bytes)?;
        Ok(())
    }

    fn load(&self, input: &mut InputStream) -> Result<(), PluginError> {
        use std::io::Read as _;
        #[cfg(feature = "reject-state")]
        return Err(PluginError::Message("fixture rejects state"));
        #[allow(unreachable_code)]
        {
            let mut bytes = [0_u8; 9];
            input.read_exact(&mut bytes)?;
            if &bytes[..4] != STATE_MAGIC || bytes[4] != STATE_VERSION {
                return Err(PluginError::Message("not a gain state"));
            }
            let gain = f32::from_le_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
            if !gain.is_finite() {
                return Err(PluginError::Message("gain is not finite"));
            }
            self.shared.set_gain(f64::from(gain));
            Ok(())
        }
    }
}

impl PluginLatencyImpl for GainMainThread<'_> {
    fn get(&self) -> u32 {
        self.shared.latency.load(Ordering::Relaxed)
    }
}

/// Answers every GUI call; the window it "creates" has a fixed size.
impl PluginGuiImpl for GainMainThread<'_> {
    fn is_api_supported(&self, configuration: GuiConfiguration) -> bool {
        configuration.api_type == GuiApiType::X11
    }

    fn get_preferred_api(&self) -> Option<GuiConfiguration<'_>> {
        Some(GuiConfiguration {
            api_type: GuiApiType::X11,
            is_floating: false,
        })
    }

    fn create(&self, configuration: GuiConfiguration) -> Result<(), PluginError> {
        if self.is_api_supported(configuration) {
            Ok(())
        } else {
            Err(PluginError::Message("only x11 is supported"))
        }
    }

    fn destroy(&self) {}

    fn set_scale(&self, _scale: f64) -> Result<(), PluginError> {
        Ok(())
    }

    fn get_size(&self) -> Option<GuiSize> {
        let (width, height) = self.size.get();
        Some(GuiSize { width, height })
    }

    fn set_size(&self, size: GuiSize) -> Result<(), PluginError> {
        if size.width > 2000 || size.height > 2000 {
            return Err(PluginError::Message("window too large"));
        }
        self.size.set((size.width, size.height));
        Ok(())
    }

    fn set_parent(&self, window: Window) -> Result<(), PluginError> {
        // The host must hand over an X11 window; anything else is the host's bug.
        match window.as_x11_handle() {
            Some(id) if id != 0 => Ok(()),
            _ => Err(PluginError::Message("parent is not an X11 window")),
        }
    }

    fn set_transient(&self, _window: Window) -> Result<(), PluginError> {
        Ok(())
    }

    fn show(&self) -> Result<(), PluginError> {
        Ok(())
    }

    fn hide(&self) -> Result<(), PluginError> {
        Ok(())
    }
}

pub struct GainProcessor<'a> {
    shared: &'a Shared,
    /// `emit-param-change`: the change the "GUI" made was already reported.
    #[cfg_attr(not(feature = "emit-param-change"), allow(dead_code))]
    reported: bool,
}

impl<'a> PluginAudioProcessor<'a, Shared, GainMainThread<'a>> for GainProcessor<'a> {
    fn activate(
        host: HostAudioProcessorHandle<'a>,
        _main_thread: &GainMainThread<'a>,
        shared: &'a Shared,
        _audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        #[cfg(feature = "change-latency")]
        host.request_callback();
        #[cfg(not(feature = "change-latency"))]
        let _ = host;
        Ok(Self {
            shared,
            reported: false,
        })
    }

    fn process(
        &mut self,
        _process: Process,
        mut audio: Audio,
        events: Events,
    ) -> Result<ProcessStatus, PluginError> {
        #[cfg(feature = "panic-in-process")]
        panic!("fixture panics in process");
        #[cfg(feature = "error-in-process")]
        return Err(PluginError::Message("fixture fails in process"));

        #[allow(unreachable_code)]
        {
            apply_param_events(self.shared, events.input);
            #[cfg(feature = "emit-param-change")]
            self.report_gui_change(events.output);
            let gain = self.shared.gain();
            for mut pair in &mut audio {
                let Some(channels) = pair.channels()?.into_f32() else {
                    continue;
                };
                for channel in channels {
                    scale(channel, gain);
                }
            }
            Ok(ProcessStatus::ContinueIfNotQuiet)
        }
    }
}

#[cfg(feature = "emit-param-change")]
impl GainProcessor<'_> {
    /// Stands in for the plugin's own GUI moving the knob: sets gain 1.5 and tells the host once.
    fn report_gui_change(&mut self, output: &mut OutputEvents) {
        use clack_plugin::events::event_types::ParamValueEvent;
        if self.reported {
            return;
        }
        self.reported = true;
        self.shared.set_gain(1.5);
        let event = ParamValueEvent::new(0, ClapId::new(PARAM_GAIN), Pckn::match_all(), 1.5);
        let _ = output.try_push(&event);
    }
}

fn scale(channel: ChannelPair<'_, f32>, gain: f32) {
    match channel {
        ChannelPair::InputOnly(_) => {}
        ChannelPair::OutputOnly(out) => out.fill(0.0),
        ChannelPair::InPlace(buf) => buf.iter_mut().for_each(|s| *s *= gain),
        ChannelPair::InputOutput(input, output) => {
            for (i, o) in input.iter().zip(output.iter_mut()) {
                *o = i * gain;
            }
        }
    }
}

impl PluginAudioProcessorParams for GainProcessor<'_> {
    fn flush(&mut self, input: &InputEvents, _output: &mut OutputEvents) {
        apply_param_events(self.shared, input);
    }
}

// SAFETY: the macro exports the `clap_entry` symbol a CLAP host looks up; it holds no other unsafe code.
#[allow(unsafe_code)]
mod export {
    use super::GainFixture;
    use clack_plugin::prelude::*;
    clack_export_entry!(SinglePluginEntry<GainFixture>);
}
