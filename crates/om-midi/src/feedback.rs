//! Controller feedback: evaluates the map's output rules against engine state and sends a note or
//! CC only when its value changes. Blinking rules change on a half-period clock. Control thread only; reads state through a [`StateReader`],
//! never touches the audio thread.

use crate::mapper::MidiMapper;
use crate::output_router::MidiOutputSink;
use crate::output_rule::OutputRule;
use om_core::StateReader;
use std::sync::Arc;
use std::time::Instant;

/// Milliseconds on a monotonic clock.
pub type Clock = Box<dyn Fn() -> f64 + Send>;

pub struct MidiFeedback {
    state: Arc<dyn StateReader>,
    mapper: Arc<MidiMapper>,
    clock: Clock,
    sink: Option<Arc<dyn MidiOutputSink>>,
    seen_generation: Option<u64>,
    rules: Vec<OutputRule>,
    /// Per rule: the value last sent, or `None` after a reset.
    last_sent: Vec<Option<i32>>,
}

/// The system clock, in milliseconds from when it was created.
pub fn system_clock() -> Clock {
    let start = Instant::now();
    Box::new(move || start.elapsed().as_secs_f64() * 1000.0)
}

impl MidiFeedback {
    /// `clock` defaults to [`system_clock`]; tests inject their own.
    pub fn new(state: Arc<dyn StateReader>, mapper: Arc<MidiMapper>, clock: Option<Clock>) -> Self {
        Self {
            state,
            mapper,
            clock: clock.unwrap_or_else(system_clock),
            sink: None,
            seen_generation: None,
            rules: Vec::new(),
            last_sent: Vec::new(),
        }
    }

    /// `None` stops sending. A new sink gets every LED re-sent on the next update.
    pub fn set_sink(&mut self, sink: Option<Arc<dyn MidiOutputSink>>) {
        self.sink = sink;
        self.resend();
    }

    /// Forgets what was sent: the next update re-sends every LED (controller reconnected).
    pub fn resend(&mut self) {
        self.last_sent = vec![None; self.rules.len()];
    }

    /// One evaluation pass at the clock's current time. Returns how many messages were sent.
    pub fn update(&mut self) -> usize {
        let Some(sink) = self.sink.clone() else {
            return 0;
        };
        if self.seen_generation != Some(self.mapper.generation()) {
            self.reload_rules();
        }
        let now = (self.clock)();
        let mut sent = 0;
        for (i, rule) in self.rules.iter().enumerate() {
            // An unknown id (a plugin that is not loaded) leaves its LED alone.
            let Some(state) = self.state.read_state(&rule.source) else {
                continue;
            };
            let value = rule.evaluate(state, now);
            if self.last_sent[i] == Some(value) {
                continue;
            }
            sink.send(&rule.to_message(value));
            self.last_sent[i] = Some(value);
            sent += 1;
        }
        sent
    }

    /// A map load or edit changes the rules: take the new set and light everything again.
    fn reload_rules(&mut self) {
        self.seen_generation = Some(self.mapper.generation());
        self.rules = self.mapper.get_outputs();
        self.resend();
    }
}
