//! Analysis result store keyed by (content hash, analyser id); a version mismatch counts as a miss.
//! Implementations are shared between worker threads.

pub trait ResultCache: Send + Sync {
    fn find(&self, content_hash: &str, analyzer_id: &str, version: i32) -> Option<String>;

    /// True when the payload was written.
    fn store(&self, content_hash: &str, analyzer_id: &str, version: i32, payload: &str) -> bool;

    /// Number of rows removed.
    fn erase(&self, content_hash: &str) -> usize;
}
