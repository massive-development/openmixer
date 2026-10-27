//! The C ABI a shared-library plugin exports, and the host's safe wrappers around it.
//!
//! A plugin library exports `extern "C" fn openmixer_plugin_entry() -> *const PluginVTable`. The
//! table's first field is the ABI version; the host reads it before anything else. Versions
//! [`PLUGIN_ABI_MIN`] to [`PLUGIN_ABI_VERSION`] are accepted and anything else is refused without
//! calling the table (`docs/spikes/DECISIONS.md`, "Plugins: C ABI dynamic loading" and "ABI 2").
//! `include/openmixer_plugin.h` is the same contract for C and C++ plugins.
//!
//! - Version 1: `info`, `create`, `destroy`, `process` (returns nothing). No prepare, no parameters.
//! - Version 2: the same four, `process` returns a status, plus optional `prepare` and parameters.
//!   A null optional function means "not supported". The four parameter functions go together.
//!
//! The rules:
//!
//! - The host owns every buffer it passes (`info`'s text buffer, `process`'s samples). The plugin
//!   must not keep a pointer to one after the call returns.
//! - `create` returns an owned instance, or null on failure. The host passes each instance to
//!   `destroy` exactly once and never uses it afterwards. An instance is used by one thread at a
//!   time but may move between threads.
//! - `process(instance, samples, frames)` processes one mono buffer of `frames` samples in place.
//!   In version 2 it returns [`STATUS_OK`], or a non-zero status after which the host stops calling
//!   that instance and passes audio through.
//! - `prepare(instance, sample_rate, max_block)` (version 2) runs on the control thread before
//!   `process` and again when either value changes; `process` never gets more than `max_block`
//!   frames after it. An instance never prepared must still process (at a rate of its choosing).
//! - Parameters (version 2): `param_count()` and `param_info(index, buf, len)` (a JSON object
//!   `{"id","name","min","max","default"}`) are read once at load. `param_get` and `param_set` take
//!   an instance and an index. The host clamps values to `[min, max]` and calls `param_set` from
//!   the audio thread (between buffers), so it must be real-time safe like `process`.
//! - `process` runs on the real-time audio thread: it must not allocate, lock, block or do I/O.
//!   `create`, `destroy`, `info` and `prepare` run on the control thread, possibly while other
//!   instances of the same plugin are processing, so any state they share needs its own
//!   synchronisation.
//! - No function may unwind: a plugin catches its own panics and reports [`STATUS_FAULT`] (Rust
//!   aborts on a panic that reaches an `extern "C"` boundary). The host still runs every call
//!   under `catch_unwind`.
//! - The library is never unloaded while an instance lives: an [`Instance`] holds the library.
//!
//! Together with the loader, this is the only module of the crate with `unsafe` code.

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

/// Newest C ABI version the host speaks.
pub const PLUGIN_ABI_VERSION: u32 = 2;

/// Oldest C ABI version the host still accepts (no prepare, no parameters).
pub const PLUGIN_ABI_MIN: u32 = 1;

/// Call succeeded.
pub const STATUS_OK: i32 = 0;
/// The plugin refused the arguments (an unknown parameter index, say). Not a fault.
pub const STATUS_REJECTED: i32 = 1;
/// The plugin panicked or is broken. The host stops using that instance.
pub const STATUS_FAULT: i32 = 2;

/// The symbol a plugin library exports.
pub const ENTRY_SYMBOL: &str = "openmixer_plugin_entry";

/// The type of [`ENTRY_SYMBOL`].
pub type PluginEntry = extern "C" fn() -> *const PluginVTable;

/// Size of the buffer the host hands `info` and `param_info`, NUL included.
pub const INFO_CAPACITY: usize = 4096;

/// Most parameters a plugin may declare.
pub const MAX_PARAMETERS: u32 = 256;

type InfoFn = extern "C" fn(*mut c_char, usize);
type CreateFn = extern "C" fn() -> *mut c_void;
type DestroyFn = extern "C" fn(*mut c_void);
type ProcessFnV1 = extern "C" fn(*mut c_void, *mut f32, usize);
type ProcessFn = extern "C" fn(*mut c_void, *mut f32, usize) -> i32;
type PrepareFn = extern "C" fn(*mut c_void, f64, usize) -> i32;
type ParamCountFn = extern "C" fn() -> u32;
type ParamInfoFn = extern "C" fn(u32, *mut c_char, usize);
type ParamGetFn = extern "C" fn(*mut c_void, u32) -> f64;
type ParamSetFn = extern "C" fn(*mut c_void, u32, f64) -> i32;

/// The version 2 function table a plugin library returns from its entry point. A version 1 table
/// is the first five fields with `process` returning nothing ([`PluginVTableV1`]).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginVTable {
    /// Must be [`PLUGIN_ABI_VERSION`] for this layout.
    pub abi_version: u32,
    /// Writes a NUL-terminated UTF-8 JSON object into the buffer (at most `len` bytes with the
    /// NUL): `{"id", "name", "version"}` are required strings; `"stage_id"` (default: the id),
    /// `"kind"` (`"fx"`, `"pre"` or `"post"`; default `"fx"`) and `"param_prefix"` (default: the
    /// stage id) are optional.
    pub info: extern "C" fn(*mut c_char, usize),
    /// A new instance, or null on failure.
    pub create: extern "C" fn() -> *mut c_void,
    /// Frees an instance from `create`.
    pub destroy: extern "C" fn(*mut c_void),
    /// Processes `frames` mono samples in place. Returns a status.
    pub process: extern "C" fn(*mut c_void, *mut f32, usize) -> i32,
    /// Sets the sample rate and the largest block. Null: the plugin does not care.
    pub prepare: Option<extern "C" fn(*mut c_void, f64, usize) -> i32>,
    /// Number of parameters. Null: no parameters.
    pub param_count: Option<extern "C" fn() -> u32>,
    /// Writes parameter `index`'s JSON description into the buffer.
    pub param_info: Option<extern "C" fn(u32, *mut c_char, usize)>,
    /// The current value of parameter `index`.
    pub param_get: Option<extern "C" fn(*mut c_void, u32) -> f64>,
    /// Sets parameter `index`. Returns a status.
    pub param_set: Option<extern "C" fn(*mut c_void, u32, f64) -> i32>,
}

/// The version 1 function table, still accepted. The host calls it with no prepare and no
/// parameters.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginVTableV1 {
    pub abi_version: u32,
    pub info: extern "C" fn(*mut c_char, usize),
    pub create: extern "C" fn() -> *mut c_void,
    pub destroy: extern "C" fn(*mut c_void),
    pub process: extern "C" fn(*mut c_void, *mut f32, usize),
}

/// [`PluginVTableV1`] with nullable function pointers (`Option<extern "C" fn>` has the layout of a
/// nullable function pointer), so a table from a library is read through this type and checked.
#[repr(C)]
struct RawV1 {
    abi_version: u32,
    info: Option<InfoFn>,
    create: Option<CreateFn>,
    destroy: Option<DestroyFn>,
    process: Option<ProcessFnV1>,
}

/// [`PluginVTable`] with every function pointer nullable.
#[repr(C)]
struct RawV2 {
    abi_version: u32,
    info: Option<InfoFn>,
    create: Option<CreateFn>,
    destroy: Option<DestroyFn>,
    process: Option<ProcessFn>,
    prepare: Option<PrepareFn>,
    param_count: Option<ParamCountFn>,
    param_info: Option<ParamInfoFn>,
    param_get: Option<ParamGetFn>,
    param_set: Option<ParamSetFn>,
}

/// The `process` of either version.
#[derive(Clone, Copy)]
enum Process {
    V1(ProcessFnV1),
    V2(ProcessFn),
}

/// The four parameter functions, present together.
#[derive(Clone, Copy)]
struct ParamFns {
    count: ParamCountFn,
    info: ParamInfoFn,
    get: ParamGetFn,
    set: ParamSetFn,
}

/// A table that passed the checks, whatever its version.
#[derive(Clone, Copy)]
pub(crate) struct CheckedTable {
    info: InfoFn,
    create: CreateFn,
    destroy: DestroyFn,
    process: Process,
    prepare: Option<PrepareFn>,
    params: Option<ParamFns>,
}

/// Why a table was refused.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TableError {
    Null,
    BadAbi(u32),
    NullFunction(&'static str),
}

/// Runs a call into the plugin under `catch_unwind`. `Err` names the call that panicked.
pub(crate) fn guarded<R>(call: &'static str, f: impl FnOnce() -> R) -> Result<R, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| format!("{call} panicked"))
}

/// Reads and checks the table the entry point returned.
///
/// # Safety
///
/// `table` is null or points to a readable table that stays valid while the library is loaded, and
/// its function pointers (when the version is accepted) are functions of that library with the
/// signatures of that version. The caller keeps the library loaded for as long as it uses the
/// result.
pub(crate) unsafe fn read_table(table: *const PluginVTable) -> Result<CheckedTable, TableError> {
    if table.is_null() {
        return Err(TableError::Null);
    }
    // SAFETY: the caller guarantees `table` points to a readable table; `abi_version` is its first
    // field in every version, so it is read before anything else is trusted.
    let found =
        unsafe { std::ptr::addr_of!((*table.cast::<RawV1>()).abi_version).read_unaligned() };
    match found {
        // SAFETY: the version says the table has the version 1 layout.
        1 => check_v1(unsafe { table.cast::<RawV1>().read_unaligned() }),
        // SAFETY: the version says the table has the version 2 layout.
        2 => check_v2(unsafe { table.cast::<RawV2>().read_unaligned() }),
        other => Err(TableError::BadAbi(other)),
    }
}

fn check_v1(raw: RawV1) -> Result<CheckedTable, TableError> {
    Ok(CheckedTable {
        info: raw.info.ok_or(TableError::NullFunction("info"))?,
        create: raw.create.ok_or(TableError::NullFunction("create"))?,
        destroy: raw.destroy.ok_or(TableError::NullFunction("destroy"))?,
        process: Process::V1(raw.process.ok_or(TableError::NullFunction("process"))?),
        prepare: None,
        params: None,
    })
}

fn check_v2(raw: RawV2) -> Result<CheckedTable, TableError> {
    let params = match raw.param_count {
        None => None,
        Some(count) => Some(ParamFns {
            count,
            info: raw
                .param_info
                .ok_or(TableError::NullFunction("param_info"))?,
            get: raw.param_get.ok_or(TableError::NullFunction("param_get"))?,
            set: raw.param_set.ok_or(TableError::NullFunction("param_set"))?,
        }),
    };
    Ok(CheckedTable {
        info: raw.info.ok_or(TableError::NullFunction("info"))?,
        create: raw.create.ok_or(TableError::NullFunction("create"))?,
        destroy: raw.destroy.ok_or(TableError::NullFunction("destroy"))?,
        process: Process::V2(raw.process.ok_or(TableError::NullFunction("process"))?),
        prepare: raw.prepare,
        params,
    })
}

/// Reads the NUL-terminated text a plugin wrote into `buf`.
fn text_of(mut buf: Vec<u8>) -> Result<String, String> {
    let end = buf
        .iter()
        .position(|&b| b == 0)
        .ok_or("text is not NUL-terminated")?;
    buf.truncate(end);
    String::from_utf8(buf).map_err(|_| "text is not UTF-8".into())
}

/// A loaded library and its checked table. The table's function pointers are valid because the
/// library is held here, and they are only reachable through this type.
pub(crate) struct LoadedLibrary {
    table: CheckedTable,
    _library: libloading::Library,
}

impl LoadedLibrary {
    /// Pairs a table with the library it was read from.
    ///
    /// # Safety
    ///
    /// `table` was read (by [`read_table`]) from `library`, so its functions live as long as it.
    pub(crate) unsafe fn new(table: CheckedTable, library: libloading::Library) -> Self {
        Self {
            table,
            _library: library,
        }
    }

    /// Calls `info` with a host buffer and returns the text it wrote.
    pub(crate) fn info(&self) -> Result<String, String> {
        let mut buf = vec![0_u8; INFO_CAPACITY];
        let info = self.table.info;
        guarded("info", || {
            info(buf.as_mut_ptr().cast::<c_char>(), buf.len())
        })?;
        text_of(buf).map_err(|e| format!("info {e}"))
    }

    /// The JSON text of every parameter, in index order. Empty when the plugin has none.
    pub(crate) fn param_infos(&self) -> Result<Vec<String>, String> {
        let Some(fns) = self.table.params else {
            return Ok(Vec::new());
        };
        let count = guarded("param_count", || (fns.count)())?;
        if count > MAX_PARAMETERS {
            return Err(format!("{count} parameters, at most {MAX_PARAMETERS}"));
        }
        (0..count)
            .map(|index| {
                let mut buf = vec![0_u8; INFO_CAPACITY];
                guarded("param_info", || {
                    (fns.info)(index, buf.as_mut_ptr().cast::<c_char>(), buf.len())
                })?;
                text_of(buf).map_err(|e| format!("param_info {index} {e}"))
            })
            .collect()
    }
}

/// One plugin instance. Holds the library, so the library outlives the instance; dropping it
/// calls `destroy` first, then releases the library.
pub(crate) struct Instance {
    ptr: *mut c_void,
    /// Set when a call panicked or reported a fault; the instance then passes audio through.
    faulted: bool,
    library: Arc<LoadedLibrary>,
}

// SAFETY: the ABI requires that an instance may move between threads as long as one thread uses it
// at a time. `Instance` is not `Sync` and its methods take `&mut self`, so that holds.
unsafe impl Send for Instance {}

impl Instance {
    /// Calls `create`. Err when it returned null or the call panicked.
    pub(crate) fn create(library: &Arc<LoadedLibrary>) -> Result<Self, String> {
        let create = library.table.create;
        // `extern "C"` function pointers do not implement `FnOnce`, so the closure is needed.
        #[allow(clippy::redundant_closure)]
        let ptr = guarded("create", || create())?;
        if ptr.is_null() {
            return Err("create returned null".into());
        }
        Ok(Self {
            ptr,
            faulted: false,
            library: Arc::clone(library),
        })
    }

    /// Records the outcome of a call that returns a status. A fault stops further calls.
    fn note(&mut self, outcome: Result<i32, String>) {
        if !matches!(outcome, Ok(STATUS_OK | STATUS_REJECTED)) {
            self.faulted = true;
        }
    }

    /// Processes one mono buffer in place. No allocation unless a call panics.
    pub(crate) fn process(&mut self, samples: &mut [f32]) {
        if self.faulted {
            return;
        }
        let process = self.library.table.process;
        let (ptr, data, len) = (self.ptr, samples.as_mut_ptr(), samples.len());
        let outcome = guarded("process", || match process {
            Process::V1(f) => {
                f(ptr, data, len);
                STATUS_OK
            }
            Process::V2(f) => f(ptr, data, len),
        });
        self.note(outcome);
    }

    /// Calls `prepare` when the plugin has one. Control thread.
    pub(crate) fn prepare(&mut self, sample_rate: f64, max_block: usize) {
        let (Some(prepare), false) = (self.library.table.prepare, self.faulted) else {
            return;
        };
        let ptr = self.ptr;
        let outcome = guarded("prepare", || prepare(ptr, sample_rate, max_block));
        self.note(outcome);
    }

    /// Sets parameter `index`. No allocation unless a call panics.
    pub(crate) fn set_param(&mut self, index: u32, value: f64) {
        let (Some(fns), false) = (self.library.table.params, self.faulted) else {
            return;
        };
        let ptr = self.ptr;
        let outcome = guarded("param_set", || (fns.set)(ptr, index, value));
        self.note(outcome);
    }

    /// The plugin's value for parameter `index`, or `None` without parameters or after a fault.
    pub(crate) fn get_param(&mut self, index: u32) -> Option<f64> {
        let (Some(fns), false) = (self.library.table.params, self.faulted) else {
            return None;
        };
        let ptr = self.ptr;
        let value = guarded("param_get", || (fns.get)(ptr, index));
        if value.is_err() {
            self.faulted = true;
        }
        value.ok()
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let destroy = self.library.table.destroy;
        let ptr = self.ptr;
        // Nothing to report from a drop: a panic here is contained and the instance is gone.
        let _ = guarded("destroy", || destroy(ptr));
        // `self.library` is released after this body, so the library is still loaded above.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn info(_: *mut c_char, _: usize) {}
    extern "C" fn create() -> *mut c_void {
        std::ptr::null_mut()
    }
    extern "C" fn destroy(_: *mut c_void) {}
    extern "C" fn process(_: *mut c_void, _: *mut f32, _: usize) -> i32 {
        STATUS_OK
    }
    extern "C" fn process_v1(_: *mut c_void, _: *mut f32, _: usize) {}
    extern "C" fn count() -> u32 {
        0
    }
    extern "C" fn param_info(_: u32, _: *mut c_char, _: usize) {}
    extern "C" fn param_get(_: *mut c_void, _: u32) -> f64 {
        0.0
    }
    extern "C" fn param_set(_: *mut c_void, _: u32, _: f64) -> i32 {
        STATUS_OK
    }

    fn table(abi_version: u32) -> PluginVTable {
        PluginVTable {
            abi_version,
            info,
            create,
            destroy,
            process,
            prepare: None,
            param_count: Some(count),
            param_info: Some(param_info),
            param_get: Some(param_get),
            param_set: Some(param_set),
        }
    }

    #[test]
    fn read_table_checks_null_and_version() {
        // SAFETY: null is allowed; the other pointers are to live local tables.
        unsafe {
            assert_eq!(read_table(std::ptr::null()).err(), Some(TableError::Null));
            for bad in [0, PLUGIN_ABI_VERSION + 1, PLUGIN_ABI_VERSION + 7] {
                assert_eq!(read_table(&table(bad)).err(), Some(TableError::BadAbi(bad)));
            }
            let ok = read_table(&table(PLUGIN_ABI_VERSION)).expect("v2 accepted");
            assert!(ok.params.is_some() && ok.prepare.is_none());
        }
    }

    #[test]
    fn read_table_accepts_the_old_version_without_extensions() {
        let old = PluginVTableV1 {
            abi_version: PLUGIN_ABI_MIN,
            info,
            create,
            destroy,
            process: process_v1,
        };
        // SAFETY: `old` is a live version 1 table.
        let ok = unsafe { read_table(std::ptr::addr_of!(old).cast()) }.expect("v1 accepted");
        assert!(matches!(ok.process, Process::V1(_)));
        assert!(ok.params.is_none() && ok.prepare.is_none());
    }

    #[test]
    fn read_table_refuses_null_functions() {
        let mut raw = RawV2 {
            abi_version: PLUGIN_ABI_VERSION,
            info: Some(info),
            create: None,
            destroy: Some(destroy),
            process: Some(process),
            prepare: None,
            param_count: None,
            param_info: None,
            param_get: None,
            param_set: None,
        };
        // SAFETY: `raw` is a live table with the `PluginVTable` layout.
        let result = unsafe { read_table(std::ptr::addr_of!(raw).cast()) };
        assert_eq!(result.err(), Some(TableError::NullFunction("create")));
        raw.create = Some(create);
        raw.param_count = Some(count);
        // SAFETY: as above.
        let result = unsafe { read_table(std::ptr::addr_of!(raw).cast()) };
        assert_eq!(result.err(), Some(TableError::NullFunction("param_info")));
    }

    #[test]
    fn guarded_contains_a_panic() {
        assert_eq!(guarded("x", || 3), Ok(3));
        assert_eq!(
            guarded("x", || -> u8 { panic!("boom") }),
            Err("x panicked".into())
        );
    }
}
