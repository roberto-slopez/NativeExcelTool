# NativeExcelTool

Progress and opt-in telemetry are documented in [the telemetry guide](docs/guides/telemetry.md). The [remaining implementation plan](docs/REMAINING_PLAN.md) records the concrete closure steps and acceptance gates.

Native Rust library for creating Excel `.xlsx` reports from object records and incrementally consumed PostgreSQL or SQL Server queries. It also builds a Node-API addon and a native DLL for C# P/Invoke.

This repository contains a working initial implementation. See [implementation status](docs/IMPLEMENTATION_STATUS.md) for the remaining items in the larger [implementation plan](docs/IMPLEMENTATION_PLAN.md). Database adapters compile, but live database integration has not yet been validated in this environment.

Reports now support exact `totalRows`, declared column `dataType`, native Excel date/currency/percentage formats, one-time uniform column-style setup, and buffered structured Excel tables. See [the typed-column guide](docs/guides/typed-columns.md) and [the runnable invoice example](examples/configs/typed-report.json).

## Build

Prerequisites: stable Rust with the Windows MSVC target and Visual Studio C++ build tools. The verified initial target is Windows x64. Node.js is required for Node smoke tests; .NET 10 is used by the included C# example. Neither Node nor .NET implements the export engine.

```powershell
cargo xtask build-all
cargo xtask build-all --release
cargo xtask schema
```

`dist/debug` or `dist/release` contains:

- `libnative_excel.rlib`: Rust library artifact; use the Cargo crate as the normal integration mechanism.
- `native_excel.node`: Node-API addon (N-API level 6).
- `native_excel_ffi.dll`: native C ABI DLL for P/Invoke.
- `index.cjs`, `index.d.ts`, `native_excel.h`, `NativeExcelNative.cs`, JSON schema, and manifest.

The DLL is loaded through P/Invoke. Its process architecture must match the host. Do not attempt to reference it as a managed .NET assembly. The Rust `.rlib` requires a matching compiler and dependency graph; source-based Cargo integration is recommended.

If a machine's rustup launcher cannot start because of a local permission/setup problem, select its installed toolchain explicitly, without changing the system installation:

```powershell
$toolchain = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin"
$env:RUSTC = Join-Path $toolchain 'rustc.exe'
$env:CARGO_HOME = Join-Path (Get-Location) '.cargo-online'
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target'
& (Join-Path $toolchain 'cargo.exe') xtask build-all
```

## Export object records

The complete runnable configuration is [objects.json](examples/configs/objects.json). It creates two separate workbooks. Add entries to `worksheets` to create more tabs in a workbook.

```json
{
  "schemaVersion": 1,
  "styles": {
    "heading": { "bold": true, "background": "#16324F", "fontColor": "#FFFFFF" },
    "money": { "numberFormat": "#,##0.00" }
  },
  "workbooks": [{
    "id": "sales",
    "output": { "path": "output/sales.xlsx", "createParents": true },
    "worksheets": [{
      "name": "Orders",
      "source": { "kind": "objects", "records": [{ "customer": "Contoso", "total": 99.95 }] },
      "columns": [
        { "field": "customer", "header": "Customer", "width": 30 },
        { "field": "total", "header": "Total", "style": "money" }
      ],
      "layout": { "headerStyle": "heading", "autofilter": true }
    }]
  }]
}
```

Unknown properties fail deserialization. The generated schema and [API reference](docs/API.md) describe the implemented contract. The original planning examples describe a broader proposed contract; use the examples in this README and `examples/configs` for this implementation.

## Rust

Add the crate through a path dependency, then use a Tokio runtime:

```toml
[dependencies]
native-excel = { path = "path/to/NativeExcelTool/crates/native-excel" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust,no_run
use native_excel::{ExcelExporter, parse_request};

#[tokio::main]
async fn main() -> native_excel::Result<()> {
    let config = std::fs::read_to_string("examples/configs/objects.json")?;
    let request = parse_request(&config)?;
    ExcelExporter::validate(&request)?;
    let job = ExcelExporter::start(request)?;
    let report = job.result().await?;
    println!("{report:?}");
    Ok(())
}
```

`job.status()` reports phase, written rows, and published files. `job.cancel()` requests cooperative cancellation. Keep the runtime alive until the result resolves. XLSX generation runs on a blocking worker, while database I/O runs on Tokio workers.

```powershell
cargo run -p native-excel --example export
```

## Node.js

```javascript
const excel = require('./dist/debug/index.cjs');
const request = require('./examples/configs/objects.json');
const report = await excel.exportWorkbooks(request);

const job = await excel.startExport(request);
console.log(job.status());
// job.cancel();
console.log(await job.result());
```

The CJS wrapper accepts ordinary objects and converts `bigint` cells into exact tagged text values. For decimals, supply `{ $type: 'decimal', value: '12345678901234567890.12' }`. Unsafe JavaScript numeric literals have already lost precision before reaching the library; use `bigint` or tagged text instead. `Date` values serialize to ISO text. Buffers and nested objects require explicit text encoding.

```powershell
node examples/node/export.cjs
node --test tests/node.test.cjs
```

## C#

Include [NativeExcelNative.cs](bindings/dotnet/NativeExcelNative.cs), place the DLL in a configured native search path, and call:

```csharp
var report = NativeExcelNative.Export(File.ReadAllText("examples/configs/objects.json"));
Console.WriteLine(report);
```

The included example resolves the DLL from `dist/debug`:

```powershell
dotnet run --project examples/dotnet
```

For jobs, call `ne_export_start_json`, poll `ne_job_status_json`, then obtain `ne_job_result_json`. Release every job in `finally`. Free every native output with `ne_buffer_free`; the `Read` helper does this automatically, including error cases. For large/precise numbers, construct tagged values before serializing the request. JSON object input currently materializes the caller's array in native memory.

## Query sources and independent streaming

Replace the object source with:

```json
{
  "kind": "query",
  "dbms": "postgresql",
  "connectionString": "host=db.example.com port=5432 user=report_user password=REPLACE_ME dbname=reporting",
  "query": "SELECT customer_id, display_name FROM customers WHERE active = $1 ORDER BY customer_id",
  "parameters": [{ "type": "boolean", "value": true }],
  "options": { "queueRows": 16, "maxRowBytes": 1048576, "queryTimeoutMs": 300000, "fetchRows": 512 }
}
```

For SQL Server, set `dbms` to `sqlserver`, use an ADO connection string, and bind `@P1` instead of `$1`:

```text
Server=tcp:db.example.com,1433;Database=Reporting;User ID=report_user;Password=REPLACE_ME;Encrypt=true;TrustServerCertificate=false
```

`columns[].field` identifies result labels from SELECT, including aliases. The entire query is supplied by the caller; fields are never interpolated into SQL. Unknown or duplicate result labels fail. Parameter types are `text`, `int32`, `int64`, `float64`, and `boolean`, each nullable.

TLS and hostname verification are required by default. PostgreSQL uses WebPKI roots; SQL Server uses its selected driver's certificate handling. Private-CA and client-certificate configuration remain pending. `allowPlaintext: true` permits the driver's plaintext configuration for explicitly configured development environments; it does not bypass certificate verification.

PostgreSQL uses a read-only transaction and a portal with a positive fetch window. SQL Server consumes TDS metadata and row events and rejects additional result sets. Use a database account restricted to reporting; SQL Server's supplied SQL executes with that account's permissions.

Standalone Node streaming:

```javascript
const rows = await excel.queryStream({
  dbms: 'postgresql', connectionString: process.env.REPORTING_CONNECTION,
  query: 'SELECT customer_id, display_name FROM customers ORDER BY customer_id'
});
for await (const row of rows) {
  console.log(row);
  // Breaking invokes return(), cancelling the native query producer.
}
```

Rust: `QueryService::stream(QueryRequest).await`, `session.schema()`, `session.next().await`, and `session.close().await`. Dropping the session aborts the producer and closes its dedicated connection. C#: `ne_query_open_json`, `ne_query_schema_json`, repeated `ne_query_next_json` (JSON `null` means EOF), and `ne_query_release` in `finally`. These C calls block; execute them on an appropriate application worker thread.

The query producer sends rows through a bounded channel. XLSX query worksheets flush completed rows to temporary disk files and use inline strings. Queue memory is bounded by row capacity times the maximum accepted serialized row size, plus conversion/in-flight overhead. Driver/protocol buffers and a single oversized value can allocate before the row-size check; this is not a hard total-process-memory guarantee. Server sorting and aggregation can delay the first row.

## Verification

```powershell
cargo test -p native-excel -p native-excel-ffi --tests
cargo clippy --workspace --all-targets -- -D warnings
```

Tests read output with an independent XLSX reader and verify multiple workbooks, exact integer text, typed numeric cells, formulas versus literal strings, column addresses beyond Z, failure cleanup, overwrite protection, cancellation, row limits, and native buffer ownership.

Live integration, large-volume RSS measurements, Excel/LibreOffice desktop smoke opens, release signing, and registry publication must be completed before claiming production certification. See [implementation status](docs/IMPLEMENTATION_STATUS.md).
