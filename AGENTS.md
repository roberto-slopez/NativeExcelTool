# Repository guidance for agents

## Purpose and current scope

NativeExcelTool is a Rust engine for producing Excel `.xlsx` reports from object records, caller-provided row streams, and PostgreSQL or SQL Server queries. The same engine is exposed through a Rust API, a command-line executable, a Node-API addon, and a C ABI used by C# P/Invoke. Excel automation is not part of the implementation.

The current version is `0.1.0-alpha.1`. Windows x64 with the MSVC target is the supported packaging environment. The alpha has runnable C# and Node.js examples and CI checks for debug and release. Do not describe it as an RC, a production-certified release, or a verified cross-platform distribution. Read the manifests and release notes before changing version-specific guidance.

## Start here

1. Inspect `git status --short` and the current branch. Preserve unrelated work already present in the shared workspace.
2. Read `README.md` for build prerequisites, public usage, and examples.
3. Read `docs/architecture/0001-initial-workspace.md` before changing module boundaries, ownership, or concurrency.
4. Read `docs/API.md` and the relevant guide under `docs/guides/` before changing the public contract.
5. Consult `docs/REMAINING_PLAN.md` for implementation boundaries and outstanding release gates.
6. Check `.github/workflows/windows.yml`, `rust-toolchain.toml`, and the Cargo manifests for the actual validation environment.

Some local checkouts contain ignored `docs/IMPLEMENTATION_PLAN.md` and `docs/IMPLEMENTATION_STATUS.md` files. They are optional historical planning material. Shared instructions, builds, documentation links, and CI must work without them.

## Repository map

| Path | Responsibility |
| --- | --- |
| `Cargo.toml`, `Cargo.lock` | Workspace membership, shared package version, dependencies, and locked dependency graph |
| `rust-toolchain.toml` | Pinned Rust toolchain and required components |
| `.cargo/config.toml` | `cargo xtask` alias and workspace target directory |
| `crates/native-excel/src/lib.rs` | Public Rust exports; most implementation modules remain private |
| `crates/native-excel/src/model.rs` | Request/response types, validation-facing options, errors, and JSON schema generation |
| `crates/native-excel/src/writer.rs` | Export validation, job lifecycle, style compilation, worksheet writing, splitting, and output publication |
| `crates/native-excel/src/values.rs` | Column-aware conversion, tagged values, precision policies, and typed cell writing |
| `crates/native-excel/src/header.rs` | Report-header layout, merges, and ordered rendering |
| `crates/native-excel/src/builder.rs` | Sparse buffered workbook/sheet builders, A1 coordinates, and append-only writing |
| `crates/native-excel/src/query.rs` | Query service, driver abstraction, PostgreSQL/SQL Server adapters, schema delivery, and bounded row channels |
| `crates/native-excel/src/input.rs` | Registered caller-input sessions, bounded batches, finishing, and cancellation |
| `crates/native-excel/src/telemetry.rs` | Per-export JSONL/Prometheus output and host-provided telemetry sinks |
| `crates/native-excel/src/cli.rs`, `src/bin/native-excel.rs` | CLI parsing, progress reporting, telemetry opt-in, and process entry point |
| `crates/native-excel-node/` | napi-rs bindings; the compiled DLL is packaged as `native_excel.node` |
| `crates/native-excel-ffi/` | C-layout buffers, native handles, process-lifetime runtime, and C ABI functions |
| `bindings/node/` | Manually maintained CommonJS wrapper, TypeScript declarations, and package metadata |
| `bindings/c/native_excel.h` | Manually maintained C declarations and ownership contract |
| `bindings/dotnet/NativeExcelNative.cs` | Manually maintained P/Invoke declarations, JSON decoding, safe handles, and managed convenience methods |
| `xtask/` | Schema generation and Windows x64 packaging, including manifest and SHA-256 checksums |
| `schemas/export-request.schema.json` | Tracked, generated JSON schema for serialized export requests |
| `crates/native-excel/tests/` | Rust integration tests, independent XLSX reads, and optional live database tests |
| `tests/node.test.cjs` | Integration tests using the packaged native addon |
| `examples/configs/` | Runnable declarative request fixtures |
| `examples/node/export.cjs`, `examples/dotnet/` | Self-contained sales examples with ten in-memory orders each |
| `scripts/` | Example execution, independent XLSX verification, and alpha ZIP packaging |
| `licenses/`, `THIRD_PARTY_NOTICES.md` | Tracked attribution used by clean-checkout builds and release packages |
| `.github/workflows/windows.yml` | Windows debug/release matrix and downloadable alpha artifact |

The domain types, query adapters, and XLSX implementation currently share the `native-excel` crate. Do not assume that separate domain, application, and infrastructure crates already exist. Binding crates marshal requests, results, handles, and cancellation; implement export behavior in the core crate.

## Original msexcel-builder reference

- `msexcel-builder/` is the canonical name of the optional local reference directory. Preserve the original upstream project name in code, documentation, test names, and attribution.
- The directory is ignored by the parent repository and can contain its own Git repository and `node_modules`. Treat it as third-party reference material. Do not edit its source or dependencies as part of maintaining NativeExcelTool.
- The Rust engine does not execute or load the JavaScript/CoffeeScript reference implementation.
- Packaging first uses `licenses/msexcel-builder.LICENSE`. The local `msexcel-builder/LICENSE` is only a fallback. A clean checkout must package successfully without the reference directory.
- Preserve upstream license text and attribution. Do not require an npm install in the reference directory for native builds or tests.

## Architecture and behavior to preserve

### Export pipeline

`ExportRequest` is deserialized and validated before an export starts. `ExcelExporter` creates an `ExportJob`; workbook writing runs through Tokio's blocking pool. Async query producers retain their driver connections and borrowed streams, while owned decoded rows cross bounded channels. Keep database protocol objects inside their producer future.

Output is staged in the destination directory and published after successful finalization. Preserve overwrite protection, cleanup on failure/cancellation, and the distinction between completed files and unpublished staging files. Publication is per file; there is no atomic transaction across several workbooks. With `continueOnError`, callers must inspect workbook errors in the returned report.

`WorkbookBuilder` and `SheetBuilder` offer buffered random-access editing. `AppendWorkbookWriter` writes rows sequentially. Keep these contracts distinct from the declarative export facade rather than routing every operation through an unnecessary binding layer.

### Schema, cells, and styles

- Rust identifiers follow Rust naming conventions; serialized request properties use the existing Serde casing, usually camelCase. Preserve external JSON names unless an intentional contract change requires migration.
- Unknown request properties are rejected where the model declares `deny_unknown_fields`. Do not silently accept unsupported options.
- Public cell/layout coordinates are generally 1-based; XLSX backend coordinates are generally 0-based. Check each API before translating coordinates. Preserve Excel limits of 16,384 columns and 1,048,576 rows, including header offsets.
- `totalRows`, when provided, is an exact record count, not an estimate.
- Explicit formulas and literal strings have different behavior. Preserve tagged/declared formula handling and do not turn ordinary text into formulas.
- Preserve exact-text policies for large integers and decimals. JavaScript numbers can lose precision before serialization; the wrapper encodes `bigint` as tagged values.
- Styles combine scopes, named styles, inline formatting, rules, and explicit cell overrides. Preserve precedence, `resetFields`, format deduplication, and application of a format before a streaming row is flushed.
- Structured Excel tables currently require buffered writing. Streaming tables are rejected with `CAPABILITY_MISMATCH`; do not advertise support or silently switch to an unbounded implementation.
- Overflow splitting must preserve every record exactly once, repeat the required headers/styles, and reject conflicting generated names and paths.

### Streaming, queries, and cancellation

- Preserve bounded channels and the configured row/batch limits. Object-record JSON is materialized in native memory; use row streams or query sources for incremental input.
- Queue limits are not a hard bound on total process memory. Drivers, in-flight values, workbooks, and output buffers have additional allocations.
- Query schemas must contain unique, nonempty labels. Use caller-supplied query text and bound parameters; do not interpolate field names or parameter values into SQL.
- PostgreSQL uses a read-only transaction and portal fetches. SQL Server executes with the permissions of the supplied reporting account; do not describe both adapters as enforcing read-only SQL.
- Preserve TLS verification defaults. Development plaintext options do not imply certificate-verification bypasses or private-CA support.
- A dropped `QuerySession` requests producer abortion. Task/connection cleanup occurs as the future is dropped; a synchronous drop is not an awaited cleanup barrier.
- Distinguish finishing an input stream, which allows queued rows to drain, from releasing an unfinished stream, which requests cancellation.
- Job status is exposed through Tokio watch channels. Subscribers can observe coalesced updates; this is not a durable, lossless event log.

### Native and managed ownership

- The C ABI owns its runtime, handle registries, and output allocations. ABI and schema versions are currently both 1; package version and ABI version are separate concepts.
- JSON functions return 0 on success and 1 on failure. A valid output location receives an owned length-delimited UTF-8 JSON buffer on either path. The data is not NUL-terminated.
- Free every returned buffer exactly once with `ne_buffer_free`, including error responses. Do not free it using C, C++, Node, or managed allocators.
- Release each job, query, and input handle through its matching function. Invalid/released handles produce structured errors.
- Validate raw pointer contracts and explain each unsafe block with a local `SAFETY:` comment. Never convert an arbitrary foreign pointer into an owned Rust allocation.
- Raw C calls that wait for rows, capacity, events, or job results can block. Keep them off UI/event-loop threads. The C# convenience layer moves selected blocking calls to managed workers; query opening in `QueryStreamAsync` still happens synchronously.
- C# uses `CallingConvention.Cdecl`, C-layout buffers, and safe handles. Its process architecture must match the native DLL. The native DLL is not a managed assembly.
- Node uses napi-rs async integration. Keep the C ABI library independent of Node and preserve async-iterator cleanup on early exit.

### Telemetry and error reporting

Telemetry is opt-in per export. Built-in sinks write local files; host-provided sinks may bridge to external collectors. Preserve absolute/distinct destination validation and avoid introducing global subscribers or implicit environment configuration into the engine.

Do not log credentials, connection strings, SQL parameter values, or cell contents through debug output or errors. Keep `QueryRequest` free of a derived `Debug` implementation. Preserve structured error codes and contextual fields. Telemetry failures must not silently change export success semantics.

## Language and comment conventions

Use professional English for maintained code comments, API documentation, and repository documentation. Describe behavior and constraints directly. Do not use emojis, decorative markers, jokes, promotional claims, conversational notes, or comments that merely repeat an obvious statement. Unicode data is valid in workbook contents and test fixtures; this rule is about documentation tone, not a restriction on supported data.

- **Rust:** use `//!` for module/crate documentation, `///` for API documentation, and `//` for implementation rationale. Start API docs with a concise summary, then a blank documentation line before details. Add `# Errors`, `# Panics`, `# Safety`, and runnable examples where relevant. Unsafe API docs must state pointer validity, lifetimes, allocation ownership, and release requirements. Comments must match cancellation and blocking behavior precisely. Use rustdoc links for Rust items and `cargo fmt` for layout.
- **C#:** use XML documentation with `/// <summary>`, `<remarks>`, `<param>`, `<returns>`, `<exception>`, and `<see cref="..."/>` where useful. Document buffer ownership, disposal, blocking, and cancellation accurately. Do not use Markdown as a substitute for XML tags inside C# API documentation.
- **JavaScript/TypeScript:** preserve the CommonJS wrapper and examples. Use JSDoc for public documentation and `//` for implementation rationale. Distinguish wrapper-only values such as `bigint` from the serialized JSON schema. Rust naming and external JS names need not be identical; preserve the established interface.
- **C headers:** use `/* ... */` or `/** ... */` for ownership and ABI contracts. Keep declarations synchronized with Rust exports and preserve C++ `extern "C"` compatibility.
- **PowerShell:** use `#` for focused implementation comments and comment-based help when a script needs a documented interface. Check native process exit codes and preserve the caller's environment/location in reusable scripts. Prefer literal paths for file operations.
- Do not add blanket warning suppressions, invent safety guarantees, or describe pending work as implemented to make documentation appear complete.

Reference conventions: [Rust documentation guidance](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html) and [C# XML documentation](https://learn.microsoft.com/en-us/dotnet/csharp/language-reference/xmldoc/).

## Toolchain and local builds

Run commands from the repository root. The current pin is Rust 1.99.1 with `rustfmt` and Clippy; consult `rust-toolchain.toml` if it changes. Native packaging requires Windows x64 and Visual Studio C++ build tools. CI uses Node.js 24 and .NET 10 SDK. Running prebuilt packages requires the Visual C++ v14 x64 runtime, not the full build tools.

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked -p native-excel -p native-excel-ffi --tests
cargo run --locked -p xtask -- build-all
```

Optimized binaries and Rust tests:

```powershell
cargo test --locked -p native-excel -p native-excel-ffi --tests --release
cargo run --locked -p xtask -- build-all --release
```

`cargo xtask build-all` is an alias for `cargo run -p xtask -- build-all`. The inner native build already uses `--locked`; the explicit `cargo run --locked` form also locks compilation of the packaging tool. To specify the packaging target, append `--target x86_64-pc-windows-msvc` after `build-all`. Other target triples and non-Windows/x64 packaging are rejected.

Build outputs go to `target/` and `dist/debug` or `dist/release`. The `CARGO_TARGET_DIR` environment variable can override artifact lookup. Build the matching native profile before running a binding example or test.

If the local rustup launcher cannot run, use an already installed toolchain whose `rustc --version` matches the pin. Set `RUSTC` and `RUSTDOC` to its executables and prepend its `bin` directory to `PATH` for that process. A workspace-local `CARGO_HOME` such as `.cargo-online` can avoid writes to a restricted global cache. Do not change global installations to work around a workspace issue. Use offline mode only when required dependencies are already cached.

## Validation by change area

Run focused checks for the affected behavior. Use the full CI sequence for native/API/packaging changes; avoid unrelated live services or new test dependencies for comment-only edits.

| Change | Relevant validation |
| --- | --- |
| Rust implementation or safety documentation | Formatting, Clippy, affected Rust tests; compile rustdoc when documentation changes |
| Cell types, styles, headers, layout, splitting | `export`, `typed_columns`, `report_header`, or `features` integration tests; inspect output with calamine or XLSX XML, not just file size |
| Query lifecycle/backpressure | `streaming` and relevant validation tests; injected `QueryDriver` tests before optional live databases |
| Telemetry/CLI | `telemetry` integration tests, including opt-in and credential redaction behavior |
| FFI exports, buffers, or handles | `native-excel-ffi` tests, C header review, native packaging, and C# smoke execution |
| Node wrapper or addon | Matching native package followed by `node --test tests/node.test.cjs` |
| C# wrapper or example | Matching native package followed by `dotnet build`/`dotnet run`; compile XML documentation when changing XML comments |
| Packaging or examples | Build native binaries, run both examples, verify XLSX contents, then package; check extracted bundle paths and checksums when packaging layout changes |

Rust documentation checks:

```powershell
$env:RUSTDOCFLAGS = '-D warnings'
cargo doc --locked --workspace --no-deps
```

Scope environment changes to the verification process or restore the previous value afterward.

Binding tests and examples in debug:

```powershell
$env:NATIVE_EXCEL_PROFILE = 'debug'
node --test tests/node.test.cjs
./scripts/run-examples.ps1 -Profile debug
```

Use `NATIVE_EXCEL_PROFILE=release` for optimized Node tests and `./scripts/run-examples.ps1 -Profile release` for optimized examples. The C# project selects the native release DLL with `--configuration Release`; its default configuration selects debug. There is no npm installation step for the packaged wrapper.

Both examples construct ten sales orders in their own language. They produce `output/orders-node.xlsx` and `output/orders-dotnet.xlsx` with a merged title and a typed summary calculated from those orders. Verification reads the XLSX ZIP/XML independently and checks the header merges, frozen rows, the `SalesOrders` table at `A7:H17`, summary values, and cross-language equality across all 101 populated cells. The optional `--table-only` mode writes a separate workbook with the table at `A1:H11`. Preserve these entry points when editing examples.

`tests/databases.rs` contains ignored live tests. They require `NATIVE_EXCEL_POSTGRES` and `NATIVE_EXCEL_SQLSERVER` connection strings with trusted TLS. Run an individual test with `-- --ignored` only when the appropriate environment is supplied. Never commit credentials or report ignored tests as passed live integration.

## Public contract changes and generated files

For a new option or changed public behavior, review all applicable surfaces together:

1. Rust model, validation, writer/conversion logic, and capabilities.
2. Node addon and `bindings/node/index.cjs` when marshalling changes.
3. `bindings/node/index.d.ts` for manually maintained TypeScript declarations.
4. Rust C ABI exports, `bindings/c/native_excel.h`, and `bindings/dotnet/NativeExcelNative.cs` when native functions change.
5. Generated schema, API guide, runnable examples, and meaningful regression tests.

Generate the tracked schema from the Rust model rather than editing its JSON manually:

```powershell
cargo run --locked -p xtask -- schema
```

Review the schema diff. The C header, TypeScript declarations, and C# wrapper are currently maintained manually; schema generation does not update them. Do not edit copies in `dist/` or test build outputs in `bin/` and `obj/` as source changes.

## Packaging, CI, and releases

The workflow checks debug and release independently: format, Clippy, Rust tests, native packaging, Node tests, both examples, and independent workbook verification. Its release matrix entry additionally runs `scripts/package-alpha.ps1` and uploads `windows-x64-alpha` containing the distribution ZIP and its SHA-256 file.

`xtask build-all` copies the Rust library artifact, CLI, Node addon, C ABI DLL, wrappers, schema, package metadata, and attribution into `dist/<profile>`. It generates a capability/version manifest and recursive per-file SHA-256 checksums, excluding the checksum file itself.

`scripts/package-alpha.ps1` currently requires an alpha version and Windows x64 release manifest. It packages native binaries, runnable example sources, interop code, the quickstart, verification scripts, and generated workbook samples. Run the release examples first so `output/orders-node.xlsx` and `output/orders-dotnet.xlsx` exist. The package script does not publish a GitHub release.

The alpha quickstart is `docs/ALPHA_QUICKSTART.md`; versioned release notes live under `docs/releases/`. Keep `Cargo.toml` workspace version, local workspace entries in `Cargo.lock`, Node package version, release notes, and tag consistent when preparing a new version. Revisit the alpha-only package script before preparing an RC or stable version.

The existing `v0.1.0-alpha.1` tag and release identify the published alpha. Do not rewrite that tag or replace its assets to include unrelated later maintenance. Follow the user's requested Git/release scope; ordinary documentation edits do not require a new release. If creating a working branch, use the `codex/` prefix. Integrate authorized changes before deleting a temporary branch, and preserve tags/releases independently of branch cleanup.

## Known boundaries and maintenance practices

- Live database certification, private-CA/client-certificate coverage, broader database execution options, shared process-wide resource quotas, performance/resilience benchmarks, full dependency license inventory/SBOM, signing, and Linux/macOS distribution remain open work. Use the tracked roadmap for current status.
- Desktop Excel/LibreOffice acceptance is separate from automated ZIP/XML and independent-reader verification. Do not claim a desktop smoke open based solely on a file being generated.
- Keep dependency updates separate from unrelated comment or naming cleanup. Use the lockfile and existing tools; do not install reference-project dependencies to validate this engine.
- Do not commit `target/`, `dist/`, `output/`, generated `.xlsx` files, native binaries, caches, `bin/`, `obj/`, or the local `msexcel-builder/` reference repository. Retain the explicitly tracked `crates/native-excel/src/bin/native-excel.rs` source despite the general `bin/` ignore rule.
- Before finishing, review `git diff --check`, changed files, stale names, and affected documentation links. Report concrete validation results and remaining environmental limits without claiming checks that were not run.
