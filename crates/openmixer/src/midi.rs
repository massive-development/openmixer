//! The MIDI side of the app, shared between the preferences, the MIDI settings page and a feedback
//! thread. It owns the mapper (the one map), the input router over the device backend, the output
//! router that lights controllers, and the feedback unit that decides what to send.
//!
//! Lock order, to keep the control threads deadlock-free: backend, then feedback, then the output
//! ports. Input callbacks run on the device threads and take only the mapper's locks.

use om_core::sync::LockExt;
use om_core::{ParameterTarget, StateReader};
use om_midi::{
    system_clock, EchoGuard, LearnRequest, MidiBackend, MidiError, MidiFeedback, MidiInputRouter,
    MidiMapper, MidiOutputRouter, MidiOutputSink,
};
use om_ui::settings::{MidiHost, MidiInputs, SettingsError};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

/// How often feedback is evaluated (LED changes and blinks).
pub const FEEDBACK_HZ: u32 = 30;
/// How often the device list is checked for hot-plug: about once a second.
const HOTPLUG_TICKS: u32 = FEEDBACK_HZ;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock_unpoisoned()
}

/// The inputs the preferences selected: every input, or the listed ids.
#[derive(Clone, Debug, Default)]
struct Selection {
    all: bool,
    ids: Vec<String>,
}

pub struct MidiService {
    mapper: Arc<MidiMapper>,
    input: MidiInputRouter,
    output: Arc<MidiOutputRouter>,
    feedback: Mutex<MidiFeedback>,
    backend: Mutex<Box<dyn MidiBackend>>,
    selection: Mutex<Selection>,
}

impl MidiService {
    /// A service over `backend`, reading engine state for feedback through `state`. Nothing is
    /// opened until inputs are selected.
    pub fn new(backend: Box<dyn MidiBackend>, state: Arc<dyn StateReader>) -> Arc<Self> {
        let mapper = Arc::new(MidiMapper::new());
        // Feedback sends and the inputs share one echo guard, so a controller's LED messages
        // coming back on its input are not heard as presses.
        let echo = Arc::new(EchoGuard::new());
        let output = Arc::new(MidiOutputRouter::with_echo(Arc::clone(&echo)));
        mapper.set_state_reader(Arc::clone(&state));
        let mut feedback = MidiFeedback::new(state, Arc::clone(&mapper), Some(system_clock()));
        let sink: Arc<dyn MidiOutputSink> = output.clone();
        feedback.set_sink(Some(sink));
        Arc::new(Self {
            input: MidiInputRouter::with_echo(Arc::clone(&mapper), echo),
            mapper,
            output,
            feedback: Mutex::new(feedback),
            backend: Mutex::new(backend),
            selection: Mutex::new(Selection::default()),
        })
    }

    /// Where produced parameter updates go (the app's parameter router).
    pub fn set_target(&self, target: Arc<dyn ParameterTarget>) {
        self.mapper.set_target(target);
    }

    pub fn mapper(&self) -> &Arc<MidiMapper> {
        &self.mapper
    }

    /// Ids of the inputs that are open now.
    pub fn open_inputs(&self) -> Vec<String> {
        self.input.open_ids()
    }

    /// Replaces the live map with the file's. A bad file changes nothing.
    pub fn load_map_file(&self, file: &Path) -> Result<(), MidiError> {
        self.mapper.load_from_file(file)
    }

    /// Writes the live map to `file`.
    pub fn save_map_file(&self, file: &Path) -> Result<(), MidiError> {
        self.mapper.save_to_file(file)
    }

    /// Selects the inputs (every one when `all`), opens them, and opens the outputs paired with
    /// them. A port that fails is reported on stderr and the rest still open.
    pub fn select_inputs(&self, all: bool, ids: &[String]) {
        *lock(&self.selection) = Selection {
            all,
            ids: ids.to_vec(),
        };
        let mut backend = lock(&self.backend);
        match self.input.enable(backend.as_mut(), all, ids) {
            Ok(report) => {
                for failure in &report.failed {
                    eprintln!("openmixer: MIDI input {failure}");
                }
            }
            Err(error) => eprintln!("openmixer: MIDI inputs: {error}"),
        }
        self.reopen_outputs(backend.as_mut(), all, ids);
    }

    /// Hot-plug: reconciles the open ports with the devices present now. True when anything changed.
    pub fn refresh(&self) -> bool {
        let selection = lock(&self.selection).clone();
        let mut backend = lock(&self.backend);
        match self.input.refresh(backend.as_mut()) {
            Ok(true) => {
                self.reopen_outputs(backend.as_mut(), selection.all, &selection.ids);
                true
            }
            Ok(false) => false,
            Err(error) => {
                eprintln!("openmixer: MIDI hot-plug: {error}");
                false
            }
        }
    }

    /// Input device names (what the settings page lists). Empty when the backend cannot enumerate.
    pub fn available_inputs(&self) -> Vec<String> {
        match lock(&self.backend).inputs() {
            Ok(ports) => ports.into_iter().map(|p| p.name).collect(),
            Err(error) => {
                eprintln!("openmixer: MIDI inputs: {error}");
                Vec::new()
            }
        }
    }

    /// One feedback pass. The feedback thread calls this at [`FEEDBACK_HZ`]; returns messages sent.
    pub fn feedback_tick(&self) -> usize {
        lock(&self.feedback).update()
    }

    /// Starts the feedback thread. It stops when the returned handle drops.
    pub fn start_feedback(self: &Arc<Self>) -> io::Result<FeedbackThread> {
        let service = Arc::clone(self);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let period = Duration::from_secs(1) / FEEDBACK_HZ;
        let handle = std::thread::Builder::new()
            .name("openmixer-midi-feedback".to_owned())
            .spawn(move || {
                let mut tick: u32 = 0;
                while !flag.load(Ordering::Acquire) {
                    service.feedback_tick();
                    tick = tick.wrapping_add(1);
                    if tick.is_multiple_of(HOTPLUG_TICKS) {
                        service.refresh();
                    }
                    std::thread::sleep(period);
                }
            })?;
        Ok(FeedbackThread {
            stop,
            handle: Some(handle),
        })
    }

    fn reopen_outputs(&self, backend: &mut dyn MidiBackend, all: bool, ids: &[String]) {
        if let Err(error) = self.output.open_paired_with(backend, ids, all) {
            eprintln!("openmixer: MIDI outputs: {error}");
        }
        lock(&self.feedback).resend();
    }
}

/// Stops and joins the feedback thread on drop.
pub struct FeedbackThread {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for FeedbackThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The preferences, the settings page's learn and map controls, and the input selection all drive
/// the service through this one trait.
impl MidiHost for MidiService {
    fn load_map(&self, file: &Path) -> Result<(), SettingsError> {
        self.load_map_file(file)
            .map_err(|e| SettingsError::Midi(Box::new(e)))
    }

    fn enable_inputs(&self, enable_all: bool, inputs: &[String]) {
        self.select_inputs(enable_all, inputs);
    }

    fn arm_learn(&self, target: &str) {
        self.mapper.arm_learn(LearnRequest::for_target(target));
    }

    fn cancel_learn(&self) {
        self.mapper.cancel_learn();
    }

    fn is_learning(&self) -> bool {
        self.mapper.is_learning()
    }

    fn save_map(&self, file: &Path) -> Result<(), SettingsError> {
        self.save_map_file(file)
            .map_err(|e| SettingsError::Midi(Box::new(e)))
    }

    fn map_generation(&self) -> u64 {
        self.mapper.generation()
    }
}

/// The MIDI settings page lists the input devices the host sees.
impl MidiInputs for MidiService {
    fn input_names(&self) -> Vec<String> {
        self.available_inputs()
    }
}
