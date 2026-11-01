use calamine::{Data, Reader, open_workbook_auto};
use native_excel::*;
use serde_json::{Value, json};
use std::io::Read;

fn config(dir: &std::path::Path, mode: &str) -> Value {
    json!({"workbooks":[{"id":"typed","output":{"path":dir.join("typed.xlsx").to_str().unwrap()},"worksheets":[{
        "name":"Typed","writerMode":mode,"totalRows":2,"bodyStyle":"body",
        "styles":{"body":{"background":"#EEFFEE","border":"thin"},"header":{"bold":true,"background":"#112233"}},
        "layout":{"startRow":3,"startColumn":2,"headerStyle":"header","autofilter":true},
        "source":{"kind":"objects","records":[
            {"day":"2026-10-31","at":"2026-10-31T10:30:00-07:00","amount":1250.5,"ratio":0.25,"active":true,"exact":{"$type":"int64","value":"9007199254740993"},"precise":"12345678901234567890.12","clock":"13:45:30"},
            {"day":null,"at":null,"amount":null,"ratio":null,"active":false,"exact":1,"precise":"0.01","clock":"00:00:00"}
        ]},
        "columns":[
            {"field":"day","dataType":"date","format":"dd/mm/yyyy"},
            {"field":"at","dataType":"datetime"},
            {"field":"amount","dataType":"number","numberFormat":"\"$\"#,##0.00"},
            {"field":"ratio","dataType":"number","format":{"numberFormat":"0.00%"}},
            {"field":"active","dataType":"boolean"},
            {"field":"exact","dataType":"integer"},
            {"field":"precise","dataType":"decimal"},
            {"field":"clock","dataType":"time"}
        ]
    }]}]})
}
fn xml(path: &std::path::Path, part: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut s = String::new();
    zip.by_name(part).unwrap().read_to_string(&mut s).unwrap();
    s
}
#[tokio::test]
async fn typed_dates_currency_and_column_styles_work_in_both_modes() {
    for mode in ["buffered", "streaming"] {
        let dir = tempfile::tempdir().unwrap();
        let request = serde_json::from_value(config(dir.path(), mode)).unwrap();
        let job = ExcelExporter::start(request).unwrap();
        assert_eq!(job.status().total_rows, Some(2));
        assert_eq!(job.result().await.unwrap().workbooks[0].rows, 2);
        let path = dir.path().join("typed.xlsx");
        let mut book = open_workbook_auto(&path).unwrap();
        let range = book.worksheet_range("Typed").unwrap();
        // Absolute positions: B4, D4 and F4 are typed Excel cells, not formatted strings.
        assert!(matches!(range.get_value((3, 1)), Some(Data::DateTime(_))));
        assert_eq!(range.get_value((3, 3)), Some(&Data::Float(1250.5)));
        assert_eq!(range.get_value((3, 5)), Some(&Data::Bool(true)));
        assert_eq!(
            range.get_value((3, 6)),
            Some(&Data::String("9007199254740993".into()))
        );
        assert_eq!(
            range.get_value((3, 7)),
            Some(&Data::String("12345678901234567890.12".into()))
        );
        let sheet = xml(&path, "xl/worksheets/sheet1.xml");
        let styles = xml(&path, "xl/styles.xml");
        assert!(sheet.contains("<cols>") && sheet.contains("style=\""));
        assert!(sheet.contains("ref=\"B3:I5\""));
        assert!(
            styles.contains("dd/mm/yyyy") && styles.contains("0.00%") && styles.contains("EEFFEE")
        );
        assert!(sheet.contains("<v>46326</v>")); // Actual Excel date serial for 2026-10-31.
    }
}
#[test]
fn total_rows_is_exact_and_headers_offsets_count_towards_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = config(dir.path(), "streaming");
    value["workbooks"][0]["worksheets"][0]["totalRows"] = json!(3);
    let request = serde_json::from_value(value).unwrap();
    assert_eq!(
        ExcelExporter::validate(&request).unwrap_err().code,
        "ROW_COUNT_MISMATCH"
    );
    let mut value = config(dir.path(), "buffered");
    value["options"] = json!({"maxRows":4});
    let request = serde_json::from_value(value).unwrap();
    assert_eq!(
        ExcelExporter::validate(&request).unwrap_err().code,
        "LIMIT_EXCEEDED"
    );
}
#[tokio::test]
async fn type_mismatch_prevents_publication_and_does_not_disclose_values() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = config(dir.path(), "streaming");
    value["workbooks"][0]["worksheets"][0]["source"]["records"][0]["active"] = json!("SECRET");
    let error = ExcelExporter::export(serde_json::from_value(value).unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.code, "CONVERSION_FAILED");
    assert!(error.message.contains("active") && !error.message.contains("SECRET"));
    assert!(!dir.path().join("typed.xlsx").exists());
}
#[tokio::test]
async fn structured_table_uses_exact_range_and_unique_headers() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = config(dir.path(), "buffered");
    value["workbooks"][0]["worksheets"][0]["table"] = json!({"name":"ReportData"});
    ExcelExporter::export(serde_json::from_value(value.clone()).unwrap())
        .await
        .unwrap();
    let table = xml(&dir.path().join("typed.xlsx"), "xl/tables/table1.xml");
    assert!(table.contains("ref=\"B3:I5\"") && table.contains("name=\"ReportData\""));
    value["workbooks"][0]["worksheets"][0]["writerMode"] = json!("streaming");
    assert_eq!(
        ExcelExporter::validate(&serde_json::from_value(value).unwrap())
            .unwrap_err()
            .code,
        "CAPABILITY_MISMATCH"
    );
}
#[test]
fn query_totals_are_validated_before_connecting_and_table_budgets_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = config(dir.path(), "buffered");
    let sheet = &mut value["workbooks"][0]["worksheets"][0];
    sheet["table"] = json!({"name":"ReportData","maxCells":1});
    assert_eq!(
        ExcelExporter::validate(&serde_json::from_value(value.clone()).unwrap())
            .unwrap_err()
            .code,
        "LIMIT_EXCEEDED"
    );
    let sheet = &mut value["workbooks"][0]["worksheets"][0];
    sheet.as_object_mut().unwrap().remove("table");
    sheet["source"] =
        json!({"kind":"query","dbms":"postgresql","connectionString":"unused","query":"unused"});
    sheet["totalRows"] = json!(1048576);
    assert_eq!(
        ExcelExporter::validate(&serde_json::from_value(value).unwrap())
            .unwrap_err()
            .code,
        "LIMIT_EXCEEDED"
    );
}
