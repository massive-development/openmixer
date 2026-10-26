//! Example C-ABI plugin: a stage (`example.cdylib-gain`) that multiplies the audio by 0.5.
//!
//! Build it as a shared library and drop the `.so`/`.dll`/`.dylib` into the plugin folder. The
//! host finds `openmixer_plugin_entry` and checks the table version (see `om_plugins::abi` and
//! `docs/spikes/DECISIONS.md`, "Plugins: C ABI dynamic loading").
//!
//! The table is mirrored here rather than imported from `om-plugins`: a C-ABI plugin depends only
//! on the layout, as a C plugin would on a header.
//!
//! Rules this file follows: no function unwinds (each body runs under `catch_unwind`), no pointer
//! the host passes in is kept after the call, and an instance is freed only by `destroy`.
//!
//! Features select broken variants for the host's tests: `bad-abi` reports the next ABI version,
//! `no-entry` leaves out the entry symbol, and `panic-in-create` panics inside `create`.

use std::ffi::{c_char, c_void};
use std::panic::catch_unwind;

/// The ABI version this plugin was written against (`om_plugins::abi::PLUGIN_ABI_VERSION`).
const ABI_VERSION: u32 = 1;

/// The gain the stage applies.
pub const GAIN: f32 = 0.5;

const INFO: &str = r#"{"id":"org.openmixer.example.cdylib-gain","name":"Example C-ABI Gain","version":"1.0","stage_id":"example.cdylib-gain","kind":"post"}"#;

/// Mirror of `om_plugins::abi::PluginVTable`.
#[repr(C)]
pub struct PluginVTable {
    abi_version: u32,
    info: extern "C" fn(*mut c_char, usize),
    create: extern "C" fn() -> *mut c_void,
    destroy: extern "C" fn(*mut c_void),
    process: extern "C" fn(*mut c_void, *mut f32, usize),
}

/// One instance. The gain example keeps no state, but the instance shows the ownership pattern.
struct Gain {
    gain: f32,
}

#[allow(unsafe_code)]
extern "C" fn info(buf: *mut c_char, len: usize) {
    let _ = catch_unwind(|| {
        let bytes = INFO.as_bytes();
        if buf.is_null() || len <= bytes.len() {
            return;
        }
        // SAFETY: the host passes a writable buffer of `len` bytes, and `bytes.len() + 1 <= len`.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast::<u8>(), bytes.len());
            *buf.add(bytes.len()) = 0;
        }
    });
}

extern "C" fn create() -> *mut c_void {
    catch_unwind(|| {
        if cfg!(feature = "panic-in-create") {
            panic!("example plugin: panic in create");
        }
        Box::into_raw(Box::new(Gain { gain: GAIN })).cast::<c_void>()
    })
    .unwrap_or(std::ptr::null_mut())
}

#[allow(unsafe_code)]
extern "C" fn destroy(instance: *mut c_void) {
    let _ = catch_unwind(|| {
        if instance.is_null() {
            return;
        }
        // SAFETY: `instance` came from `create` (`Box::into_raw`) and the host destroys it once.
        drop(unsafe { Box::from_raw(instance.cast::<Gain>()) });
    });
}

#[allow(unsafe_code)]
extern "C" fn process(instance: *mut c_void, samples: *mut f32, frames: usize) {
    let _ = catch_unwind(|| {
        if instance.is_null() || samples.is_null() {
            return;
        }
        // SAFETY: `instance` is a live `Gain` from `create`, used by one thread at a time; the host
        // passes `frames` writable samples it owns for the duration of this call.
        let (gain, samples) = unsafe {
            (
                &*instance.cast::<Gain>(),
                std::slice::from_raw_parts_mut(samples, frames),
            )
        };
        for s in samples {
            *s *= gain.gain;
        }
    });
}

static VTABLE: PluginVTable = PluginVTable {
    abi_version: if cfg!(feature = "bad-abi") {
        ABI_VERSION + 1
    } else {
        ABI_VERSION
    },
    info,
    create,
    destroy,
    process,
};

/// The entry point the host looks up. The table is static, so the pointer stays valid while the
/// library is loaded.
#[cfg(not(feature = "no-entry"))]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn openmixer_plugin_entry() -> *const PluginVTable {
    &VTABLE
}

/// Keeps the table referenced in the `no-entry` variant, which exports nothing the host knows.
#[cfg(feature = "no-entry")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn some_other_symbol() -> *const PluginVTable {
    &VTABLE
}
