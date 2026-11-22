//! The stems settings page: the preferences come from the settings host (the main thread, which
//! owns the composition), the model, licence and cache from the stem host in the library hub (the
//! control thread). One command joins them; the host's own actions go through the library queue.

use crate::commands::ShellState;
use crate::settings::{with_host, StemsPrefsChange, StemsPrefsPage};
use om_ui::library::hub::LibraryPanel;
use om_ui::settings::format_gib;
use om_ui::stems::ModelState;
use serde::Serialize;
use tauri::State;

const GIB: u64 = 1024 * 1024 * 1024;

/// The licence the page shows before a download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LicenceDto {
    pub name: String,
    pub text: String,
}

/// What the stem host reports; empty when the app has no host (no library).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostFacts {
    pub available: bool,
    /// `unavailable`, `notInstalled`, `licenceNotAccepted`, `downloading`, `preparing`,
    /// `installed` or `failed`.
    pub state: &'static str,
    /// The state in words, as the model line prints it.
    pub line: String,
    /// Download progress, 0..1.
    pub progress: Option<f32>,
    pub ready: bool,
    pub licence: Option<LicenceDto>,
    pub cache_bytes: u64,
    /// Separations queued or running.
    pub in_flight: usize,
}

impl HostFacts {
    fn unavailable() -> Self {
        Self {
            available: false,
            state: "unavailable",
            line: "Stems are not available: the library did not open.".to_owned(),
            progress: None,
            ready: false,
            licence: None,
            cache_bytes: 0,
            in_flight: 0,
        }
    }
}

/// The page: the preferences and the host's facts, with the cache usage in words.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StemsPageDto {
    pub prefs: StemsPrefsPage,
    pub host: HostFacts,
    /// `1.5 GB of 20.0 GB used`.
    pub usage: String,
}

fn state_name(state: &ModelState) -> (&'static str, Option<f32>) {
    match state {
        ModelState::NotInstalled(_) => ("notInstalled", None),
        ModelState::LicenceNotAccepted => ("licenceNotAccepted", None),
        ModelState::Downloading(p) => ("downloading", Some(*p)),
        ModelState::Preparing => ("preparing", None),
        ModelState::Installed(_) => ("installed", None),
        ModelState::Failed(_) => ("failed", None),
    }
}

/// The stem host's facts, read on the control thread.
fn host_facts(panel: &LibraryPanel) -> HostFacts {
    let hub = panel.hub().borrow();
    let Some(host) = hub.stem_host.as_deref() else {
        return HostFacts::unavailable();
    };
    let state = host.model_state();
    let (name, progress) = state_name(&state);
    HostFacts {
        available: true,
        state: name,
        line: state.line(),
        progress,
        ready: state.ready(),
        licence: host.licence().map(|(name, text)| LicenceDto { name, text }),
        cache_bytes: host.cache_bytes(),
        in_flight: hub.stems.in_flight(),
    }
}

fn page(prefs: StemsPrefsPage, host: HostFacts) -> StemsPageDto {
    let cap = u64::try_from(prefs.max_cache_gb).unwrap_or(0) * GIB;
    StemsPageDto {
        usage: format!(
            "{} of {} used",
            format_gib(host.cache_bytes),
            format_gib(cap)
        ),
        prefs,
        host,
    }
}

/// The page as it stands.
#[tauri::command]
pub fn stems_settings(state: State<'_, ShellState>) -> Result<StemsPageDto, String> {
    let prefs = with_host(|host| host.stems_prefs())?;
    let host = state
        .library
        .call(|panel| host_facts(panel))
        .unwrap_or_else(|_| HostFacts::unavailable());
    Ok(page(prefs, host))
}

/// One edit of the preferences: saved, then pushed to the stem host (the cap and auto-prepare take
/// effect at once); the page afterwards.
#[tauri::command]
pub fn stems_settings_change(
    change: StemsPrefsChange,
    state: State<'_, ShellState>,
) -> Result<StemsPageDto, String> {
    let prefs = with_host(|host| host.change_stems_prefs(change))??;
    let pushed = prefs.clone();
    let host = state
        .library
        .call(move |panel| {
            if let Some(host) = panel.hub().borrow_mut().stem_host.as_deref_mut() {
                host.set_max_cache_bytes(u64::try_from(pushed.max_cache_gb).unwrap_or(0) * GIB);
                host.set_auto_prepare(pushed.auto_prepare);
            }
            host_facts(panel)
        })
        .unwrap_or_else(|_| HostFacts::unavailable());
    Ok(page(prefs, host))
}

/// The user accepted the licence: the model download starts (nothing is fetched before this).
#[tauri::command(async)]
pub fn stems_accept_download(state: State<'_, ShellState>) -> Result<(), String> {
    state.library.call(
        |panel| match panel.hub().borrow_mut().stem_host.as_deref_mut() {
            Some(host) => host.accept_and_download().map_err(|e| e.to_string()),
            None => Err("stems are not available".to_owned()),
        },
    )?
}

/// Stops a running model download.
#[tauri::command(async)]
pub fn stems_cancel_download(state: State<'_, ShellState>) -> Result<(), String> {
    state.library.call(|panel| {
        if let Some(host) = panel.hub().borrow_mut().stem_host.as_deref_mut() {
            host.cancel_download();
        }
    })
}

/// Cancels the separations, removes every stem set and index row; how many sets went. The front
/// end confirms first.
#[tauri::command(async)]
pub fn stems_clear_cache(state: State<'_, ShellState>) -> Result<usize, String> {
    state.library.call(
        |panel| match panel.hub().borrow_mut().stem_host.as_deref_mut() {
            Some(host) => host.clear_cache().map_err(|e| e.to_string()),
            None => Err("stems are not available".to_owned()),
        },
    )?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usage_reads_bytes_against_the_cap() {
        let prefs = StemsPrefsPage {
            max_cache_gb: 20,
            max_cache_limit: 1000,
            auto_prepare: false,
            auto_load: true,
        };
        let host = HostFacts {
            cache_bytes: 3 * GIB / 2,
            ..HostFacts::unavailable()
        };
        assert_eq!(page(prefs, host).usage, "1.5 GB of 20.0 GB used");
    }

    #[test]
    fn a_download_reports_its_progress() {
        assert_eq!(
            state_name(&ModelState::Downloading(0.5)),
            ("downloading", Some(0.5))
        );
        assert_eq!(state_name(&ModelState::Installed(1)).0, "installed");
        assert!(serde_json::to_string(&HostFacts::unavailable())
            .unwrap()
            .contains("\"cacheBytes\":0"));
    }
}
