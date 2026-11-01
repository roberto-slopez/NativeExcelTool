use crate::*;
use rust_xlsxwriter::{
    Color, Format, FormatAlign, FormatBorder, FormatUnderline, Table, TableColumn, TableStyle,
    Workbook,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct ExportJob {
    cancel: CancellationToken,
    status: Arc<Mutex<JobStatus>>,
    result: Arc<Mutex<Option<Result<ExportReport>>>>,
    done: Arc<tokio::sync::Notify>,
}
impl ExportJob {
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    pub fn status(&self) -> JobStatus {
        self.status.lock().unwrap().clone()
    }
    pub fn try_result(&self) -> Option<Result<ExportReport>> {
        self.result.lock().unwrap().clone()
    }
    pub async fn result(&self) -> Result<ExportReport> {
        loop {
            let notified = self.done.notified();
            if let Some(r) = self.try_result() {
                return r;
            }
            notified.await;
        }
    }
}
pub struct ExcelExporter;
impl ExcelExporter {
    pub fn validate(request: &ExportRequest) -> Result<()> {
        let options = &request.options;
        if request.schema_version != 1
            || request.workbooks.is_empty()
            || request.workbooks.len() > options.max_workbooks
            || options.max_rows == 0
            || options.max_rows > 1_048_576
            || !(1..=9).contains(&options.compression)
        {
            return Err(ExportError::invalid(
                "Invalid schema version or export limits",
            ));
        }
        if options.compression != 6 {
            return Err(ExportError::new(
                "UNSUPPORTED_OPTION",
                "This writer version supports compression level 6",
            ));
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for book in &request.workbooks {
            if !ids.insert(&book.id)
                || book.id.is_empty()
                || !paths.insert(normalized_path(&book.output.path)?)
            {
                return Err(ExportError::invalid("Duplicate workbook ID or output path"));
            }
            if book.worksheets.is_empty() || book.worksheets.len() > options.max_sheets {
                return Err(ExportError::invalid("Invalid worksheet count"));
            }
            let mut names = BTreeSet::new();
            let mut table_names = BTreeSet::new();
            for sheet in &book.worksheets {
                if sheet.name.is_empty()
                    || sheet.name.chars().count() > 31
                    || sheet.name.contains(['[', ']', ':', '*', '?', '/', '\\'])
                    || sheet.name.starts_with('\'')
                    || sheet.name.ends_with('\'')
                    || !names.insert(sheet.name.to_lowercase())
                {
                    return Err(ExportError::invalid("Invalid or duplicate worksheet name"));
                }
                let l = &sheet.layout;
                if sheet.columns.is_empty()
                    || l.start_column == 0
                    || l.start_row == 0
                    || l.start_row > options.max_rows
                    || (l.start_column as usize + sheet.columns.len() - 1) > 16_384
                    || l.freeze_rows > 1_048_576
                {
                    return Err(ExportError::invalid("Invalid column count or coordinates"));
                }
                let count = expected_rows(sheet)?;
                if let Some(count) = count
                    && u64::from(l.start_row - 1) + u64::from(l.header) + u64::from(count)
                        > u64::from(options.max_rows)
                {
                    return Err(ExportError::new(
                        "LIMIT_EXCEEDED",
                        "Declared data and headers exceed worksheet capacity",
                    ));
                }
                if let Some(table) = &sheet.table {
                    if is_streaming(sheet) {
                        return Err(ExportError::new(
                            "CAPABILITY_MISMATCH",
                            "Structured tables require writerMode buffered; the pinned backend retains per-cell table metadata",
                        ));
                    }
                    let count = count.ok_or_else(|| {
                        ExportError::invalid(
                            "Structured tables require totalRows for query sources",
                        )
                    })?;
                    if count == 0 || !l.header {
                        return Err(ExportError::invalid(
                            "Structured tables require headers and at least one data row",
                        ));
                    }
                    if (u64::from(count) + 1) * sheet.columns.len() as u64 > table.max_cells {
                        return Err(ExportError::new(
                            "LIMIT_EXCEEDED",
                            "Structured table exceeds maxCells budget",
                        ));
                    }
                    if table.name.is_empty()
                        || !table_names.insert(table.name.to_lowercase())
                        || table.name.len() > 255
                        || !table
                            .name
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                        || !table
                            .name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_')
                        || table.name.eq_ignore_ascii_case("r")
                        || table.name.eq_ignore_ascii_case("c")
                        || is_cell_reference(&table.name)
                    {
                        return Err(ExportError::invalid(
                            "Invalid or duplicate Excel table name",
                        ));
                    }
                    let headers: BTreeSet<_> = sheet
                        .columns
                        .iter()
                        .map(|c| c.header.as_deref().unwrap_or(&c.field).to_lowercase())
                        .collect();
                    if headers.len() != sheet.columns.len() || headers.contains("") {
                        return Err(ExportError::invalid(
                            "Structured table headers must be nonempty and unique",
                        ));
                    }
                }
                let mut styles = request.styles.clone();
                styles.extend(book.styles.clone());
                styles.extend(sheet.styles.clone());
                if styles.len() + sheet.columns.len() > options.max_styles {
                    return Err(ExportError::new("LIMIT_EXCEEDED", "Style budget exceeded"));
                }
                for style in styles.values() {
                    compile_style(style)?;
                }
                if let Some(id) = &sheet.body_style {
                    named(&styles, id)?;
                }
                if let Some(id) = &l.header_style {
                    named(&styles, id)?;
                }
                for col in &sheet.columns {
                    if col.field.is_empty() {
                        return Err(ExportError::invalid("Empty field"));
                    }
                    if let Some(id) = &col.style {
                        named(&styles, id)?;
                    }
                    compile_style(&column_style(sheet, col, &styles)?)?;
                }
                if is_streaming(sheet) && !l.merges.is_empty() {
                    return Err(ExportError::new(
                        "CAPABILITY_MISMATCH",
                        "Merges require buffered writing",
                    ));
                }
                for m in &l.merges {
                    if m.first_row == 0
                        || m.first_column == 0
                        || m.first_row > m.last_row
                        || m.first_column > m.last_column
                        || m.last_row > options.max_rows
                        || m.last_column > 16_384
                    {
                        return Err(ExportError::invalid("Invalid merge coordinates"));
                    }
                    if let Some(id) = &m.style {
                        named(&styles, id)?;
                    }
                    if m.last_row >= l.start_row {
                        return Err(ExportError::invalid(
                            "Merges must precede the data/header rows",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn start(request: ExportRequest) -> Result<ExportJob> {
        Self::validate(&request)?;
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| ExportError::invalid("A Tokio runtime is required"))?;
        let job = ExportJob {
            cancel: CancellationToken::new(),
            status: Arc::new(Mutex::new(JobStatus {
                phase: "running".into(),
                rows_written: 0,
                files_published: 0,
                total_rows: request
                    .workbooks
                    .iter()
                    .flat_map(|b| &b.worksheets)
                    .try_fold(0u64, |total, s| {
                        expected_rows(s)
                            .ok()
                            .flatten()
                            .map(|count| total + u64::from(count))
                    }),
            })),
            result: Arc::new(Mutex::new(None)),
            done: Arc::new(tokio::sync::Notify::new()),
        };
        let worker = job.clone();
        tokio::spawn(async move {
            let work = worker.clone();
            let result = tokio::task::spawn_blocking(move || execute(request, &work, &handle))
                .await
                .unwrap_or_else(|_| {
                    Err(ExportError::new("INTERNAL_ERROR", "Export worker failed"))
                });
            worker.status.lock().unwrap().phase = match &result {
                Ok(_) => "succeeded",
                Err(e) if e.code == "CANCELLED" => "cancelled",
                Err(_) => "failed",
            }
            .into();
            *worker.result.lock().unwrap() = Some(result);
            worker.done.notify_waiters();
        });
        Ok(job)
    }
    pub async fn export(request: ExportRequest) -> Result<ExportReport> {
        Self::start(request)?.result().await
    }
}
fn normalized_path(path: &str) -> Result<String> {
    let p = Path::new(path);
    if p.extension().and_then(|s| s.to_str()) != Some("xlsx") {
        return Err(ExportError::invalid("Output must have .xlsx extension"));
    }
    let absolute = std::path::absolute(p)?;
    Ok(absolute.to_string_lossy().replace('\\', "/").to_lowercase())
}
fn check_cancel(job: &ExportJob) -> Result<()> {
    if job.cancel.is_cancelled() {
        Err(ExportError::new("CANCELLED", "Export cancelled"))
    } else {
        Ok(())
    }
}
fn is_streaming(sheet: &WorksheetSpec) -> bool {
    sheet.writer_mode == WriterMode::Streaming
        || (sheet.writer_mode == WriterMode::Auto && matches!(sheet.source, Source::Query { .. }))
}
fn expected_rows(sheet: &WorksheetSpec) -> Result<Option<u32>> {
    if let Source::Objects { records } = &sheet.source {
        let actual = u32::try_from(records.len())
            .map_err(|_| ExportError::new("LIMIT_EXCEEDED", "Too many object records"))?;
        if sheet.total_rows.is_some_and(|count| count != actual) {
            return Err(ExportError::new(
                "ROW_COUNT_MISMATCH",
                "totalRows does not match object record count",
            ));
        }
        Ok(Some(actual))
    } else {
        Ok(sheet.total_rows)
    }
}
fn is_cell_reference(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    let letters = name.bytes().take_while(u8::is_ascii_alphabetic).count();
    if letters > 0
        && letters <= 3
        && name[letters..]
            .parse::<u32>()
            .is_ok_and(|r| (1..=1_048_576).contains(&r))
    {
        let col = name[..letters]
            .bytes()
            .fold(0u32, |n, c| n * 26 + u32::from(c - b'A' + 1));
        if col <= 16_384 {
            return true;
        }
    }
    if let Some(rest) = name.strip_prefix('R')
        && let Some((row, col)) = rest.split_once('C')
    {
        return row.parse::<u32>().is_ok() && col.parse::<u16>().is_ok();
    }
    false
}
fn column_style(
    sheet: &WorksheetSpec,
    col: &Column,
    styles: &BTreeMap<String, Style>,
) -> Result<Style> {
    let mut style = sheet
        .body_style
        .as_ref()
        .map(|id| named(styles, id))
        .transpose()?
        .cloned()
        .unwrap_or_default();
    if let Some(id) = &col.style {
        style = style.overlay(named(styles, id)?);
    }
    if let Some(format) = &col.format {
        style = style.overlay(&format.as_style());
    }
    if let Some(format) = &col.number_format {
        style.number_format = Some(format.clone());
    }
    if style.number_format.is_none() {
        style.number_format = match col.data_type {
            DataType::Date => Some("yyyy-mm-dd".into()),
            DataType::Datetime => Some("yyyy-mm-dd hh:mm:ss".into()),
            DataType::Time => Some("hh:mm:ss".into()),
            _ => None,
        };
    }
    if style
        .number_format
        .as_ref()
        .is_some_and(|v| v.is_empty() || v.len() > 255 || v.chars().any(char::is_control))
    {
        return Err(ExportError::invalid(
            "Excel number format must contain 1..255 bytes without control characters",
        ));
    }
    Ok(style)
}
fn execute(
    request: ExportRequest,
    job: &ExportJob,
    runtime: &tokio::runtime::Handle,
) -> Result<ExportReport> {
    let mut report = ExportReport { workbooks: vec![] };
    for spec in &request.workbooks {
        check_cancel(job)?;
        let result = write_book(&request, spec, job, runtime);
        let rows = result.as_ref().copied().unwrap_or(0);
        let error = result.err();
        report.workbooks.push(WorkbookReport {
            id: spec.id.clone(),
            path: spec.output.path.clone(),
            rows,
            error: error.clone(),
        });
        if let Some(e) = error
            && (!request.options.continue_on_error || e.code == "CANCELLED")
        {
            return Err(e);
        }
    }
    Ok(report)
}
fn write_book(
    request: &ExportRequest,
    spec: &WorkbookSpec,
    job: &ExportJob,
    runtime: &tokio::runtime::Handle,
) -> Result<u64> {
    let path = Path::new(&spec.output.path);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if spec.output.create_parents {
        std::fs::create_dir_all(parent)?;
    }
    if !spec.output.overwrite && path.exists() {
        return Err(ExportError::new("OUTPUT_EXISTS", "Output already exists"));
    }
    let staged = tempfile::NamedTempFile::new_in(parent)?;
    let mut book = Workbook::new();
    if let Some(dir) = &request.options.temp_directory {
        book.set_tempdir(dir)?;
    }
    let mut total = 0;
    for sheet in &spec.worksheets {
        check_cancel(job)?;
        let ws = if is_streaming(sheet) {
            book.add_worksheet_with_constant_memory()
        } else {
            book.add_worksheet()
        };
        ws.set_name(&sheet.name)?;
        let mut styles = request.styles.clone();
        styles.extend(spec.styles.clone());
        styles.extend(sheet.styles.clone());
        let l = &sheet.layout;
        let first_row = l.start_row - 1;
        let first_col = l.start_column - 1;
        let header = l
            .header_style
            .as_ref()
            .map(|id| named(&styles, id))
            .transpose()?
            .cloned()
            .unwrap_or_default();
        let header_format = compile_style(
            &Style {
                number_format: Some("General".into()),
                ..Default::default()
            }
            .overlay(&header),
        )?;
        let count = expected_rows(sheet)?;
        for (i, col) in sheet.columns.iter().enumerate() {
            let c = first_col + i as u16;
            if let Some(width) = col.width {
                ws.set_column_width(c, width)?;
            }
            ws.set_column_format(c, &compile_style(&column_style(sheet, col, &styles)?)?)?;
            if l.header {
                ws.write_string_with_format(
                    first_row,
                    c,
                    col.header.as_deref().unwrap_or(&col.field),
                    &header_format,
                )?;
            }
        }
        ws.set_freeze_panes(l.freeze_rows, 0)?;
        for m in &l.merges {
            let f = m
                .style
                .as_ref()
                .map(|id| named(&styles, id))
                .transpose()?
                .cloned()
                .unwrap_or_default();
            ws.merge_range(
                m.first_row - 1,
                m.first_column - 1,
                m.last_row - 1,
                m.last_column - 1,
                &m.text,
                &compile_style(&f)?,
            )?;
        }
        let mut row_index = first_row + u32::from(l.header);
        let data_start = row_index;
        let mut write = |row: &Row| -> Result<()> {
            check_cancel(job)?;
            if count.is_some_and(|count| row_index - data_start >= count) {
                return Err(ExportError::new(
                    "ROW_COUNT_MISMATCH",
                    "Source produced more rows than totalRows",
                ));
            }
            if row_index >= request.options.max_rows {
                return Err(ExportError::new(
                    "LIMIT_EXCEEDED",
                    "Worksheet row limit exceeded",
                ));
            }
            if let Some(h) = l.row_height {
                ws.set_row_height(row_index, h)?;
            }
            for (i, col) in sheet.columns.iter().enumerate() {
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
                crate::values::write_value(
                    ws,
                    row_index,
                    first_col + i as u16,
                    value,
                    col.data_type,
                )
                .map_err(|mut error| {
                    error.message = format!(
                        "Field {} at worksheet row {}: {}",
                        col.field,
                        row_index + 1,
                        error.message
                    );
                    error
                })?;
            }
            row_index += 1;
            total += 1;
            job.status.lock().unwrap().rows_written += 1;
            Ok(())
        };
        match &sheet.source {
            Source::Objects { records } => {
                for row in records {
                    write(row)?;
                }
            }
            Source::Query { request } => {
                let mut session = runtime.block_on(async { tokio::select! { _ = job.cancel.cancelled() => Err(ExportError::new("CANCELLED","Export cancelled")), r = QueryService::stream(request.clone()) => r } })?;
                for col in &sheet.columns {
                    if !session.schema().contains(&col.field) {
                        return Err(ExportError::new(
                            "SCHEMA_MISMATCH",
                            format!("Missing selected field {}", col.field),
                        ));
                    }
                }
                loop {
                    let row = runtime.block_on(async { tokio::select! { _ = job.cancel.cancelled() => Err(ExportError::new("CANCELLED","Export cancelled")), row = session.next() => row } })?;
                    match row {
                        Some(row) => write(&row)?,
                        None => break,
                    }
                }
            }
        }
        if count.is_some_and(|count| row_index - data_start != count) {
            return Err(ExportError::new(
                "ROW_COUNT_MISMATCH",
                "Source produced fewer rows than totalRows",
            ));
        }
        if let Some(table) = &sheet.table {
            let columns: Vec<_> = sheet
                .columns
                .iter()
                .map(|c| {
                    TableColumn::new()
                        .set_header(c.header.as_deref().unwrap_or(&c.field))
                        .set_header_format(&header_format)
                })
                .collect();
            let table = Table::new()
                .set_name(&table.name)
                .set_style(TableStyle::None)
                .set_columns(&columns);
            ws.add_table(
                first_row,
                first_col,
                row_index - 1,
                first_col + sheet.columns.len() as u16 - 1,
                &table,
            )?;
        } else if l.autofilter && l.header && row_index > first_row + 1 {
            ws.autofilter(
                first_row,
                first_col,
                row_index - 1,
                first_col + sheet.columns.len() as u16 - 1,
            )?;
        }
    }
    check_cancel(job)?;
    job.status.lock().unwrap().phase = "finalizing".into();
    book.save(staged.path())?;
    check_cancel(job)?;
    job.status.lock().unwrap().phase = "publishing".into();
    if spec.output.overwrite {
        staged
            .persist(path)
            .map_err(|e| ExportError::from(e.error))?;
    } else {
        staged
            .persist_noclobber(path)
            .map_err(|e| ExportError::from(e.error))?;
    }
    job.status.lock().unwrap().files_published += 1;
    Ok(total)
}
fn named<'a>(styles: &'a BTreeMap<String, Style>, id: &str) -> Result<&'a Style> {
    styles
        .get(id)
        .ok_or_else(|| ExportError::invalid(format!("Unknown style {id}")))
}
fn color(s: &str) -> Result<Color> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 {
        return Err(ExportError::invalid("Colors must be #RRGGBB"));
    }
    Ok(Color::RGB(
        u32::from_str_radix(s, 16).map_err(|_| ExportError::invalid("Invalid color"))?,
    ))
}
fn compile_style(s: &Style) -> Result<Format> {
    let mut f = Format::new();
    if let Some(v) = &s.font_name {
        f = f.set_font_name(v);
    }
    if let Some(v) = s.font_size {
        if !(1.0..=409.0).contains(&v) {
            return Err(ExportError::invalid("Invalid font size"));
        }
        f = f.set_font_size(v);
    }
    if s.bold == Some(true) {
        f = f.set_bold();
    }
    if s.italic == Some(true) {
        f = f.set_italic();
    }
    if s.underline == Some(true) {
        f = f.set_underline(FormatUnderline::Single);
    }
    if s.strikeout == Some(true) {
        f = f.set_font_strikethrough();
    }
    if let Some(v) = &s.font_color {
        f = f.set_font_color(color(v)?);
    }
    if let Some(v) = &s.background {
        f = f.set_background_color(color(v)?);
    }
    if let Some(v) = &s.border {
        f = f.set_border(match v.as_str() {
            "thin" => FormatBorder::Thin,
            "medium" => FormatBorder::Medium,
            "thick" => FormatBorder::Thick,
            "double" => FormatBorder::Double,
            _ => return Err(ExportError::invalid("Invalid border")),
        });
    }
    if let Some(v) = &s.border_color {
        f = f.set_border_color(color(v)?);
    }
    if let Some(v) = &s.horizontal {
        f = f.set_align(match v.as_str() {
            "left" => FormatAlign::Left,
            "center" => FormatAlign::Center,
            "right" => FormatAlign::Right,
            _ => return Err(ExportError::invalid("Invalid horizontal alignment")),
        });
    }
    if let Some(v) = &s.vertical {
        f = f.set_align(match v.as_str() {
            "top" => FormatAlign::Top,
            "center" => FormatAlign::VerticalCenter,
            "bottom" => FormatAlign::Bottom,
            _ => return Err(ExportError::invalid("Invalid vertical alignment")),
        });
    }
    if s.wrap == Some(true) {
        f = f.set_text_wrap();
    }
    if let Some(v) = s.rotation {
        if !(-90..=90).contains(&v) {
            return Err(ExportError::invalid("Rotation must be -90..90"));
        }
        f = f.set_rotation(v);
    }
    if let Some(v) = &s.number_format {
        f = f.set_num_format(v);
    }
    if s.locked == Some(false) {
        f = f.set_unlocked();
    }
    Ok(f)
}
