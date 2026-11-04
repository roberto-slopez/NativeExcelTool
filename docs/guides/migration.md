# Migration from msexcel-builder

This guide maps the legacy `msexcel-builder` v0.1.0 API to `NativeExcelTool`.

## Key behavioral improvements over msexcel-builder

1. **Validated Excel coordinates through `XFD` (16,384):** `msexcel-builder`'s `tool.i2a` only mapped single letters and failed past column `Z`. `NativeExcelTool` converts and validates columns `1..=16384` (`A..=XFD`) and rows `1..=1048576`.
2. **Typed values instead of string-only cells:** `msexcel-builder`'s `Sheet.set` coerced every value to a shared string. `NativeExcelTool` writes native numbers, booleans, dates/times, formulas, exact decimals/large integers, and strings.
3. **Sparse buffered and constant-memory streaming modes:** `msexcel-builder` pre-allocated a dense `rows * cols` JavaScript object grid in the `Sheet` constructor and built the entire ZIP in memory. `NativeExcelTool` uses sparse cell storage in `WorkbookBuilder` and constant-memory temporary disk spooling in streaming exports.
4. **Cooperative cancellation:** `msexcel-builder`'s `workbook.cancel()` only logged a deprecation warning. `NativeExcelTool` cancels active jobs, aborts database/input producers, and removes unpublished staging files.

## API mapping

| `msexcel-builder` | `NativeExcelTool` (Rust `WorkbookBuilder`) | `NativeExcelTool` (Declarative JSON / Node / C#) |
| --- | --- | --- |
| `excelbuilder.createWorkbook(path, file)` | `WorkbookBuilder::new(path)` | `workbooks: [{ id, output: { path } }]` |
| `workbook.createSheet(name, cols, rows)` | `wb.create_sheet(name, cols, rows)?` | `worksheets: [{ name, columns, source }]` |
| `sheet.set(col, row, str)` | `sheet.set(col, row, value)?` or `sheet.set_a1("B2", value)?` | `source: { kind: "objects" | "rowStream" | "query" }` |
| `sheet.merge({col,row}, {col,row})` | `sheet.merge((c1, r1), (c2, r2))?` | `layout.merges` or automatic `reportHeader` |
| `sheet.width(col, wd)` | `sheet.width(col, wd)?` | `columns[].width` or `layout.autofit` |
| `sheet.height(row, ht)` | `sheet.height(row, ht)?` | `layout.rowHeight` / `layout.rowHeights` |
| `sheet.font(col, row, font_s)` | `sheet.font(col, row, FontSpec { ... })?` | `Style` (`fontName`, `fontSize`, `bold`, `italic`, `fontColor`) |
| `sheet.fill(col, row, fill_s)` | `sheet.fill(col, row, FillSpec { ... })?` | `Style` (`fillPattern`, `foregroundColor`, `background`) |
| `sheet.border(col, row, bder_s)` | `sheet.border(col, row, BorderSpec { left, right, top, bottom, color })?` | `Style` (`border` or `borderLeft`/`borderRight`/`borderTop`/`borderBottom`) |
| `sheet.align(col, row, align)` | `sheet.align(col, row, align)?` | `Style.horizontal` (`left`, `center`, `right`) |
| `sheet.valign(col, row, valign)` | `sheet.valign(col, row, valign)?` | `Style.vertical` (`top`, `center`, `bottom`) |
| `sheet.rotate(col, row, angle)` | `sheet.rotate(col, row, angle)?` | `Style.rotation` (`-90..=90`) |
| `sheet.wrap(col, row, 'true')` | `sheet.wrap(col, row, true)?` | `Style.wrap` (`true` / `false`) |
| `workbook.save(cb)` | `wb.save()?` | `excel.exportWorkbooks(request)` / `NativeExcelNative.Export(json)` |
| `workbook.cancel()` | `wb.cancel()` | `job.cancel()` / `ne_job_cancel` |
