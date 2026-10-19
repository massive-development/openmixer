//! Analyser extension point. The
//! job service (a later step) runs these on worker threads, possibly concurrently, so implementations
//! are stateless or internally synchronised.

use crate::{AnalysisContext, AudioClip};

/// Interface version every analyser must report; the registry rejects other versions.
pub const INTERFACE_VERSION: i32 = 1;

pub trait Analyzer: Send + Sync {
    /// Bump when the wire format of the payload changes.
    fn interface_version(&self) -> i32 {
        INTERFACE_VERSION
    }

    fn id(&self) -> &str;
    /// Bump when the output changes so cached results from older versions are recomputed.
    fn version(&self) -> i32;

    /// JSON payload for the cache; `None` when cancelled or the clip cannot be analysed.
    fn analyze(&self, clip: &AudioClip, context: &dyn AnalysisContext) -> Option<String>;
}
