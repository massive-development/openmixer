//! Automix in the app: the shared handle the rest of the app talks to ([`AutomixShared`]) and the
//! runner the window steps ([`AutomixRunner`]). The audio thread is never involved: the runner
//! reads `DjEngine::snapshot()` and sends `Command::Param` through [`AutomixSink`], so every automix
//! move is an ordinary control change that the user-input observer does not see.
//!
//! - `automix.enable | start | cancel | style` reach [`AutomixControl`] through the typed
//!   `Namespace::Automix` target the composition registers. They only record a request.
//! - The host ([`om_automix::AutomixHost`], see [`crate::automix_host`]) reads the library and the
//!   track loader, which are not `Send`. It is therefore handed to the runner on the window thread
//!   ([`AutomixRunner::set_host`]), and the runner is stepped from the window's frame tick. Without
//!   a host automix stays idle (the enable flag still reads back).
//! - While automix is off and idle the runner does nothing, not even a snapshot.
//! - User input reaches the controller through the router's input observer
//!   ([`om_core::ParameterRouter::set_input_observer`]), which the composition points at
//!   [`AutomixShared::notify_user_input`].
//!
//! Threads: `AutomixShared` is shared by any thread; `AutomixRunner` and its host live on the
//! window thread.

use om_automix::{AutomixConfig, AutomixControl, AutomixController, AutomixHost};
use om_core::{AutomixSnapshot, CommandSink, EngineSnapshot, ParamId};
use om_engine::DjEngine;
use std::sync::{Arc, Mutex, MutexGuard};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the app shares: the control target, the published snapshot and the inboxes.
pub struct AutomixShared {
    control: Arc<AutomixControl>,
    snapshot: Mutex<AutomixSnapshot>,
    notices: Mutex<Vec<ParamId>>,
    config: Mutex<AutomixConfig>,
}

impl AutomixShared {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            control: Arc::new(AutomixControl::new()),
            snapshot: Mutex::new(AutomixSnapshot::default()),
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

    /// Settings for the controller; read when the runner is created.
    pub fn set_config(&self, config: AutomixConfig) {
        *lock(&self.config) = config;
    }

    /// Reports user input on `id`. If it touches the decks being mixed, the running plan is
    /// cancelled on the next tick. Automix's own ids are ignored.
    pub fn notify_user_input(&self, id: &ParamId) {
        if !matches!(id, ParamId::Automix(_)) && self.control.is_enabled() {
            lock(&self.notices).push(id.clone());
        }
    }
}

/// The window's side: the controller, its host and the sink its steps go through.
pub struct AutomixRunner {
    shared: Arc<AutomixShared>,
    controller: AutomixController,
    host: Option<Box<dyn AutomixHost>>,
    sink: Arc<dyn CommandSink>,
    /// When the controller last ran; the window's frame rate is faster than the control rate.
    last_step: Option<f64>,
}

/// The controller's step interval: the control clock's 33 ms (30 Hz).
pub const AUTOMIX_PERIOD: f64 = 1.0 / 30.0;

impl AutomixRunner {
    /// A runner that sends its steps through `sink`, which must not notify the input observer
    /// (use [`om_core::AutomixSink`]).
    pub fn new(shared: Arc<AutomixShared>, sink: Arc<dyn CommandSink>) -> Self {
        let config = lock(&shared.config).clone();
        let controller = AutomixController::new(config, shared.control());
        Self {
            shared,
            controller,
            host: None,
            sink,
            last_step: None,
        }
    }

    /// Gives the runner its host. Replaces any earlier one.
    pub fn set_host(&mut self, host: Box<dyn AutomixHost>) {
        self.host = Some(host);
    }

    /// The controller's state, for tests and the window.
    pub fn state(&self) -> om_core::AutomixState {
        self.controller.state()
    }

    /// One step at `now` seconds on any monotonic clock. Does nothing without a host, does nothing
    /// while automix is off and idle, and runs at most once per [`AUTOMIX_PERIOD`] (so the window's
    /// frame rate does not shorten the controller's tick-based grace periods).
    pub fn step(&mut self, engine: &DjEngine, now: f64) {
        if self
            .last_step
            .is_some_and(|last| now - last < AUTOMIX_PERIOD)
        {
            return;
        }
        self.last_step = Some(now);
        let busy = self.shared.control.is_enabled()
            || self.controller.state() != om_core::AutomixState::Idle;
        if busy {
            let notices: Vec<ParamId> = std::mem::take(&mut *lock(&self.shared.notices));
            for id in &notices {
                self.controller.notify_user_input(id);
            }
            if let Some(host) = self.host.as_deref_mut() {
                let snap = engine.snapshot();
                self.controller.tick(now, &snap, self.sink.as_ref(), host);
            }
        }
        let latest = self.controller.snapshot();
        let mut published = lock(&self.shared.snapshot);
        if *published != latest {
            *published = latest;
        }
    }

}
