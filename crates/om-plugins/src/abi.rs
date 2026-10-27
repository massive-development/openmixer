//! The C ABI a shared-library plugin exports, and the host's safe wrappers around it.
//!
//! A plugin library exports `extern "C" fn openmixer_plugin_entry() -> *const PluginVTable`. The
//! table's first field is [`PLUGIN_ABI_VERSION`]; the host reads it before anything else and calls
//! nothing in a table of another version. The rules (`docs/spikes/DECISIONS.md`, "Plugins: C ABI
//! dynamic loading"):
//!
//! - The host owns every buffer it passes (`info`'s text buffer, `process`'s samples). The plugin
//!   must not keep a pointer to one after the call returns.
//! - `create` returns an owned instance, or null on failure. The host passes each instance to
//!   `destroy` exactly once and never uses it afterwards. An instance is used by one thread at a
//!   time but may move between threads.
//! - `process(instance, samples, frames)` processes one mono buffer of `frames` samples in place.
//! - `process` runs on the real-time audio thread: it must not allocate, lock, block or do I/O.
//!   `create`, `destroy` and `info` run on the control thread, possibly while other instances of
//!   the same plugin are processing, so any state they share needs its own synchronisation.
//! - No function may unwind: a plugin catches its own panics (Rust aborts on a panic that reaches an
//!   `extern "C"` boundary). The host still runs every call under `catch_unwind`.
//! - The library is never unloaded while an instance lives: an [`Instance`] holds the library.
//!
//! Together with the loader, this is the only module of the crate with `unsafe` code.

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

/// C ABI version the host speaks. A table reporting another version is refused.
pub const PLUGIN_ABI_VERSION: u32 = 1;

/// The symbol a plugin library exports.
pub const ENTRY_SYMBOL: &str = "openmixer_plugin_entry";

/// The type of [`ENTRY_SYMBOL`].
pub type PluginEntry = extern "C" fn() -> *const PluginVTable;

/// Size of the buffer the host hands `info`, NUL included.
pub const INFO_CAPACITY: usize = 4096;

/// The function table a plugin library returns from its entry point.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginVTable {
    /// Must equal [`PLUGIN_ABI_VERSION`].
    pub abi_version: u32,
    /// Writes a NUL-terminated UTF-8 JSON object into the buffer (at most `len` bytes with the
    /// NUL): `{"id", "name", "version"}` are required strings; `"stage_id"` (default: the id) and
    /// `"kind"` (`"fx"`, `"pre"` or `"post"`; default `"fx"`) are optional.
    pub info: extern "C" fn(*mut c_char, usize),
    /// A new instance, or null on failure.
    pub create: extern "C" fn() -> *mut c_void,
    /// Frees an instance from `create`.
    pub destroy: extern "C" fn(*mut c_void),
    /// Processes `frames` mono samples in place.
    pub process: extern "C" fn(*mut c_void, *mut f32, usize),
}

/// [`PluginVTable`] with nullable function pointers. `Option<extern "C" fn>` has the layout of a
/// nullable function pointer, so a table from a library is read through this type and checked.
#[repr(C)]
struct RawVTable {
    abi_version: u32,
    info: Option<extern "C" fn(*mut c_char, usize)>,
    create: Option<extern "C" fn() -> *mut c_void>,
    destroy: Option<extern "C" fn(*mut c_void)>,
    process: Option<extern "C" fn(*mut c_void, *mut f32, usize)>,
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
/// its function pointers (when the version matches) are functions of that library with the
/// signatures above. The caller keeps the library loaded for as long as it uses the result.
pub(crate) unsafe fn read_table(table: *const PluginVTable) -> Result<PluginVTable, TableError> {
    if table.is_null() {
        return Err(TableError::Null);
    }
    let raw = table.cast::<RawVTable>();
    // SAFETY: the caller guarantees `table` points to a readable table; `abi_version` is its first
    // field in every version, so it is read before anything else is trusted.
    let found = unsafe { std::ptr::addr_of!((*raw).abi_version).read_unaligned() };
    if found != PLUGIN_ABI_VERSION {
        return Err(TableError::BadAbi(found));
    }
    // SAFETY: the version matches, so the table has this layout. `RawVTable` has the same layout as
    // `PluginVTable` with nullable function pointers, so a null field reads as `None`.
    let raw = unsafe { raw.read_unaligned() };
    Ok(PluginVTable {
        abi_version: raw.abi_version,
        info: raw.info.ok_or(TableError::NullFunction("info"))?,
        create: raw.create.ok_or(TableError::NullFunction("create"))?,
        destroy: raw.destroy.ok_or(TableError::NullFunction("destroy"))?,
        process: raw.process.ok_or(TableError::NullFunction("process"))?,
    })
}

/// A loaded library and its checked table. The table's function pointers are valid because the
/// library is held here, and they are only reachable through this type.
pub(crate) struct LoadedLibrary {
    table: PluginVTable,
    _library: libloading::Library,
}

impl LoadedLibrary {
    /// Pairs a table with the library it was read from.
    ///
    /// # Safety
    ///
    /// `table` was read (by [`read_table`]) from `library`, so its functions live as long as it.
    pub(crate) unsafe fn new(table: PluginVTable, library: libloading::Library) -> Self {
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
        let end = buf
            .iter()
            .position(|&b| b == 0)
            .ok_or("info text is not NUL-terminated")?;
        buf.truncate(end);
        String::from_utf8(buf).map_err(|_| "info text is not UTF-8".into())
    }
}

/// One plugin instance. Holds the library, so the library outlives the instance; dropping it
/// calls `destroy` first, then releases the library.
pub(crate) struct Instance {
    ptr: *mut c_void,
    /// Set when a call panicked on the host side; the instance then passes audio through.
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

    /// Processes one mono buffer in place. No allocation unless a call panics.
    pub(crate) fn process(&mut self, samples: &mut [f32]) {
        if self.faulted {
            return;
        }
        let process = self.library.table.process;
        let (ptr, data, len) = (self.ptr, samples.as_mut_ptr(), samples.len());
        if guarded("process", || process(ptr, data, len)).is_err() {
            self.faulted = true;
        }
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
    extern "C" fn process(_: *mut c_void, _: *mut f32, _: usize) {}

    fn table(abi_version: u32) -> PluginVTable {
        PluginVTable {
            abi_version,
            info,
            create,
            destroy,
            process,
        }
    }

    #[test]
    fn read_table_checks_null_and_version() {
        // SAFETY: null is allowed; the other pointers are to live local tables.
        unsafe {
            assert_eq!(read_table(std::ptr::null()).err(), Some(TableError::Null));
            let bad = table(PLUGIN_ABI_VERSION + 7);
            assert_eq!(
                read_table(&bad).err(),
                Some(TableError::BadAbi(PLUGIN_ABI_VERSION + 7))
            );
            assert!(read_table(&table(PLUGIN_ABI_VERSION)).is_ok());
        }
    }

    #[test]
    fn read_table_refuses_null_functions() {
        let raw = RawVTable {
            abi_version: PLUGIN_ABI_VERSION,
            info: Some(info),
            create: None,
            destroy: Some(destroy),
            process: Some(process),
        };
        // SAFETY: `raw` is a live table with the `PluginVTable` layout.
        let result = unsafe { read_table(std::ptr::addr_of!(raw).cast()) };
        assert_eq!(result.err(), Some(TableError::NullFunction("create")));
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
