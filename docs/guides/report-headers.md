# Dynamic report headers

Each worksheet accepts an optional `reportHeader`, separate from the data-column heading controlled by `layout.header`. It supports a large merged title and any number of metadata fields arranged in logical columns. All properties are passed through the same request in Rust, Node.js, and C#.

The complete runnable example is `examples/configs/report-header.json`.

## Two logical columns across A:H

For a data table with eight columns starting at A, use this worksheet fragment:

```json
{
  "reportHeader": {
    "columnCount": 2,
    "columns": [
      { "fields": [{ "label": "Date", "value": "2026-11-01", "dataType": "date", "numberFormat": "dd/mm/yyyy" }] },
      { "fields": [{ "label": "Module", "value": "Sales" }] }
    ]
  },
  "layout": { "header": false }
}
```

The generated layout is:

| Excel row | A:D | E:H |
| --- | --- | --- |
| 1 | Date (merged, aligned left) | Module (merged, aligned left) |
| 2 | 01/11/2026 (merged, centered) | Sales (merged, centered) |
| 3 | First data record across the original eight data columns | |

`layout.header: false` means actual records begin on row 3. With the default `layout.header: true`, row 3 contains the data-column headings and the first record begins on row 4. Header fields are additional metadata, not data records, so they do not contribute to `totalRows` or `rowsWritten`.

## Add a title and more fields

```json
{
  "reportHeader": {
    "title": { "text": "Sales Report", "style": { "fontSize": 24, "fontColor": "#16324F" }, "height": 36 },
    "columnCount": 2,
    "columns": [
      { "fields": [
        { "label": "Date", "value": "2026-11-01", "dataType": "date", "numberFormat": "dd/mm/yyyy" },
        { "label": "Prepared by", "value": "Reporting Team" }
      ] },
      { "fields": [
        { "label": "Module", "value": "Sales" },
        { "label": "Currency", "value": "USD" }
      ] }
    ]
  }
}
```

The title occupies A1:H1. The first labels/values occupy rows 2/3, and the second pair occupies rows 4/5. With normal data headings enabled, row 6 is the table heading and records start on row 7. In the complete example, the structured table covers A6:H8.

Each logical column has its own `fields` array. They may have different lengths. The longest array determines the header height; shorter columns leave the unused rows blank. Data starts below the entire header, never below an individual shorter block. Values and labels are supplied by the caller, so any module, date, owner, filter description, or other report context can be passed without hard-coded field names. A title-only report header is also supported.

## Distribution algorithm

The available width is `worksheet.columns.length`, starting at `layout.startColumn`. `columnCount` is the number of logical header blocks, not the number of physical Excel columns. It defaults to the number of supplied header columns, or one for a title-only header. It must be from one through the data-column count and cannot be smaller than the number of supplied header columns.

For width W and block count N, each block gets `floor(W / N)` physical columns. The first `W % N` blocks get one additional physical column. Spans are contiguous and never overlap. Eight physical columns and three blocks produce A:C, D:F, G:H. Starting at C instead produces C:E, F:H, I:J. Single-cell spans write ordinary cells rather than invalid one-cell merge ranges. Physical column widths still come from the data-column definitions, so equal column counts do not necessarily imply equal widths in pixels.

Unused logical blocks are allowed: specify a larger `columnCount` than the supplied `columns` array length to reserve empty space on the right. More blocks than physical data columns are rejected because Excel cannot merge a fractional column.

## Parameters and styling

| Parameter | Default | Meaning |
| --- | --- | --- |
| `title` | absent | Object with required text, optional inline Style, optional row height |
| `title.style` | 20-point, bold, centered, wrapped | Custom title properties overlay the defaults |
| `title.height` | 32 points | Height of the single title row |
| `columnCount` | inferred | Logical header blocks distributed across the table width |
| `columns[].fields[]` | explicit arrays | Ordered label/value pairs stacked within each block |
| `labelStyle` | bold, left aligned, vertically centered, wrapped | Common label style |
| `valueStyle` | centered horizontally/vertically, wrapped | Common value style |
| `labelHeight`, `valueHeight` | Excel default | Optional row heights, from 1 to 409 points |
| `gapRows` | 0 | Additional blank rows after the report header |
| `fields[].label` | required | Literal label text |
| `fields[].value` | required | Caller-supplied value; null leaves a blank formatted value |
| `fields[].dataType` | auto | Same conversion rules as typed data columns |
| `fields[].numberFormat` | type default or General | Native Excel display format for that value |
| `fields[].labelStyle`, `fields[].valueStyle` | empty | Per-field inline style overrides |

Styles are independent of body-column defaults. Header-level styles overlay the standard defaults; field-level styles overlay header-level styles. `numberFormat` has final precedence for the field value. Labels default to left alignment, values to centered alignment, and explicit styles can customize these defaults. Plain labels and text values beginning with `=` remain literal text. Formulas require the existing explicit formula type/tag. Dates can be stored as native Excel serials using `dataType: date`. See `typed-columns.md` for accepted ISO inputs, timestamps, precision, and Excel formats.

## Positioning, limits, and streaming

`layout.startRow` anchors the report header when `reportHeader` is present. `layout.startColumn` anchors both the report header and data table. The table row is calculated as:

```text
tableRow = startRow + (title present ? 1 : 0)
         + 2 * maximum fields in any header column + gapRows
firstRecordRow = tableRow + (layout.header ? 1 : 0)
```

All positions in this formula are one-based. Without `reportHeader`, existing `startRow` behavior is unchanged. Worksheet row capacity includes the report header, gaps, data headings, offsets, and data records. Structured-table and autofilter ranges start at the computed table heading, not at the report title. `totalRows` remains the exact number of data records only.

With an automatic report header and `freezeRows > 0`, freezing includes all rows before the table plus the requested number of table rows. Thus `freezeRows: 1` freezes the report header and the data-column heading. With `layout.header: false`, it freezes the first data row as well; set `freezeRows: 0` to disable freezing. Without an automatic report header, the existing absolute freeze count remains unchanged.

Automatic header merges are horizontal and rendered row by row, so they work in both buffered and streaming modes, including query sources. They do not require loading all report records. Streaming structured Excel tables still have their existing restriction. Arbitrary `layout.merges` remain buffered-only and must not overlap the reserved automatic header rectangle, including gap rows; merges outside that rectangle must still end before the data table. Invalid counts, overlaps, styles, and header sizes fail rather than overwrite cells.

## Run the example

```powershell
cargo run -p native-excel --bin native-excel -- examples/configs/report-header.json
```

For the full sales report in both languages, build with `cargo xtask build-all` and run `./scripts/run-examples.ps1 -Profile debug`. Each example builds the header from the orders in its own source file. The six summary fields fit into three logical blocks and leave a blank row before the table at `A7:H17`. The same JSON contract is available through the C ABI, and the TypeScript `ReportHeader` and `HeaderField` types describe its parameters.
