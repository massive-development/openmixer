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
use clack_plugin::events::spaces::CoreEventSpace;
use clack_plugin::prelude::*;
use std::ffi::CStr;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};

const PARAM_GAIN: u32 = 1;
/// Not 1.0, so a test can tell a processed block from a passed-through one.
const DEFAULT_GAIN: f32 = 0.5;
const LATENCY_FRAMES: u32 = 16;

pub struct GainFixture;

/// State the main thread and the audio processor share: the gain as `f32` bits.
pub struct Shared {
    gain: AtomicU32,
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
            .register::<PluginLatency>();
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
        })
    }

    fn new_main_thread<'a>(
        _host: HostMainThreadHandle<'a>,
        shared: &'a Shared,
    ) -> Result<GainMainThread<'a>, PluginError> {
        Ok(GainMainThread { shared })
    }
}

pub struct GainMainThread<'a> {
    shared: &'a Shared,
}

impl<'a> PluginMainThread<'a, Shared> for GainMainThread<'a> {}

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

impl PluginLatencyImpl for GainMainThread<'_> {
    fn get(&self) -> u32 {
        LATENCY_FRAMES
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
        Some(GuiSize {
            width: 320,
            height: 200,
        })
    }

    fn set_size(&self, _size: GuiSize) -> Result<(), PluginError> {
        Ok(())
    }

    fn set_parent(&self, _window: Window) -> Result<(), PluginError> {
        Ok(())
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
}

impl<'a> PluginAudioProcessor<'a, Shared, GainMainThread<'a>> for GainProcessor<'a> {
    fn activate(
        _host: HostAudioProcessorHandle<'a>,
        _main_thread: &GainMainThread<'a>,
        shared: &'a Shared,
        _audio_config: PluginAudioConfiguration,
    ) -> Result<Self, PluginError> {
        Ok(Self { shared })
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
