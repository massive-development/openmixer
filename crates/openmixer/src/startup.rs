//! Start-up steps both windows share (GPUI in the binary's `gui.rs`, Tauri in `gui_tauri.rs`): the
//! output (cpal, or the silent mock), starting an output on the engine once the preferences reached it, and the
//! demo library's tracks and decks (always in the scratch folder).

use crate::audio_settings::{
    CpalBackend, MockBackend, OutputBackend, OutputSettings, SharedOutput,
};
use crate::composition::AppComposition;
use crate::flags::{self, DemoScene};
use crate::{demo_scene, library_smoke, AppError};
use om_audio::DjEngineAudio;
use om_engine::DjEngine;
use om_ui::labels::{notice_text, Notice};
use om_ui::library::{LibraryServices, TrackLoader};
use std::path::{Path, PathBuf};

/// The silent mock output, started by the window like the cpal one.
pub fn mock_output() -> SharedOutput {
    let backend: Box<dyn OutputBackend> = Box::new(MockBackend::default());
    let output = SharedOutput::new(OutputSettings::new(backend));
    if let Err(error) = output.lock().start_up("") {
        // The default mock cannot fail to open; keep the window up with no device if it does.
        eprintln!("openmixer: mock device: {error}");
    }
    output
}

/// The output a window runs on, and the notice it shows (if any).
pub struct OpenedOutput {
    pub output: SharedOutput,
    pub banner: Option<String>,
    /// The stored device state the output was opened with, so `start_output` does not reopen it.
    pub started_with: Option<String>,
}

/// The cpal output at the `stored` setup (else the default output), or the silent mock when `mock`
/// is set or no device opens. The mock shows the demo notice unless `demo_run` (staged for
/// screenshots); a failed device shows the no-device notice.
pub fn open_output(mock: bool, demo_run: bool, stored: &str) -> OpenedOutput {
    if mock {
        let banner = (!demo_run).then(|| notice_text(Notice::Demo).to_owned());
        return OpenedOutput {
            output: mock_output(),
            banner,
            started_with: None,
        };
    }
    let cpal = SharedOutput::new(OutputSettings::new(Box::new(CpalBackend)));
    let opened = cpal.lock().start_up(stored);
    match opened {
        Ok(()) => OpenedOutput {
            output: cpal,
            banner: None,
            started_with: (!stored.is_empty()).then(|| stored.to_owned()),
        },
        Err(error) => {
            eprintln!("openmixer: no audio device ({error}); running silent on the mock device");
            OpenedOutput {
                output: mock_output(),
                banner: Some(notice_text(Notice::NoDevice).to_owned()),
                started_with: None,
            }
        }
    }
}

/// Pushes the preferences into `output`'s routing and device, then starts it on `audio`.
/// `started_with` is the stored state the device was opened with, so it is not opened twice.
pub fn start_output(
    composition: &mut AppComposition,
    output: &SharedOutput,
    audio: DjEngineAudio,
    started_with: Option<&str>,
) -> Result<(), AppError> {
    {
        let mut settings = output.lock();
        let pairs = settings.pairs();
        composition.apply_preferences(&pairs, Some(&mut *settings), started_with);
    }
    output.lock().start(Box::new(audio))
}

/// The file `--demo-scene=track` links in (from [`flags::DEMO_TRACK_ENV`]); `Err` says why that
/// scene cannot run without it.
pub fn demo_user_track(scene: Option<DemoScene>) -> Result<Option<PathBuf>, String> {
    let track = flags::demo_track(std::env::var_os(flags::DEMO_TRACK_ENV));
    if scene == Some(DemoScene::Track) && track.is_none() {
        return Err(format!(
            "--demo-scene=track needs {} set to an audio file",
            flags::DEMO_TRACK_ENV
        ));
    }
    Ok(track)
}

/// Ingests the demo tracks into the scratch library at `root`: for `--demo-scene=track` the
/// user's file (linked, sorting first) with the generated tracks, else the generated tracks.
pub fn ingest_demo_tracks(
    services: &mut LibraryServices,
    root: &Path,
    scene: Option<DemoScene>,
    user_track: Option<&Path>,
) -> Result<usize, AppError> {
    let (Some(DemoScene::Track), Some(source)) = (scene, user_track) else {
        return library_smoke::ingest_generated(services, root);
    };
    library_smoke::link_user_track(root, source)?;
    let count = library_smoke::ingest_generated_plus(services, root, 1)?;
    library_smoke::queue_all_for_analysis(services)?;
    Ok(count)
}

/// Loads the second demo track into deck B and parks it; parks deck A too unless `scene` places
/// it. A failed load only says so on stderr.
pub fn load_demo_deck_b(
    services: &mut LibraryServices,
    loader: &mut TrackLoader,
    engine: &DjEngine,
    scene: Option<DemoScene>,
) {
    match library_smoke::load_nth_into_deck(services, loader, engine, 1, 1) {
        Ok(_) => demo_scene::park(engine, 1),
        Err(error) => eprintln!("openmixer: demo deck B: {error}"),
    }
    // The sampler and stems scenes stage only the window, so deck A is parked for them too.
    if matches!(scene, None | Some(DemoScene::Sampler | DemoScene::Stems)) {
        demo_scene::park(engine, 0);
    }
}
