//! Opt-in, per-export telemetry. No global subscriber, environment reads, or network I/O.
use crate::{ExportError, ExportRequest, JobStatus, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct TelemetryOptions {
    pub log_path: Option<String>,
    pub metrics_path: Option<String>,
    pub sample_interval_ms: u64,
}
impl Default for TelemetryOptions {
    fn default() -> Self {
        Self {
            log_path: None,
            metrics_path: None,
            sample_interval_ms: 1000,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryEvent {
    pub job_id: String,
    pub timestamp_unix_ms: u128,
    pub elapsed_ms: u128,
    pub phase: String,
    pub rows_written: u64,
    pub total_rows: Option<u64>,
    pub files_published: u64,
    pub telemetry_errors: u64,
}
/// Host adapters may bridge events to OpenTelemetry or another collector.
/// Implementations must return promptly, avoid panics, and redact their own added attributes.
pub trait TelemetrySink: Send + Sync {
    fn record(&self, event: &TelemetryEvent) -> std::io::Result<()>;
}
pub(crate) struct Telemetry {
    log: Option<File>,
    metrics: Option<PathBuf>,
    sink: Option<Arc<dyn TelemetrySink>>,
    started: Instant,
    id: String,
}
static IDS: AtomicU64 = AtomicU64::new(1);
fn normalized(path: &Path) -> Result<PathBuf> {
    // Resolve the existing parent to catch aliases and symlinks in destination directories.
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .or(Some(Path::new(".")))
        .ok_or_else(|| ExportError::invalid("Telemetry path needs a parent directory"))?;
    Ok(std::fs::canonicalize(parent)?.join(
        path.file_name()
            .ok_or_else(|| ExportError::invalid("Telemetry path must name a file"))?,
    ))
}
fn key(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}
pub(crate) fn validate(request: &ExportRequest) -> Result<()> {
    let config = &request.options.telemetry;
    if !(25..=60_000).contains(&request.options.progress_interval_ms)
        || !(25..=60_000).contains(&config.sample_interval_ms)
    {
        return Err(ExportError::invalid(
            "Progress and telemetry intervals must be 25..60000 milliseconds",
        ));
    }
    let mut paths = std::collections::BTreeSet::new();
    for value in [&config.log_path, &config.metrics_path]
        .into_iter()
        .flatten()
    {
        let path = Path::new(value);
        if !path.is_absolute() {
            return Err(ExportError::invalid("Telemetry paths must be absolute"));
        }
        let target = normalized(path)?;
        if path.is_dir()
            || path
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            || !paths.insert(key(&target))
        {
            return Err(ExportError::invalid(
                "Telemetry destinations must be distinct regular files",
            ));
        }
        for book in &request.workbooks {
            let output = Path::new(&book.output.path);
            // A telemetry destination's parent exists, so aliases can be resolved without creating output directories.
            if let Ok(output) = normalized(output)
                && key(&output) == key(&target)
            {
                return Err(ExportError::invalid(
                    "Telemetry must not overwrite an Excel output",
                ));
            }
        }
    }
    Ok(())
}
impl Telemetry {
    pub(crate) fn open(
        options: &TelemetryOptions,
        sink: Option<Arc<dyn TelemetrySink>>,
    ) -> Result<Option<Self>> {
        if options.log_path.is_none() && options.metrics_path.is_none() && sink.is_none() {
            return Ok(None);
        }
        let log = options
            .log_path
            .as_ref()
            .map(|path| OpenOptions::new().create(true).append(true).open(path))
            .transpose()?;
        // Probe the metrics directory without touching an existing metrics file.
        if let Some(path) = &options.metrics_path {
            let probe = tempfile::NamedTempFile::new_in(Path::new(path).parent().unwrap())?;
            drop(probe);
        }
        Ok(Some(Self {
            log,
            metrics: options.metrics_path.as_ref().map(PathBuf::from),
            sink,
            started: Instant::now(),
            id: format!(
                "{}-{}",
                std::process::id(),
                IDS.fetch_add(1, Ordering::Relaxed)
            ),
        }))
    }
    pub(crate) fn record(&mut self, status: &JobStatus) -> u64 {
        let event = TelemetryEvent {
            job_id: self.id.clone(),
            timestamp_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            elapsed_ms: self.started.elapsed().as_millis(),
            phase: status.phase.clone(),
            rows_written: status.rows_written,
            total_rows: status.total_rows,
            files_published: status.files_published,
            telemetry_errors: status.telemetry_errors,
        };
        let mut failures = 0;
        if let Some(log) = &mut self.log {
            let mut bytes = serde_json::to_vec(&event).expect("event serialization");
            bytes.push(b'\n');
            if log.write_all(&bytes).and_then(|_| log.flush()).is_err() {
                failures += 1;
            }
        }
        if let Some(path) = &self.metrics {
            let result = (|| -> std::io::Result<()> {
                let mut staged = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
                writeln!(
                    staged,
                    "# TYPE native_excel_rows_written gauge\nnative_excel_rows_written{{job=\"{}\"}} {}\n# TYPE native_excel_files_published gauge\nnative_excel_files_published{{job=\"{}\"}} {}\n# TYPE native_excel_elapsed_seconds gauge\nnative_excel_elapsed_seconds{{job=\"{}\"}} {}\n# TYPE native_excel_telemetry_errors gauge\nnative_excel_telemetry_errors{{job=\"{}\"}} {}\n# TYPE native_excel_job_terminal gauge\nnative_excel_job_terminal{{job=\"{}\",phase=\"{}\"}} {}",
                    event.job_id,
                    event.rows_written,
                    event.job_id,
                    event.files_published,
                    event.job_id,
                    event.elapsed_ms as f64 / 1000.0,
                    event.job_id,
                    event.telemetry_errors,
                    event.job_id,
                    event.phase,
                    u8::from(matches!(
                        event.phase.as_str(),
                        "succeeded" | "failed" | "cancelled"
                    ))
                )?;
                if let Some(total) = event.total_rows {
                    writeln!(
                        staged,
                        "# TYPE native_excel_total_rows gauge\nnative_excel_total_rows{{job=\"{}\"}} {}",
                        event.job_id, total
                    )?;
                }
                staged.persist(path).map_err(|e| e.error)?;
                Ok(())
            })();
            if result.is_err() {
                failures += 1;
            }
        }
        if let Some(sink) = &self.sink
            && !matches!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sink.record(&event))),
                Ok(Ok(()))
            )
        {
            failures += 1;
        }
        failures
    }
}
