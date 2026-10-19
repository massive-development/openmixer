//! The audio settings model over the mock backend. No real device is opened: `MockBackend` only.
//! Covers the audio settings model behaviour: device types, devices, rates, buffers, the
//! output pairs, restore of the stored state, and the device readout.

use om_audio::AudioCallback;
use om_ui::preferences::{AudioPrefs, Preferences};
use om_ui::settings::{AudioSettingsModel, AudioSettingsPage};
use openmixer::app_targets::AudioDeviceControl;
use openmixer::audio_settings::{Current, MockBackend, OutputSettings, SharedOutput};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Counts how often the callback was prepared and released, and renders silence.
struct Counting {
    prepared: Arc<AtomicUsize>,
    released: Arc<AtomicUsize>,
}

impl AudioCallback for Counting {
    fn prepare(&mut self, _sample_rate: f64, _max_block_size: usize) {
        self.prepared.fetch_add(1, Ordering::SeqCst);
    }
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for channel in outputs.iter_mut() {
            channel.fill(0.0);
        }
    }
    fn release(&mut self) {
        self.released.fetch_add(1, Ordering::SeqCst);
    }
}

struct Probe {
    prepared: Arc<AtomicUsize>,
    released: Arc<AtomicUsize>,
}

impl Probe {
    fn new() -> Self {
        Self {
            prepared: Arc::new(AtomicUsize::new(0)),
            released: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn callback(&self) -> Box<dyn AudioCallback> {
        Box::new(Counting {
            prepared: Arc::clone(&self.prepared),
            released: Arc::clone(&self.released),
        })
    }
}

fn model() -> SharedOutput {
    SharedOutput::new(OutputSettings::new(Box::new(MockBackend::default())))
}

fn started(model: &SharedOutput) -> Probe {
    model.lock().refresh();
    let probe = Probe::new();
    model
        .lock()
        .start_up("")
        .expect("default mock device opens");
    model
        .lock()
        .start(probe.callback())
        .expect("callback starts on the mock");
    probe
}

#[test]
fn lists_device_types_devices_and_their_rates_and_buffers() {
    let model = model();
    model.lock().refresh();
    assert_eq!(model.device_types(), vec!["Mock".to_owned()]);
    assert_eq!(
        model.output_devices(),
        vec!["Mock Output".to_owned(), "Mock Stereo".to_owned()]
    );
    // Nothing is open yet, so there are no rates or buffers to offer.
    assert!(model.sample_rates().is_empty());
    assert!(model.buffer_sizes().is_empty());
    assert_eq!(model.output_channels(), 0);
}

#[test]
fn start_up_opens_the_default_device_on_every_output() {
    let model = model();
    model.lock().refresh();
    model.lock().start_up("").expect("opens");
    assert_eq!(model.device_type(), "Mock");
    assert_eq!(model.output_device(), "Mock Output");
    assert_eq!(model.output_channels(), 4, "all outputs, not one pair");
    assert_eq!(model.sample_rate(), 48_000);
    assert_eq!(model.sample_rates(), vec![44_100, 48_000, 96_000]);
    assert_eq!(model.buffer_size(), 512);
    assert_eq!(model.buffer_sizes(), vec![128, 256, 512, 1024]);
}

#[test]
fn changing_the_rate_restarts_the_callback_on_the_new_setup() {
    let model = model();
    let probe = started(&model);
    assert_eq!(probe.prepared.load(Ordering::SeqCst), 1);

    let mut model = model;
    model.set_sample_rate(96_000).expect("96 kHz is offered");
    assert_eq!(model.sample_rate(), 96_000);
    assert_eq!(
        probe.released.load(Ordering::SeqCst),
        0,
        "the callback is handed back and restarted, not released"
    );
    assert_eq!(
        probe.prepared.load(Ordering::SeqCst),
        2,
        "prepared for the new rate"
    );
    assert!(model.lock().device().is_some_and(|d| d.is_running()));
    model.lock().stop();
}

#[test]
fn a_failed_change_keeps_the_previous_setup_running() {
    let model = model();
    let probe = started(&model);
    let mut model = model;
    let error = model
        .set_sample_rate(22_050)
        .expect_err("22.05 kHz is not offered");
    assert!(error.contains("22050"), "error names the rate: {error}");
    assert_eq!(model.sample_rate(), 48_000, "the old rate is still open");
    assert!(model.lock().device().is_some_and(|d| d.is_running()));
    assert_eq!(probe.released.load(Ordering::SeqCst), 0);
    model.lock().stop();
}

#[test]
fn choosing_a_stereo_device_offers_one_pair_and_its_own_rates() {
    let model = model();
    started(&model);
    let mut model = model;
    model
        .set_output_device("Mock Stereo")
        .expect("stereo device opens");
    assert_eq!(model.output_channels(), 2);
    assert_eq!(model.sample_rate(), 44_100, "the device default rate");
    assert_eq!(model.sample_rates(), vec![44_100, 48_000]);
    assert_eq!(model.buffer_sizes(), vec![256, 512]);
    model.lock().stop();
}

#[test]
fn unknown_device_type_and_device_are_refused_and_nothing_changes() {
    let model = model();
    started(&model);
    let mut model = model;
    assert!(model.set_device_type("ASIO").is_err());
    assert!(model.set_output_device("No such device").is_err());
    assert_eq!(model.output_device(), "Mock Output");
    model.lock().stop();
}

#[test]
fn the_state_string_restores_the_same_setup_on_a_fresh_model() {
    let first = model();
    started(&first);
    let mut first = first;
    first.set_buffer_size(256).expect("256 is offered");
    first.lock().stop();
    let state = first.state_string();
    assert!(!state.is_empty());
    assert_eq!(
        Current::decode(&state)
            .expect("stored state decodes")
            .buffer_size,
        256
    );

    let second = model();
    second.lock().refresh();
    second.lock().start_up(&state).expect("restores");
    assert_eq!(second.buffer_size(), 256);
    assert_eq!(second.state_string(), state);
}

#[test]
fn an_unreadable_stored_state_falls_back_to_the_default_device() {
    let model = model();
    model.lock().refresh();
    model
        .lock()
        .start_up("{ not a setup")
        .expect("falls back to the default");
    assert_eq!(model.output_device(), "Mock Output");
    assert_eq!(model.sample_rate(), 48_000);
}

#[test]
fn a_stored_state_for_a_device_that_is_gone_falls_back() {
    let gone = Current {
        host: "Mock".to_owned(),
        device: "Removed device".to_owned(),
        sample_rate: 48_000,
        buffer_size: 512,
        channels: 2,
    }
    .encode();
    let model = model();
    model.lock().refresh();
    model.lock().start_up(&gone).expect("default opens");
    assert_eq!(model.output_device(), "Mock Output");
}

#[test]
fn the_control_side_initialises_a_device_from_a_state_and_reports_it() {
    let mut settings = OutputSettings::new(Box::new(MockBackend::default()));
    settings.refresh();
    settings.start_up("").expect("opens");
    let state = Current {
        host: "Mock".to_owned(),
        device: "Mock Stereo".to_owned(),
        sample_rate: 44_100,
        buffer_size: 256,
        channels: 2,
    }
    .encode();
    settings.initialise(&state).expect("initialises");
    assert_eq!(settings.current_state(), state);
    assert_eq!(settings.output_channels(), 2);
    assert!(settings.initialise("nonsense").is_err());
}

#[test]
fn the_pairs_survive_a_device_change_and_the_page_reads_them() {
    let model = model();
    started(&model);
    let pairs = model.lock().pairs();
    pairs.set_master_pair(1);
    pairs.set_cue_pair(0);
    let mut model = model;
    model
        .set_output_device("Mock Output")
        .expect("reopen on the same device");
    assert_eq!(
        model.lock().pairs().master_pair(),
        1,
        "routing is kept across opens"
    );

    let mut prefs = Preferences {
        audio: AudioPrefs {
            device_state: String::new(),
            master_pair: 1,
            cue_pair: 0,
        },
        ..Preferences::default()
    };
    let mut page = AudioSettingsPage::new(&mut prefs, &mut model);
    assert_eq!(page.pair_labels(), vec!["Out 1+2", "Out 3+4"]);
    assert_eq!(page.shown_cue_pair(), Some(0));
    page.set_output_device("Mock Output").expect("same device");
    assert!(
        !prefs.audio.device_state.is_empty(),
        "the page stores the device state after a change"
    );
    model.lock().stop();
}

#[test]
fn the_readout_is_empty_without_a_device_and_follows_the_device_when_open() {
    let model = model();
    assert_eq!(model.lock().stats().sample_rate, 0.0);
    started(&model);
    let stats = model.lock().stats();
    assert_eq!(stats.sample_rate, 48_000.0);
    assert_eq!(stats.buffer_size, 512);
    model.lock().stop();
}

#[test]
fn a_stop_hands_the_callback_back_once_and_a_second_stop_finds_nothing() {
    let model = model();
    let probe = started(&model);
    let callback = model.lock().stop();
    assert!(callback.is_some());
    assert!(model.lock().stop().is_none());
    drop(callback);
    assert_eq!(
        probe.released.load(Ordering::SeqCst),
        0,
        "the owner decides on release"
    );
}
