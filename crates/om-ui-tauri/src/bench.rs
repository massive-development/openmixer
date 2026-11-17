//! The `--ui-bench` run's report: the front end's frame statistics, checked here and written as
//! JSON. Runs on the main thread inside the `bench_done` command, once per run.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Mutex;

/// Why a bench report was not written.
#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("malformed bench stats: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error("bench stats field {0} is not a finite number")]
    NonFinite(&'static str),
    #[error("could not write the bench report to {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// `summarise` in `web/src/lib/bench.ts`, in ms.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub n: u32,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

/// `BenchStats` in `web/src/lib/bench.ts`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BenchReport {
    pub bench_seconds: u32,
    pub fps_cap: u32,
    /// CSS width, CSS height, device pixel ratio.
    pub viewport: [f64; 3],
    pub frame_ms: Summary,
    pub work_ms: Summary,
    pub fps: Option<f64>,
    pub frames_over_2x_refresh: u32,
    pub decks_playing: u32,
}

impl Summary {
    fn finite(&self) -> bool {
        [self.mean, self.p50, self.p95, self.p99, self.max]
            .iter()
            .all(|v| v.is_finite())
    }
}

/// The report in `value`, refused when a field is missing, extra, mistyped or not finite.
pub fn parse_report(value: serde_json::Value) -> Result<BenchReport, BenchError> {
    let report: BenchReport = serde_json::from_value(value)?;
    if !report.viewport.iter().all(|v| v.is_finite()) {
        return Err(BenchError::NonFinite("viewport"));
    }
    if !report.frame_ms.finite() {
        return Err(BenchError::NonFinite("frameMs"));
    }
    if !report.work_ms.finite() {
        return Err(BenchError::NonFinite("workMs"));
    }
    if report.fps.is_some_and(|f| !f.is_finite()) {
        return Err(BenchError::NonFinite("fps"));
    }
    Ok(report)
}

/// Writes `report` to `path` as JSON, creating its folder.
pub fn write_report(path: &Path, report: &BenchReport) -> Result<(), BenchError> {
    let write_error = |source| BenchError::Write {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(write_error)?;
    }
    let json = serde_json::to_vec_pretty(report)?;
    std::fs::write(path, json).map_err(write_error)
}

/// A bench run's settings and its verdict, shared by the command and `run`.
pub(crate) struct BenchRun {
    pub(crate) seconds: u32,
    pub(crate) out: PathBuf,
    /// `None` until `bench_done` ran; then whether the report was written, or why not.
    pub(crate) outcome: Mutex<Option<Result<(), String>>>,
    /// Stops the watchdog.
    pub(crate) done: Sender<()>,
}

impl BenchRun {
    /// Checks and writes `stats`; the verdict is kept for `run`.
    pub(crate) fn finish(&self, stats: serde_json::Value) -> Result<(), String> {
        let result = parse_report(stats)
            .and_then(|report| write_report(&self.out, &report))
            .map_err(|e| e.to_string());
        *crate::lock(&self.outcome) = Some(result.clone());
        // The watchdog may already have given up; nothing else listens.
        let _ = self.done.send(());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn summary() -> serde_json::Value {
        json!({ "n": 3, "mean": 16.6, "p50": 16.6, "p95": 17.0, "p99": 17.0, "max": 17.0 })
    }

    fn stats() -> serde_json::Value {
        json!({
            "benchSeconds": 10, "fpsCap": 60, "viewport": [1600.0, 780.0, 1.0],
            "frameMs": summary(), "workMs": summary(), "fps": 60.2,
            "framesOver2xRefresh": 0, "decksPlaying": 2
        })
    }

    #[test]
    fn a_full_report_parses() {
        let report = parse_report(stats()).unwrap();
        assert_eq!(report.bench_seconds, 10);
        assert_eq!(report.frame_ms.n, 3);
        assert_eq!(report.decks_playing, 2);
    }

    #[test]
    fn no_frames_means_no_fps() {
        let mut value = stats();
        value["fps"] = serde_json::Value::Null;
        assert_eq!(parse_report(value).unwrap().fps, None);
    }

    #[test]
    fn a_missing_or_extra_field_is_refused() {
        let mut value = stats();
        value.as_object_mut().unwrap().remove("workMs");
        assert!(matches!(parse_report(value), Err(BenchError::Malformed(_))));
        let mut value = stats();
        value["extra"] = json!(1);
        assert!(matches!(parse_report(value), Err(BenchError::Malformed(_))));
        assert!(parse_report(json!("not an object")).is_err());
    }

    #[test]
    fn a_negative_count_is_refused() {
        let mut value = stats();
        value["decksPlaying"] = json!(-1);
        assert!(matches!(parse_report(value), Err(BenchError::Malformed(_))));
    }

    #[test]
    fn a_non_finite_number_is_refused() {
        let mut report = parse_report(stats()).unwrap();
        report.work_ms.max = f64::INFINITY;
        // JSON has no infinity: check the struct the way `parse_report` does.
        assert!(!report.work_ms.finite());
        report.viewport[2] = f64::NAN;
        assert!(!report.viewport.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn a_nan_sent_as_null_is_refused() {
        // `JSON.stringify` turns NaN and Infinity into null.
        let mut value = stats();
        value["workMs"]["mean"] = serde_json::Value::Null;
        assert!(matches!(parse_report(value), Err(BenchError::Malformed(_))));
    }

    #[test]
    fn the_report_round_trips_through_the_file() {
        let dir = tempfile_dir();
        let path = dir.join("nested").join("bench.json");
        let report = parse_report(stats()).unwrap();
        write_report(&path, &report).unwrap();
        let back: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(parse_report(back).unwrap(), report);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("om-ui-tauri-bench-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
