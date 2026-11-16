//! The Tauri commands the front end calls. Tauri runs them on the main thread or, for the ones
//! marked `async`, on its async runtime; never on the audio thread. None waits on a plugin.

use crate::deck_info::{self, DeckInfo};
use crate::decks::Decks;
use crate::stream::{PacketSink, StreamConfig};
use crate::stream_slot::StreamSlot;
use om_core::ParameterRouter;
use om_engine::DjEngine;
use om_ui_bridge::{dispatch_control, slice_control_ids, FILL, PLAYED_BRIGHTNESS};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use tauri::ipc::{Channel, InvokeResponseBody, Response};
use tauri::{AppHandle, State};

/// What the commands share for the life of the window.
pub(crate) struct ShellState {
    pub(crate) theme_css: String,
    pub(crate) smoke: bool,
    pub(crate) ready: Arc<AtomicBool>,
    pub(crate) ready_signal: Sender<()>,
    pub(crate) streams: Arc<StreamSlot>,
    pub(crate) decks: Arc<Decks>,
}

/// The waveform constants the shader shares with the GPUI strip.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformStyle {
    /// Share of the half-height a full-scale column reaches.
    pub fill: f32,
    /// Brightness the played part keeps.
    pub played_brightness: f32,
}

/// The active theme as one `:root` block of `--om-*` variables.
#[tauri::command]
pub fn theme_css(state: State<'_, ShellState>) -> String {
    state.theme_css.clone()
}

/// Whether this is a smoke run, so the front end proves the stream before it calls `ready`.
#[tauri::command]
pub fn smoke(state: State<'_, ShellState>) -> bool {
    state.smoke
}

/// Called by the front end after its first paint; a smoke run ends here with code 0.
#[tauri::command]
pub fn ready(app: AppHandle, state: State<'_, ShellState>) {
    state.ready.store(true, Ordering::Release);
    // The smoke watchdog may already have given up; nothing else listens.
    let _ = state.ready_signal.send(());
    tracing::info!(smoke = state.smoke, "front end ready");
    if state.smoke {
        app.exit(0);
    }
}

/// The waveform constants of the GPUI strip.
#[tauri::command]
pub fn waveform_style() -> WaveformStyle {
    WaveformStyle {
        fill: FILL,
        played_brightness: PLAYED_BRIGHTNESS,
    }
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

/// Front-end keys to canonical control ids for the engine's decks and channels.
#[tauri::command]
pub fn control_ids(engine: State<'_, Arc<DjEngine>>) -> HashMap<String, String> {
    let decks = u8::try_from(engine.num_decks()).unwrap_or(u8::MAX);
    let channels = u8::try_from(engine.mixer().num_channels()).unwrap_or(u8::MAX);
    slice_control_ids(decks, channels).into_iter().collect()
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

/// Deck `deck`'s packed waveform peaks; empty while it has none.
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

/// Raw packets into the front end's channel; a failed send means the webview is gone.
struct ChannelSink(Channel<InvokeResponseBody>);

impl PacketSink for ChannelSink {
    fn send(&mut self, bytes: &[u8]) -> bool {
        self.0.send(InvokeResponseBody::Raw(bytes.to_vec())).is_ok()
    }
}
