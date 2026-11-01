use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt};

pub type Result<T> = std::result::Result<T, ExportError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    pub code: String,
    pub message: String,
}
impl ExportError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_CONFIG", message)
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
    pub options: ExportOptions,
    pub workbooks: Vec<WorkbookSpec>,
}
fn version() -> u32 {
    1
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbookSpec {
    pub id: String,
    pub output: Output,
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
    pub worksheets: Vec<WorksheetSpec>,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Output {
    pub path: String,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub create_parents: bool,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorksheetSpec {
    pub name: String,
    pub source: Source,
    pub columns: Vec<Column>,
    #[serde(default)]
    pub total_rows: Option<u32>,
    #[serde(default)]
    pub writer_mode: WriterMode,
    #[serde(default)]
    pub body_style: Option<String>,
    #[serde(default)]
    pub table: Option<TableSpec>,
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
    #[serde(default)]
    pub layout: Layout,
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
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ColumnFormat {
    Excel(String),
    Style(Style),
}
impl ColumnFormat {
    pub fn as_style(&self) -> Style {
        match self {
            Self::Excel(value) => Style {
                number_format: Some(value.clone()),
                ..Default::default()
            },
            Self::Style(value) => value.clone(),
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
    pub temp_directory: Option<String>,
    pub max_styles: usize,
    pub max_workbooks: usize,
    pub max_sheets: usize,
    pub max_rows: u32,
    pub compression: u8,
    pub continue_on_error: bool,
}
impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            temp_directory: None,
            max_styles: 10_000,
            max_workbooks: 32,
            max_sheets: 256,
            max_rows: 1_048_576,
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
        Self::Style(style)
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
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Layout {
    pub header: bool,
    pub header_style: Option<String>,
    pub start_row: u32,
    pub start_column: u16,
    pub freeze_rows: u32,
    pub autofilter: bool,
    pub row_height: Option<f64>,
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
            autofilter: false,
            row_height: None,
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
#[derive(Clone, Default, Serialize, Deserialize, JsonSchema)]
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
    pub border: Option<String>,
    pub border_color: Option<String>,
    pub horizontal: Option<String>,
    pub vertical: Option<String>,
    pub wrap: Option<bool>,
    pub rotation: Option<i16>,
    pub number_format: Option<String>,
    pub locked: Option<bool>,
}
impl Style {
    pub fn overlay(&self, other: &Self) -> Self {
        let mut base = serde_json::to_value(self).expect("style serialization");
        for (k, v) in serde_json::to_value(other)
            .expect("style serialization")
            .as_object()
            .expect("style object")
        {
            if !v.is_null() {
                base[k] = v.clone();
            }
        }
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
    pub error: Option<ExportError>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub phase: String,
    pub rows_written: u64,
    pub files_published: u64,
    pub total_rows: Option<u64>,
}
pub type Row = BTreeMap<String, Value>;

pub fn parse_request(json: &str) -> Result<ExportRequest> {
    serde_json::from_str(json)
        .map_err(|_| ExportError::invalid("Invalid export JSON; check schema and unknown fields"))
}
pub fn request_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(ExportRequest)).expect("schema serialization")
}
