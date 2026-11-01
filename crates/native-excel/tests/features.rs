use calamine::{Data, Reader, open_workbook_auto};
use native_excel::*;
use serde_json::json;
use std::io::Read;

fn read_zip_part(path: &std::path::Path, part: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    zip.by_name(part)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

#[test]
fn excel_coordinates_roundtrip_through_xfd() {
    assert_eq!(column_index_to_name(1).unwrap(), "A");
    assert_eq!(column_index_to_name(26).unwrap(), "Z");
    assert_eq!(column_index_to_name(27).unwrap(), "AA");
    assert_eq!(column_index_to_name(52).unwrap(), "AZ");
    assert_eq!(column_index_to_name(53).unwrap(), "BA");
    assert_eq!(column_index_to_name(702).unwrap(), "ZZ");
    assert_eq!(column_index_to_name(703).unwrap(), "AAA");
    assert_eq!(column_index_to_name(16_384).unwrap(), "XFD");
    assert!(column_index_to_name(0).is_err());
    assert!(column_index_to_name(16_385).is_err());

    for idx in [1u16, 26, 27, 52, 53, 702, 703, 16_384] {
        let name = column_index_to_name(idx).unwrap();
        assert_eq!(column_name_to_index(&name).unwrap(), idx);
    }
    assert_eq!(parse_cell_ref("$XFD$1048576").unwrap(), (16_384, 1_048_576));
}

#[test]
fn workbook_builder_matches_oldexcel_capabilities_with_typed_cells() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("builder.xlsx");
    let mut wb = WorkbookBuilder::new(&out);
    let sheet = wb.create_sheet("Sheet 1", 30, 12).unwrap();
    sheet.set(1, 1, "Title").unwrap();
    sheet.merge((1, 1), (4, 1)).unwrap();
    sheet
        .font(
            1,
            1,
            FontSpec {
                name: Some("Calibri".into()),
                size: Some(16.0),
                bold: Some(true),
                color: Some("#1F497D".into()),
                ..Default::default()
            },
        )
        .unwrap();
    sheet
        .fill(
            1,
            1,
            FillSpec {
                pattern: Some("solid".into()),
                bg_color: Some("#D9E1F2".into()),
                ..Default::default()
            },
        )
        .unwrap();
    sheet
        .border(
            2,
            2,
            BorderSpec {
                left: Some("thin".into()),
                right: Some("medium".into()),
                top: Some("double".into()),
                bottom: Some("thick".into()),
                color: Some("#000000".into()),
            },
        )
        .unwrap();
    sheet.align(2, 2, "center").unwrap();
    sheet.valign(2, 2, "center").unwrap();
    sheet.rotate(2, 2, 45).unwrap();
    sheet.wrap(2, 2, true).unwrap();
    sheet.width(2, 24.0).unwrap();
    sheet.height(2, 28.0).unwrap();
    sheet.set_a1("AB3", 42.5).unwrap();
    sheet.set_formula(2, 4, "=SUM(AB3,10)").unwrap();
    wb.save().unwrap();

    let mut reader = open_workbook_auto(&out).unwrap();
    let range = reader.worksheet_range("Sheet 1").unwrap();
    assert_eq!(range.get_value((0, 0)), Some(&Data::String("Title".into())));
    assert_eq!(range.get_value((2, 27)), Some(&Data::Float(42.5)));
    let styles_xml = read_zip_part(&out, "xl/styles.xml");
    assert!(styles_xml.contains("double") && styles_xml.contains("medium"));
}

#[test]
fn append_workbook_writer_streams_rows_sequentially() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("append.xlsx");
    let mut writer = AppendWorkbookWriter::new(
        &out,
        "Streamed",
        vec![
            Column::new("id", DataType::Integer),
            Column::new("name", DataType::Text),
        ],
        false,
    )
    .unwrap();
    let mut row1 = Row::new();
    row1.insert("id".into(), json!(1));
    row1.insert("name".into(), json!("Alpha"));
    writer.write_row(&row1).unwrap();
    let count = writer.finish().unwrap();
    assert_eq!(count, 1);

    let mut reader = open_workbook_auto(&out).unwrap();
    let range = reader.worksheet_range("Streamed").unwrap();
    assert_eq!(range.get_value((1, 0)), Some(&Data::Float(1.0)));
    assert_eq!(range.get_value((1, 1)), Some(&Data::String("Alpha".into())));
}

#[tokio::test]
async fn incremental_caller_input_session_exports_batches_and_backpressures() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("stream_input.xlsx");
    let session = InputSession::open(InputOptions {
        queue_rows: 2,
        max_batch_bytes: 1024 * 1024,
        max_row_bytes: 65536,
    })
    .unwrap();
    let input_id = session.id().to_owned();

    let req: ExportRequest = serde_json::from_value(json!({
        "workbooks": [{
            "id": "input-book",
            "output": { "path": out.to_str().unwrap() },
            "worksheets": [{
                "name": "Records",
                "totalRows": 3,
                "source": { "kind": "rowStream", "inputId": input_id },
                "columns": [
                    { "field": "id", "dataType": "integer" },
                    { "field": "label", "dataType": "text" }
                ]
            }]
        }]
    }))
    .unwrap();

    let job = ExcelExporter::start(req).unwrap();
    session
        .write_batch(vec![
            serde_json::from_value(json!({"id": 1, "label": "One"})).unwrap(),
            serde_json::from_value(json!({"id": 2, "label": "Two"})).unwrap(),
        ])
        .await
        .unwrap();
    session
        .write_batch(vec![
            serde_json::from_value(json!({"id": 3, "label": "Three"})).unwrap(),
        ])
        .await
        .unwrap();
    session.finish().unwrap();

    let report = job.result().await.unwrap();
    assert_eq!(report.workbooks[0].rows, 3);
    let mut reader = open_workbook_auto(&out).unwrap();
    let range = reader.worksheet_range("Records").unwrap();
    assert_eq!(range.get_value((3, 1)), Some(&Data::String("Three".into())));
}

#[tokio::test]
async fn overflow_split_worksheet_and_split_workbook_conserve_every_record() {
    let dir = tempfile::tempdir().unwrap();
    let sheet_split_path = dir.path().join("split_sheets.xlsx");
    let records: Vec<_> = (1..=5)
        .map(|i| json!({"id": i, "name": format!("Item {i}")}))
        .collect();

    let req: ExportRequest = serde_json::from_value(json!({
        "options": { "maxRows": 3 },
        "workbooks": [{
            "id": "split-sheet-book",
            "output": { "path": sheet_split_path.to_str().unwrap() },
            "worksheets": [{
                "name": "Data",
                "source": { "kind": "objects", "records": records },
                "columns": [
                    { "field": "id", "dataType": "integer" },
                    { "field": "name", "dataType": "text" }
                ],
                "overflow": { "policy": "splitWorksheet", "namePattern": "Data_{index}", "maxSplits": 5 }
            }]
        }]
    }))
    .unwrap();

    let report = ExcelExporter::export(req).await.unwrap();
    assert_eq!(report.workbooks[0].rows, 5);
    assert_eq!(report.workbooks[0].sheets_completed, 3);

    let mut book = open_workbook_auto(&sheet_split_path).unwrap();
    let s1 = book.worksheet_range("Data").unwrap();
    let s2 = book.worksheet_range("Data_2").unwrap();
    let s3 = book.worksheet_range("Data_3").unwrap();
    assert_eq!(s1.get_value((1, 0)), Some(&Data::Float(1.0)));
    assert_eq!(s1.get_value((2, 0)), Some(&Data::Float(2.0)));
    assert_eq!(s2.get_value((1, 0)), Some(&Data::Float(3.0)));
    assert_eq!(s2.get_value((2, 0)), Some(&Data::Float(4.0)));
    assert_eq!(s3.get_value((1, 0)), Some(&Data::Float(5.0)));

    let wb_split_path = dir.path().join("split_book.xlsx");
    let req_wb: ExportRequest = serde_json::from_value(json!({
        "options": { "maxRows": 3 },
        "workbooks": [{
            "id": "split-wb-book",
            "output": { "path": wb_split_path.to_str().unwrap() },
            "worksheets": [{
                "name": "Data",
                "source": { "kind": "objects", "records": records },
                "columns": [
                    { "field": "id", "dataType": "integer" },
                    { "field": "name", "dataType": "text" }
                ],
                "overflow": { "policy": "splitWorkbook", "maxSplits": 5 }
            }]
        }]
    }))
    .unwrap();

    let report_wb = ExcelExporter::export(req_wb).await.unwrap();
    assert_eq!(report_wb.workbooks[0].rows, 5);
    assert_eq!(report_wb.workbooks[0].split_files.len(), 2);
    assert!(dir.path().join("split_book.xlsx").exists());
    assert!(dir.path().join("split_book_2.xlsx").exists());
    assert!(dir.path().join("split_book_3.xlsx").exists());
}

#[tokio::test]
async fn declarative_rules_cell_overrides_and_extended_policies_work_in_both_modes() {
    for mode in ["buffered", "streaming"] {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("rules.xlsx");
        let req: ExportRequest = serde_json::from_value(json!({
            "styles": {
                "base": { "bold": true, "background": "#EEEEEE" },
                "zebra": { "background": "#DDEEFF", "resetFields": ["bold"] },
                "alert": { "fontColor": "#CC0000", "borderBottom": "double", "borderBottomColor": "#CC0000" }
            },
            "workbooks": [{
                "id": "rules-book",
                "metadata": { "title": "Rules Report", "author": "NativeExcelTool", "company": "Contoso" },
                "output": { "path": out.to_str().unwrap(), "returnBytes": true },
                "worksheets": [{
                    "name": "Styled",
                    "writerMode": mode,
                    "bodyStyle": "base",
                    "caseInsensitiveFields": true,
                    "extraFields": "ignore",
                    "source": {
                        "kind": "objects",
                        "records": [
                            { "Customer": { "Name": "  alice  " }, "Amount": "1250.556", "Payload": "48656c6c6f", "Notes": null },
                            { "Customer": { "Name": "bob" }, "Amount": "50.10", "Payload": "576f726c64", "Notes": "ok" }
                        ]
                    },
                    "columns": [
                        { "field": "customer_name", "path": ["customer", "name"], "dataType": "text", "transforms": ["trim", "uppercase"] },
                        { "field": "amount", "dataType": "decimal", "precisionPolicy": "round", "roundScale": 2 },
                        { "field": "payload", "dataType": "binary", "binaryEncoding": "base64" },
                        { "field": "notes", "dataType": "text", "missingAsNull": true, "nullPlaceholder": "N/A" }
                    ],
                    "rules": [
                        { "when": { "rowParity": "even" }, "style": "zebra" },
                        { "when": { "field": "amount", "op": "gt", "value": 1000 }, "field": "amount", "style": "alert" }
                    ],
                    "cellOverrides": [
                        { "row": 2, "field": "customer_name", "format": { "italic": true } }
                    ],
                    "layout": {
                        "gridlines": false,
                        "zoom": 120,
                        "tabColor": "#336699",
                        "autofit": true,
                        "autofitSampleRows": 10,
                        "print": { "landscape": true, "fitToWidth": 1, "fitToHeight": 1 }
                    }
                }]
            }]
        }))
        .unwrap();

        let report = ExcelExporter::export(req).await.unwrap();
        assert_eq!(report.workbooks[0].rows, 2);
        assert!(
            report.workbooks[0]
                .bytes_base64
                .as_ref()
                .is_some_and(|b| !b.is_empty())
        );

        let mut reader = open_workbook_auto(&out).unwrap();
        let range = reader.worksheet_range("Styled").unwrap();
        assert_eq!(range.get_value((1, 0)), Some(&Data::String("ALICE".into())));
        assert_eq!(range.get_value((1, 1)), Some(&Data::Float(1250.56)));
        assert_eq!(
            range.get_value((1, 2)),
            Some(&Data::String("SGVsbG8=".into()))
        );
        assert_eq!(range.get_value((1, 3)), Some(&Data::String("N/A".into())));
        assert_eq!(range.get_value((2, 0)), Some(&Data::String("BOB".into())));

        let core_xml = read_zip_part(&out, "docProps/core.xml");
        assert!(core_xml.contains("Rules Report") && core_xml.contains("NativeExcelTool"));
        let styles_xml = read_zip_part(&out, "xl/styles.xml");
        assert!(styles_xml.contains("CC0000") && styles_xml.contains("DDEEFF"));
    }
}

#[tokio::test]
async fn column_inference_and_extra_field_policy_work() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("inferred.xlsx");
    let req: ExportRequest = serde_json::from_value(json!({
        "workbooks": [{
            "id": "infer",
            "output": { "path": out.to_str().unwrap() },
            "worksheets": [{
                "name": "Inferred",
                "inferColumns": true,
                "extraFields": "error",
                "source": {
                    "kind": "objects",
                    "records": [
                        { "a": 1, "b": "hello" },
                        { "a": 2, "b": "world" }
                    ]
                }
            }]
        }]
    }))
    .unwrap();
    let report = ExcelExporter::export(req).await.unwrap();
    assert_eq!(report.workbooks[0].rows, 2);

    let bad_req: ExportRequest = serde_json::from_value(json!({
        "workbooks": [{
            "id": "infer-bad",
            "output": { "path": dir.path().join("bad.xlsx").to_str().unwrap() },
            "worksheets": [{
                "name": "Inferred",
                "inferColumns": true,
                "extraFields": "error",
                "source": {
                    "kind": "objects",
                    "records": [
                        { "a": 1 },
                        { "a": 2, "unexpected": 3 }
                    ]
                }
            }]
        }]
    }))
    .unwrap();
    assert_eq!(
        ExcelExporter::export(bad_req).await.unwrap_err().code,
        "SCHEMA_MISMATCH"
    );
}
