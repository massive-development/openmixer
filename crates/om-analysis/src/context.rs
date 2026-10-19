//! Cancellation and progress reporting for analysis steps. Port of the `AnalysisContext` family in
//! `src/analysis/Analyzer.h`. Methods take `&self`: implementations that cross threads keep their
//! state in atomics.

/// Cancellation and progress for one analysis run.
pub trait AnalysisContext {
    fn is_cancelled(&self) -> bool;
    /// Progress 0..1 within the current step.
    fn set_progress(&self, fraction: f32);
}

/// Context for direct (non-service) calls: never cancelled, progress ignored.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAnalysisContext;

impl AnalysisContext for NullAnalysisContext {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn set_progress(&self, _fraction: f32) {}
}

/// Maps a sub-step's 0..1 progress into `[start, start + length]` of a parent context.
pub struct SubProgressContext<'a> {
    parent: &'a dyn AnalysisContext,
    base: f32,
    span: f32,
}

impl<'a> SubProgressContext<'a> {
    pub fn new(parent: &'a dyn AnalysisContext, start: f32, length: f32) -> Self {
        Self {
            parent,
            base: start,
            span: length,
        }
    }
}

impl AnalysisContext for SubProgressContext<'_> {
    fn is_cancelled(&self) -> bool {
        self.parent.is_cancelled()
    }

    fn set_progress(&self, fraction: f32) {
        self.parent
            .set_progress(self.base + self.span * fraction.clamp(0.0, 1.0));
    }
}
