use native_excel::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
fn request(dir: &std::path::Path) -> ExportRequest {
    serde_json::from_value(json!({"workbooks":[{"id":"SECRET_ID","output":{"path":dir.join("result.xlsx").to_str().unwrap()},"worksheets":[{"name":"Private","source":{"kind":"objects","records":[{"secret":"SECRET_VALUE"}]},"columns":[{"field":"secret"}]}]}]})).unwrap()
}
#[tokio::test]
async fn defaults_do_not_create_telemetry_files_but_progress_is_available() {
    let dir = tempfile::tempdir().unwrap();
    let job = ExcelExporter::start(request(dir.path())).unwrap();
    let progress = job.subscribe_progress();
    job.result().await.unwrap();
    assert_eq!(progress.borrow().phase, "succeeded");
    assert_eq!(job.status().telemetry_errors, 0);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[tokio::test]
async fn jsonl_and_prometheus_flush_terminal_state_without_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path());
    req.options.telemetry.log_path = Some(dir.path().join("events.jsonl").to_str().unwrap().into());
    req.options.telemetry.metrics_path =
        Some(dir.path().join("metrics.prom").to_str().unwrap().into());
    let job = ExcelExporter::start(req).unwrap();
    job.result().await.unwrap();
    assert_eq!(job.status().telemetry_errors, 0);
    let log = std::fs::read_to_string(dir.path().join("events.jsonl")).unwrap();
    assert!(!log.contains("SECRET") && !log.contains("result.xlsx"));
    let events: Vec<Value> = log
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(events.first().unwrap()["phase"], "running");
    assert_eq!(events.last().unwrap()["phase"], "succeeded");
    assert_eq!(events.last().unwrap()["rowsWritten"], 1);
    let metrics = std::fs::read_to_string(dir.path().join("metrics.prom")).unwrap();
    assert!(metrics.contains("native_excel_rows_written{job="));
    assert!(metrics.contains("phase=\"succeeded\"} 1"));
}
#[test]
fn paths_must_be_absolute_distinct_and_not_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path());
    req.options.telemetry.log_path = Some("relative.jsonl".into());
    assert!(ExcelExporter::validate(&req).is_err());
    req.options.telemetry.log_path = Some(dir.path().join("result.xlsx").to_str().unwrap().into());
    assert!(ExcelExporter::validate(&req).is_err());
    req.options.telemetry.log_path = Some(dir.path().join("same.log").to_str().unwrap().into());
    req.options.telemetry.metrics_path = req.options.telemetry.log_path.clone();
    assert!(ExcelExporter::validate(&req).is_err());
}
struct Collector(Mutex<Vec<String>>);
impl TelemetrySink for Collector {
    fn record(&self, event: &TelemetryEvent) -> std::io::Result<()> {
        self.0.lock().unwrap().push(event.phase.clone());
        Ok(())
    }
}
#[tokio::test]
async fn custom_sink_is_opt_in_and_export_survives_sink_failure() {
    let dir = tempfile::tempdir().unwrap();
    let collector = Arc::new(Collector(Mutex::new(vec![])));
    let job = ExcelExporter::start_with_sink(request(dir.path()), Some(collector.clone())).unwrap();
    job.result().await.unwrap();
    assert_eq!(collector.0.lock().unwrap().last().unwrap(), "succeeded");
    struct Failing;
    impl TelemetrySink for Failing {
        fn record(&self, _: &TelemetryEvent) -> std::io::Result<()> {
            Err(std::io::Error::other("sink failure"))
        }
    }
    let mut req = request(dir.path());
    req.workbooks[0].output.overwrite = true;
    let job = ExcelExporter::start_with_sink(req, Some(Arc::new(Failing))).unwrap();
    job.result().await.unwrap();
    assert!(job.status().telemetry_errors >= 2);
}
#[test]
fn cli_always_reports_progress_and_gates_telemetry() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("request.json");
    std::fs::write(
        &config,
        serde_json::to_string(&request(dir.path())).unwrap(),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_native-excel"))
        .arg(&config)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Progress:") && !stderr.contains("Telemetry:"));
    let mut req = request(dir.path());
    req.workbooks[0].output.overwrite = true;
    std::fs::write(&config, serde_json::to_string(&req).unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_native-excel"))
        .arg(&config)
        .arg("--telemetry")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Progress:") && stderr.contains("Telemetry:"));
    assert!(!stderr.contains("SECRET_VALUE"));
    serde_json::from_slice::<Value>(&output.stdout).unwrap();
}
