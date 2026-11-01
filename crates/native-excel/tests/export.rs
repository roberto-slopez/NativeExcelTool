use calamine::{Data, Reader, open_workbook_auto};
use native_excel::*;
use serde_json::{Value, json};
use std::io::Read;
fn fixture(dir: &std::path::Path) -> ExportRequest {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../examples/configs/objects.json")).unwrap();
    for (i, book) in value["workbooks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        book["output"]["path"] = json!(dir.join(format!("book{i}.xlsx")).to_str().unwrap());
        book["output"]["overwrite"] = json!(false);
    }
    serde_json::from_value(value).unwrap()
}
#[tokio::test]
async fn multiple_books_preserve_types_and_precision() {
    let dir = tempfile::tempdir().unwrap();
    let request = fixture(dir.path());
    let result = ExcelExporter::export(request).await.unwrap();
    assert_eq!(result.workbooks.len(), 2);
    assert_eq!(result.workbooks[0].rows, 2);
    let mut book = open_workbook_auto(dir.path().join("book0.xlsx")).unwrap();
    let range = book.worksheet_range("Orders").unwrap();
    assert_eq!(
        range.get_value((1, 0)),
        Some(&Data::String("9007199254740993".into()))
    );
    assert_eq!(range.get_value((1, 2)), Some(&Data::Float(1250.5)));
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(dir.path().join("book0.xlsx")).unwrap()).unwrap();
    let mut styles = String::new();
    archive
        .by_name("xl/styles.xml")
        .unwrap()
        .read_to_string(&mut styles)
        .unwrap();
    assert!(styles.contains("16324F"));
    assert!(styles.contains("#,##0.00"));
    assert!(styles.contains("<b/>") || styles.contains("<b />"));
}
#[tokio::test]
async fn existing_output_is_never_overwritten_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let request = fixture(dir.path());
    let path = dir.path().join("book0.xlsx");
    std::fs::write(&path, b"original").unwrap();
    assert_eq!(
        ExcelExporter::export(request).await.unwrap_err().code,
        "OUTPUT_EXISTS"
    );
    assert_eq!(std::fs::read(path).unwrap(), b"original");
}
#[tokio::test]
async fn failures_remove_unpublished_output() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = fixture(dir.path());
    request.workbooks[0].worksheets[0].columns[0].field = "missing".into();
    assert_eq!(
        ExcelExporter::export(request).await.unwrap_err().code,
        "SCHEMA_MISMATCH"
    );
    assert!(!dir.path().join("book0.xlsx").exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn rejects_duplicate_names_and_unknown_options() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = fixture(dir.path());
    let sheet = request.workbooks[0].worksheets[0].clone();
    request.workbooks[0].worksheets.push(sheet);
    assert!(ExcelExporter::validate(&request).is_err());
    let mut value = serde_json::to_value(fixture(dir.path())).unwrap();
    value["workbooks"][0]["output"]["typo"] = json!(true);
    assert!(parse_request(&value.to_string()).is_err());
}
#[test]
fn query_source_deserializes_without_ignoring_options() {
    let source = json!({"kind":"query","dbms":"postgresql","connectionString":"host=localhost","query":"SELECT 1 AS id","options":{"queueRows":1}});
    let result = serde_json::from_value::<Source>(source);
    assert!(result.is_ok(), "{}", result.err().unwrap());
}
#[tokio::test]
async fn text_is_not_a_formula_and_columns_extend_beyond_z() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = fixture(dir.path());
    request.workbooks.truncate(1);
    let sheet = &mut request.workbooks[0].worksheets[0];
    let record = (0..28)
        .map(|i| (format!("field{i}"), json!("=1+1")))
        .collect();
    sheet.source = Source::Objects {
        records: vec![record],
    };
    sheet.columns = (0..28)
        .map(|i| Column {
            field: format!("field{i}"),
            data_type: DataType::Auto,
            number_format: None,
            header: None,
            width: None,
            style: None,
            format: None,
            missing_as_null: false,
            ..Default::default()
        })
        .collect();
    ExcelExporter::export(request).await.unwrap();
    let mut book = open_workbook_auto(dir.path().join("book0.xlsx")).unwrap();
    let range = book.worksheet_range("Orders").unwrap();
    assert_eq!(range.get_value((1, 27)), Some(&Data::String("=1+1".into())));
}
#[tokio::test]
async fn row_limit_is_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = fixture(dir.path());
    request.options.max_rows = 2;
    assert_eq!(
        ExcelExporter::export(request).await.unwrap_err().code,
        "LIMIT_EXCEEDED"
    );
}
#[tokio::test]
async fn batch_can_continue_after_independent_failure() {
    let dir = tempfile::tempdir().unwrap();
    let mut request = fixture(dir.path());
    request.options.continue_on_error = true;
    request.workbooks[0].worksheets[0].columns[0].field = "missing".into();
    let report = ExcelExporter::export(request).await.unwrap();
    assert!(report.workbooks[0].error.is_some());
    assert!(report.workbooks[1].error.is_none());
    assert!(dir.path().join("book1.xlsx").exists());
}
#[tokio::test]
async fn cancellation_is_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let request = fixture(dir.path());
    let job = ExcelExporter::start(request).unwrap();
    job.cancel();
    assert_eq!(job.result().await.unwrap_err().code, "CANCELLED");
    assert_eq!(job.status().phase, "cancelled");
}
#[tokio::test]
async fn invalid_query_limits_fail_without_connection() {
    let req:QueryRequest=serde_json::from_value(json!({"dbms":"postgresql","connectionString":"","query":"SELECT 1","options":{"queueRows":0}})).unwrap();
    assert_eq!(
        QueryService::stream(req).await.err().unwrap().code,
        "INVALID_CONFIG"
    );
}
