# Implemented API reference

The generated JSON schema is authoritative for accepted keys. Call `cargo xtask schema` to regenerate it. All coordinates exposed in configuration are one-based. Returned database row maps use exact column labels. Unknown configuration properties fail. No configuration files or credential environment variables are read implicitly.

## Export request

| Field | Default | Behavior |
| --- | --- | --- |
| `schemaVersion` | 1 | Only version 1 is accepted |
| `styles` | Empty map | Request-scoped named custom styles |
| `workbooks` | Required | Nonempty array of independent workbook specifications |
| `options.tempDirectory` | OS temporary directory | Temporary worksheet spool directory |
| `options.maxStyles` | 10000 | Per-sheet named/column-style validation budget |
| `options.maxWorkbooks` | 32 | Maximum requested files |
| `options.maxSheets` | 256 | Maximum sheets per workbook |
| `options.maxRows` | 1048576 | Inclusive row capacity, including header/offset rows |
| `options.compression` | 6 | Only level 6 currently supported; other values fail |
| `options.continueOnError` | false | Continue independent workbooks after failure and return per-book errors |

A workbook requires unique `id`, `output`, and nonempty `worksheets`. It may supply a `styles` map. Output has required `.xlsx` `path`, `overwrite` (false), and `createParents` (false). Paths are caller-controlled filesystem destinations. Do not pass untrusted paths without application validation. Workbooks execute sequentially. Completed earlier files remain published if a later file fails. Fail-fast mode returns the error; job counters retain the number of earlier publications.

A worksheet requires a valid unique `name`, `source`, and nonempty `columns`. Styles resolve from worksheet scope, then workbook scope, then request scope. A definition with the same name replaces the outer definition. `bodyStyle` provides a uniform default for every report column. A column's named `style` overlays that body style, and its inline `format` overlays the result; explicit false values replace inherited true values. `numberFormat` has highest display-format precedence.

Worksheet options include `totalRows` (optional exact number of data records, excluding headers), `writerMode` (`auto`, `buffered`, or `streaming`), and optional `table: { "name": "ReportData", "maxCells": 1000000 }`. `auto` uses buffered writing for objects and streaming for queries. Known counts are checked before writing and again against actual source completion. Count mismatches return `ROW_COUNT_MISMATCH` and prevent publication. The optional count never triggers a database COUNT query. Job status exposes the sum of known `totalRows`, or null if any worksheet's count is unknown. Object counts are inferred when omitted.

See [typed columns and efficient formatting](guides/typed-columns.md) for complete examples, conversion rules, and memory/performance trade-offs.

## Columns and values

| Column field | Default | Behavior |
| --- | --- | --- |
| `field` | Required | Exact record key/result label |
| `dataType` | auto | auto, text, integer, number, decimal, boolean, date, datetime, time, formula |
| `numberFormat` | Type default | Excel invariant display format string; overrides other display formats |
| `header` | Field name | Displayed header text |
| `width` | Writer default | Excel column width units |
| `style` | None | Scoped named body style |
| `format` | None | Excel display format string, or backward-compatible inline Style object |
| `missingAsNull` | false | Missing object fields become blank instead of failing |

Extra object keys are ignored; key order never changes the column order. Dotted field names are literal keys. Empty record arrays produce headers. `writerMode` may explicitly select buffered or streaming writing for either source kind. Caller-owned object arrays already occupy memory even when the writer streams them to disk.

Null writes a blank, booleans write booleans, and ordinary numbers write numeric cells. Integer JSON values larger than 15 decimal digits write exact text. Strings always write literal strings, including strings starting with `=`. Nested objects/arrays require explicit serialization or one of these tagged values:

```json
{ "$type": "decimal", "value": "12345678901234567890.12" }
```

With `dataType: auto`, tags `int64`, `decimal`, `date`, `timestamp`, and `json` preserve backward-compatible text output. Explicit `date`, `datetime`, and `time` columns convert ISO input to Excel serials and use native Excel display formats. `number` columns accept numeric values or numeric text and deliberately select Excel floating-point precision; use them for numeric currency and percentages. `decimal` columns preserve exact decimal text. Formatting text does not make it numeric. A `formula` tag or explicit `formula` column writes a formula; ordinary text remains literal. Formula evaluation is performed by the spreadsheet application. Conversion failures include the field and worksheet row but omit the source value.

Database decoding supports booleans, common integer/float types, text, decimal, UUID, and common dates/timestamps. PostgreSQL JSON becomes compact text. PostgreSQL numeric decoding is limited by the selected decimal library; cast higher-precision numerics to text in SELECT. Binary, unsupported vendor types, and arrays require explicit SQL text conversion. Non-finite database floats must be rejected rather than serialized as null.

An `int64` query parameter accepts either a JSON integer or an exact decimal string, such as `{ "type": "int64", "value": "9007199254740993" }`. The Node wrapper also converts `bigint` parameter values to this exact string representation.

## Styles

All style properties are optional. Colors must be six-digit RGB with optional `#` prefix.

| Property | Values |
| --- | --- |
| `fontName`, `fontSize` | Font name; size between 1 and 409 |
| `bold`, `italic`, `underline`, `strikeout` | Boolean |
| `fontColor`, `background`, `borderColor` | RGB text |
| `border` | thin, medium, thick, double; applies to all sides |
| `horizontal` | left, center, right |
| `vertical` | top, center, bottom |
| `wrap` | Boolean |
| `rotation` | Integer from -90 to 90 |
| `numberFormat` | Excel invariant custom format string |
| `locked` | False writes an unlocked style; sheet protection is not currently enabled |

## Layout

| Field | Default | Behavior |
| --- | --- | --- |
| `header` | true | Emit column labels |
| `headerStyle` | None | Named format for all header cells |
| `startRow`, `startColumn` | 1, 1 | Header position, or first data position when header is false |
| `freezeRows` | 1 | Absolute count of top rows frozen |
| `autofilter` | false | Filter the header/data rectangle when nonempty |
| `rowHeight` | Writer default | Data row height in points |
| `merges` | Empty | Buffered mode only; all merge rows must precede the header/data start |

Each merge requires `firstRow`, `firstColumn`, `lastRow`, `lastColumn`, and `text`; optional `style` names a custom format. Excel limit violations fail. Structured tables are supported in buffered mode with nonempty data and unique headers/names; query tables require `totalRows`. `table.maxCells` caps the backend's per-cell table metadata, including the header. Streaming structured tables are rejected; ordinary streaming ranges use efficient column formats. Automatic worksheet/workbook splitting remains pending.

## Query options

| Field | Default | Behavior |
| --- | --- | --- |
| `connectTimeoutMs` | 15000 | Positive connection deadline |
| `queryTimeoutMs` | 300000 | Positive deadline covering producer execution and downstream waiting |
| `queueRows` | 16 | Bounded decoded-row queue, from 1 to 4096 |
| `maxRowBytes` | 1048576 | Maximum serialized row bytes, up to 64 MiB |
| `fetchRows` | 512 | PostgreSQL portal window, from 1 to 65536; SQL Server uses TDS streaming |
| `allowPlaintext` | false | Permit the connection string's plaintext mode |

Credentials live only in the request and driver. Debug formatting is deliberately absent on request types containing connection strings. Driver error messages are sanitized to avoid leaking credentials, SQL parameters, or data. Queries execute in their native dialect. Parameter arrays bind values through driver APIs. SQL Server allows one tabular result set. No automatic query retries, pooling, or cross-database snapshot guarantee is provided.

## Native ABI

See `bindings/c/native_excel.h` for exact signatures. ABI version is 1. Functions use C calling convention. Inputs are borrowed UTF-8 byte arrays for the duration of the call. Inputs are limited to 32 MiB. Output points to an empty writable `NeBuffer`; do not overwrite an unreleased buffer. Status 0 indicates success; status 1 contains an error JSON object with `code` and `message`.

Each successful output allocation, including errors and null values, must be freed exactly once with `ne_buffer_free`. A buffer has pointer and native-sized length; its memory belongs to Rust. Never use the host allocator to free it. Handles are monotonic IDs; released IDs are invalid and are not reused. Native pointers must be valid; the library cannot safely probe arbitrary invalid addresses.

`ne_job_result_json` blocks until terminal status. `ne_query_next_json` blocks until a row/error/EOF, with only one active read allowed. Another thread can call `ne_job_cancel` or `ne_query_release`; releasing a query aborts its producer and wakes the blocked reader. Do not unload the DLL while calls or jobs are active. Rust panics are caught at ABI boundaries where unwinding is possible; process aborts and invalid caller memory cannot be recovered this way.

## Errors and cancellation

Codes include `INVALID_CONFIG`, `UNSUPPORTED_OPTION`, `CAPABILITY_MISMATCH`, `SCHEMA_MISMATCH`, `ROW_COUNT_MISMATCH`, `CONVERSION_FAILED`, `UNSUPPORTED_TYPE`, `LIMIT_EXCEEDED`, `OUTPUT_EXISTS`, `IO_ERROR`, `XLSX_ERROR`, `QUERY_FAILED`, `TIMEOUT`, `CANCELLED`, `INVALID_HANDLE`, `BUSY`, and `INTERNAL_ERROR`.

Export job phases are running, finalizing, publishing, succeeded, failed, or cancelled. Cancellation is checked between rows and before publication. ZIP finalization is synchronous and may delay cancellation until it returns. Publication is the commit point; cancellation after a successful publication does not remove that file. Query session drop/release closes the dedicated connection. Notification delivery uses polling in the current native interfaces.
