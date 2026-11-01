//! Command-line feedback and explicitly enabled console telemetry.
use crate::*;
use std::sync::Arc;
struct ConsoleSink;
impl TelemetrySink for ConsoleSink {
    fn record(&self, event: &TelemetryEvent) -> std::io::Result<()> {
        use std::io::Write;
        writeln!(
            std::io::stderr().lock(),
            "Telemetry: {}",
            serde_json::to_string(event)?
        )
    }
}
pub async fn run(args: impl IntoIterator<Item = String>) -> Result<()> {
    let mut path = None;
    let mut enabled = false;
    let mut log = None;
    let mut metrics = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--telemetry" => enabled = true,
            "--log-path" => {
                log =
                    Some(args.next().ok_or_else(|| {
                        ExportError::invalid("--log-path requires an absolute path")
                    })?)
            }
            "--metrics-path" => {
                metrics = Some(args.next().ok_or_else(|| {
                    ExportError::invalid("--metrics-path requires an absolute path")
                })?)
            }
            "--help" | "-h" => {
                println!(
                    "native-excel [request.json] [--telemetry] [--log-path ABSOLUTE] [--metrics-path ABSOLUTE]\nProgress is always written to stderr. The final JSON report goes to stdout."
                );
                return Ok(());
            }
            value if !value.starts_with('-') && path.is_none() => path = Some(value.to_owned()),
            _ => return Err(ExportError::invalid("Unknown argument; use --help")),
        }
    }
    eprintln!("Progress: validating request");
    let path = path.unwrap_or_else(|| "examples/configs/objects.json".into());
    let mut request = parse_request(&std::fs::read_to_string(path)?)?;
    if log.is_some() {
        request.options.telemetry.log_path = log;
    }
    if metrics.is_some() {
        request.options.telemetry.metrics_path = metrics;
    }
    if !enabled
        && (request.options.telemetry.log_path.is_some()
            || request.options.telemetry.metrics_path.is_some())
    {
        return Err(ExportError::invalid(
            "CLI telemetry requires --telemetry, including destinations configured in JSON",
        ));
    }
    let job = ExcelExporter::start_with_sink(
        request,
        if enabled {
            Some(Arc::new(ConsoleSink))
        } else {
            None
        },
    )?;
    let mut progress = tokio::time::interval(std::time::Duration::from_millis(250));
    let result = loop {
        tokio::select! {
            result=job.result()=>break result,
            _=progress.tick()=>feedback(&job.status()),
        }
    };
    feedback(&job.status());
    let report = result?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report serialization")
    );
    Ok(())
}
fn feedback(status: &JobStatus) {
    eprintln!(
        "Progress: {} rows={}/{} files={} telemetryErrors={}",
        status.phase,
        status.rows_written,
        status
            .total_rows
            .map(|n| n.to_string())
            .unwrap_or_else(|| "unknown".into()),
        status.files_published,
        status.telemetry_errors
    );
}
