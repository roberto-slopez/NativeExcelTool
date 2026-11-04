//! Buffered workbook builders and append-only streaming writers.
//!
//! The buffered builders support cell editing patterns from `msexcel-builder`
//! with validated coordinates and sparse cell storage.
use crate::{Column, DataType, ExportError, Result, Row, Style};
use rust_xlsxwriter::Workbook;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Converts a 1-based column index (`1..=16384`) to its Excel column name (`A..=XFD`).
pub fn column_index_to_name(col: u16) -> Result<String> {
    if col == 0 || col > 16_384 {
        return Err(ExportError::invalid(
            "Excel column index must be 1..=16384 (A..=XFD)",
        ));
    }
    let mut n = u32::from(col);
    let mut chars = Vec::with_capacity(3);
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        chars.push((b'A' + rem) as char);
        n = (n - 1) / 26;
    }
    chars.reverse();
    Ok(chars.into_iter().collect())
}

/// Converts an Excel column name (`A..=XFD`, case-insensitive) to a 1-based column index (`1..=16384`).
pub fn column_name_to_index(name: &str) -> Result<u16> {
    if name.is_empty() || name.len() > 3 || !name.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(ExportError::invalid("Invalid Excel column name"));
    }
    let col = name.bytes().fold(0u32, |acc, b| {
        acc * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
    });
    if col == 0 || col > 16_384 {
        return Err(ExportError::invalid(
            "Excel column name exceeds XFD (16384)",
        ));
    }
    Ok(col as u16)
}

/// Parses a human-facing A1 cell coordinate (e.g., `"A1"`, `"AA10"`, `"XFD1048576"`)
/// into 1-based `(column, row)`.
pub fn parse_cell_ref(cell: &str) -> Result<(u16, u32)> {
    let trimmed = cell.trim().replace('$', "");
    let letters = trimmed.bytes().take_while(u8::is_ascii_alphabetic).count();
    if letters == 0 || letters >= trimmed.len() {
        return Err(ExportError::invalid("Invalid A1 cell coordinate"));
    }
    let col = column_name_to_index(&trimmed[..letters])?;
    let row: u32 = trimmed[letters..]
        .parse()
        .map_err(|_| ExportError::invalid("Invalid A1 row coordinate"))?;
    if row == 0 || row > 1_048_576 {
        return Err(ExportError::invalid(
            "Excel row coordinate must be 1..=1048576",
        ));
    }
    Ok((col, row))
}

#[derive(Clone, Debug, Default)]
pub struct FontSpec {
    pub name: Option<String>,
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikeout: Option<bool>,
    pub color: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct FillSpec {
    pub pattern: Option<String>,
    pub fg_color: Option<String>,
    pub bg_color: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct BorderSpec {
    pub left: Option<String>,
    pub right: Option<String>,
    pub top: Option<String>,
    pub bottom: Option<String>,
    pub color: Option<String>,
}

/// Buffered worksheet builder supporting sparse random-access cell edits, styles, and merges.
///
/// Supports the cell editing operations of the `msexcel-builder` `Sheet` class
/// without allocating a dense grid for the declared dimensions.
#[derive(Clone)]
pub struct SheetBuilder {
    name: String,
    max_cols: u16,
    max_rows: u32,
    cells: BTreeMap<(u32, u16), (Value, DataType)>,
    styles: BTreeMap<(u32, u16), Style>,
    col_widths: BTreeMap<u16, f64>,
    row_heights: BTreeMap<u32, f64>,
    merges: Vec<((u16, u32), (u16, u32))>,
}

impl SheetBuilder {
    pub fn new(name: impl Into<String>, cols: u16, rows: u32) -> Result<Self> {
        let name = name.into();
        if name.is_empty()
            || name.chars().count() > 31
            || name.contains(['[', ']', ':', '*', '?', '/', '\\'])
            || name.starts_with('\'')
            || name.ends_with('\'')
        {
            return Err(ExportError::invalid("Invalid worksheet name"));
        }
        if cols == 0 || cols > 16_384 || rows == 0 || rows > 1_048_576 {
            return Err(ExportError::invalid(
                "Sheet dimensions must be within 1..=16384 columns and 1..=1048576 rows",
            ));
        }
        Ok(Self {
            name,
            max_cols: cols,
            max_rows: rows,
            cells: BTreeMap::new(),
            styles: BTreeMap::new(),
            col_widths: BTreeMap::new(),
            row_heights: BTreeMap::new(),
            merges: Vec::new(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn check_coords(&self, col: u16, row: u32) -> Result<()> {
        if col == 0 || col > self.max_cols || row == 0 || row > self.max_rows {
            return Err(ExportError::invalid(
                "Cell coordinates are out of declared sheet bounds",
            ));
        }
        Ok(())
    }

    /// Sets a cell value at 1-based `(col, row)`. Empty strings or `Value::Null` clear the value.
    pub fn set(&mut self, col: u16, row: u32, value: impl Into<Value>) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        let val = value.into();
        if val.is_null() || val.as_str() == Some("") {
            self.cells.remove(&(row, col));
        } else {
            self.cells.insert((row, col), (val, DataType::Auto));
        }
        Ok(self)
    }

    /// Sets a typed cell value at 1-based `(col, row)`.
    pub fn set_typed(
        &mut self,
        col: u16,
        row: u32,
        value: impl Into<Value>,
        data_type: DataType,
    ) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        self.cells.insert((row, col), (value.into(), data_type));
        Ok(self)
    }

    /// Sets a cell value using an A1 reference such as `"B2"` or `"AA10"`.
    pub fn set_a1(&mut self, cell: &str, value: impl Into<Value>) -> Result<&mut Self> {
        let (col, row) = parse_cell_ref(cell)?;
        self.set(col, row, value)
    }

    /// Sets an explicit formula at 1-based `(col, row)`.
    pub fn set_formula(
        &mut self,
        col: u16,
        row: u32,
        formula: impl Into<String>,
    ) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        self.cells.insert(
            (row, col),
            (Value::String(formula.into()), DataType::Formula),
        );
        Ok(self)
    }

    /// Merges a rectangular range between 1-based `(from_col, from_row)` and `(to_col, to_row)`.
    pub fn merge(&mut self, from: (u16, u32), to: (u16, u32)) -> Result<&mut Self> {
        self.check_coords(from.0, from.1)?;
        self.check_coords(to.0, to.1)?;
        if from.0 > to.0 || from.1 > to.1 {
            return Err(ExportError::invalid("Invalid merge range coordinates"));
        }
        self.merges.push((from, to));
        Ok(self)
    }

    pub fn width(&mut self, col: u16, width: f64) -> Result<&mut Self> {
        if col == 0 || col > self.max_cols || !(0.0..=255.0).contains(&width) {
            return Err(ExportError::invalid("Invalid column index or width"));
        }
        self.col_widths.insert(col, width);
        Ok(self)
    }

    pub fn height(&mut self, row: u32, height: f64) -> Result<&mut Self> {
        if row == 0 || row > self.max_rows || !(1.0..=409.0).contains(&height) {
            return Err(ExportError::invalid("Invalid row index or height"));
        }
        self.row_heights.insert(row, height);
        Ok(self)
    }

    pub fn font(&mut self, col: u16, row: u32, font: FontSpec) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        let entry = self.styles.entry((row, col)).or_default();
        if let Some(name) = font.name {
            entry.font_name = Some(name);
        }
        if let Some(size) = font.size {
            entry.font_size = Some(size);
        }
        if let Some(bold) = font.bold {
            entry.bold = Some(bold);
        }
        if let Some(italic) = font.italic {
            entry.italic = Some(italic);
        }
        if let Some(underline) = font.underline {
            entry.underline = Some(underline);
        }
        if let Some(strikeout) = font.strikeout {
            entry.strikeout = Some(strikeout);
        }
        if let Some(color) = font.color {
            entry.font_color = Some(color);
        }
        Ok(self)
    }

    pub fn fill(&mut self, col: u16, row: u32, fill: FillSpec) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        let entry = self.styles.entry((row, col)).or_default();
        if let Some(pattern) = fill.pattern {
            entry.fill_pattern = Some(pattern);
        }
        if let Some(fg) = fill.fg_color {
            entry.foreground_color = Some(fg);
        }
        if let Some(bg) = fill.bg_color {
            entry.background = Some(bg);
        }
        Ok(self)
    }

    pub fn border(&mut self, col: u16, row: u32, border: BorderSpec) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        let entry = self.styles.entry((row, col)).or_default();
        if let Some(left) = border.left {
            entry.border_left = Some(left);
        }
        if let Some(right) = border.right {
            entry.border_right = Some(right);
        }
        if let Some(top) = border.top {
            entry.border_top = Some(top);
        }
        if let Some(bottom) = border.bottom {
            entry.border_bottom = Some(bottom);
        }
        if let Some(color) = border.color {
            entry.border_color = Some(color);
        }
        Ok(self)
    }

    pub fn align(&mut self, col: u16, row: u32, align: impl Into<String>) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        self.styles.entry((row, col)).or_default().horizontal = Some(align.into());
        Ok(self)
    }

    pub fn valign(&mut self, col: u16, row: u32, valign: impl Into<String>) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        self.styles.entry((row, col)).or_default().vertical = Some(valign.into());
        Ok(self)
    }

    pub fn rotate(&mut self, col: u16, row: u32, rotation: i16) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        self.styles.entry((row, col)).or_default().rotation = Some(rotation);
        Ok(self)
    }

    pub fn wrap(&mut self, col: u16, row: u32, wrap: bool) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        self.styles.entry((row, col)).or_default().wrap = Some(wrap);
        Ok(self)
    }

    pub fn number_format(
        &mut self,
        col: u16,
        row: u32,
        format: impl Into<String>,
    ) -> Result<&mut Self> {
        self.check_coords(col, row)?;
        self.styles.entry((row, col)).or_default().number_format = Some(format.into());
        Ok(self)
    }
}

/// Buffered workbook builder with worksheet creation, saving, and cancellation.
///
/// Provides Rust equivalents of the `msexcel-builder` workbook operations.
pub struct WorkbookBuilder {
    output_path: PathBuf,
    overwrite: bool,
    sheets: Vec<SheetBuilder>,
    cancelled: bool,
}

impl WorkbookBuilder {
    pub fn new(output_path: impl AsRef<Path>) -> Self {
        Self {
            output_path: output_path.as_ref().to_path_buf(),
            overwrite: false,
            sheets: Vec::new(),
            cancelled: false,
        }
    }

    pub fn with_overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    pub fn create_sheet(
        &mut self,
        name: impl Into<String>,
        cols: u16,
        rows: u32,
    ) -> Result<&mut SheetBuilder> {
        if self.cancelled {
            return Err(ExportError::new(
                "CANCELLED",
                "Workbook builder was cancelled",
            ));
        }
        let sheet = SheetBuilder::new(name, cols, rows)?;
        if self
            .sheets
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(&sheet.name))
        {
            return Err(ExportError::invalid("Duplicate worksheet name"));
        }
        self.sheets.push(sheet);
        Ok(self.sheets.last_mut().unwrap())
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.sheets.clear();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    fn build_workbook(&self) -> Result<Workbook> {
        if self.cancelled {
            return Err(ExportError::new(
                "CANCELLED",
                "Workbook builder was cancelled",
            ));
        }
        if self.sheets.is_empty() {
            return Err(ExportError::invalid(
                "WorkbookBuilder requires at least one worksheet",
            ));
        }
        let mut book = Workbook::new();
        for sheet in &self.sheets {
            let ws = book.add_worksheet();
            ws.set_name(&sheet.name)?;
            for (&col, &width) in &sheet.col_widths {
                ws.set_column_width(col - 1, width)?;
            }
            for (&row, &height) in &sheet.row_heights {
                ws.set_row_height(row - 1, height)?;
            }
            for &(from, to) in &sheet.merges {
                let style = sheet
                    .styles
                    .get(&(from.1, from.0))
                    .cloned()
                    .unwrap_or_default();
                let format = crate::writer::compile_style(&style)?;
                let text = sheet
                    .cells
                    .get(&(from.1, from.0))
                    .and_then(|(v, _)| v.as_str())
                    .unwrap_or("");
                ws.merge_range(from.1 - 1, from.0 - 1, to.1 - 1, to.0 - 1, text, &format)?;
            }
            let mut coords: std::collections::BTreeSet<(u32, u16)> =
                sheet.cells.keys().copied().collect();
            coords.extend(sheet.styles.keys().copied());
            for (row, col) in coords {
                if sheet
                    .merges
                    .iter()
                    .any(|&(from, to)| col >= from.0 && col <= to.0 && row >= from.1 && row <= to.1)
                {
                    if let Some((val, kind)) = sheet.cells.get(&(row, col))
                        && sheet.merges.iter().any(|&(from, _)| (col, row) == from)
                        && val.as_str().is_none()
                    {
                        let style = sheet.styles.get(&(row, col)).cloned().unwrap_or_default();
                        let format = crate::writer::compile_style(&style)?;
                        crate::values::write_value_with_format(
                            ws,
                            row - 1,
                            col - 1,
                            val,
                            *kind,
                            Some(&format),
                            &Column::new("", *kind),
                        )?;
                    }
                    continue;
                }
                let format = sheet
                    .styles
                    .get(&(row, col))
                    .map(crate::writer::compile_style)
                    .transpose()?;
                if let Some((val, kind)) = sheet.cells.get(&(row, col)) {
                    crate::values::write_value_with_format(
                        ws,
                        row - 1,
                        col - 1,
                        val,
                        *kind,
                        format.as_ref(),
                        &Column::new("", *kind),
                    )?;
                } else if let Some(fmt) = format {
                    ws.write_blank(row - 1, col - 1, &fmt)?;
                }
            }
        }
        Ok(book)
    }

    pub fn save(&self) -> Result<()> {
        let mut book = self.build_workbook()?;
        if self.output_path.extension().and_then(|s| s.to_str()) != Some("xlsx") {
            return Err(ExportError::invalid("Output must have .xlsx extension"));
        }
        if !self.overwrite && self.output_path.exists() {
            return Err(ExportError::new("OUTPUT_EXISTS", "Output already exists"));
        }
        let parent = self
            .output_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let staged = tempfile::NamedTempFile::new_in(parent)?;
        book.save(staged.path())?;
        if self.overwrite {
            staged
                .persist(&self.output_path)
                .map_err(|e| ExportError::from(e.error))?;
        } else {
            staged
                .persist_noclobber(&self.output_path)
                .map_err(|e| ExportError::from(e.error))?;
        }
        Ok(())
    }

    pub fn save_to_bytes(&self, max_bytes: usize) -> Result<Vec<u8>> {
        let mut book = self.build_workbook()?;
        let bytes = book.save_to_buffer()?;
        if bytes.len() > max_bytes {
            return Err(ExportError::new(
                "LIMIT_EXCEEDED",
                "Generated workbook bytes exceed maxBytes budget",
            ));
        }
        Ok(bytes)
    }
}

/// Append-only streaming writer for caller-driven incremental row emission.
pub struct AppendWorkbookWriter {
    output_path: PathBuf,
    overwrite: bool,
    book: Workbook,
    columns: Vec<Column>,
    next_row: u32,
    rows_written: u64,
}

impl AppendWorkbookWriter {
    pub fn new(
        output_path: impl AsRef<Path>,
        sheet_name: &str,
        columns: Vec<Column>,
        overwrite: bool,
    ) -> Result<Self> {
        let output_path = output_path.as_ref().to_path_buf();
        if output_path.extension().and_then(|s| s.to_str()) != Some("xlsx") {
            return Err(ExportError::invalid("Output must have .xlsx extension"));
        }
        if columns.is_empty() || columns.len() > 16_384 {
            return Err(ExportError::invalid(
                "AppendWorkbookWriter requires 1..=16384 columns",
            ));
        }
        if !overwrite && output_path.exists() {
            return Err(ExportError::new("OUTPUT_EXISTS", "Output already exists"));
        }
        let mut book = Workbook::new();
        let ws = book.add_worksheet_with_constant_memory();
        ws.set_name(sheet_name)?;
        let header_style = crate::writer::compile_style(&Style {
            bold: Some(true),
            number_format: Some("General".into()),
            ..Default::default()
        })?;
        for (i, col) in columns.iter().enumerate() {
            if let Some(width) = col.width {
                ws.set_column_width(i as u16, width)?;
            }
            let fmt = crate::writer::compile_style(&crate::writer::default_column_style(col)?)?;
            ws.set_column_format(i as u16, &fmt)?;
            ws.write_string_with_format(
                0,
                i as u16,
                col.header.as_deref().unwrap_or(&col.field),
                &header_style,
            )?;
        }
        Ok(Self {
            output_path,
            overwrite,
            book,
            columns,
            next_row: 1,
            rows_written: 0,
        })
    }

    pub fn write_row(&mut self, row: &Row) -> Result<()> {
        if self.next_row >= 1_048_576 {
            return Err(ExportError::new(
                "LIMIT_EXCEEDED",
                "Worksheet row limit exceeded",
            ));
        }
        let ws = self.book.worksheet_from_index(0)?;
        for (i, col) in self.columns.iter().enumerate() {
            let value = match row.get(&col.field) {
                Some(v) => v,
                None if col.missing_as_null => &Value::Null,
                None => {
                    return Err(ExportError::new(
                        "SCHEMA_MISMATCH",
                        format!("Missing field {}", col.field),
                    ));
                }
            };
            crate::values::write_value_with_format(
                ws,
                self.next_row,
                i as u16,
                value,
                col.data_type,
                None,
                col,
            )?;
        }
        self.next_row += 1;
        self.rows_written += 1;
        Ok(())
    }

    pub fn write_batch(&mut self, rows: &[Row]) -> Result<()> {
        for row in rows {
            self.write_row(row)?;
        }
        Ok(())
    }

    pub fn rows_written(&self) -> u64 {
        self.rows_written
    }

    pub fn finish(mut self) -> Result<u64> {
        let parent = self
            .output_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let staged = tempfile::NamedTempFile::new_in(parent)?;
        self.book.save(staged.path())?;
        if self.overwrite {
            staged
                .persist(&self.output_path)
                .map_err(|e| ExportError::from(e.error))?;
        } else {
            staged
                .persist_noclobber(&self.output_path)
                .map_err(|e| ExportError::from(e.error))?;
        }
        Ok(self.rows_written)
    }
}
