//! A separator whose model is loaded on its first job, on the worker thread.
//!
//! Loading an ONNX session takes hundreds of milliseconds and a lot of memory. The app creates the
//! stem service when the user first asks for stems, on the UI thread; a [`LazySeparator`] lets
//! that creation return at once, and the load happens inside the first job.

use crate::separator::{StemError, StemSeparator, StemSink, StereoSource};
use om_analysis::AnalysisContext;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// Builds the real separator. Runs once, on the worker thread, the first time a job needs it.
pub type SeparatorFactory = Box<dyn FnOnce() -> Result<Arc<dyn StemSeparator>, StemError> + Send>;

enum LazyState {
    Pending(SeparatorFactory),
    Ready(Arc<dyn StemSeparator>),
    /// The factory failed or panicked. Every later job fails with this message.
    Failed(String),
}

/// A [`StemSeparator`] that builds its inner separator on first use. The id, version and native
/// rate are given up front, so the cache key is known before the model is loaded.
pub struct LazySeparator {
    id: String,
    version: i32,
    native_rate: f64,
    loaded: AtomicBool,
    state: Mutex<LazyState>,
}

impl LazySeparator {
    pub fn new(
        id: impl Into<String>,
        version: i32,
        native_rate: f64,
        factory: SeparatorFactory,
    ) -> Self {
        Self {
            id: id.into(),
            version,
            native_rate,
            loaded: AtomicBool::new(false),
            state: Mutex::new(LazyState::Pending(factory)),
        }
    }

    /// The inner separator, loading it if this is the first call.
    fn resolve(&self) -> Result<Arc<dyn StemSeparator>, StemError> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        match std::mem::replace(
            &mut *state,
            LazyState::Failed("the stem model failed to load".into()),
        ) {
            LazyState::Ready(separator) => {
                *state = LazyState::Ready(Arc::clone(&separator));
                Ok(separator)
            }
            LazyState::Failed(message) => {
                *state = LazyState::Failed(message.clone());
                Err(StemError::Inference(message))
            }
            LazyState::Pending(factory) => match factory() {
                Ok(separator) => {
                    *state = LazyState::Ready(Arc::clone(&separator));
                    self.loaded.store(true, Ordering::Release);
                    Ok(separator)
                }
                Err(error) => {
                    let message = error.to_string();
                    *state = LazyState::Failed(message.clone());
                    Err(StemError::Inference(message))
                }
            },
        }
    }
}

impl StemSeparator for LazySeparator {
    fn id(&self) -> &str {
        &self.id
    }

    fn is_ready(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    fn version(&self) -> i32 {
        self.version
    }

    fn native_rate(&self) -> f64 {
        self.native_rate
    }

    fn separate_to(
        &self,
        input: &mut dyn StereoSource,
        sink: &mut dyn StemSink,
        ctx: &dyn AnalysisContext,
    ) -> Result<(), StemError> {
        self.resolve()?.separate_to(input, sink, ctx)
    }
}
