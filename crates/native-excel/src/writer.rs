use crate::*;
use rust_xlsxwriter::{
    Color, DocProperties, Format, FormatAlign, FormatBorder, FormatPattern, FormatUnderline, Table,
    TableColumn, TableStyle, Workbook, Worksheet,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct ExportJob {
    progress: tokio::sync::watch::Sender<JobStatus>,
    cancel: CancellationToken,
    status: Arc<Mutex<JobStatus>>,
    result: Arc<Mutex<Option<Result<ExportReport>>>>,
    done: Arc<tokio::sync::Notify>,
}
impl ExportJob {
    pub fn subscribe_progress(&self) -> tokio::sync::watch::Receiver<JobStatus> {
        self.progress.subscribe()
    }
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
    pub fn capabilities() -> Capabilities {
        Capabilities {
            abi_version: 1,
            schema_version: 1,
            dbms: vec!["postgresql".into(), "sqlserver".into()],
            writer_modes: vec!["auto".into(), "buffered".into(), "streaming".into()],
            source_kinds: vec!["objects".into(), "query".into(), "rowStream".into()],
            overflow_policies: vec![
                "error".into(),
                "splitWorksheet".into(),
                "splitWorkbook".into(),
            ],
            streaming_tables: false,
            max_rows: 1_048_576,
            max_columns: 16_384,
        }
    }

    pub async fn export_objects(
        path: impl Into<String>,
        sheet_name: impl Into<String>,
        columns: Vec<Column>,
        records: Vec<Row>,
    ) -> Result<ExportReport> {
        let request = ExportRequest {
            schema_version: 1,
            styles: BTreeMap::new(),
            defaults: None,
            options: ExportOptions::default(),
            workbooks: vec![WorkbookSpec {
                id: "export".into(),
                output: Output {
                    path: path.into(),
                    overwrite: false,
                    create_parents: true,
                    return_bytes: false,
                    sync_data: false,
                },
                metadata: None,
                defaults: None,
                styles: BTreeMap::new(),
                worksheets: vec![WorksheetSpec {
                    name: sheet_name.into(),
                    source: Source::Objects { records },
                    columns,
                    infer_columns: false,
                    case_insensitive_fields: false,
                    extra_fields: ExtraFieldPolicy::Ignore,
                    report_header: None,
                    total_rows: None,
                    writer_mode: WriterMode::Buffered,
                    body_style: None,
                    table: None,
                    overflow: OverflowSpec::default(),
                    rules: vec![],
                    cell_overrides: vec![],
                    styles: BTreeMap::new(),
                    layout: Layout::default(),
                }],
            }],
        };
        Self::export(request).await
    }

    pub async fn export_query(
        path: impl Into<String>,
        sheet_name: impl Into<String>,
        columns: Vec<Column>,
        query: QueryRequest,
    ) -> Result<ExportReport> {
        let request = ExportRequest {
            schema_version: 1,
            styles: BTreeMap::new(),
            defaults: None,
            options: ExportOptions::default(),
            workbooks: vec![WorkbookSpec {
                id: "export".into(),
                output: Output {
                    path: path.into(),
                    overwrite: false,
                    create_parents: true,
                    return_bytes: false,
                    sync_data: false,
                },
                metadata: None,
                defaults: None,
                styles: BTreeMap::new(),
                worksheets: vec![WorksheetSpec {
                    name: sheet_name.into(),
                    source: Source::Query { request: query },
                    columns,
                    infer_columns: false,
                    case_insensitive_fields: false,
                    extra_fields: ExtraFieldPolicy::Ignore,
                    report_header: None,
                    total_rows: None,
                    writer_mode: WriterMode::Streaming,
                    body_style: None,
                    table: None,
                    overflow: OverflowSpec::default(),
                    rules: vec![],
                    cell_overrides: vec![],
                    styles: BTreeMap::new(),
                    layout: Layout::default(),
                }],
            }],
        };
        Self::export(request).await
    }

    pub fn validate(request: &ExportRequest) -> Result<()> {
        let resolved = resolve_request_defaults(request)?;
        crate::telemetry::validate(&resolved)?;
        let options = &resolved.options;
        if resolved.schema_version != 1
            || resolved.workbooks.is_empty()
            || resolved.workbooks.len() > options.max_workbooks
            || options.max_rows == 0
            || options.max_rows > 1_048_576
            || options.max_output_bytes == 0
            || options.max_output_bytes > 512 * 1024 * 1024
            || options.export_timeout_ms == Some(0)
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
        let mut stream_ids = BTreeSet::new();
        for book in &resolved.workbooks {
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
                if !is_valid_sheet_name(&sheet.name) || !names.insert(sheet.name.to_lowercase()) {
                    return Err(ExportError::invalid("Invalid or duplicate worksheet name"));
                }
                if let Source::RowStream { input_id } = &sheet.source {
                    if input_id.is_empty() || !stream_ids.insert(input_id.clone()) {
                        return Err(ExportError::invalid(
                            "RowStream inputId must be nonempty and unique across worksheets",
                        ));
                    }
                    if !crate::input::InputService::exists(input_id) {
                        return Err(ExportError::new(
                            "INVALID_HANDLE",
                            format!("Unknown or already consumed rowStream inputId {input_id}"),
                        ));
                    }
                }
                if sheet.overflow.policy != OverflowPolicy::Error {
                    if sheet.overflow.max_splits == 0 || sheet.overflow.max_splits > 1024 {
                        return Err(ExportError::invalid("overflow.maxSplits must be 1..=1024"));
                    }
                    if sheet.table.is_some() {
                        return Err(ExportError::new(
                            "CAPABILITY_MISMATCH",
                            "Structured tables cannot be combined with overflow splitting",
                        ));
                    }
                    if sheet.overflow.policy == OverflowPolicy::SplitWorkbook
                        && book.worksheets.len() > 1
                    {
                        return Err(ExportError::invalid(
                            "splitWorkbook requires a single source worksheet per workbook",
                        ));
                    }
                    if sheet.overflow.policy == OverflowPolicy::SplitWorksheet {
                        let first_split = format_split_sheet_name(sheet, 2)?;
                        if !is_valid_sheet_name(&first_split)
                            || names.contains(&first_split.to_lowercase())
                        {
                            return Err(ExportError::invalid(
                                "Invalid or colliding overflow worksheet namePattern",
                            ));
                        }
                    }
                }
                let l = &sheet.layout;
                if sheet.columns.is_empty()
                    || l.start_column == 0
                    || l.start_row == 0
                    || l.start_row > options.max_rows
                    || (l.start_column as usize + sheet.columns.len() - 1) > 16_384
                    || l.freeze_rows > 1_048_576
                    || l.freeze_columns > 16_384
                {
                    return Err(ExportError::invalid("Invalid column count or coordinates"));
                }
                if l.zoom.is_some_and(|z| !(10..=400).contains(&z)) {
                    return Err(ExportError::invalid("Layout zoom must be 10..=400"));
                }
                if let Some(tc) = &l.tab_color {
                    color(tc)?;
                }
                if is_streaming(sheet)
                    && l.autofit
                    && l.autofit_sample_rows.is_none_or(|n| n == 0 || n > 10_000)
                {
                    return Err(ExportError::new(
                        "CAPABILITY_MISMATCH",
                        "Streaming autofit requires autofitSampleRows in 1..=10000",
                    ));
                }
                for (&r, &h) in &l.row_heights {
                    if r == 0 || r > options.max_rows || !(1.0..=409.0).contains(&h) {
                        return Err(ExportError::invalid("Invalid layout rowHeights entry"));
                    }
                }
                for &c in &l.hidden_columns {
                    if c == 0 || c > 16_384 {
                        return Err(ExportError::invalid("Invalid hidden column index"));
                    }
                }
                for &r in &l.hidden_rows {
                    if r == 0 || r > options.max_rows {
                        return Err(ExportError::invalid("Invalid hidden row index"));
                    }
                }
                let count = expected_rows(sheet)?;
                let header_plan = crate::header::plan(sheet, options.max_rows)?;
                if sheet.report_header.is_some()
                    && l.freeze_rows > 0
                    && header_plan
                        .table_row
                        .checked_add(l.freeze_rows)
                        .is_none_or(|n| n > 1_048_576)
                {
                    return Err(ExportError::invalid("Freeze rows exceed worksheet limit"));
                }
                let data_start = u64::from(header_plan.table_row) + u64::from(l.header);
                if data_start >= u64::from(options.max_rows) {
                    return Err(ExportError::new(
                        "LIMIT_EXCEEDED",
                        "Headers leave no capacity for data rows",
                    ));
                }
                let per_sheet_capacity = u64::from(options.max_rows) - data_start;
                if let Some(count) = count {
                    let max_capacity = match sheet.overflow.policy {
                        OverflowPolicy::Error => per_sheet_capacity,
                        OverflowPolicy::SplitWorksheet | OverflowPolicy::SplitWorkbook => {
                            per_sheet_capacity
                                .saturating_mul(u64::from(sheet.overflow.max_splits) + 1)
                        }
                    };
                    if u64::from(count) > max_capacity {
                        return Err(ExportError::new(
                            "LIMIT_EXCEEDED",
                            "Declared data and headers exceed worksheet capacity",
                        ));
                    }
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
                let mut styles = resolved.styles.clone();
                styles.extend(book.styles.clone());
                styles.extend(sheet.styles.clone());
                if sheet.rules.len() > 128 {
                    return Err(ExportError::new(
                        "LIMIT_EXCEEDED",
                        "Worksheet exceeds maximum of 128 style rules",
                    ));
                }
                if styles.len()
                    + sheet.columns.len()
                    + header_plan.style_count
                    + sheet.rules.len()
                    + sheet.cell_overrides.len()
                    > options.max_styles
                {
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
                let mut col_fields_ci = BTreeSet::new();
                for col in &sheet.columns {
                    if col.field.is_empty() {
                        return Err(ExportError::invalid("Empty field"));
                    }
                    if let Some(path) = &col.path
                        && (path.is_empty() || path.iter().any(String::is_empty))
                    {
                        return Err(ExportError::invalid(
                            "Column path segments must be nonempty",
                        ));
                    }
                    if sheet.case_insensitive_fields
                        && !col_fields_ci.insert(col.field.to_lowercase())
                    {
                        return Err(ExportError::invalid(
                            "Case-insensitive column field collision",
                        ));
                    }
                    if let Some(tz) = &col.timezone
                        && !tz.eq_ignore_ascii_case("preserveText")
                        && !tz.eq_ignore_ascii_case("utc")
                        && tz != "Z"
                    {
                        parse_tz_check(tz)?;
                    }
                    if col.data_type == DataType::Binary && col.binary_encoding.is_none() {
                        return Err(ExportError::invalid(
                            "Binary columns require binaryEncoding (hex or base64)",
                        ));
                    }
                    if let Some(id) = &col.style {
                        named(&styles, id)?;
                    }
                    compile_style(&column_style(sheet, col, &styles)?)?;
                }
                for rule in &sheet.rules {
                    validate_rule_condition(&rule.when, 1)?;
                    if rule.style.is_none() && rule.format.is_none() {
                        return Err(ExportError::invalid("StyleRule requires style or format"));
                    }
                    if let Some(id) = &rule.style {
                        named(&styles, id)?;
                    }
                    if let Some(fmt) = &rule.format {
                        compile_style(fmt)?;
                    }
                }
                for ov in &sheet.cell_overrides {
                    if ov.row == 0 || !sheet.columns.iter().any(|c| c.field == ov.field) {
                        return Err(ExportError::invalid(
                            "CellStyleOverride requires valid 1-based row and column field",
                        ));
                    }
                    if ov.style.is_none() && ov.format.is_none() {
                        return Err(ExportError::invalid(
                            "CellStyleOverride requires style or format",
                        ));
                    }
                    if let Some(id) = &ov.style {
                        named(&styles, id)?;
                    }
                    if let Some(fmt) = &ov.format {
                        compile_style(fmt)?;
                    }
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
                    if m.last_row > header_plan.table_row {
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
        Self::start_with_sink(request, None)
    }

    pub fn start_with_sink(
        request: ExportRequest,
        sink: Option<Arc<dyn TelemetrySink>>,
    ) -> Result<ExportJob> {
        Self::validate(&request)?;
        let resolved = resolve_request_defaults(&request)?;
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| ExportError::invalid("A Tokio runtime is required"))?;
        let mut telemetry = crate::telemetry::Telemetry::open(&resolved.options.telemetry, sink)?;
        let sample_interval = resolved.options.telemetry.sample_interval_ms;
        let progress_interval = resolved.options.progress_interval_ms;
        let export_timeout = resolved.options.export_timeout_ms;
        let job = ExportJob {
            progress: tokio::sync::watch::channel(JobStatus::default()).0,
            cancel: CancellationToken::new(),
            status: Arc::new(Mutex::new(JobStatus {
                phase: "running".into(),
                rows_written: 0,
                files_published: 0,
                telemetry_errors: 0,
                total_rows: resolved
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
        job.progress.send_replace(job.status());
        let worker = job.clone();
        tokio::spawn(async move {
            record_telemetry(&worker, &mut telemetry).await;
            let work = worker.clone();
            let mut task = tokio::task::spawn_blocking(move || execute(resolved, &work, &handle));
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(
                progress_interval.min(sample_interval),
            ));
            let deadline = export_timeout
                .map(|ms| tokio::time::Instant::now() + std::time::Duration::from_millis(ms));
            let mut sampled: Option<std::time::Instant> = None;
            let result = loop {
                tokio::select! {
                    result = &mut task => break result.unwrap_or_else(|_| {
                        Err(ExportError::new("INTERNAL_ERROR", "Export worker failed"))
                    }),
                    _ = interval.tick() => {
                        if let Some(dl) = deadline && tokio::time::Instant::now() >= dl {
                            worker.cancel();
                            break Err(ExportError::new("TIMEOUT", "Export deadline exceeded"));
                        }
                        worker.progress.send_replace(worker.status());
                        if sampled.is_none_or(|t| t.elapsed().as_millis() >= u128::from(sample_interval)) {
                            record_telemetry(&worker, &mut telemetry).await;
                            sampled = Some(std::time::Instant::now());
                        }
                    }
                }
            };
            worker.status.lock().unwrap().phase = match &result {
                Ok(_) => "succeeded",
                Err(e) if e.code == "CANCELLED" => "cancelled",
                Err(_) => "failed",
            }
            .into();
            record_telemetry(&worker, &mut telemetry).await;
            worker.progress.send_replace(worker.status());
            *worker.result.lock().unwrap() = Some(result);
            worker.done.notify_waiters();
        });
        Ok(job)
    }

    pub async fn export(request: ExportRequest) -> Result<ExportReport> {
        Self::start(request)?.result().await
    }
}

fn resolve_request_defaults(request: &ExportRequest) -> Result<ExportRequest> {
    let mut resolved = request.clone();
    let req_defaults = resolved.defaults.clone().unwrap_or_default();
    for book in &mut resolved.workbooks {
        let book_defaults = book.defaults.clone().unwrap_or_default();
        let eff_writer_mode = book_defaults.writer_mode.or(req_defaults.writer_mode);
        let eff_body_style = book_defaults
            .body_style
            .or_else(|| req_defaults.body_style.clone());
        let eff_header_style = book_defaults
            .header_style
            .or_else(|| req_defaults.header_style.clone());
        let eff_row_height = book_defaults.row_height.or(req_defaults.row_height);
        let eff_autofilter = book_defaults.autofilter.or(req_defaults.autofilter);
        let eff_freeze_rows = book_defaults.freeze_rows.or(req_defaults.freeze_rows);

        for sheet in &mut book.worksheets {
            if sheet.writer_mode == WriterMode::Auto
                && let Some(wm) = eff_writer_mode
            {
                sheet.writer_mode = wm;
            }
            if sheet.body_style.is_none() {
                sheet.body_style = eff_body_style.clone();
            }
            if sheet.layout.header_style.is_none() {
                sheet.layout.header_style = eff_header_style.clone();
            }
            if sheet.layout.row_height.is_none() {
                sheet.layout.row_height = eff_row_height;
            }
            if !sheet.layout.autofilter
                && let Some(af) = eff_autofilter
            {
                sheet.layout.autofilter = af;
            }
            if sheet.layout.freeze_rows == 1
                && let Some(fr) = eff_freeze_rows
            {
                sheet.layout.freeze_rows = fr;
            }
            if sheet.infer_columns && sheet.columns.is_empty() {
                match &sheet.source {
                    Source::Objects { records } => {
                        let first = records.first().ok_or_else(|| {
                            ExportError::invalid(
                                "inferColumns requires at least one record when columns are omitted",
                            )
                        })?;
                        if first.is_empty() {
                            return Err(ExportError::invalid(
                                "inferColumns cannot derive columns from an empty object record",
                            ));
                        }
                        sheet.columns = first
                            .keys()
                            .map(|k| Column::new(k.clone(), DataType::Auto))
                            .collect();
                    }
                    _ => {
                        return Err(ExportError::invalid(
                            "inferColumns is only supported for object sources",
                        ));
                    }
                }
            }
        }
    }
    Ok(resolved)
}

fn is_valid_sheet_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= 31
        && !name.contains(['[', ']', ':', '*', '?', '/', '\\'])
        && !name.starts_with('\'')
        && !name.ends_with('\'')
}

fn format_split_sheet_name(sheet: &WorksheetSpec, index: u32) -> Result<String> {
    let name = if let Some(pattern) = &sheet.overflow.name_pattern {
        if !pattern.contains("{index}") {
            return Err(ExportError::invalid(
                "overflow.namePattern must contain {index}",
            ));
        }
        pattern.replace("{index}", &index.to_string())
    } else {
        format!("{}_{}", sheet.name, index)
    };
    if !is_valid_sheet_name(&name) {
        return Err(ExportError::invalid(
            "Generated overflow worksheet name is invalid or exceeds 31 characters",
        ));
    }
    Ok(name)
}

fn format_split_workbook_path(
    base_path: &Path,
    sheet: &WorksheetSpec,
    index: u32,
) -> Result<PathBuf> {
    let parent = base_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stem = base_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| ExportError::invalid("Invalid workbook output filename"))?;
    let filename = if let Some(pattern) = &sheet.overflow.name_pattern {
        if !pattern.contains("{index}") {
            return Err(ExportError::invalid(
                "overflow.namePattern must contain {index}",
            ));
        }
        let base = pattern.replace("{index}", &index.to_string());
        if base.ends_with(".xlsx") {
            base
        } else {
            format!("{base}.xlsx")
        }
    } else {
        format!("{stem}_{index}.xlsx")
    };
    Ok(parent.join(filename))
}

fn parse_tz_check(tz: &str) -> Result<()> {
    let sign = tz.as_bytes().first().copied();
    if !matches!(sign, Some(b'+') | Some(b'-')) {
        return Err(ExportError::invalid(
            "Timezone must be UTC, preserveText, or +HH:MM / -HH:MM",
        ));
    }
    let (hh, mm) = tz[1..]
        .split_once(':')
        .ok_or_else(|| ExportError::invalid("Timezone must be +HH:MM or -HH:MM"))?;
    let hours: u32 = hh
        .parse()
        .map_err(|_| ExportError::invalid("Invalid timezone hours"))?;
    let mins: u32 = mm
        .parse()
        .map_err(|_| ExportError::invalid("Invalid timezone minutes"))?;
    if hours > 14 || mins > 59 {
        return Err(ExportError::invalid("Timezone offset out of range"));
    }
    Ok(())
}

fn validate_rule_condition(cond: &RuleCondition, depth: usize) -> Result<()> {
    if depth > 16 {
        return Err(ExportError::new(
            "LIMIT_EXCEEDED",
            "StyleRule expression depth exceeds 16",
        ));
    }
    let mut branches = 0;
    if cond.row_parity.is_some() {
        branches += 1;
    }
    if cond.field.is_some() || cond.op.is_some() {
        if cond.field.as_deref().is_none_or(str::is_empty) || cond.op.is_none() {
            return Err(ExportError::invalid(
                "Comparison RuleCondition requires both field and op",
            ));
        }
        branches += 1;
    }
    if let Some(all) = &cond.all {
        if all.is_empty() {
            return Err(ExportError::invalid("RuleCondition.all cannot be empty"));
        }
        for c in all {
            validate_rule_condition(c, depth + 1)?;
        }
        branches += 1;
    }
    if let Some(any) = &cond.any {
        if any.is_empty() {
            return Err(ExportError::invalid("RuleCondition.any cannot be empty"));
        }
        for c in any {
            validate_rule_condition(c, depth + 1)?;
        }
        branches += 1;
    }
    if let Some(not) = &cond.not {
        validate_rule_condition(not, depth + 1)?;
        branches += 1;
    }
    if branches == 0 {
        return Err(ExportError::invalid("Empty RuleCondition"));
    }
    Ok(())
}

fn evaluate_condition(cond: &RuleCondition, row: &Row, data_row_1based: u32, ci: bool) -> bool {
    let mut matched = true;
    if let Some(parity) = cond.row_parity {
        let is_even = data_row_1based.is_multiple_of(2);
        matched &= match parity {
            RowParity::Even => is_even,
            RowParity::Odd => !is_even,
        };
    }
    if let (Some(field), Some(op)) = (&cond.field, cond.op) {
        let val = lookup_simple(row, field, ci).unwrap_or(&Value::Null);
        let target = cond.value.as_ref().unwrap_or(&Value::Null);
        matched &= match op {
            ComparisonOp::IsNull => val.is_null(),
            ComparisonOp::NotNull => !val.is_null(),
            ComparisonOp::Eq => values_equal(val, target),
            ComparisonOp::Ne => !values_equal(val, target),
            ComparisonOp::Gt => compare_values(val, target).is_some_and(|o| o.is_gt()),
            ComparisonOp::Gte => compare_values(val, target).is_some_and(|o| o.is_ge()),
            ComparisonOp::Lt => compare_values(val, target).is_some_and(|o| o.is_lt()),
            ComparisonOp::Lte => compare_values(val, target).is_some_and(|o| o.is_le()),
        };
    }
    if let Some(all) = &cond.all {
        matched &= all
            .iter()
            .all(|c| evaluate_condition(c, row, data_row_1based, ci));
    }
    if let Some(any) = &cond.any {
        matched &= any
            .iter()
            .any(|c| evaluate_condition(c, row, data_row_1based, ci));
    }
    if let Some(not) = &cond.not {
        matched &= !evaluate_condition(not, row, data_row_1based, ci);
    }
    matched
}

fn values_equal(a: &Value, b: &Value) -> bool {
    if let (Some(fa), Some(fb)) = (as_f64(a), as_f64(b)) {
        (fa - fb).abs() < f64::EPSILON
    } else {
        a == b
    }
}

fn compare_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    if let (Some(fa), Some(fb)) = (as_f64(a), as_f64(b)) {
        fa.partial_cmp(&fb)
    } else if let (Some(sa), Some(sb)) = (a.as_str(), b.as_str()) {
        Some(sa.cmp(sb))
    } else {
        None
    }
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| {
        v.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| {
            v.as_object()
                .and_then(|o| o.get("value"))
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<f64>().ok())
        })
    })
}

fn lookup_simple<'a>(row: &'a Row, key: &str, ci: bool) -> Option<&'a Value> {
    if !ci {
        row.get(key)
    } else {
        row.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }
}

fn extract_field_value<'a>(row: &'a Row, col: &Column, ci: bool) -> Option<&'a Value> {
    if let Some(path) = &col.path {
        let first = path.first()?;
        let mut current = lookup_simple(row, first, ci)?;
        for segment in &path[1..] {
            let obj = current.as_object()?;
            current = if !ci {
                obj.get(segment)?
            } else {
                obj.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(segment))
                    .map(|(_, v)| v)?
            };
        }
        Some(current)
    } else {
        lookup_simple(row, &col.field, ci)
    }
}

fn check_row_schema_policies(row: &Row, sheet: &WorksheetSpec) -> Result<()> {
    if sheet.case_insensitive_fields {
        let mut seen = BTreeSet::new();
        for k in row.keys() {
            if !seen.insert(k.to_lowercase()) {
                return Err(ExportError::new(
                    "SCHEMA_MISMATCH",
                    "Record contains case-colliding keys under caseInsensitiveFields",
                ));
            }
        }
    }
    if sheet.extra_fields == ExtraFieldPolicy::Error {
        for k in row.keys() {
            let known = sheet.columns.iter().any(|c| {
                let root = c.path.as_ref().and_then(|p| p.first()).unwrap_or(&c.field);
                if sheet.case_insensitive_fields {
                    root.eq_ignore_ascii_case(k)
                } else {
                    root == k
                }
            });
            if !known {
                return Err(ExportError::new(
                    "SCHEMA_MISMATCH",
                    format!("Unexpected extra field {k}"),
                ));
            }
        }
    }
    Ok(())
}

async fn record_telemetry(job: &ExportJob, telemetry: &mut Option<crate::telemetry::Telemetry>) {
    if let Some(mut logger) = telemetry.take() {
        let status = job.status();
        match tokio::task::spawn_blocking(move || {
            let errors = logger.record(&status);
            (logger, errors)
        })
        .await
        {
            Ok((logger, errors)) => {
                *telemetry = Some(logger);
                job.status.lock().unwrap().telemetry_errors += errors;
            }
            Err(_) => job.status.lock().unwrap().telemetry_errors += 1,
        }
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
        || (sheet.writer_mode == WriterMode::Auto
            && matches!(
                sheet.source,
                Source::Query { .. } | Source::RowStream { .. }
            ))
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

pub(crate) fn default_column_style(col: &Column) -> Result<Style> {
    let empty_sheet = WorksheetSpec {
        name: "Sheet1".into(),
        source: Source::Objects { records: vec![] },
        columns: vec![],
        infer_columns: false,
        case_insensitive_fields: false,
        extra_fields: ExtraFieldPolicy::Ignore,
        report_header: None,
        total_rows: None,
        writer_mode: WriterMode::Buffered,
        body_style: None,
        table: None,
        overflow: OverflowSpec::default(),
        rules: vec![],
        cell_overrides: vec![],
        styles: BTreeMap::new(),
        layout: Layout::default(),
    };
    column_style(&empty_sheet, col, &BTreeMap::new())
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
            DataType::Datetime
                if !col
                    .timezone
                    .as_deref()
                    .is_some_and(|tz| tz.eq_ignore_ascii_case("preserveText")) =>
            {
                Some("yyyy-mm-dd hh:mm:ss".into())
            }
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
        match result {
            Ok(outcome) => {
                report.workbooks.push(WorkbookReport {
                    id: spec.id.clone(),
                    path: spec.output.path.clone(),
                    rows: outcome.rows,
                    sheets_completed: outcome.sheets_completed,
                    split_files: outcome.split_files,
                    bytes_base64: outcome.bytes_base64,
                    error: None,
                });
            }
            Err(err) => {
                let err = err.with_context(Some(spec.id.clone()), None, None, None);
                report.workbooks.push(WorkbookReport {
                    id: spec.id.clone(),
                    path: spec.output.path.clone(),
                    rows: 0,
                    sheets_completed: 0,
                    split_files: vec![],
                    bytes_base64: None,
                    error: Some(err.clone()),
                });
                if !request.options.continue_on_error || err.code == "CANCELLED" {
                    return Err(err);
                }
            }
        }
    }
    Ok(report)
}

struct BookOutcome {
    rows: u64,
    sheets_completed: u32,
    split_files: Vec<String>,
    bytes_base64: Option<String>,
}

fn apply_workbook_metadata(book: &mut Workbook, metadata: &Option<WorkbookMetadata>) {
    if let Some(meta) = metadata {
        let mut props = DocProperties::new();
        if let Some(v) = &meta.title {
            props = props.set_title(v);
        }
        if let Some(v) = &meta.author {
            props = props.set_author(v);
        }
        if let Some(v) = &meta.subject {
            props = props.set_subject(v);
        }
        if let Some(v) = &meta.company {
            props = props.set_company(v);
        }
        if let Some(v) = &meta.manager {
            props = props.set_manager(v);
        }
        if let Some(v) = &meta.category {
            props = props.set_category(v);
        }
        if let Some(v) = &meta.keywords {
            props = props.set_keywords(v);
        }
        if let Some(v) = &meta.comments {
            props = props.set_comment(v);
        }
        book.set_properties(&props);
    }
}

fn setup_worksheet_chrome(
    ws: &mut Worksheet,
    sheet: &WorksheetSpec,
    sheet_name: &str,
    styles: &BTreeMap<String, Style>,
    header_plan: &crate::header::HeaderPlan,
    header_format: &Format,
    base_col_styles: &[Style],
) -> Result<()> {
    ws.set_name(sheet_name)?;
    let l = &sheet.layout;
    let first_row = header_plan.table_row;
    let first_col = l.start_column - 1;
    if let Some(gl) = l.gridlines {
        ws.set_screen_gridlines(gl);
    }
    if let Some(zoom) = l.zoom {
        ws.set_zoom(zoom);
    }
    if let Some(tc) = &l.tab_color {
        ws.set_tab_color(color(tc)?);
    }
    if l.protect_sheet {
        ws.protect();
    }
    if let Some(print) = &l.print {
        if let Some(landscape) = print.landscape {
            if landscape {
                ws.set_landscape();
            } else {
                ws.set_portrait();
            }
        }
        if let Some(ps) = print.paper_size {
            ws.set_paper_size(ps);
        }
        if print.fit_to_width.is_some() || print.fit_to_height.is_some() {
            ws.set_print_fit_to_pages(
                print.fit_to_width.unwrap_or(1),
                print.fit_to_height.unwrap_or(1),
            );
        }
        if let Some([first, last]) = print.repeat_rows {
            if first == 0 || first > last {
                return Err(ExportError::invalid("Invalid print.repeatRows"));
            }
            ws.set_repeat_rows(first - 1, last - 1)?;
        }
        if let Some(hdr) = &print.header {
            ws.set_header(hdr);
        }
        if let Some(ftr) = &print.footer {
            ws.set_footer(ftr);
        }
        if let Some(m) = &print.margins {
            ws.set_margins(m.left, m.right, m.top, m.bottom, m.header, m.footer);
        }
    }
    for (i, col) in sheet.columns.iter().enumerate() {
        let c = first_col + i as u16;
        if let Some(width) = col.width {
            ws.set_column_width(c, width)?;
        }
        ws.set_column_format(c, &compile_style(&base_col_styles[i])?)?;
    }
    for &c in &l.hidden_columns {
        ws.set_column_hidden(c - 1)?;
    }
    for (&r, &h) in &l.row_heights {
        ws.set_row_height(r - 1, h)?;
    }
    for &r in &l.hidden_rows {
        ws.set_row_hidden(r - 1)?;
    }
    header_plan.write(ws)?;
    for (i, col) in sheet.columns.iter().enumerate() {
        let c = first_col + i as u16;
        if l.header {
            ws.write_string_with_format(
                first_row,
                c,
                col.header.as_deref().unwrap_or(&col.field),
                header_format,
            )?;
        }
    }
    let freeze = if sheet.report_header.is_some() && l.freeze_rows > 0 {
        first_row
            .checked_add(l.freeze_rows)
            .filter(|n| *n <= 1_048_576)
            .ok_or_else(|| ExportError::invalid("Freeze rows exceed worksheet limit"))?
    } else {
        l.freeze_rows
    };
    ws.set_freeze_panes(freeze, l.freeze_columns)?;
    for m in &l.merges {
        let f = m
            .style
            .as_ref()
            .map(|id| named(styles, id))
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
    Ok(())
}

fn finalize_sheet_footer(
    ws: &mut Worksheet,
    sheet: &WorksheetSpec,
    first_row: u32,
    first_col: u16,
    row_index: u32,
    header_format: &Format,
    sampled_widths: Option<&[f64]>,
) -> Result<()> {
    if let Some(widths) = sampled_widths {
        for (i, &w) in widths.iter().enumerate() {
            if sheet.columns[i].width.is_none() {
                ws.set_column_width(first_col + i as u16, w)?;
            }
        }
    } else if sheet.layout.autofit && !is_streaming(sheet) {
        ws.autofit();
    }
    if let Some(table) = &sheet.table {
        let columns: Vec<_> = sheet
            .columns
            .iter()
            .map(|c| {
                TableColumn::new()
                    .set_header(c.header.as_deref().unwrap_or(&c.field))
                    .set_header_format(header_format)
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
    } else if sheet.layout.autofilter && sheet.layout.header && row_index > first_row + 1 {
        ws.autofilter(
            first_row,
            first_col,
            row_index - 1,
            first_col + sheet.columns.len() as u16 - 1,
        )?;
    }
    Ok(())
}

fn publish_workbook_file(
    mut book: Workbook,
    path: &Path,
    spec: &WorkbookSpec,
    request: &ExportRequest,
    job: &ExportJob,
    capture_bytes: bool,
) -> Result<Option<String>> {
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
    check_cancel(job)?;
    job.status.lock().unwrap().phase = "finalizing".into();
    book.save(staged.path())?;
    if spec.output.sync_data {
        staged.as_file().sync_all()?;
    }
    let bytes_base64 = if capture_bytes {
        let meta = std::fs::metadata(staged.path())?;
        if meta.len() > request.options.max_output_bytes as u64 {
            return Err(ExportError::new(
                "LIMIT_EXCEEDED",
                "Generated workbook exceeds maxOutputBytes",
            ));
        }
        let raw = std::fs::read(staged.path())?;
        Some(crate::values::encode_base64(&raw))
    } else {
        None
    };
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
    Ok(bytes_base64)
}

fn write_book(
    request: &ExportRequest,
    spec: &WorkbookSpec,
    job: &ExportJob,
    runtime: &tokio::runtime::Handle,
) -> Result<BookOutcome> {
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

    let mut book = Workbook::new();
    if let Some(dir) = &request.options.temp_directory {
        book.set_tempdir(dir)?;
    }
    apply_workbook_metadata(&mut book, &spec.metadata);

    let mut total_rows_written = 0u64;
    let mut sheets_completed = 0u32;
    let mut split_files = Vec::new();
    let mut current_book_path = path.to_path_buf();

    for sheet in &spec.worksheets {
        check_cancel(job)?;
        let streaming = is_streaming(sheet);
        let mut styles = request.styles.clone();
        styles.extend(spec.styles.clone());
        styles.extend(sheet.styles.clone());
        let l = &sheet.layout;
        let header_plan = crate::header::plan(sheet, request.options.max_rows)?;
        let first_row = header_plan.table_row;
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
        let mut base_col_styles = Vec::with_capacity(sheet.columns.len());
        for col in &sheet.columns {
            base_col_styles.push(column_style(sheet, col, &styles)?);
        }

        {
            let ws = if streaming {
                book.add_worksheet_with_constant_memory()
            } else {
                book.add_worksheet()
            };
            setup_worksheet_chrome(
                ws,
                sheet,
                &sheet.name,
                &styles,
                &header_plan,
                &header_format,
                &base_col_styles,
            )?;
        }
        let mut current_ws_index = book.worksheets().len() - 1;

        let data_start = first_row + u32::from(l.header);
        let mut row_index = data_start;
        let mut sheet_records_written = 0u32;
        let mut split_count = 0u32;
        let mut style_cache: BTreeMap<String, Format> = BTreeMap::new();
        let sample_limit = if streaming && l.autofit {
            l.autofit_sample_rows.unwrap_or(0)
        } else {
            0
        };
        let mut sampled_widths: Vec<f64> = sheet
            .columns
            .iter()
            .map(|c| {
                let hdr_len = c.header.as_deref().unwrap_or(&c.field).chars().count();
                (hdr_len as f64 + 2.0).clamp(8.0, 60.0)
            })
            .collect();

        let mut write = |row: &Row| -> Result<()> {
            check_cancel(job)?;
            if count.is_some_and(|c| sheet_records_written >= c) {
                return Err(ExportError::new(
                    "ROW_COUNT_MISMATCH",
                    "Source produced more rows than totalRows",
                ));
            }
            check_row_schema_policies(row, sheet)?;

            if row_index >= request.options.max_rows {
                match sheet.overflow.policy {
                    OverflowPolicy::Error => {
                        return Err(ExportError::new(
                            "LIMIT_EXCEEDED",
                            "Worksheet row limit exceeded",
                        ));
                    }
                    OverflowPolicy::SplitWorksheet => {
                        split_count += 1;
                        if split_count > sheet.overflow.max_splits
                            || (sheets_completed + 1) as usize >= request.options.max_sheets
                        {
                            return Err(ExportError::new(
                                "LIMIT_EXCEEDED",
                                "Overflow worksheet split limit exceeded",
                            ));
                        }
                        let widths_opt = (sample_limit > 0).then_some(sampled_widths.as_slice());
                        {
                            let ws = book.worksheet_from_index(current_ws_index)?;
                            finalize_sheet_footer(
                                ws,
                                sheet,
                                first_row,
                                first_col,
                                row_index,
                                &header_format,
                                widths_opt,
                            )?;
                        }
                        sheets_completed += 1;
                        let next_name = format_split_sheet_name(sheet, split_count + 1)?;
                        {
                            let ws = if streaming {
                                book.add_worksheet_with_constant_memory()
                            } else {
                                book.add_worksheet()
                            };
                            setup_worksheet_chrome(
                                ws,
                                sheet,
                                &next_name,
                                &styles,
                                &header_plan,
                                &header_format,
                                &base_col_styles,
                            )?;
                        }
                        current_ws_index = book.worksheets().len() - 1;
                        row_index = data_start;
                    }
                    OverflowPolicy::SplitWorkbook => {
                        split_count += 1;
                        if split_count > sheet.overflow.max_splits {
                            return Err(ExportError::new(
                                "LIMIT_EXCEEDED",
                                "Overflow workbook split limit exceeded",
                            ));
                        }
                        let widths_opt = (sample_limit > 0).then_some(sampled_widths.as_slice());
                        {
                            let ws = book.worksheet_from_index(current_ws_index)?;
                            finalize_sheet_footer(
                                ws,
                                sheet,
                                first_row,
                                first_col,
                                row_index,
                                &header_format,
                                widths_opt,
                            )?;
                        }
                        sheets_completed += 1;
                        let mut finished_book = Workbook::new();
                        if let Some(dir) = &request.options.temp_directory {
                            finished_book.set_tempdir(dir)?;
                        }
                        apply_workbook_metadata(&mut finished_book, &spec.metadata);
                        std::mem::swap(&mut book, &mut finished_book);
                        let _ = publish_workbook_file(
                            finished_book,
                            &current_book_path,
                            spec,
                            request,
                            job,
                            false,
                        )?;
                        if split_count > 1 {
                            split_files.push(current_book_path.to_string_lossy().into_owned());
                        }
                        current_book_path =
                            format_split_workbook_path(path, sheet, split_count + 1)?;
                        {
                            let ws = if streaming {
                                book.add_worksheet_with_constant_memory()
                            } else {
                                book.add_worksheet()
                            };
                            setup_worksheet_chrome(
                                ws,
                                sheet,
                                &sheet.name,
                                &styles,
                                &header_plan,
                                &header_format,
                                &base_col_styles,
                            )?;
                        }
                        current_ws_index = book.worksheets().len() - 1;
                        row_index = data_start;
                    }
                }
            }

            let ws = book.worksheet_from_index(current_ws_index)?;
            if let Some(h) = l.row_height
                && !l.row_heights.contains_key(&(row_index + 1))
            {
                ws.set_row_height(row_index, h)?;
            }

            let data_row_1based = sheet_records_written + 1;
            let has_rules_or_overrides =
                !sheet.rules.is_empty() || !sheet.cell_overrides.is_empty();

            for (i, col) in sheet.columns.iter().enumerate() {
                let value = match extract_field_value(row, col, sheet.case_insensitive_fields) {
                    Some(v) if !v.is_null() => v,
                    Some(v) => col.default_value.as_ref().unwrap_or(v),
                    None if col.default_value.is_some() => col.default_value.as_ref().unwrap(),
                    None if col.missing_as_null => &Value::Null,
                    None => {
                        return Err(ExportError::new(
                            "SCHEMA_MISMATCH",
                            format!("Missing field {}", col.field),
                        )
                        .with_context(
                            Some(spec.id.clone()),
                            Some(sheet.name.clone()),
                            Some(row_index + 1),
                            Some(col.field.clone()),
                        ));
                    }
                };

                if sample_limit > 0 && sheet_records_written < sample_limit {
                    let s_len = value
                        .as_str()
                        .map(|s| s.chars().count())
                        .unwrap_or_else(|| value.to_string().len());
                    let candidate = (s_len as f64 + 2.0).clamp(8.0, 60.0);
                    if candidate > sampled_widths[i] {
                        sampled_widths[i] = candidate;
                    }
                }

                let custom_fmt = if has_rules_or_overrides {
                    resolve_cell_style_override(
                        sheet,
                        col,
                        &base_col_styles[i],
                        &styles,
                        row,
                        data_row_1based,
                        &mut style_cache,
                    )?
                } else {
                    None
                };

                crate::values::write_value_with_format(
                    ws,
                    row_index,
                    first_col + i as u16,
                    value,
                    col.data_type,
                    custom_fmt.as_ref(),
                    col,
                )
                .map_err(|mut error| {
                    error.message = format!(
                        "Field {} at worksheet row {}: {}",
                        col.field,
                        row_index + 1,
                        error.message
                    );
                    error.with_context(
                        Some(spec.id.clone()),
                        Some(sheet.name.clone()),
                        Some(row_index + 1),
                        Some(col.field.clone()),
                    )
                })?;
            }
            row_index += 1;
            sheet_records_written += 1;
            total_rows_written += 1;
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
                let mut session = runtime.block_on(async {
                    tokio::select! {
                        _ = job.cancel.cancelled() => Err(ExportError::new("CANCELLED", "Export cancelled")),
                        r = QueryService::stream(request.clone()) => r
                    }
                })?;
                if sheet.case_insensitive_fields {
                    let mut seen = BTreeSet::new();
                    for label in session.schema() {
                        if !seen.insert(label.to_lowercase()) {
                            return Err(ExportError::new(
                                "SCHEMA_MISMATCH",
                                "Query result contains case-colliding column labels",
                            ));
                        }
                    }
                }
                for col in &sheet.columns {
                    let root = col
                        .path
                        .as_ref()
                        .and_then(|p| p.first())
                        .unwrap_or(&col.field);
                    let present = if sheet.case_insensitive_fields {
                        session
                            .schema()
                            .iter()
                            .any(|s| s.eq_ignore_ascii_case(root))
                    } else {
                        session.schema().contains(root)
                    };
                    if !present {
                        return Err(ExportError::new(
                            "SCHEMA_MISMATCH",
                            format!("Missing selected field {}", col.field),
                        ));
                    }
                }
                loop {
                    let row = runtime.block_on(async {
                        tokio::select! {
                            _ = job.cancel.cancelled() => Err(ExportError::new("CANCELLED", "Export cancelled")),
                            row = session.next() => row
                        }
                    })?;
                    match row {
                        Some(row) => write(&row)?,
                        None => break,
                    }
                }
            }
            Source::RowStream { input_id } => {
                let mut rx = crate::input::InputService::take_receiver(input_id)?;
                loop {
                    let next_row = runtime.block_on(async {
                        tokio::select! {
                            _ = job.cancel.cancelled() => Err(ExportError::new("CANCELLED", "Export cancelled")),
                            item = rx.recv() => item.transpose()
                        }
                    })?;
                    match next_row {
                        Some(row) => write(&row)?,
                        None => break,
                    }
                }
            }
        }

        if count.is_some_and(|c| sheet_records_written != c) {
            return Err(ExportError::new(
                "ROW_COUNT_MISMATCH",
                "Source produced fewer rows than totalRows",
            ));
        }

        let widths_opt = (sample_limit > 0).then_some(sampled_widths.as_slice());
        {
            let ws = book.worksheet_from_index(current_ws_index)?;
            finalize_sheet_footer(
                ws,
                sheet,
                first_row,
                first_col,
                row_index,
                &header_format,
                widths_opt,
            )?;
        }
        sheets_completed += 1;
    }

    let bytes_base64 = publish_workbook_file(
        book,
        &current_book_path,
        spec,
        request,
        job,
        spec.output.return_bytes,
    )?;
    if current_book_path != path {
        split_files.push(current_book_path.to_string_lossy().into_owned());
    }

    Ok(BookOutcome {
        rows: total_rows_written,
        sheets_completed,
        split_files,
        bytes_base64,
    })
}

fn resolve_cell_style_override(
    sheet: &WorksheetSpec,
    col: &Column,
    base_style: &Style,
    styles: &BTreeMap<String, Style>,
    row: &Row,
    data_row_1based: u32,
    cache: &mut BTreeMap<String, Format>,
) -> Result<Option<Format>> {
    let mut current = base_style.clone();
    let mut modified = false;
    for rule in &sheet.rules {
        if let Some(target_field) = &rule.field
            && target_field != &col.field
        {
            continue;
        }
        if evaluate_condition(
            &rule.when,
            row,
            data_row_1based,
            sheet.case_insensitive_fields,
        ) {
            if let Some(id) = &rule.style {
                current = current.overlay(named(styles, id)?);
                modified = true;
            }
            if let Some(fmt) = &rule.format {
                current = current.overlay(fmt);
                modified = true;
            }
        }
    }
    for ov in &sheet.cell_overrides {
        if ov.row == data_row_1based && ov.field == col.field {
            if let Some(id) = &ov.style {
                current = current.overlay(named(styles, id)?);
                modified = true;
            }
            if let Some(fmt) = &ov.format {
                current = current.overlay(fmt);
                modified = true;
            }
        }
    }
    if !modified || &current == base_style {
        return Ok(None);
    }
    let key = serde_json::to_string(&current).expect("style serialization");
    if let Some(cached) = cache.get(&key) {
        return Ok(Some(cached.clone()));
    }
    let compiled = compile_style(&current)?;
    cache.insert(key, compiled.clone());
    Ok(Some(compiled))
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

fn parse_border(v: &str) -> Result<FormatBorder> {
    match v {
        "thin" => Ok(FormatBorder::Thin),
        "medium" => Ok(FormatBorder::Medium),
        "thick" => Ok(FormatBorder::Thick),
        "double" => Ok(FormatBorder::Double),
        "dashed" => Ok(FormatBorder::Dashed),
        "dotted" => Ok(FormatBorder::Dotted),
        _ => Err(ExportError::invalid("Invalid border")),
    }
}

pub(crate) fn compile_style(s: &Style) -> Result<Format> {
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
    if let Some(v) = &s.foreground_color {
        f = f.set_foreground_color(color(v)?);
    }
    if let Some(p) = &s.fill_pattern {
        f = f.set_pattern(match p.as_str() {
            "solid" => FormatPattern::Solid,
            "none" => FormatPattern::None,
            "gray125" => FormatPattern::Gray125,
            "gray0625" => FormatPattern::Gray0625,
            "lightGray" => FormatPattern::LightGray,
            "mediumGray" => FormatPattern::MediumGray,
            "darkGray" => FormatPattern::DarkGray,
            _ => return Err(ExportError::invalid("Invalid fillPattern")),
        });
    }
    if let Some(v) = &s.border {
        f = f.set_border(parse_border(v)?);
    }
    if let Some(v) = &s.border_color {
        f = f.set_border_color(color(v)?);
    }
    if let Some(v) = &s.border_left {
        f = f.set_border_left(parse_border(v)?);
    }
    if let Some(v) = &s.border_left_color {
        f = f.set_border_left_color(color(v)?);
    }
    if let Some(v) = &s.border_right {
        f = f.set_border_right(parse_border(v)?);
    }
    if let Some(v) = &s.border_right_color {
        f = f.set_border_right_color(color(v)?);
    }
    if let Some(v) = &s.border_top {
        f = f.set_border_top(parse_border(v)?);
    }
    if let Some(v) = &s.border_top_color {
        f = f.set_border_top_color(color(v)?);
    }
    if let Some(v) = &s.border_bottom {
        f = f.set_border_bottom(parse_border(v)?);
    }
    if let Some(v) = &s.border_bottom_color {
        f = f.set_border_bottom_color(color(v)?);
    }
    if let Some(v) = &s.border_diagonal {
        f = f.set_border_diagonal(parse_border(v)?);
    }
    if let Some(v) = &s.border_diagonal_color {
        f = f.set_border_diagonal_color(color(v)?);
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
    if s.shrink_to_fit == Some(true) {
        f = f.set_shrink();
    }
    if let Some(indent) = s.indent {
        f = f.set_indent(indent);
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
    if s.hidden == Some(true) {
        f = f.set_hidden();
    }
    Ok(f)
}
