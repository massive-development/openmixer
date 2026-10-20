//! Automix in the app: the shared handle the rest of the app talks to ([`AutomixShared`]) and the
//! runner the control thread steps ([`AutomixRunner`]). The audio thread is never involved: the
//! runner reads `DjEngine::snapshot()` and sends `Command::Param` through the parameter router, the
//! same path MIDI and the UI use, so every automix move is an ordinary control change.
//!
//! - `automix.enable | start | cancel | style` reach [`AutomixControl`] through the typed
//!   `Namespace::Automix` target the composition registers. They only record a request.
//! - The host ([`om_automix::AutomixHost`]: queue, track loading, analysis lookup) is created on the
//!   control thread from a factory ([`AutomixShared::set_host_factory`]) because the library and the
//!   track loader are not `Send`. Without a host, automix stays idle (the enable flag still reads
//!   back). The host over the window's `TrackLoader` belongs to the UI panel work (A5).
//! - While automix is off and idle the runner does nothing, not even a snapshot.
//! - [`AutomixShared::notify_user_input`] is for the binding layer (MIDI mapper, UI controls) to call
//!   for user input only; the runner's own commands do not pass through it.
//!
//! Threads: `AutomixShared` is shared by any thread; `AutomixRunner` lives on the control thread.

use om_automix::{AutomixConfig, AutomixControl, AutomixController, AutomixHost};
use om_core::{AutomixSnapshot, CommandSink, EngineSnapshot, ParamId};
use om_engine::DjEngine;
use std::sync::{Arc, Mutex, MutexGuard};

/// Builds the host on the control thread.
pub type HostFactory = Box<dyn FnOnce() -> Box<dyn AutomixHost> + Send>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the app shares: the control target, the published snapshot and the inboxes.
pub struct AutomixShared {
    control: Arc<AutomixControl>,
    snapshot: Mutex<AutomixSnapshot>,
    factory: Mutex<Option<HostFactory>>,
    notices: Mutex<Vec<ParamId>>,
    config: Mutex<AutomixConfig>,
}

impl AutomixShared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            control: Arc::new(AutomixControl::new()),
            snapshot: Mutex::new(AutomixSnapshot::default()),
            factory: Mutex::new(None),
            notices: Mutex::new(Vec::new()),
            config: Mutex::new(AutomixConfig::default()),
        })
    }

    /// The `automix.*` target to register with the router.
    pub fn control(&self) -> Arc<AutomixControl> {
        Arc::clone(&self.control)
    }

    /// The controller's latest snapshot.
    pub fn snapshot(&self) -> AutomixSnapshot {
        lock(&self.snapshot).clone()
    }

    /// Puts the controller's snapshot into an engine snapshot (the engine leaves it at default).
    pub fn overlay(&self, snap: &mut EngineSnapshot) {
        snap.automix = self.snapshot();
    }

    /// Supplies the host. It is built once, on the control thread, at the next step.
    pub fn set_host_factory(&self, factory: HostFactory) {
        *lock(&self.factory) = Some(factory);
    }

    /// Settings for the controller; read when the runner starts.
    pub fn set_config(&self, config: AutomixConfig) {
        *lock(&self.config) = config;
    }

    /// Reports user input on `id` (MIDI mapper, UI). If it touches the decks being mixed, the
    /// running plan is cancelled on the next control tick.
    pub fn notify_user_input(&self, id: &ParamId) {
        if !matches!(id, ParamId::Automix(_)) && self.control.is_enabled() {
            lock(&self.notices).push(id.clone());
        }
    }
}

/// The control thread's side: the controller, its host and the clock.
pub struct AutomixRunner {
    shared: Arc<AutomixShared>,
    controller: AutomixController,
    host: Option<Box<dyn AutomixHost>>,
}

impl AutomixRunner {
    pub fn new(shared: Arc<AutomixShared>) -> Self {
        let config = lock(&shared.config).clone();
        let controller = AutomixController::new(config, shared.control());
        Self {
            shared,
            controller,
            host: None,
        }
    }

    /// One control tick at `now` seconds. Builds a snapshot only when automix is on or busy.
    pub fn step(&mut self, engine: &DjEngine, sink: &dyn CommandSink, now: f64) {
        if self.host.is_none() {
            if let Some(factory) = lock(&self.shared.factory).take() {
                self.host = Some(factory());
            }
        }
        let busy = self.shared.control.is_enabled()
            || self.controller.state() != om_core::AutomixState::Idle;
        if busy {
            let notices: Vec<ParamId> = std::mem::take(&mut *lock(&self.shared.notices));
            for id in &notices {
                self.controller.notify_user_input(id);
            }
            if let Some(host) = self.host.as_deref_mut() {
                let snap = engine.snapshot();
                self.controller.tick(now, &snap, sink, host);
            }
        }
        let latest = self.controller.snapshot();
        let mut published = lock(&self.shared.snapshot);
        if *published != latest {
            *published = latest;
        }
    }
}
