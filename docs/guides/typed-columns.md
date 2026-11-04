# Typed columns, totals, and efficient Excel formatting

All Rust, Node.js, and C ABI/C# exports accept the same configuration. The complete runnable example is `examples/configs/typed-report.json`. It generates a structured invoice table with typed dates, numeric currency, percentages, and booleans.

## Specify the record count

Set `totalRows` on each worksheet to the exact number of data records. Headers, merged titles, and starting offsets are excluded from this number:

```json
{
  "name": "Invoices",
  "totalRows": 50000,
  "writerMode": "streaming",
  "bodyStyle": "reportBody",
  "source": {
    "kind": "query",
    "dbms": "postgresql",
    "connectionString": "host=db.example.com dbname=reports user=report_user password=REPLACE_ME",
    "query": "SELECT invoice_date, amount FROM invoices ORDER BY invoice_id"
  },
  "columns": [
    { "field": "invoice_date", "dataType": "date", "format": "dd/mm/yyyy" },
    { "field": "amount", "dataType": "number", "format": "\"$\"#,##0.00" }
  ]
}
```

This is a worksheet fragment; define `reportBody` in the worksheet/workbook/request style registry and wrap it in the normal export request. The count is caller-supplied and must match the query. The library does not run an extra count query or load the result into an array to discover its size.

Object arrays supply their length automatically; an explicit conflicting `totalRows` fails validation. Query counts remain optional for normal ranges. A known count allows preflight capacity checks and supplies progress totals. Too many query rows fail when the first excess row arrives; too few fail at EOF. Both return `ROW_COUNT_MISMATCH`, discard staging output, and preserve any previously existing final file. An empty normal report accepts zero records. A structured table requires at least one record.

For a start row of 3, one header row, and 50,000 data records, the last occupied row is 50,003. The capacity check includes those offsets and headers. This does not allocate 50,000 blank records.

## Apply a uniform style once per column

`bodyStyle` supplies the common table-body style. Each column can add or replace properties and select its Excel number format. Formats are resolved and registered before row consumption:

```json
{
  "styles": {
    "reportBody": { "fontName": "Calibri", "fontSize": 11, "border": "thin", "borderColor": "#DDDDDD" },
    "heading": { "bold": true, "fontColor": "#FFFFFF", "background": "#16324F" }
  }
}
```

The writer calls the backend's column-format API once for each report column. The value loop then writes typed values without passing or looking up a custom Format object for every cell. The header uses a separately compiled explicit format. Nulls leave blank cells with the column's default style.

This changes uniform style setup from a pass over every report cell into a pass over the columns. Writing and validating the actual data still necessarily visits each value. Excel may store a style reference on individual written cells; that is file encoding, not a repeated application-level style pass. No universal speedup percentage is claimed without workload measurements.

Column defaults apply to the entire worksheet column, including blank cells outside the report. This is an intentional trade-off that keeps uniform styling efficient in both writer modes. The known total defines the report/table/filter range; it does not restrict a worksheet column default to a subset of rows. A precise bounded range with direct cell formats would require visiting those cells in the pinned backend. Its `set_range_format` API performs that loop internally, so it is not used as a misleading constant-cost optimization.

Style precedence is: worksheet `bodyStyle`, then named column `style`, then column `format` object/string, then column `numberFormat`. Named definitions resolve worksheet before workbook before request. Header formatting is separate. Legacy inline Style objects still work:

```json
{ "field": "amount", "dataType": "number", "format": { "numberFormat": "#,##0.00", "fontColor": "#006600" } }
```

## Declare column types

| `dataType` | Accepted non-null input | Excel storage and default display |
| --- | --- | --- |
| `auto` | Existing scalar/tagged contract | Backward-compatible inference; date tags remain text |
| `text` | Text, recognized tagged text, number, boolean | Literal text; leading `=` remains text |
| `integer` | Integral JSON number, signed decimal integer text/tag | Numeric up to 15 decimal digits; larger values remain exact text |
| `number` | JSON number, numeric text/tag | Finite Excel floating-point number; General display unless formatted |
| `decimal` | Decimal text or a decimal-tagged string | Exact text; no implicit binary floating-point conversion |
| `boolean` | JSON true/false | Boolean cell; strings such as "true" fail |
| `date` | `YYYY-MM-DD` text or recognized tag | Excel date serial; default `yyyy-mm-dd` |
| `datetime` | ISO local datetime or RFC 3339 timestamp | Excel date/time serial; default `yyyy-mm-dd hh:mm:ss` |
| `time` | `HH:MM:SS` with optional fractional seconds | Fraction of an Excel day; default `hh:mm:ss` |
| `formula` | Formula text or a formula tag | Explicit Excel formula |

All types permit null as a blank. Missing keys still fail unless `missingAsNull` is true. Invalid values return `CONVERSION_FAILED` with field and row context, without including the actual value. The library rejects dates outside years 1900 through 9999. Offset-aware timestamps are normalized to UTC before writing; Excel serial cells cannot retain timezone metadata. Local datetimes remain local and are not adjusted using the machine's timezone. Use text/auto to preserve an original offset exactly.

Every new report should declare `dataType` for each column. The default remains `auto` so existing version-1 configurations continue to work. Database SELECT labels and aliases are matched before the same conversion rules are applied. PostgreSQL/SQL Server dates and decimals decoded as text are converted according to the declared column type.

In Rust, construct a typed column with `Column::new("invoice_date", DataType::Date).with_number_format("dd/mm/yyyy")`. Existing inline Rust styles can be wrapped with `ColumnFormat::from(style)`; string formats also implement `From<&str>` and `From<String>`.

JavaScript `Date` values serialize as full ISO timestamps, suitable for `datetime`. For a `date` column, pass an ISO date string such as `date.toISOString().slice(0, 10)` if the intended date is its UTC calendar date. Use a date string constructed from the intended local calendar date when that is the business meaning; this library does not infer a local timezone from a Date object.

Excel numeric storage supports about 15 significant decimal digits. A numeric currency report therefore uses `number`, explicitly choosing Excel precision. For accounting values that must preserve every decimal digit beyond that limit, use `decimal`; exact text is preserved, but numeric Excel currency formatting and arithmetic do not apply to text. A currency format changes display only; it neither rounds the stored number to two places nor converts between currencies.

## Excel display formats

Use either a string `format` or `numberFormat`. If both are specified, `numberFormat` wins. These are Excel number-format codes, not Rust/Chrono strftime patterns, JavaScript tokens, or .NET formatting strings. Input date parsing remains ISO; display formatting does not change accepted input syntax.

| Example | Display purpose |
| --- | --- |
| `dd/mm/yyyy` | Day/month/year date |
| `yyyy-mm-dd` | ISO-style date display |
| `yyyy-mm-dd hh:mm:ss` | Date and time |
| `hh:mm:ss` | Time of day |
| `#,##0.00` | Thousands separator and two decimals |
| `"$"#,##0.00` | Literal dollar symbol with two decimals |
| `"EUR "#,##0.00` | Literal currency label |
| `0.00%` | Percentage: underlying 0.16 displays as 16.00% |
| `0.00E+00` | Scientific notation |
| `000000` | Leading zero display for numeric codes |

In JSON, embedded quotation marks must be escaped: `"format": "\"$\"#,##0.00"`. Codes are stored using Excel's invariant syntax; the spreadsheet application may localize displayed separators. Codes must contain 1 through 255 bytes with no control characters. They are passed to Excel rather than evaluated by this library. Apply numeric formats to numeric/date cells; a format cannot turn a text cell into a number.

## Structured Excel tables

Add `"table": { "name": "InvoiceData", "maxCells": 1000000 }` to a buffered worksheet. The exact table rectangle includes the header and the actual data records. A query table requires an explicit `totalRows`; object arrays infer it. Headers must be nonempty and unique case-insensitively. Table names must be unique within the workbook and use valid ASCII identifier names rather than Excel cell references. Direct custom column styles are retained; no built-in table theme is imposed.

The pinned backend stores a per-cell overlap map for structured tables. They therefore require `writerMode: "buffered"` and are capped by `maxCells` (header included). Streaming plus `table` returns `CAPABILITY_MISMATCH` before connecting to a database. Buffered query mode also retains cell data: select it explicitly only when its memory cost is acceptable. Normal streaming reports still support typed columns, custom styles, totals, panes, and worksheet autofilters without a structured table object.

## Run the complete example

```powershell
cargo run -p native-excel --example export -- examples/configs/typed-report.json
```

For typed columns in the C# and Node.js sales reports, run `cargo xtask build-all` followed by `./scripts/run-examples.ps1 -Profile debug`. Use the `release` profile with optimized binaries. Both examples pass typed columns through the shared JSON request.
