//! Example C-ABI version 2 plugin: a stage (`example.cdylib-params`) with `prepare` and three
//! parameters. Built from the C header alone (`crates/om-plugins/include/openmixer_plugin.h`),
//! with no dependency on the host crates.
//!
//! - `gain` (0..2, default 1) multiplies the audio.
//! - `tone` (0..1, default 0): at 0.5 or more the input is replaced by a 1 kHz sine at the rate
//!   given to `prepare`, so a test can see the rate arrive.
//! - `crash` (0..1, default 0): at 0.5 or more `process` panics. The panic is caught at the
//!   boundary and reported as a fault; the host stops using the instance.
//!
//! Every exported function runs its body under `catch_unwind` (a panic across `extern "C"` aborts).
//! `process` and `param_set` do not allocate.

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

const OK: i32 = 0;
const REJECTED: i32 = 1;
const FAULT: i32 = 2;

const INFO: &str = r#"{"id":"org.openmixer.example.cdylib-params","name":"Example C-ABI Params","version":"1.0","stage_id":"example.cdylib-params","kind":"fx"}"#;

const PARAMS: [&str; 3] = [
    r#"{"id":"gain","name":"Gain","min":0.0,"max":2.0,"default":1.0}"#,
    r#"{"id":"tone","name":"Tone","min":0.0,"max":1.0,"default":0.0}"#,
    r#"{"id":"crash","name":"Crash","min":0.0,"max":1.0,"default":0.0}"#,
];

/// Mirror of `OpenMixerPluginVTable` in the header.
#[repr(C)]
pub struct PluginVTable {
    abi_version: u32,
    info: extern "C" fn(*mut c_char, usize),
    create: extern "C" fn() -> *mut c_void,
    destroy: extern "C" fn(*mut c_void),
    process: extern "C" fn(*mut c_void, *mut f32, usize) -> i32,
    prepare: extern "C" fn(*mut c_void, f64, usize) -> i32,
    param_count: extern "C" fn() -> u32,
    param_info: extern "C" fn(u32, *mut c_char, usize),
    param_get: extern "C" fn(*mut c_void, u32) -> f64,
    param_set: extern "C" fn(*mut c_void, u32, f64) -> i32,
}

struct State {
    gain: f64,
    tone: f64,
    crash: f64,
    sample_rate: f64,
    /// Sample index of the tone, reset by `prepare`.
    n: u64,
}

/// Runs `f` and turns a panic into `default`.
fn guard<R>(default: R, f: impl FnOnce() -> R) -> R {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(default)
}

/// Copies `text` and a NUL into the host buffer, or nothing when it does not fit.
#[allow(unsafe_code)]
fn write_text(text: &str, buf: *mut c_char, len: usize) {
    let bytes = text.as_bytes();
    if buf.is_null() || len <= bytes.len() {
        return;
    }
    // SAFETY: the host passes a writable buffer of `len` bytes and `bytes.len() + 1 <= len`.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast::<u8>(), bytes.len());
        *buf.add(bytes.len()) = 0;
    }
}

/// The live instance behind `ptr`, or `None` for null.
#[allow(unsafe_code)]
fn state<'a>(ptr: *mut c_void) -> Option<&'a mut State> {
    // SAFETY: a non-null `ptr` came from `create` (`Box::into_raw`) and is used by one thread at
    // a time until the host passes it to `destroy`.
    unsafe { ptr.cast::<State>().as_mut() }
}

extern "C" fn info(buf: *mut c_char, len: usize) {
    guard((), || write_text(INFO, buf, len));
}

extern "C" fn create() -> *mut c_void {
    guard(std::ptr::null_mut(), || {
        let state = State {
            gain: 1.0,
            tone: 0.0,
            crash: 0.0,
            sample_rate: 48_000.0,
            n: 0,
        };
        Box::into_raw(Box::new(state)).cast::<c_void>()
    })
}

#[allow(unsafe_code)]
extern "C" fn destroy(instance: *mut c_void) {
    guard((), || {
        if !instance.is_null() {
            // SAFETY: `instance` came from `create` and the host destroys it once.
            drop(unsafe { Box::from_raw(instance.cast::<State>()) });
        }
    });
}

#[allow(unsafe_code)]
extern "C" fn process(instance: *mut c_void, samples: *mut f32, frames: usize) -> i32 {
    guard(FAULT, || {
        let (Some(state), false) = (state(instance), samples.is_null()) else {
            return REJECTED;
        };
        // SAFETY: the host passes `frames` writable samples it owns for the duration of the call.
        let samples = unsafe { std::slice::from_raw_parts_mut(samples, frames) };
        render(state, samples);
        OK
    })
}

fn render(state: &mut State, samples: &mut [f32]) {
    assert!(state.crash < 0.5, "example plugin: crash parameter is set");
    let step = std::f64::consts::TAU * 1000.0 / state.sample_rate;
    for s in samples {
        if state.tone >= 0.5 {
            *s = (step * state.n as f64).sin() as f32;
            state.n += 1;
        }
        *s *= state.gain as f32;
    }
}

extern "C" fn prepare(instance: *mut c_void, sample_rate: f64, max_block: usize) -> i32 {
    guard(FAULT, || {
        let Some(state) = state(instance) else {
            return REJECTED;
        };
        if !(sample_rate.is_finite() && sample_rate > 0.0 && max_block > 0) {
            return REJECTED;
        }
        state.sample_rate = sample_rate;
        state.n = 0;
        OK
    })
}

extern "C" fn param_count() -> u32 {
    PARAMS.len() as u32
}

extern "C" fn param_info(index: u32, buf: *mut c_char, len: usize) {
    guard((), || {
        if let Some(text) = PARAMS.get(index as usize) {
            write_text(text, buf, len);
        }
    });
}

extern "C" fn param_get(instance: *mut c_void, index: u32) -> f64 {
    guard(f64::NAN, || match (state(instance), index) {
        (Some(s), 0) => s.gain,
        (Some(s), 1) => s.tone,
        (Some(s), 2) => s.crash,
        _ => f64::NAN,
    })
}

extern "C" fn param_set(instance: *mut c_void, index: u32, value: f64) -> i32 {
    guard(FAULT, || match (state(instance), index) {
        (Some(s), 0) => {
            s.gain = value;
            OK
        }
        (Some(s), 1) => {
            s.tone = value;
            OK
        }
        (Some(s), 2) => {
            s.crash = value;
            OK
        }
        _ => REJECTED,
    })
}

static VTABLE: PluginVTable = PluginVTable {
    abi_version: 2,
    info,
    create,
    destroy,
    process,
    prepare,
    param_count,
    param_info,
    param_get,
    param_set,
};

/// The entry point the host looks up. The table is static, so it stays valid while the library is
/// loaded.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn openmixer_plugin_entry() -> *const PluginVTable {
    &VTABLE
}
