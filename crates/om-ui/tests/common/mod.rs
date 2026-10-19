//! Shared rig for the widget tests: a real engine behind the real router, with a recorder in front
//! that keeps every routed id so tests check both the id and the engine effect.

#![allow(dead_code)]

use om_core::{ParameterRouter, ParameterTarget};
use om_engine::grid::{BeatGrid, FixedTempoGrid};
use om_engine::DjEngine;
use std::sync::{Arc, Mutex};

pub struct Recorder {
    forward: Arc<ParameterRouter>,
    calls: Mutex<Vec<(String, f64)>>,
}

impl ParameterTarget for Recorder {
    fn set_parameter(&self, id: &str, value: f64) -> bool {
        self.calls.lock().unwrap().push((id.to_owned(), value));
        self.forward.set_parameter(id, value)
    }
}

impl Recorder {
    pub fn calls(&self) -> Vec<(String, f64)> {
        self.calls.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.calls.lock().unwrap().clear();
    }

    pub fn saw(&self, id: &str, value: f64) -> bool {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(i, v)| i == id && *v == value)
    }

    pub fn len(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct Rig {
    pub engine: Arc<DjEngine>,
    pub router: Arc<ParameterRouter>,
    pub recorder: Arc<Recorder>,
}

impl Rig {
    pub fn new(decks: usize) -> Self {
        let (engine, _audio) = DjEngine::new(decks);
        let engine = Arc::new(engine);
        let router = Arc::new(ParameterRouter::new());
        engine.register(&router);
        let recorder = Arc::new(Recorder {
            forward: router.clone(),
            calls: Mutex::new(Vec::new()),
        });
        Self {
            engine,
            router,
            recorder,
        }
    }

    pub fn target(&self) -> Arc<dyn ParameterTarget> {
        self.recorder.clone()
    }

    pub fn grid(bpm: f64) -> Arc<dyn BeatGrid> {
        Arc::new(FixedTempoGrid::new(bpm, 44_100.0, 0))
    }
}
