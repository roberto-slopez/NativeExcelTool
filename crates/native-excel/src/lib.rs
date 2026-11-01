//! Typed Excel export and incremental database query APIs.
mod builder;
pub mod cli;
mod header;
mod input;
mod model;
mod query;
mod telemetry;
mod values;
mod writer;

pub use builder::{
    AppendWorkbookWriter, BorderSpec, FillSpec, FontSpec, SheetBuilder, WorkbookBuilder,
    column_index_to_name, column_name_to_index, parse_cell_ref,
};
pub use input::{InputOptions, InputService, InputSession};
pub use model::*;
pub use query::{PostgresDriver, QueryDriver, QueryService, QuerySession, SqlServerDriver};
pub use telemetry::{TelemetryEvent, TelemetryOptions, TelemetrySink};
pub use writer::{ExcelExporter, ExportJob};
