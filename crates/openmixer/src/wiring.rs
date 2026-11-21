//! Service wiring that the window needs and that outlived the GPUI window: the stems host into the
//! library hub, the automix runner over the library, and a saved preferences change. Each takes the
//! parts it needs and none opens a window, so a front end (the Tauri shell) calls them as they are.

use crate::app_targets::AudioDeviceControl;
use crate::audio_settings::SharedOutput;
use crate::automix_host::AppAutomixHost;
use crate::automix_queue::LibraryQueue;
use crate::automix_service::{AutomixRunner, AutomixShared};
use crate::automix_settings::queue_source;
use crate::composition::AppComposition;
use om_automix::queue::QueueConfig;
use om_core::CommandSink;
use om_engine::DjEngine;
use om_ui::library::hub::{LibraryHub, SharedHub};
use om_ui::library::StemSource;
use om_ui::preferences::{AutomixPrefs, Preferences};
use om_ui::stems::StemHost;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

/// Hands the stems host to the hub, shared by the browser and the settings page. Cached stems load
/// with their track when the preference says so, through the same sink the MIDI and UI controls use.
pub fn attach_stems_host(hub: &mut LibraryHub, composition: &mut AppComposition) {
    let Some(host) = composition.take_stems_host() else {
        return;
    };
    let source = host.cache_root().map(|root| {
        let (sep_id, version) = StemHost::separator(&host);
        StemSource {
            root,
            sep_id,
            version,
        }
    });
    hub.loader.configure_stems(
        source,
        composition.prefs.stems.auto_load,
        Some(composition.stem_command_sink()),
    );
    hub.stem_host = Some(Box::new(host));
}

/// The automix runner, with its host over the library when there is one. The caller steps it on the
/// window thread, because the host reads the library and the track loader that live there.
pub fn automix_runner(
    composition: &AppComposition,
    engine: &Arc<DjEngine>,
    hub: Option<&SharedHub>,
    prefs: &AutomixPrefs,
    seed: u64,
) -> AutomixRunner {
    automix_runner_over(
        composition.automix.clone(),
        composition.automix_sink.clone(),
        engine,
        hub,
        prefs,
        seed,
    )
}

/// [`automix_runner`] from the two parts it takes off the composition, which are `Send` where the
/// composition is not: a front end that builds the runner on its own thread captures these.
pub fn automix_runner_over(
    automix: Arc<AutomixShared>,
    sink: Arc<dyn CommandSink>,
    engine: &Arc<DjEngine>,
    hub: Option<&SharedHub>,
    prefs: &AutomixPrefs,
    seed: u64,
) -> AutomixRunner {
    let mut runner = AutomixRunner::new(Arc::clone(&automix), sink);
    if let Some(hub) = hub {
        let queue = LibraryQueue::shared(
            hub.borrow().services.library_handle(),
            queue_source(prefs),
            QueueConfig {
                smart: prefs.smart,
                ..QueueConfig::default()
            },
            seed,
        )
        .with_search_feed(Arc::clone(&hub.borrow().search_feed));
        let host = AppAutomixHost::new(Arc::clone(engine), Rc::clone(hub), queue)
            .with_shared_queue(automix.queue_settings());
        runner.set_host(Box::new(host));
    }
    runner
}

/// A saved change from the settings overlay: keep the composition's copy, save the file, and push
/// the sections that changed into the subsystems.
pub fn apply_changes(
    composition: &RefCell<AppComposition>,
    output: &SharedOutput,
    prefs: &Preferences,
    prefs_file: &Path,
) {
    let mut composition = composition.borrow_mut();
    composition.prefs = prefs.clone();
    if !composition.prefs_replaceable {
        eprintln!(
            "openmixer: not saving preferences: {} could not be read, so it is left as it is",
            prefs_file.display()
        );
    } else if let Err(error) = prefs.save(prefs_file) {
        eprintln!("openmixer: could not save preferences: {error}");
    }
    let mut settings = output.lock();
    let pairs = settings.pairs();
    let device: &mut dyn AudioDeviceControl = &mut *settings;
    composition.apply_preferences(&pairs, Some(device), None);
}
