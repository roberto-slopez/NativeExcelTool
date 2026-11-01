# Telemetry and progress

Telemetry is off by default. Progress remains available independently: the Rust API provides `status()` and `subscribe_progress()`, Node/C# provide job status polling, and the command always prints progress to stderr. Library calls never print console output unless a host explicitly installs a sink that does so.

## Command

```powershell
cargo run -p native-excel --bin native-excel -- examples/configs/typed-report.json
cargo run -p native-excel --bin native-excel -- examples/configs/typed-report.json --telemetry
cargo run -p native-excel --bin native-excel -- examples/configs/typed-report.json --telemetry --log-path C:\Reports\events.jsonl --metrics-path C:\Reports\export.prom
```

The first command prints progress and a final JSON report, without telemetry. `--telemetry` enables sanitized console events. The optional file flags additionally select JSONL logging and Prometheus snapshots. File destinations in the JSON request also require `--telemetry` when using the command; otherwise it fails rather than silently enabling telemetry. Parent directories must already exist. The packaged Windows command is `dist/release/native-excel.exe`. The `export` Cargo example supports the same arguments.

Progress starts during validation, is polled every 250 ms during execution, and includes a terminal update even for fast jobs. It shows phase, written rows, known total or `unknown`, published files, and telemetry error count. Errors are reported on stderr with a nonzero exit code. Successful stdout contains only the final JSON report, so it can be redirected separately from progress and telemetry.

## Libraries: explicit absolute paths

Set the shared request options in Rust, Node, or C#:

```json
{
  "options": {
    "progressIntervalMs": 250,
    "telemetry": {
      "logPath": "C:\\Reports\\events.jsonl",
      "metricsPath": "C:\\Reports\\export.prom",
      "sampleIntervalMs": 1000
    }
  }
}
```

This is a request fragment; include the usual `workbooks`. Omit the entire telemetry object to disable it. Either path can be supplied alone. Relative paths, directories, file symlinks, identical destinations, and destinations matching an Excel output are rejected before export. Destination directories must exist. Paths are explicitly provided by the caller; no telemetry settings are read from environment variables. Normal filesystem permissions apply. Configure separate files for concurrent jobs when separate metrics snapshots are needed.

| Option | Default | Behavior |
| --- | --- | --- |
| `options.progressIntervalMs` | 250 | Coalesced Rust progress update interval, 25..60000 ms |
| `options.telemetry.logPath` | absent | Enables append-only UTF-8 JSONL event logging at an absolute path |
| `options.telemetry.metricsPath` | absent | Enables atomic Prometheus textfile snapshots at an absolute path |
| `options.telemetry.sampleIntervalMs` | 1000 | Periodic event sampling interval, 25..60000 ms; initial and terminal events are also emitted |

With no path and no custom sink, no telemetry file is opened and no telemetry exporter runs. Progress counters still operate. This configuration applies to export jobs; standalone query sessions currently use their existing row-stream lifecycle and do not create export telemetry files.

JSONL records include `jobId`, `timestampUnixMs`, monotonic `elapsedMs`, `phase`, `rowsWritten`, `totalRows`, `filesPublished`, and `telemetryErrors`. They deliberately omit connection strings, SQL text, parameters, record values, output paths, workbook IDs, and exception messages. The per-process job ID distinguishes records in an appended log. This is operational export telemetry, not a statement-level database trace or profiler.

The Prometheus destination contains gauge samples for rows written, total rows when known, files published, elapsed seconds, telemetry errors, and terminal status with phase/job labels. Point a suitable textfile collector at its directory; the library does not start an HTTP server or send data over the network. Snapshots replace the previous file atomically on the local filesystem. A shared metrics path represents the latest writer, not an aggregate of simultaneous exports. Historical JSONL logs append; rotation/retention are controlled by the host. Network filesystem publication guarantees can differ.

File configuration/open failures fail before exporting. A failure during telemetry emission increments `job.status().telemetry_errors` and does not fail or roll back the report. A broken destination may not be able to record its own failure; status polling is the authoritative way to inspect that counter. Terminal telemetry is flushed before `job.result()` resolves. Filesystem flush is not a durable disk-sync guarantee.

## Progress without logging

```rust,no_run
let job = native_excel::ExcelExporter::start(request)?;
let mut progress = job.subscribe_progress();
loop {
    let status = progress.borrow_and_update().clone();
    // Deliver status to your application UI, callback, or progress display.
    if matches!(status.phase.as_str(), "succeeded" | "failed" | "cancelled") { break; }
    if progress.changed().await.is_err() { break; }
}
let report = job.result().await?;
```

The subscription is a bounded latest-value watch, so slow consumers do not accumulate an unbounded queue. Intermediate snapshots may be coalesced. Subscribers must stop when the phase is terminal; a retained job handle keeps the subscription sender alive. In Node, use `startExport(request)` and `job.status()` while awaiting `job.result()`. In C#, use `ne_job_status_json` from an application timer/worker and free each returned buffer. The library leaves display choices to its host.

## Custom sinks and OpenTelemetry integration

Rust hosts may call `ExcelExporter::start_with_sink(request, Some(Arc::new(sink)))`. Implement the `TelemetrySink` trait to receive the same sanitized events without a log file. This is explicit activation. A sink can coexist with file logging and Prometheus snapshots. A host can adapt it to its existing OpenTelemetry SDK, tracing system, or event bus; no global subscriber or OpenTelemetry provider is installed by this library.

```rust
struct HostSink;
impl native_excel::TelemetrySink for HostSink {
    fn record(&self, event: &native_excel::TelemetryEvent) -> std::io::Result<()> {
        // Forward the numeric measurements to an already configured host exporter.
        let _rows = event.rows_written;
        Ok(())
    }
}
```

Custom callbacks execute on a blocking worker, outside the data-writing loop. They must return promptly; an indefinitely blocked callback can delay monitoring and result delivery. Errors and unwindable panics are contained and counted, but process aborts cannot be recovered. A host panic hook may still run. Host-added fields and network destinations are the host's responsibility. A built-in OTLP network exporter is a remaining optional integration; the implemented alternatives are Prometheus textfiles and the custom-sink port.
