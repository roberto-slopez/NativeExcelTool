# Remaining implementation and release plan

This tracked checklist completes the roadmap with concrete closure steps. It distinguishes shipped code from work that still requires implementation or external verification. It does not certify unfinished capabilities as complete.

Reconciled on November 1, 2026 against source, declarations, tests, packaging, and CI. Original steps 01-42 are retained in `IMPLEMENTATION_PLAN.md`; its section 17 maps subsequent user requests as U01-U10. The legacy reference folder was renamed from `references` to `oldexcel` (ignored by Git), and its MIT notice is tracked in `licenses/msexcel-builder.LICENSE` so clean checkouts package without requiring `oldexcel`.

Required baseline items are R01-R18 and R20-R25. R19 is retained for historical continuity but is an optional assistant proposal, not a required release gate.

## Completed baseline and user-requirement increments

- [x] **U01-U10**: Exact optional `totalRows`, declared column `dataType`, Excel `format`/`numberFormat`, uniform column-style setup, dynamic report titles and metadata blocks (`reportHeader`), opt-in JSONL/Prometheus/custom-sink telemetry, unconditional CLI progress feedback, `.gitignore` excluding `oldexcel` and generated artifacts, and English documentation.
- [x] **R01**: Incremental caller-input sessions (`InputSession`, `InputService`, `Source::RowStream`) and bounded batches across Rust, Node (`openInputStream`), and C ABI / C# (`ne_input_*`, `SafeInputHandle`).
- [x] **R03**: Worksheet and workbook overflow splitting (`splitWorksheet`, `splitWorkbook`) with deterministic naming, repeated headers/styles, and exact record conservation.
- [x] **R04**: Declarative style rules (`rules` with `rowParity`, field comparisons, `all`/`any`/`not`) and explicit 1-based `cellOverrides`, plus `resetFields` inheritance clearing in both buffered and streaming modes.
- [x] **R05**: Extended layout and formatting capability matrix: per-edge borders (`borderLeft`/`Right`/`Top`/`Bottom`/`Diagonal` and colors), fill patterns (`fillPattern`, `foregroundColor`), `shrinkToFit`, `indent`, `hidden`, `gridlines`, `zoom`, `tabColor`, `freezeColumns`, `protectSheet`, `rowHeights`, `hiddenColumns`/`hiddenRows`, and `print` setup.
- [x] **R06 & R21**: Column inference (`inferColumns`), explicit nested path selectors (`path`), case-insensitive field matching with collision rejection (`caseInsensitiveFields`), `extraFields` policy (`ignore`/`error`), `defaultValue`, `nullPlaceholder`, declarative string `transforms` (`trim`, `uppercase`, `lowercase`), `precisionPolicy` (`exactText`, `numeric`, `round` with `roundScale`), `timezone` (`utc`, `preserveText`, `+HH:MM`/`-HH:MM`), and `DataType::Binary` (`hex`, `base64`).
- [x] **R11**: Buffered random-access `WorkbookBuilder` and `SheetBuilder` with `oldexcel` (`msexcel-builder`) parity, validated Excel coordinates through `XFD` (`16384`), and streaming `AppendWorkbookWriter`.
- [x] **R22**: Bounded byte output (`output.returnBytes`, `options.maxOutputBytes`, `WorkbookBuilder::save_to_bytes`), buffered exact autofit and streaming sampled autofit (`autofit`, `autofitSampleRows`), and file sync (`output.syncData`).
- [x] **R23 & R14 (local packaging)**: Clean-checkout reproducibility in `xtask` using tracked `licenses/msexcel-builder.LICENSE` (with `oldexcel/LICENSE` fallback), strict CLI argument validation (`schema` and `build-all [--release] [--target x86_64-pc-windows-msvc]`), recursive `checksums.txt` covering nested license files, and enriched `manifest.json`.
- [x] **R24 & R25**: Workbook `metadata` (`DocProperties`), request/workbook `defaults`, `ExcelExporter::capabilities()` / `ne_capabilities_json` / Node `capabilities()`, `ne_job_next_event_json` / Node `job.events()`, `exportTimeoutMs`, contextual `ExportError` fields, convenience methods (`export_objects`, `export_query`), C# `SafeJobHandle`/`SafeQueryHandle`/`SafeInputHandle`/`ExportAsync`/`QueryStreamAsync`, and `docs/guides/migration.md`.

## Ordered closure steps

| Step | State | Deliverable | Acceptance gate |
| --- | --- | --- | --- |
| R01 | Implemented & locally verified | Incremental caller-input sessions and bounded batches in Rust, Node, and C ABI | Slow writers backpressure input; early close/disposal frees every session; batch fixtures pass in Rust and Node |
| R02 | Explicit capability boundary | Streaming structured tables through a proven bounded backend or narrowly scoped OOXML adapter | Currently rejected with `CAPABILITY_MISMATCH` before source execution because `rust_xlsxwriter` retains per-cell table metadata |
| R03 | Implemented & locally verified | Worksheet/workbook overflow splitting | Headers and styles repeat; every source record appears exactly once; generated names and paths cannot collide |
| R04 | Implemented & locally verified | Declarative style rules and explicit row/cell overrides | Deterministic precedence/reset behavior; bounded rule depth/style count; rules work before streaming rows flush |
| R05 | Implemented & locally verified | Complete layout and formatting capability matrix | Printing, gridlines, tab colors, per-edge borders and fill patterns have fixtures, validation, and docs |
| R06 | Implemented & locally verified | Configurable timezone/precision/binary policies | Exact values and policies round-trip; unsupported or lossy conversions fail unless explicitly requested |
| R07 | External verification pending | Private CA/client-certificate and authentication matrix | Requires designated test servers and certificates |
| R08 | Partial / external verification pending | Isolation, lock/idle timeouts, direct/portal modes and bounded connect retry policy | Requires live database verification |
| R09 | Partial | Shared decoded-byte permits, spool quotas and bounded multi-workbook concurrency | Per-stream queue/row/batch budgets and export deadlines are implemented; cross-workbook process quotas remain open |
| R10 | Implemented & locally verified | Portable progress iterators and richer error context across all bindings | `subscribe_progress`, `ne_job_next_event_json`, Node `job.events()`, and contextual `ExportError` fields implemented |
| R11 | Implemented & locally verified | Buffered edit builders and append-only public writers | `WorkbookBuilder`, `SheetBuilder`, and `AppendWorkbookWriter` exercise reference-like cell writing and streaming |
| R12 | Pending | Optional database feature builds and domain/adapter crate separation | Current isolation is by modules plus `QueryDriver` trait |
| R13 | Partial | Generated C/TypeScript declarations and ABI/schema compatibility checks | Schema is generated via `xtask schema`; C header and TypeScript declarations are synchronized and verified by tests |
| R14 | Partial (clean-checkout fixed) | Full dependency-license bundle, SBOM and compatibility manifest | Tracked `licenses/msexcel-builder.LICENSE`, nested checksums, and manifest hashes are implemented; full third-party SBOM remains open before binary distribution |
| R15 | External verification pending | Cross-platform packages, installation tests and signing | Windows x64 is verified locally; Linux/macOS runners and signing remain release steps |
| R16 | External verification pending | Live database certification | Ignored live PostgreSQL and SQL Server tests await designated test connection strings |
| R17 | Pending | Performance and resilience acceptance | 10k/100k/1m-row benchmark suite and fault injection remain open |
| R18 | Partial | Desktop interoperability and complete cookbook | API, migration (`oldexcel`), typed-column, report-header, and telemetry guides exist; desktop Excel/LibreOffice smoke opens remain external |
| R19 | Optional proposal P01 | Native OTLP exporter and telemetry rotation | Optional assistant proposal; not required for baseline closure |
| R20 | Pending external gates | Release candidate and registry publication | Depends on external gates R07/R08/R15/R16/R17/R18 |
| R21 | Implemented & locally verified | Full object projection and column policies | Column inference, `path` selectors, case-collision rejection, default/null/extra-field policies, and transforms implemented |
| R22 | Implemented & locally verified | Remaining writer/output options | `returnBytes` with `maxOutputBytes`, buffered/sampled `autofit`, and `syncData` implemented |
| R23 | Implemented & locally verified | Clean-checkout build and CLI argument contract | `xtask` uses tracked `licenses/msexcel-builder.LICENSE` (with `oldexcel/LICENSE` fallback) and rejects unsupported arguments |
| R24 | Implemented & locally verified | Remaining configuration and architectural contracts | Workbook `metadata`, `defaults` inheritance, and convenience export methods implemented |
| R25 | Implemented & locally verified | Remaining lifecycle and native API contracts | `capabilities`, event polling, `exportTimeoutMs`, contextual errors, and C# `SafeHandle`/async helpers implemented |

## Verification evidence (November 1, 2026)

- 34 Rust and C ABI tests passed (`export`, `report_header`, `typed_columns`, `streaming`, `telemetry`, `features`, and `native-excel-ffi` unit tests); 2 live database tests remain explicitly ignored pending connection strings.
- 8 Node integration tests passed using the packaged native addon (`tests/node.test.cjs`), including `capabilities()`, `openInputStream()` + `rowStream`, `splitWorksheet`, and declarative `rules`.
- C# example (`examples/dotnet`) and `cargo clippy --workspace --all-targets -- -D warnings` passed with zero warnings.
- `cargo xtask build-all` succeeded without depending on `references/LICENSE`.
