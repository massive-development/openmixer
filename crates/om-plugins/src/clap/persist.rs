//! Saves and restores a plugin instance's state through the CLAP `state` extension (ADR 0007,
//! step 6). Both run on the instance's control thread. The bytes are the plugin's own and opaque to
//! the host; the app stores them, versioned, beside the preferences (`openmixer::plugin_state`).

use super::param_bus::ParamSet;
use super::worker::Ctx;
use crate::{PluginError, MAX_STATE_BYTES};
use clack_extensions::params::PluginParams;
use clack_extensions::state::PluginState;
use clack_host::prelude::ClapId;
use std::io::{self, Write};

/// A `Write` into a vector that refuses to grow past [`MAX_STATE_BYTES`].
struct Capped(Vec<u8>);

impl Write for Capped {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.0.len() + buf.len() > MAX_STATE_BYTES {
            return Err(io::Error::other("plugin state is too large"));
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn error(plugin: &str, reason: impl std::fmt::Display) -> PluginError {
    PluginError::State {
        plugin: plugin.to_owned(),
        reason: reason.to_string(),
    }
}

/// The instance's state; `None` when it has no `state` extension.
pub(super) fn save(ctx: &mut Ctx, plugin: &str) -> Result<Option<Vec<u8>>, PluginError> {
    let handle = ctx.instance.plugin_handle();
    let Some(ext) = handle.get_extension::<PluginState>() else {
        return Ok(None);
    };
    let mut out = Capped(Vec::new());
    ext.save(&handle, &mut out).map_err(|e| error(plugin, e))?;
    Ok(Some(out.0))
}

/// Loads `bytes` into the instance, then reads every parameter back into `params`, so the host's
/// snapshot (and the values a new stage sends on its first block) match what the state set.
pub(super) fn load(
    ctx: &mut Ctx,
    plugin: &str,
    bytes: &[u8],
    params: &ParamSet,
) -> Result<(), PluginError> {
    if bytes.len() > MAX_STATE_BYTES {
        return Err(error(plugin, "plugin state is too large"));
    }
    let handle = ctx.instance.plugin_handle();
    let Some(ext) = handle.get_extension::<PluginState>() else {
        return Err(error(plugin, "the plugin has no state extension"));
    };
    let mut reader = bytes;
    ext.load(&handle, &mut reader)
        .map_err(|e| error(plugin, e))?;
    if let Some(param_ext) = handle.get_extension::<PluginParams>() {
        for (index, p) in params.params.iter().enumerate() {
            let value = ClapId::from_raw(p.id).and_then(|id| param_ext.get_value(&handle, id));
            if let Some(value) = value {
                params.store_from_plugin(index, value);
            }
        }
    }
    Ok(())
}
