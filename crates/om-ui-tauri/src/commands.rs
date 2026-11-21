//! The Tauri commands the front end calls. Tauri runs them on the main thread or, for the ones
//! marked `async`, on its async runtime; never on the audio thread. None waits on a plugin.

use crate::bench::BenchRun;
use crate::controls::{control_dtos, ControlDto};
use crate::deck_info::{self, DeckInfo};
use crate::decks::Decks;
use crate::drive::{parse_layout, write_json_atomic, DriveLayout};
use crate::settings::{
    with_host, AppearanceChange, AppearancePage, AppearanceResult, AudioChange, AudioPage,
};
use crate::stream::{PacketSink, StreamConfig};
use crate::stream_slot::StreamSlot;
use crate::strip_ink::StripInkDto;
use crate::ShellTheme;
use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui::binding::MixerBinding;
use om_ui_bridge::{dispatch_control, slice_controls, FILL, MIN_HALF_PX, PLAYED_BRIGHTNESS};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use tauri::ipc::{Channel, InvokeResponseBody, Response};
use tauri::{AppHandle, State};

/// What the commands share for the life of the window.
pub(crate) struct ShellState {
    /// The active theme; an appearance edit replaces it.
    pub(crate) theme: Mutex<ShellTheme>,
    pub(crate) config: RunConfig,
    /// Set for a `--ui-bench` run.
    pub(crate) bench: Option<Arc<BenchRun>>,
    pub(crate) smoke_play: bool,
    pub(crate) ready: Arc<AtomicBool>,
    pub(crate) ready_signal: Sender<()>,
    pub(crate) streams: Arc<StreamSlot>,
    pub(crate) decks: Arc<Decks>,
    /// `--ui-drive-out`: where `ui_layout` writes the layout map; `None` refuses it.
    pub(crate) drive_layout: Option<PathBuf>,
}

/// The waveform constants the shader shares with the GPUI strip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformStyle {
    /// Share of the half-height a full-scale column reaches.
    pub fill: f32,
    /// Brightness the played part keeps.
    pub played_brightness: f32,
    /// A column with any height reaches at least this far from the centre line, in px.
    pub min_half_px: f32,
}

/// The active theme as one `:root` block of `--om-*` variables.
#[tauri::command]
pub fn theme_css(state: State<'_, ShellState>) -> String {
    crate::lock(&state.theme).css.clone()
}

/// How the shell runs the window, as the front end reads it once at start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfig {
    /// The front end proves the stream before it calls `ready`.
    pub smoke: bool,
    /// Seconds of frames to record (`--ui-bench`).
    pub bench_seconds: Option<u32>,
    /// The waveform lanes' frame cap; 0 is uncapped.
    pub fps_cap: u32,
    /// The notice the top bar shows (the mock device); null for none.
    pub banner: Option<String>,
    /// Whether the top bar shows the frame counter; when not, the front end does not compute it.
    pub hud: bool,
    /// A `--ui-drive-out` run: the front end sends its layout map through `ui_layout`.
    pub drive: bool,
}

/// The run's smoke, bench and frame-cap settings.
#[tauri::command]
pub fn run_config(state: State<'_, ShellState>) -> RunConfig {
    state.config.clone()
}

/// The bench run's frame statistics: checked, written, and the window exits (1 when refused).
#[tauri::command]
pub fn bench_done(
    app: AppHandle,
    state: State<'_, ShellState>,
    stats: serde_json::Value,
) -> Result<(), String> {
    let Some(bench) = &state.bench else {
        return Err("bench_done outside a --ui-bench run".to_owned());
    };
    let result = bench.finish(stats);
    match &result {
        Ok(()) => {
            tracing::info!(seconds = bench.seconds, out = %bench.out.display(), "bench written")
        }
        Err(error) => tracing::error!(%error, "bench refused"),
    }
    app.exit(if result.is_ok() { 0 } else { 1 });
    result
}

/// Whether the smoke run must start deck A through `control` before `ready`: a library loads it.
#[tauri::command]
pub fn smoke_play(state: State<'_, ShellState>) -> bool {
    state.smoke_play
}

/// Called by the front end after its first paint; a smoke run ends here with code 0.
#[tauri::command]
pub fn ready(app: AppHandle, state: State<'_, ShellState>) {
    state.ready.store(true, Ordering::Release);
    // The smoke watchdog may already have given up; nothing else listens.
    let _ = state.ready_signal.send(());
    tracing::info!(smoke = state.config.smoke, "front end ready");
    if state.config.smoke {
        app.exit(0);
    }
}

/// The waveform constants of the GPUI strip.
#[tauri::command]
pub fn waveform_style() -> WaveformStyle {
    WaveformStyle {
        fill: FILL,
        played_brightness: PLAYED_BRIGHTNESS,
        min_half_px: MIN_HALF_PX,
    }
}

/// Deck `deck`'s waveform lane colours and marker sizes.
#[tauri::command]
pub fn strip_ink(deck: usize, state: State<'_, ShellState>) -> Result<StripInkDto, String> {
    crate::lock(&state.theme)
        .strip_inks
        .get(deck)
        .cloned()
        .ok_or_else(|| format!("no strip ink for deck {deck}"))
}

/// Writes one control: the id is parsed and validated here, off the audio thread.
#[tauri::command]
pub fn control(
    router: State<'_, Arc<ParameterRouter>>,
    id: String,
    value: f64,
) -> Result<(), String> {
    dispatch_control(&router, &id, value).map_err(|e| e.to_string())
}

/// Front-end keys to each control's id, range, default and kind, from the GPUI widget models.
#[tauri::command]
pub fn controls(
    engine: State<'_, Arc<DjEngine>>,
    router: State<'_, Arc<ParameterRouter>>,
) -> HashMap<String, ControlDto> {
    slice_specs(&engine, &router)
}

fn slice_specs(
    engine: &Arc<DjEngine>,
    router: &Arc<ParameterRouter>,
) -> HashMap<String, ControlDto> {
    let decks = u8::try_from(engine.num_decks()).unwrap_or(u8::MAX);
    let mixer = MixerBinding::typed(Arc::clone(engine), Arc::clone(router));
    control_dtos(slice_controls(&mixer, decks))
}

/// The controls' rectangles for an outside driver: checked, then written to the drive path.
#[tauri::command]
pub fn ui_layout(
    engine: State<'_, Arc<DjEngine>>,
    router: State<'_, Arc<ParameterRouter>>,
    state: State<'_, ShellState>,
    map: serde_json::Value,
) -> Result<(), String> {
    let Some(path) = &state.drive_layout else {
        return Err("ui_layout outside a --ui-drive-out run".to_owned());
    };
    let specs = slice_specs(&engine, &router);
    let result = parse_layout(map)
        .and_then(|layout| {
            let file = DriveLayout {
                layout: &layout,
                specs: &specs,
            };
            write_json_atomic(path, &file)
        })
        .map_err(|e| e.to_string());
    if let Err(error) = &result {
        tracing::warn!(%error, "layout map refused");
    }
    result
}

/// Streams snapshot packets into `channel` at 30 Hz, replacing any earlier stream.
#[tauri::command(async)]
pub fn start_stream(
    channel: Channel<InvokeResponseBody>,
    engine: State<'_, Arc<DjEngine>>,
    state: State<'_, ShellState>,
) -> Result<(), String> {
    let sink = ChannelSink(channel);
    let engine = Arc::clone(&engine);
    state
        .streams
        .start(engine, sink, StreamConfig::default())
        .map_err(|e| format!("could not start the stream: {e}"))
}

/// Deck `deck`'s peaks generation (u64 LE) then its packed waveform peaks, none while it has none.
#[tauri::command(async)]
pub fn deck_peaks(
    deck: usize,
    engine: State<'_, Arc<DjEngine>>,
    state: State<'_, ShellState>,
) -> Result<Response, String> {
    deck_info::deck_peaks(&state.decks, &engine, deck).map(Response::new)
}

/// Deck `deck`'s track info and peak layout.
#[tauri::command(async)]
pub fn deck_info(
    deck: usize,
    engine: State<'_, Arc<DjEngine>>,
    state: State<'_, ShellState>,
) -> Result<DeckInfo, String> {
    deck_info::deck_info(&state.decks, &engine, deck)
}

/// Queues a load of the audio file at `path` into `deck`. The control thread runs it within a
/// tick: the deck then changes (`deck-changed`), or `load-failed` says why it did not.
#[tauri::command]
pub fn load_file(deck: usize, path: String, state: State<'_, ShellState>) -> Result<(), String> {
    state.decks.request_load(deck, PathBuf::from(path))
}

/// The audio settings page as it stands.
#[tauri::command]
pub fn audio_settings() -> Result<AudioPage, String> {
    with_host(|host| host.audio())
}

/// One edit of the audio page: saved and applied, then the page as it stands.
#[tauri::command]
pub fn audio_change(change: AudioChange) -> Result<AudioPage, String> {
    with_host(|host| host.change_audio(change))?
}

/// The appearance settings page as it stands.
#[tauri::command]
pub fn appearance_settings() -> Result<AppearancePage, String> {
    with_host(|host| host.appearance())
}

/// One edit of the appearance page; the shell keeps the new theme for `theme_css` and `strip_ink`
/// and returns it so the front end can switch at once.
#[tauri::command]
pub fn appearance_change(
    change: AppearanceChange,
    state: State<'_, ShellState>,
) -> Result<AppearanceResult, String> {
    let result = with_host(|host| host.change_appearance(change))??;
    *crate::lock(&state.theme) = ShellTheme {
        css: result.css.clone(),
        strip_inks: result.strip_inks.clone(),
    };
    Ok(result)
}

/// Raw packets into the front end's channel; a failed send means the webview is gone.
struct ChannelSink(Channel<InvokeResponseBody>);

impl PacketSink for ChannelSink {
    fn send(&mut self, bytes: &[u8]) -> bool {
        self.0.send(InvokeResponseBody::Raw(bytes.to_vec())).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(banner: Option<&str>) -> String {
        let config = RunConfig {
            smoke: false,
            bench_seconds: None,
            fps_cap: 60,
            banner: banner.map(str::to_owned),
            hud: true,
            drive: false,
        };
        serde_json::to_string(&config).unwrap()
    }

    #[test]
    fn the_run_config_names_the_banner_or_null() {
        assert!(config(Some("Demo mode")).contains("\"banner\":\"Demo mode\""));
        assert!(config(None).contains("\"banner\":null"));
        assert!(config(None).contains("\"hud\":true"));
        assert!(config(None).contains("\"drive\":false"));
    }
}
