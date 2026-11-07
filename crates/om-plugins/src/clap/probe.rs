//! Reads what a plugin declares (ports, parameters, latency) on its control thread.

use super::params::ClapParam;
use super::ports::{AudioPort, PortLayout, PortRole};
use super::worker::Ctx;
use clack_extensions::audio_ports::{AudioPortFlags, AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::latency::PluginLatency;
use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};

/// The plugin's audio ports; empty when it has no `audio-ports` extension.
pub(super) fn read_layout(ctx: &mut Ctx) -> PortLayout {
    let handle = ctx.instance.plugin_handle();
    let Some(ext) = handle.get_extension::<PluginAudioPorts>() else {
        return PortLayout::default();
    };
    let mut buffer = AudioPortInfoBuffer::new();
    let mut side = |is_input: bool| {
        (0..ext.count(&handle, is_input))
            .filter_map(|i| {
                let info = ext.get(&handle, i, is_input, &mut buffer)?;
                Some(AudioPort {
                    id: info.id.get(),
                    name: String::from_utf8_lossy(info.name).into_owned(),
                    channels: info.channel_count,
                    role: if info.flags.contains(AudioPortFlags::IS_MAIN) {
                        PortRole::Main
                    } else {
                        PortRole::Sidechain
                    },
                })
            })
            .collect()
    };
    PortLayout {
        inputs: side(true),
        outputs: side(false),
    }
}

/// The plugin's parameters with their current values; empty without the `params` extension.
pub(super) fn read_params(ctx: &mut Ctx) -> Vec<(ClapParam, f64)> {
    let handle = ctx.instance.plugin_handle();
    let Some(ext) = handle.get_extension::<PluginParams>() else {
        return Vec::new();
    };
    let mut buffer = ParamInfoBuffer::new();
    (0..ext.count(&handle))
        .filter_map(|i| {
            let info = ext.get_info(&handle, i, &mut buffer)?;
            let param = ClapParam {
                id: info.id.get(),
                name: String::from_utf8_lossy(info.name).into_owned(),
                module: String::from_utf8_lossy(info.module).into_owned(),
                min: info.min_value,
                max: info.max_value,
                default: info.default_value,
                stepped: info.flags.contains(ParamInfoFlags::IS_STEPPED),
                hidden: info.flags.contains(ParamInfoFlags::IS_HIDDEN),
                read_only: info.flags.contains(ParamInfoFlags::IS_READONLY),
            };
            let value = ext
                .get_value(&handle, info.id)
                .unwrap_or(info.default_value);
            Some((param, value))
        })
        .collect()
}

/// Latency in frames, 0 without the `latency` extension. Valid after activation.
pub(super) fn read_latency(ctx: &mut Ctx) -> u32 {
    let mut handle = ctx.instance.plugin_handle();
    handle
        .get_extension::<PluginLatency>()
        .map_or(0, |ext| ext.get(&mut handle))
}
