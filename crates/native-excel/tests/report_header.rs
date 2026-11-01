use calamine::{Data, Reader, open_workbook_auto};
use native_excel::*;
use serde_json::{Value, json};
use std::io::Read;
fn config(dir: &std::path::Path, mode: &str) -> Value {
    let columns: Vec<_> = (0..8)
        .map(|i| json!({"field":format!("c{i}"),"dataType":"integer"}))
        .collect();
    let record: serde_json::Map<_, _> = (0..8).map(|i| (format!("c{i}"), json!(i + 1))).collect();
    json!({"workbooks":[{"id":"header","output":{"path":dir.join("header.xlsx").to_str().unwrap()},"worksheets":[{
        "name":"Report","writerMode":mode,"source":{"kind":"objects","records":[record]},"columns":columns,
        "layout":{"header":false},"reportHeader":{"columnCount":2,"columns":[
            {"fields":[{"label":"Date","value":"2026-11-01","dataType":"date","numberFormat":"dd/mm/yyyy"}]},
            {"fields":[{"label":"Module","value":"Sales"}]}
        ]}
    }]}]})
}
fn xml(path: &std::path::Path, part: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    zip.by_name(part)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}
#[tokio::test]
async fn two_blocks_over_eight_columns_place_records_on_row_three_in_both_modes() {
    for mode in ["buffered", "streaming"] {
        let dir = tempfile::tempdir().unwrap();
        ExcelExporter::export(serde_json::from_value(config(dir.path(), mode)).unwrap())
            .await
            .unwrap();
        let path = dir.path().join("header.xlsx");
        let mut book = open_workbook_auto(&path).unwrap();
        let range = book.worksheet_range("Report").unwrap();
        assert_eq!(range.get_value((0, 0)), Some(&Data::String("Date".into())));
        assert_eq!(
            range.get_value((0, 4)),
            Some(&Data::String("Module".into()))
        );
        assert!(matches!(range.get_value((1, 0)), Some(Data::DateTime(_))));
        assert_eq!(range.get_value((1, 4)), Some(&Data::String("Sales".into())));
        assert_eq!(range.get_value((2, 0)), Some(&Data::Float(1.0)));
        let sheet = xml(&path, "xl/worksheets/sheet1.xml");
        for reference in ["A1:D1", "E1:H1", "A2:D2", "E2:H2"] {
            assert!(sheet.contains(reference), "{reference} missing in {mode}");
        }
        let styles = xml(&path, "xl/styles.xml");
        assert!(styles.contains("horizontal=\"left\"") && styles.contains("horizontal=\"center\""));
    }
}
#[tokio::test]
async fn title_uneven_spans_multiple_fields_and_offsets_move_table_and_filter() {
    for mode in ["buffered", "streaming"] {
        let dir = tempfile::tempdir().unwrap();
        let mut value = config(dir.path(), mode);
        let sheet = &mut value["workbooks"][0]["worksheets"][0];
        sheet["layout"] = json!({"startRow":2,"startColumn":3,"autofilter":true});
        sheet["reportHeader"]["title"] = json!({"text":"Sales report"});
        sheet["reportHeader"]["columnCount"] = json!(3);
        sheet["reportHeader"]["gapRows"] = json!(1);
        sheet["reportHeader"]["columns"][0]["fields"]
            .as_array_mut()
            .unwrap()
            .push(json!({"label":"Owner","value":"Team A"}));
        ExcelExporter::export(serde_json::from_value(value).unwrap())
            .await
            .unwrap();
        let path = dir.path().join("header.xlsx");
        let mut book = open_workbook_auto(&path).unwrap();
        let range = book.worksheet_range("Report").unwrap();
        assert_eq!(
            range.get_value((1, 2)),
            Some(&Data::String("Sales report".into()))
        );
        assert_eq!(
            range.get_value((5, 2)),
            Some(&Data::String("Team A".into()))
        );
        assert_eq!(range.get_value((7, 2)), Some(&Data::String("c0".into())));
        assert_eq!(range.get_value((8, 2)), Some(&Data::Float(1.0)));
        let sheet = xml(&path, "xl/worksheets/sheet1.xml");
        for reference in ["C2:J2", "C3:E3", "F3:H3", "C5:E5", "C8:J9"] {
            assert!(sheet.contains(reference), "{reference} missing");
        }
        assert!(sheet.contains("ySplit=\"8\""));
        assert!(xml(&path, "xl/styles.xml").contains("sz val=\"20\""));
    }
}
#[test]
fn invalid_block_counts_overlaps_styles_and_capacity_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let base = config(dir.path(), "buffered");
    for count in [0, 9] {
        let mut value = base.clone();
        value["workbooks"][0]["worksheets"][0]["reportHeader"]["columnCount"] = json!(count);
        assert!(ExcelExporter::validate(&serde_json::from_value(value).unwrap()).is_err());
    }
    let mut value = base.clone();
    value["options"] = json!({"maxRows":2});
    assert_eq!(
        ExcelExporter::validate(&serde_json::from_value(value).unwrap())
            .unwrap_err()
            .code,
        "LIMIT_EXCEEDED"
    );
    let mut value = base.clone();
    value["workbooks"][0]["worksheets"][0]["layout"]["merges"] =
        json!([{"firstRow":1,"firstColumn":1,"lastRow":1,"lastColumn":8,"text":"overlap"}]);
    assert!(ExcelExporter::validate(&serde_json::from_value(value).unwrap()).is_err());
    let mut value = base;
    value["workbooks"][0]["worksheets"][0]["reportHeader"]["labelStyle"] = json!({"fontSize":-1});
    assert!(ExcelExporter::validate(&serde_json::from_value(value).unwrap()).is_err());
    assert!(!dir.path().join("header.xlsx").exists());
}
#[tokio::test]
async fn single_cell_spans_do_not_create_invalid_merges_and_tables_start_after_header() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = config(dir.path(), "buffered");
    let sheet = &mut value["workbooks"][0]["worksheets"][0];
    sheet["reportHeader"]["columnCount"] = json!(8);
    sheet["layout"] = json!({"header":true});
    sheet["table"] = json!({"name":"ReportData"});
    ExcelExporter::export(serde_json::from_value(value).unwrap())
        .await
        .unwrap();
    let path = dir.path().join("header.xlsx");
    assert!(!xml(&path, "xl/worksheets/sheet1.xml").contains("mergeCell"));
    assert!(xml(&path, "xl/tables/table1.xml").contains("ref=\"A3:H4\""));
}
