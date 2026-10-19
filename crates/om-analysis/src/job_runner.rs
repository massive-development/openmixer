//! Runs one job to completion on the calling thread: hash, cache lookup, decode on a miss, analyse,
//! store. Progress: 0-5% hash, 5-20% decode, 20-100% split across the analysers that must run.

use crate::{
    AnalysisContext, AnalysisJob, Analyzer, AnalyzerRegistry, AudioClip, ClipSource, JobStatus,
    ResultCache,
};
use std::sync::Arc;

const HASH_END: f32 = 0.05;
const LOAD_END: f32 = 0.2;

/// Maps a step's 0..1 progress into its slice of the job's progress.
struct StepContext<'a> {
    job: &'a AnalysisJob,
    base: f32,
    span: f32,
}

impl AnalysisContext for StepContext<'_> {
    fn is_cancelled(&self) -> bool {
        self.job.is_cancelled()
    }

    fn set_progress(&self, fraction: f32) {
        self.job
            .set_progress(self.base + self.span * fraction.clamp(0.0, 1.0));
    }
}

/// Runs `analyzer_ids` against `source` and leaves the job in its final status. Results already
/// in `cache` are reused; new results are stored there. Listeners are the caller's business.
pub fn run_analysis_job(
    job: &AnalysisJob,
    source: &mut dyn ClipSource,
    analyzer_ids: &[String],
    registry: &AnalyzerRegistry,
    cache: Option<&dyn ResultCache>,
) {
    if !job.is_cancelled() {
        job.set_status(JobStatus::Running);
        match resolve(analyzer_ids, registry) {
            Ok(analyzers) => process(job, source, &analyzers, cache),
            Err(missing) => job.fail(format!("unknown analyzer: {missing}")),
        }
    }
    finish(job);
}

fn resolve(ids: &[String], registry: &AnalyzerRegistry) -> Result<Vec<Arc<dyn Analyzer>>, String> {
    ids.iter()
        .map(|id| registry.find(id).ok_or_else(|| id.clone()))
        .collect()
}

fn process(
    job: &AnalysisJob,
    source: &mut dyn ClipSource,
    analyzers: &[Arc<dyn Analyzer>],
    cache: Option<&dyn ResultCache>,
) {
    let Some(hash) = source.content_hash() else {
        job.fail(format!("cannot read {}", source.describe()));
        return;
    };
    job.set_content_hash(hash.clone());
    job.set_progress(HASH_END);

    let missing = take_cached(job, analyzers, cache, &hash);
    if !missing.is_empty() {
        analyse_missing(job, source, &missing, cache, &hash);
    }
}

/// Serves what the cache has and returns the analysers that still have to run.
fn take_cached(
    job: &AnalysisJob,
    analyzers: &[Arc<dyn Analyzer>],
    cache: Option<&dyn ResultCache>,
    hash: &str,
) -> Vec<Arc<dyn Analyzer>> {
    let Some(cache) = cache else {
        return analyzers.to_vec();
    };
    analyzers
        .iter()
        .filter(|a| match cache.find(hash, a.id(), a.version()) {
            Some(hit) => {
                job.add_result(a.id(), hit, true);
                false
            }
            None => true,
        })
        .cloned()
        .collect()
}

fn analyse_missing(
    job: &AnalysisJob,
    source: &mut dyn ClipSource,
    missing: &[Arc<dyn Analyzer>],
    cache: Option<&dyn ResultCache>,
    hash: &str,
) {
    let load_step = StepContext {
        job,
        base: HASH_END,
        span: LOAD_END - HASH_END,
    };
    let clip = source.load(&load_step);
    if job.is_cancelled() {
        return;
    }
    let Some(clip) = clip else {
        job.fail(format!("cannot decode {}", source.describe()));
        return;
    };
    run_analyzers(job, &clip, missing, cache, hash);
}

/// Stops early when cancelled or when an analyser fails (the job is then marked accordingly).
fn run_analyzers(
    job: &AnalysisJob,
    clip: &AudioClip,
    analyzers: &[Arc<dyn Analyzer>],
    cache: Option<&dyn ResultCache>,
    hash: &str,
) {
    let span = (1.0 - LOAD_END) / analyzers.len() as f32;
    for (index, analyzer) in analyzers.iter().enumerate() {
        let step = StepContext {
            job,
            base: LOAD_END + span * index as f32,
            span,
        };
        let payload = analyzer.analyze(clip, &step);
        if job.is_cancelled() {
            return;
        }
        let Some(payload) = payload else {
            job.fail(format!("analyzer failed: {}", analyzer.id()));
            return;
        };
        if let Some(cache) = cache {
            cache.store(hash, analyzer.id(), analyzer.version(), &payload);
        }
        job.add_result(analyzer.id(), payload, false);
    }
}

fn finish(job: &AnalysisJob) {
    if job.status() == JobStatus::Failed {
        return;
    }
    if job.is_cancelled() {
        job.set_status(JobStatus::Cancelled);
        return;
    }
    job.set_progress(1.0);
    job.set_status(JobStatus::Done);
}
