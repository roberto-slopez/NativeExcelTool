# Implemented API reference

The generated JSON schema (`schemas/export-request.schema.json`) is authoritative for accepted keys. Call `cargo xtask schema` to regenerate it. All coordinates exposed in configuration are one-based. Returned database row maps use exact column labels unless `caseInsensitiveFields` is enabled. Unknown configuration properties fail. No configuration files or credential environment variables are read implicitly.

## Export request

Export options accept `progressIntervalMs` (250 by default), `exportTimeoutMs` (optional overall job deadline in ms), `maxOutputBytes` (64 MiB default for `returnBytes: true`), and `telemetry: { logPath?, metricsPath?, sampleIntervalMs? }`. Telemetry is disabled when no destination/custom sink is supplied. File paths must be absolute and have existing parent directories. Logs are JSONL; metrics are Prometheus textfile snapshots. See [telemetry and progress](guides/telemetry.md) for CLI flags, redaction, failure behavior, and custom sink integration. `JobStatus` exposes `telemetryErrors`; Rust jobs provide `subscribe_progress()` and Node/C ABI jobs expose event polling (`nextEvent()` / `events()` / `ne_job_next_event_json`).

| Field | Default | Behavior |
| --- | --- | --- |
| `schemaVersion` | 1 | Only version 1 is accepted |
| `styles` | Empty map | Request-scoped named custom styles |
| `defaults` | None | Request-scoped defaults (`writerMode`, `bodyStyle`, `headerStyle`, `rowHeight`, `autofilter`, `freezeRows`) |
| `workbooks` | Required | Nonempty array of independent workbook specifications |
| `options.tempDirectory` | OS temporary directory | Temporary worksheet spool directory |
| `options.exportTimeoutMs` | None | Optional overall export deadline in milliseconds |
| `options.maxStyles` | 10000 | Per-sheet named/column/rule/override style validation budget |
| `options.maxWorkbooks` | 32 | Maximum requested files |
| `options.maxSheets` | 256 | Maximum sheets per workbook |
| `options.maxRows` | 1048576 | Inclusive row capacity per worksheet, including header/offset rows |
| `options.maxOutputBytes` | 67108864 | Maximum bytes returned when `output.returnBytes` is true |
| `options.compression` | 6 | Only level 6 currently supported; other values fail |
| `options.continueOnError` | false | Continue independent workbooks after failure and return per-book errors |

A workbook requires unique `id`, `output`, and nonempty `worksheets`. It may supply `metadata` (`title`, `author`, `subject`, `company`, `manager`, `category`, `keywords`, `comments`), `defaults`, and a `styles` map. Output has required `.xlsx` `path`, `overwrite` (false), `createParents` (false), `returnBytes` (false, returning base64 bytes in `WorkbookReport.bytesBase64` bounded by `maxOutputBytes`), and `syncData` (false, calling `sync_all()` before atomic publication).

A worksheet requires a valid unique `name`, `source`, and either nonempty `columns` or `inferColumns: true` (for non-empty object arrays). Styles resolve from worksheet scope, then workbook scope, then request scope. `bodyStyle` provides a uniform default for every report column. A column's named `style` overlays that body style, and its inline `format` overlays the result; explicit `false` values replace inherited `true` values, and `resetFields` clears specified inherited style properties. `numberFormat` has highest display-format precedence.

Worksheet options include:
- `inferColumns`: derive column definitions from the first object record when `columns` is empty.
- `caseInsensitiveFields`: match record keys or SQL labels case-insensitively while rejecting case-colliding keys.
- `extraFields`: `"ignore"` (default) or `"error"` to reject undeclared record fields.
- `totalRows`: optional exact number of data records, excluding headers.
- `writerMode`: `"auto"`, `"buffered"`, or `"streaming"`.
- `table`: `{ "name": "ReportData", "maxCells": 1000000 }` (buffered mode only).
- `overflow`: `{ "policy": "error" | "splitWorksheet" | "splitWorkbook", "namePattern"?: "Sheet_{index}", "maxSplits"?: 256 }`.
- `rules`: declarative `StyleRule[]` (up to 128 rules, expression depth up to 16) matching `rowParity` (`"odd" | "even"`), field comparisons (`eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `isNull`, `notNull`), and boolean combinators (`all`, `any`, `not`), evaluated before each row flushes in both buffered and streaming modes.
- `cellOverrides`: explicit 1-based data-row and column-field style overrides (`{ row, field, style?, format? }`).

## Sources and incremental caller input

Worksheets accept three source kinds:
1. `objects`: `{ "kind": "objects", "records": [...] }`.
2. `query`: `{ "kind": "query", "dbms": "postgresql" | "sqlserver", "connectionString": "...", "query": "...", "parameters"?: [...], "options"?: {...} }`.
3. `rowStream`: `{ "kind": "rowStream", "inputId": "..." }`, consuming bounded batches submitted through `InputSession::open` (Rust), `excel.openInputStream` (Node), or `ne_input_open_json` / `SafeInputHandle` (C ABI / C#).

## Columns and values

Worksheets additionally accept optional `reportHeader` with a merged `title`, logical `columnCount`, and dynamic `columns[].fields[]` label/value pairs. See [report headers](guides/report-headers.md) and [typed columns](guides/typed-columns.md).

| Column field | Default | Behavior |
| --- | --- | --- |
| `field` | Required | Exact record key/result label (dotted keys are literal unless `path` is set) |
| `path` | None | Explicit nested object path segments, e.g. `["customer", "name"]` |
| `dataType` | `auto` | `auto`, `text`, `integer`, `number`, `decimal`, `boolean`, `date`, `datetime`, `time`, `formula`, `binary` |
| `numberFormat` | Type default | Excel invariant display format string; overrides other display formats |
| `header` | Field name | Displayed header text |
| `width` | Writer default | Excel column width units |
| `style` | None | Scoped named body style |
| `format` | None | Excel display format string, or inline `Style` object |
| `missingAsNull` | false | Missing object fields become blank instead of failing |
| `defaultValue` | None | Fallback value when field is missing or null |
| `nullPlaceholder` | None | Literal text written when the effective cell value is null |
| `transforms` | Empty | Ordered string transformations: `trim`, `uppercase`, `lowercase` |
| `precisionPolicy` | `exactText` | `exactText`, `numeric`, or `round` (with `roundScale`, default 2) for `integer`/`decimal` |
| `timezone` | `utc` | `utc`, `preserveText`, or fixed offset `+HH:MM` / `-HH:MM` for `datetime` columns |
| `binaryEncoding` | None | Required for `binary` columns: `hex` or `base64` |

## Styles

All style properties are optional. Colors must be six-digit RGB with optional `#` prefix.

| Property | Values |
| --- | --- |
| `fontName`, `fontSize` | Font name; size between 1 and 409 |
| `bold`, `italic`, `underline`, `strikeout` | Boolean |
| `fontColor`, `background`, `foregroundColor` | RGB text |
| `fillPattern` | `solid`, `none`, `gray125`, `gray0625`, `lightGray`, `mediumGray`, `darkGray` |
| `border`, `borderColor` | Uniform all-side border (`thin`, `medium`, `thick`, `double`, `dashed`, `dotted`) and RGB color |
| `borderLeft`, `borderRight`, `borderTop`, `borderBottom`, `borderDiagonal` | Per-edge border style |
| `borderLeftColor`, `borderRightColor`, `borderTopColor`, `borderBottomColor`, `borderDiagonalColor` | Per-edge RGB border color |
| `horizontal` | `left`, `center`, `right` |
| `vertical` | `top`, `center`, `bottom` |
| `wrap`, `shrinkToFit` | Boolean |
| `indent` | Unsigned integer indent level |
| `rotation` | Integer from -90 to 90 |
| `numberFormat` | Excel invariant custom format string |
| `locked`, `hidden` | Protection flags (`protectSheet: true` on `layout` activates worksheet protection) |
| `resetFields` | Array of style property names to clear during style inheritance overlay |

## Layout

| Field | Default | Behavior |
| --- | --- | --- |
| `header` | true | Emit column labels |
| `headerStyle` | None | Named format for all header cells |
| `startRow`, `startColumn` | 1, 1 | Report-header origin when `reportHeader` exists; otherwise table origin |
| `freezeRows`, `freezeColumns` | 1, 0 | Frozen top rows and left columns |
| `autofilter` | false | Filter the header/data rectangle when nonempty |
| `autofit`, `autofitSampleRows` | false, None | Exact autofit in buffered mode; bounded sampled autofit (`1..=10000` rows) in streaming mode |
| `rowHeight`, `rowHeights` | Default, Empty | Default data row height and 1-based per-row height map |
| `hiddenColumns`, `hiddenRows` | Empty | 1-based hidden column and row indices |
| `gridlines` | None | Show or hide worksheet screen gridlines |
| `zoom` | None | Worksheet zoom percentage (`10..=400`) |
| `tabColor` | None | Worksheet tab RGB color |
| `protectSheet` | false | Enable worksheet protection |
| `print` | None | Print setup (`landscape`, `paperSize`, `fitToWidth`, `fitToHeight`, `repeatRows`, `header`, `footer`, `margins`) |
| `merges` | Empty | Buffered manual merges preceding the table origin |

## Programmatic Rust Builders

In addition to `ExcelExporter` and `QueryService`, the `native-excel` crate provides:
- `WorkbookBuilder` and `SheetBuilder`: buffered random-access builders matching `oldexcel` (`create_sheet`, `set`, `set_typed`, `set_a1`, `set_formula`, `merge`, `width`, `height`, `font`, `fill`, `border`, `align`, `valign`, `rotate`, `wrap`, `number_format`, `save`, `save_to_bytes`, `cancel`), plus `column_index_to_name`, `column_name_to_index`, and `parse_cell_ref` validated through `XFD` (`16384`).
- `AppendWorkbookWriter`: streaming append-only row writer (`new`, `write_row`, `write_batch`, `finish`).
- `InputSession` and `InputService`: bounded incremental caller-input streams for `Source::RowStream`.
- `ExcelExporter::capabilities()`, `ExcelExporter::export_objects(...)`, and `ExcelExporter::export_query(...)`.

## Native ABI

See `bindings/c/native_excel.h` and `bindings/dotnet/NativeExcelNative.cs` for exact signatures and `SafeHandle` wrappers (`SafeJobHandle`, `SafeQueryHandle`, `SafeInputHandle`). Exported functions include `ne_abi_version`, `ne_buffer_free`, `ne_capabilities_json`, `ne_schema_json`, `ne_validate_json`, `ne_export_start_json`, `ne_job_status_json`, `ne_job_next_event_json`, `ne_job_result_json`, `ne_job_cancel`, `ne_job_release`, `ne_query_open_json`, `ne_query_schema_json`, `ne_query_next_json`, `ne_query_cancel`, `ne_query_release`, `ne_input_open_json`, `ne_input_write_batch_json`, `ne_input_finish`, and `ne_input_release`.
