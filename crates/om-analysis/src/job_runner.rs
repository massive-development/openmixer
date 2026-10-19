//! Runs one job to completion on the calling thread: hash, cache lookup, decode on a miss, analyse,
//! store. Progress: 0-5% hash, 5-20% decode, 20-100% split across the analysers that must run.
//! The pool owns the job's final status: a failure is reported through `JobCtx::fail`, and the
//! pool marks the job done (progress 1.0) or cancelled when the runner returns.

use crate::job::JobOutput;
use crate::{AnalysisContext, Analyzer, AnalyzerRegistry, AudioClip, ClipSource, ResultCache};
use om_core::jobs::JobCtx;
use std::sync::Arc;

const HASH_END: f32 = 0.05;
const LOAD_END: f32 = 0.2;

/// Where a run reports (the pool's context) and what it writes (the job's output).
struct Run<'a> {
    ctx: &'a JobCtx,
    out: &'a JobOutput,
}

impl Run<'_> {
    fn is_cancelled(&self) -> bool {
        self.ctx.is_cancelled()
    }

    fn set_progress(&self, fraction: f32) {
        self.ctx.set_progress(fraction);
    }

    fn fail(&self, message: String) {
        self.ctx.fail(message);
    }

    fn set_content_hash(&self, hash: String) {
        self.out.set_content_hash(hash);
    }

    fn add_result(&self, analyzer_id: &str, payload: String, from_cache: bool) {
        self.out.add_result(analyzer_id, payload, from_cache);
    }
}

/// Maps a step's 0..1 progress into its slice of the job's progress.
struct StepContext<'a> {
    ctx: &'a JobCtx,
    base: f32,
    span: f32,
}

impl AnalysisContext for StepContext<'_> {
    fn is_cancelled(&self) -> bool {
        self.ctx.is_cancelled()
    }

    fn set_progress(&self, fraction: f32) {
        self.ctx
            .set_progress(self.base + self.span * fraction.clamp(0.0, 1.0));
    }
}

/// Runs `analyzer_ids` against `source`, writing into `out`. Results already in `cache` are reused;
/// new results are stored there. Listeners are the caller's business.
pub fn run_analysis_job(
    ctx: &JobCtx,
    out: &JobOutput,
    source: &mut dyn ClipSource,
    analyzer_ids: &[String],
    registry: &AnalyzerRegistry,
    cache: Option<&dyn ResultCache>,
) {
    let run = Run { ctx, out };
    match resolve(analyzer_ids, registry) {
        Ok(analyzers) => process(&run, source, &analyzers, cache),
        Err(missing) => run.fail(format!("unknown analyzer: {missing}")),
    }
}

fn resolve(ids: &[String], registry: &AnalyzerRegistry) -> Result<Vec<Arc<dyn Analyzer>>, String> {
    ids.iter()
        .map(|id| registry.find(id).ok_or_else(|| id.clone()))
        .collect()
}

fn process(
    run: &Run,
    source: &mut dyn ClipSource,
    analyzers: &[Arc<dyn Analyzer>],
    cache: Option<&dyn ResultCache>,
) {
    let Some(hash) = source.content_hash() else {
        run.fail(format!("cannot read {}", source.describe()));
        return;
    };
    run.set_content_hash(hash.clone());
    run.set_progress(HASH_END);

    let missing = take_cached(run, analyzers, cache, &hash);
    if !missing.is_empty() {
        analyse_missing(run, source, &missing, cache, &hash);
    }
}

/// Serves what the cache has and returns the analysers that still have to run.
fn take_cached(
    run: &Run,
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
                run.add_result(a.id(), hit, true);
                false
            }
            None => true,
        })
        .cloned()
        .collect()
}

fn analyse_missing(
    run: &Run,
    source: &mut dyn ClipSource,
    missing: &[Arc<dyn Analyzer>],
    cache: Option<&dyn ResultCache>,
    hash: &str,
) {
    let load_step = StepContext {
        ctx: run.ctx,
        base: HASH_END,
        span: LOAD_END - HASH_END,
    };
    let clip = source.load(&load_step);
    if run.is_cancelled() {
        return;
    }
    let Some(clip) = clip else {
        run.fail(format!("cannot decode {}", source.describe()));
        return;
    };
    run_analyzers(run, &clip, missing, cache, hash);
}

/// Stops early when cancelled or when an analyser fails (the job is then marked accordingly).
fn run_analyzers(
    run: &Run,
    clip: &AudioClip,
    analyzers: &[Arc<dyn Analyzer>],
    cache: Option<&dyn ResultCache>,
    hash: &str,
) {
    let span = (1.0 - LOAD_END) / analyzers.len() as f32;
    for (index, analyzer) in analyzers.iter().enumerate() {
        let step = StepContext {
            ctx: run.ctx,
            base: LOAD_END + span * index as f32,
            span,
        };
        let payload = analyzer.analyze(clip, &step);
        if run.is_cancelled() {
            return;
        }
        let Some(payload) = payload else {
            run.fail(format!("analyzer failed: {}", analyzer.id()));
            return;
        };
        if let Some(cache) = cache {
            cache.store(hash, analyzer.id(), analyzer.version(), &payload);
        }
        run.add_result(analyzer.id(), payload, false);
    }
}
