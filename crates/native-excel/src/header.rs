//! Plan horizontal merges and render header rows in order, including in streaming mode.
use crate::*;
use rust_xlsxwriter::Worksheet;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct HeaderPlan {
    pub table_row: u32,
    pub rows: Vec<HeaderRow>,
    pub style_count: usize,
}
pub(crate) struct HeaderRow {
    row: u32,
    height: Option<f64>,
    cells: Vec<HeaderCell>,
}
struct HeaderCell {
    first: u16,
    last: u16,
    value: Value,
    kind: DataType,
    style: Style,
}
pub(crate) fn plan(sheet: &WorksheetSpec, max_rows: u32) -> Result<HeaderPlan> {
    let first_row = sheet.layout.start_row - 1;
    let Some(header) = &sheet.report_header else {
        return Ok(HeaderPlan {
            table_row: first_row,
            rows: vec![],
            style_count: 0,
        });
    };
    let width = sheet.columns.len();
    let slots = header
        .column_count
        .map(usize::from)
        .unwrap_or(header.columns.len().max(1));
    if slots == 0 || slots > width || header.columns.len() > slots {
        return Err(ExportError::invalid(
            "reportHeader columnCount must be 1..data column count and accommodate all header columns",
        ));
    }
    let fields = header
        .columns
        .iter()
        .map(|c| c.fields.len())
        .max()
        .unwrap_or(0);
    if header.title.is_none() && fields == 0 {
        return Err(ExportError::invalid(
            "reportHeader needs a title or at least one field",
        ));
    }
    let height = u64::from(header.title.is_some()) + 2 * fields as u64 + u64::from(header.gap_rows);
    let table_row = u64::from(first_row) + height;
    if table_row >= u64::from(max_rows) {
        return Err(ExportError::new(
            "LIMIT_EXCEEDED",
            "Report header leaves no row for the table",
        ));
    }
    for m in &sheet.layout.merges {
        if m.first_row <= table_row as u32
            && m.last_row >= sheet.layout.start_row
            && usize::from(m.first_column) < usize::from(sheet.layout.start_column) + width
            && m.last_column >= sheet.layout.start_column
        {
            return Err(ExportError::invalid(
                "Manual merges must not overlap the automatic report header",
            ));
        }
    }
    for height in [
        header.label_height,
        header.value_height,
        header.title.as_ref().and_then(|t| t.height),
    ]
    .into_iter()
    .flatten()
    {
        if !(1.0..=409.0).contains(&height) {
            return Err(ExportError::invalid(
                "Header row heights must be 1..409 points",
            ));
        }
    }
    let mut rows: BTreeMap<u32, HeaderRow> = BTreeMap::new();
    let first_col = sheet.layout.start_column - 1;
    let normal = Style {
        font_name: Some("Calibri".into()),
        number_format: Some("General".into()),
        vertical: Some("center".into()),
        wrap: Some(true),
        ..Default::default()
    };
    if let Some(title) = &header.title {
        let style = normal
            .overlay(&Style {
                bold: Some(true),
                font_size: Some(20.0),
                horizontal: Some("center".into()),
                ..Default::default()
            })
            .overlay(&title.style);
        rows.insert(
            first_row,
            HeaderRow {
                row: first_row,
                height: Some(title.height.unwrap_or(32.0)),
                cells: vec![HeaderCell {
                    first: first_col,
                    last: first_col + width as u16 - 1,
                    value: Value::String(title.text.clone()),
                    kind: DataType::Text,
                    style,
                }],
            },
        );
    }
    let top = first_row + u32::from(header.title.is_some());
    let mut offset = 0usize;
    for slot in 0..slots {
        let span = width / slots + usize::from(slot < width % slots);
        if let Some(column) = header.columns.get(slot) {
            for (index, field) in column.fields.iter().enumerate() {
                let label = normal
                    .overlay(&Style {
                        bold: Some(true),
                        horizontal: Some("left".into()),
                        ..Default::default()
                    })
                    .overlay(&header.label_style)
                    .overlay(&field.label_style);
                let mut value = normal
                    .overlay(&Style {
                        horizontal: Some("center".into()),
                        ..Default::default()
                    })
                    .overlay(&header.value_style)
                    .overlay(&field.value_style);
                if let Some(format) = &field.number_format {
                    value.number_format = Some(format.clone());
                } else if value.number_format.as_deref() == Some("General") {
                    value.number_format = match field.data_type {
                        DataType::Date => Some("yyyy-mm-dd".into()),
                        DataType::Datetime => Some("yyyy-mm-dd hh:mm:ss".into()),
                        DataType::Time => Some("hh:mm:ss".into()),
                        _ => value.number_format,
                    };
                }
                for (row, text, kind, style, height) in [
                    (
                        top + index as u32 * 2,
                        Value::String(field.label.clone()),
                        DataType::Text,
                        label,
                        header.label_height,
                    ),
                    (
                        top + index as u32 * 2 + 1,
                        field.value.clone(),
                        field.data_type,
                        value,
                        header.value_height,
                    ),
                ] {
                    rows.entry(row)
                        .or_insert_with(|| HeaderRow {
                            row,
                            height,
                            cells: vec![],
                        })
                        .cells
                        .push(HeaderCell {
                            first: first_col + offset as u16,
                            last: first_col + (offset + span - 1) as u16,
                            value: text,
                            kind,
                            style,
                        });
                }
            }
        }
        offset += span;
    }
    let mut styles = BTreeSet::new();
    for row in rows.values() {
        for cell in &row.cells {
            crate::writer::compile_style(&cell.style)?;
            if cell
                .style
                .number_format
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 255 || s.chars().any(char::is_control))
            {
                return Err(ExportError::invalid("Invalid header Excel number format"));
            }
            styles.insert(serde_json::to_string(&cell.style).expect("style serialization"));
        }
    }
    Ok(HeaderPlan {
        table_row: table_row as u32,
        rows: rows.into_values().collect(),
        style_count: styles.len(),
    })
}
impl HeaderPlan {
    pub(crate) fn write(&self, ws: &mut Worksheet) -> Result<()> {
        for row in &self.rows {
            if let Some(height) = row.height {
                ws.set_row_height(row.row, height)?;
            }
            for cell in &row.cells {
                let format = crate::writer::compile_style(&cell.style)?;
                if cell.first != cell.last {
                    ws.merge_range(row.row, cell.first, row.row, cell.last, "", &format)?;
                } else {
                    ws.write_blank(row.row, cell.first, &format)?;
                }
                crate::values::write_value(ws, row.row, cell.first, &cell.value, cell.kind)?;
                ws.set_cell_format(row.row, cell.first, &format)?;
            }
        }
        Ok(())
    }
}
