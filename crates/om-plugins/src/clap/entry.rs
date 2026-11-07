//! The one `unsafe` call of the CLAP host: opening a `.clap` library (ADR 0007, decision 3).

#![allow(unsafe_code)]

use clack_host::entry::{PluginEntry, PluginEntryError};
use std::path::Path;

/// Opens the library at `path` and runs its CLAP entry `init`.
pub(super) fn open(path: &Path) -> Result<PluginEntry, PluginEntryError> {
    // SAFETY: the user chose to load this file (loading is open by design, no allow-list). The entry
    // is used only through clack's safe API afterwards, and clack keeps the library mapped while the
    // entry or any instance made from it lives. A non-compliant plugin can still corrupt the process;
    // that is the in-process trade-off the ADR accepts (see "Crash isolation").
    unsafe { PluginEntry::load(path) }
}
