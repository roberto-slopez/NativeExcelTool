use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt};

pub type Result<T> = std::result::Result<T, ExportError>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workbook_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worksheet_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    pub code: String,
    pub message: String,
    #[serde(flatten, default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Box<ErrorContext>>,
}
impl ExportError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            context: None,
        }
    }
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_CONFIG", message)
    }
    pub fn with_context(
        mut self,
        workbook_id: Option<String>,
        worksheet_name: Option<String>,
        row_index: Option<u32>,
        field: Option<String>,
    ) -> Self {
        let ctx = self.context.get_or_insert_with(Default::default);
        if ctx.workbook_id.is_none() {
            ctx.workbook_id = workbook_id;
        }
        if ctx.worksheet_name.is_none() {
            ctx.worksheet_name = worksheet_name;
        }
        if ctx.row_index.is_none() {
            ctx.row_index = row_index;
        }
        if ctx.field.is_none() {
            ctx.field = field;
        }
        self
    }
}
impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ExportError {}
impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self::new("IO_ERROR", e.to_string())
    }
}
impl From<rust_xlsxwriter::XlsxError> for ExportError {
    fn from(e: rust_xlsxwriter::XlsxError) -> Self {
        Self::new("XLSX_ERROR", e.to_string())
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportRequest {
    #[serde(default = "version")]
    pub schema_version: u32,
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
    #[serde(default)]
    pub defaults: Option<ExportDefaults>,
    #[serde(default)]
    pub options: ExportOptions,
    pub workbooks: Vec<WorkbookSpec>,
}
fn version() -> u32 {
    1
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportDefaults {
    pub writer_mode: Option<WriterMode>,
    pub body_style: Option<String>,
    pub header_style: Option<String>,
    pub row_height: Option<f64>,
    pub autofilter: Option<bool>,
    pub freeze_rows: Option<u32>,
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbookMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub company: Option<String>,
    pub manager: Option<String>,
    pub category: Option<String>,
    pub keywords: Option<String>,
    pub comments: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbookSpec {
    pub id: String,
    pub output: Output,
    #[serde(default)]
    pub metadata: Option<WorkbookMetadata>,
    #[serde(default)]
    pub defaults: Option<ExportDefaults>,
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
    pub worksheets: Vec<WorksheetSpec>,
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Output {
    pub path: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub create_parents: bool,
    #[serde(default)]
    pub return_bytes: bool,
    #[serde(default)]
    pub sync_data: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OverflowPolicy {
    #[default]
    Error,
    SplitWorksheet,
    SplitWorkbook,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct OverflowSpec {
    pub policy: OverflowPolicy,
    pub name_pattern: Option<String>,
    pub max_splits: u32,
}
impl Default for OverflowSpec {
    fn default() -> Self {
        Self {
            policy: OverflowPolicy::Error,
            name_pattern: None,
            max_splits: 256,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExtraFieldPolicy {
    #[default]
    Ignore,
    Error,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RowParity {
    Odd,
    Even,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ComparisonOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    IsNull,
    NotNull,
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleCondition {
    pub row_parity: Option<RowParity>,
    pub field: Option<String>,
    pub op: Option<ComparisonOp>,
    pub value: Option<Value>,
    pub all: Option<Vec<RuleCondition>>,
    pub any: Option<Vec<RuleCondition>>,
    pub not: Option<Box<RuleCondition>>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleRule {
    pub when: RuleCondition,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub format: Option<Style>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellStyleOverride {
    /// 1-based data record index (1 = first data row).
    pub row: u32,
    /// Column `field` name.
    pub field: String,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub format: Option<Style>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksheetSpec {
    pub name: String,
    pub source: Source,
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub infer_columns: bool,
    #[serde(default)]
    pub case_insensitive_fields: bool,
    #[serde(default)]
    pub extra_fields: ExtraFieldPolicy,
    #[serde(default)]
    pub report_header: Option<ReportHeader>,
    #[serde(default)]
    pub total_rows: Option<u32>,
    #[serde(default)]
    pub writer_mode: WriterMode,
    #[serde(default)]
    pub body_style: Option<String>,
    #[serde(default)]
    pub table: Option<TableSpec>,
    #[serde(default)]
    pub overflow: OverflowSpec,
    #[serde(default)]
    pub rules: Vec<StyleRule>,
    #[serde(default)]
    pub cell_overrides: Vec<CellStyleOverride>,
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
    #[serde(default)]
    pub layout: Layout,
}
#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportHeader {
    pub title: Option<ReportTitle>,
    pub column_count: Option<u16>,
    pub columns: Vec<HeaderColumn>,
    pub label_style: Style,
    pub value_style: Style,
    pub label_height: Option<f64>,
    pub value_height: Option<f64>,
    pub gap_rows: u32,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportTitle {
    pub text: String,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub height: Option<f64>,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderColumn {
    pub fields: Vec<HeaderField>,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderField {
    pub label: String,
    pub value: Value,
    #[serde(default)]
    pub data_type: DataType,
    #[serde(default)]
    pub number_format: Option<String>,
    #[serde(default)]
    pub label_style: Style,
    #[serde(default)]
    pub value_style: Style,
}
#[derive(Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WriterMode {
    #[default]
    Auto,
    Buffered,
    Streaming,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableSpec {
    pub name: String,
    #[serde(default = "default_table_cells")]
    pub max_cells: u64,
}
fn default_table_cells() -> u64 {
    1_000_000
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DataType {
    #[default]
    Auto,
    Text,
    Integer,
    Number,
    Decimal,
    Boolean,
    Date,
    Datetime,
    Time,
    Formula,
    Binary,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PrecisionPolicy {
    #[default]
    ExactText,
    Numeric,
    Round,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BinaryEncoding {
    Hex,
    Base64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ColumnTransform {
    Trim,
    Uppercase,
    Lowercase,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ColumnFormat {
    Excel(String),
    Style(Box<Style>),
}
impl ColumnFormat {
    pub fn as_style(&self) -> Style {
        match self {
            Self::Excel(value) => Style {
                number_format: Some(value.clone()),
                ..Default::default()
            },
            Self::Style(value) => (**value).clone(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Source {
    Objects {
        records: Vec<BTreeMap<String, Value>>,
    },
    Query {
        #[serde(flatten)]
        request: QueryRequest,
    },
    #[serde(rename_all = "camelCase")]
    RowStream { input_id: String },
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryRequest {
    pub dbms: Dbms,
    pub connection_string: String,
    pub query: String,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    #[serde(default)]
    pub options: QueryOptions,
}
// Credentials deliberately do not implement Debug.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Dbms {
    Postgresql,
    Sqlserver,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Parameter {
    Text(Option<String>),
    Int32(Option<i32>),
    Int64(
        #[serde(deserialize_with = "deserialize_int64")]
        #[schemars(with = "Option<Int64Input>")]
        Option<i64>,
    ),
    Float64(Option<f64>),
    Boolean(Option<bool>),
}
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum Int64Input {
    Integer(i64),
    Text(String),
}
fn deserialize_int64<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<i64>, D::Error> {
    let value = Option::<Int64Input>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(Int64Input::Integer(v)) => Ok(Some(v)),
        Some(Int64Input::Text(v)) => v.parse().map(Some).map_err(serde::de::Error::custom),
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryOptions {
    pub fetch_rows: u32,
    pub connect_timeout_ms: u64,
    pub query_timeout_ms: u64,
    pub queue_rows: usize,
    pub max_row_bytes: usize,
    pub allow_plaintext: bool,
}
impl Default for QueryOptions {
    fn default() -> Self {
        Self {
            fetch_rows: 512,
            connect_timeout_ms: 15_000,
            query_timeout_ms: 300_000,
            queue_rows: 16,
            max_row_bytes: 1024 * 1024,
            allow_plaintext: false,
        }
    }
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportOptions {
    pub telemetry: crate::TelemetryOptions,
    pub progress_interval_ms: u64,
    pub export_timeout_ms: Option<u64>,
    pub temp_directory: Option<String>,
    pub max_styles: usize,
    pub max_workbooks: usize,
    pub max_sheets: usize,
    pub max_rows: u32,
    pub max_output_bytes: usize,
    pub compression: u8,
    pub continue_on_error: bool,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            telemetry: Default::default(),
            progress_interval_ms: 250,
            export_timeout_ms: None,
            temp_directory: None,
            max_styles: 10_000,
            max_workbooks: 32,
            max_sheets: 256,
            max_rows: 1_048_576,
            max_output_bytes: 64 * 1024 * 1024,
            compression: 6,
            continue_on_error: false,
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Column {
    pub field: String,
    #[serde(default)]
    pub path: Option<Vec<String>>,
    #[serde(default)]
    pub data_type: DataType,
    #[serde(default)]
    pub number_format: Option<String>,
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub format: Option<ColumnFormat>,
    #[serde(default)]
    pub missing_as_null: bool,
    #[serde(default)]
    pub default_value: Option<Value>,
    #[serde(default)]
    pub null_placeholder: Option<String>,
    #[serde(default)]
    pub transforms: Vec<ColumnTransform>,
    #[serde(default)]
    pub precision_policy: PrecisionPolicy,
    #[serde(default)]
    pub round_scale: Option<u32>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub binary_encoding: Option<BinaryEncoding>,
}
impl Column {
    pub fn new(field: impl Into<String>, data_type: DataType) -> Self {
        Self {
            field: field.into(),
            data_type,
            ..Default::default()
        }
    }
    pub fn with_number_format(mut self, format: impl Into<String>) -> Self {
        self.number_format = Some(format.into());
        self
    }
}
impl From<Style> for ColumnFormat {
    fn from(style: Style) -> Self {
        Self::Style(Box::new(style))
    }
}
impl From<String> for ColumnFormat {
    fn from(format: String) -> Self {
        Self::Excel(format)
    }
}
impl From<&str> for ColumnFormat {
    fn from(format: &str) -> Self {
        Self::Excel(format.into())
    }
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PageMargins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub header: f64,
    pub footer: f64,
}

#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct PrintOptions {
    pub landscape: Option<bool>,
    pub paper_size: Option<u8>,
    pub fit_to_width: Option<u16>,
    pub fit_to_height: Option<u16>,
    pub repeat_rows: Option<[u32; 2]>,
    pub header: Option<String>,
    pub footer: Option<String>,
    pub margins: Option<PageMargins>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Layout {
    pub header: bool,
    pub header_style: Option<String>,
    pub start_row: u32,
    pub start_column: u16,
    pub freeze_rows: u32,
    pub freeze_columns: u16,
    pub autofilter: bool,
    pub autofit: bool,
    pub autofit_sample_rows: Option<u32>,
    pub row_height: Option<f64>,
    pub row_heights: BTreeMap<u32, f64>,
    pub hidden_columns: Vec<u16>,
    pub hidden_rows: Vec<u32>,
    pub gridlines: Option<bool>,
    pub zoom: Option<u16>,
    pub tab_color: Option<String>,
    pub protect_sheet: bool,
    pub print: Option<PrintOptions>,
    pub merges: Vec<Merge>,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            header: true,
            header_style: None,
            start_row: 1,
            start_column: 1,
            freeze_rows: 1,
            freeze_columns: 0,
            autofilter: false,
            autofit: false,
            autofit_sample_rows: None,
            row_height: None,
            row_heights: BTreeMap::new(),
            hidden_columns: vec![],
            hidden_rows: vec![],
            gridlines: None,
            zoom: None,
            tab_color: None,
            protect_sheet: false,
            print: None,
            merges: vec![],
        }
    }
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Merge {
    pub first_row: u32,
    pub first_column: u16,
    pub last_row: u32,
    pub last_column: u16,
    pub text: String,
    #[serde(default)]
    pub style: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikeout: Option<bool>,
    pub font_color: Option<String>,
    pub background: Option<String>,
    pub foreground_color: Option<String>,
    pub fill_pattern: Option<String>,
    pub border: Option<String>,
    pub border_color: Option<String>,
    pub border_left: Option<String>,
    pub border_left_color: Option<String>,
    pub border_right: Option<String>,
    pub border_right_color: Option<String>,
    pub border_top: Option<String>,
    pub border_top_color: Option<String>,
    pub border_bottom: Option<String>,
    pub border_bottom_color: Option<String>,
    pub border_diagonal: Option<String>,
    pub border_diagonal_color: Option<String>,
    pub horizontal: Option<String>,
    pub vertical: Option<String>,
    pub wrap: Option<bool>,
    pub shrink_to_fit: Option<bool>,
    pub indent: Option<u8>,
    pub rotation: Option<i16>,
    pub number_format: Option<String>,
    pub locked: Option<bool>,
    pub hidden: Option<bool>,
    pub reset_fields: Vec<String>,
}
impl Style {
    pub fn overlay(&self, other: &Self) -> Self {
        let mut base = serde_json::to_value(self).expect("style serialization");
        if let Some(obj) = base.as_object_mut() {
            for reset in &other.reset_fields {
                if reset != "resetFields" && obj.contains_key(reset) {
                    obj.insert(reset.clone(), Value::Null);
                }
            }
        }
        for (k, v) in serde_json::to_value(other)
            .expect("style serialization")
            .as_object()
            .expect("style object")
        {
            if k == "resetFields" {
                continue;
            }
            if !v.is_null() {
                base[k] = v.clone();
            }
        }
        base["resetFields"] = Value::Array(vec![]);
        serde_json::from_value(base).expect("style deserialization")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub workbooks: Vec<WorkbookReport>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookReport {
    pub id: String,
    pub path: String,
    pub rows: u64,
    #[serde(default)]
    pub sheets_completed: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub split_files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_base64: Option<String>,
    pub error: Option<ExportError>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub phase: String,
    pub rows_written: u64,
    pub files_published: u64,
    pub total_rows: Option<u64>,
    pub telemetry_errors: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub abi_version: u32,
    pub schema_version: u32,
    pub dbms: Vec<String>,
    pub writer_modes: Vec<String>,
    pub source_kinds: Vec<String>,
    pub overflow_policies: Vec<String>,
    pub streaming_tables: bool,
    pub max_rows: u32,
    pub max_columns: u16,
}

pub type Row = BTreeMap<String, Value>;

pub fn parse_request(json: &str) -> Result<ExportRequest> {
    serde_json::from_str(json)
        .map_err(|_| ExportError::invalid("Invalid export JSON; check schema and unknown fields"))
}
pub fn request_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(ExportRequest)).expect("schema serialization")
}
