//! Start-up steps both windows share (GPUI in the binary's `gui.rs`, Tauri in `gui_tauri.rs`): the
//! silent mock output, and starting an output on the engine once the preferences reached it.

use crate::audio_settings::{MockBackend, OutputBackend, OutputSettings, SharedOutput};
use crate::composition::AppComposition;
use crate::AppError;
use om_audio::DjEngineAudio;

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
