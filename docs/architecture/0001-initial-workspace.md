# ADR 0001: Initial workspace and ownership

Status: accepted for the initial implementation.

The initial implementation consolidates domain, query adapters, and XLSX writing into modules of `native-excel`. Native bindings and build tooling are separate crates. This avoids publishing unstable internal crate APIs while the contracts are still being exercised. Splitting the core into the larger planned workspace remains a follow-up, not an existing guarantee.

`ExportRequest`, `WorkbookSpec`, `WorksheetSpec`, `Column`, and `Style` are parameter objects. `ExcelExporter` is the public facade. `QueryDriver` is the dependency-inversion port; PostgreSQL and SQL Server implementations supply producer strategies. `QueryService::stream_with` supports injected implementations and deterministic source-contract tests. Native binding crates only marshal configuration, results, handles, and cancellation.

The producer owns the database connection and any stream borrowing it. Only decoded owned rows cross the bounded channel. The public query session is not self-referential. A cancellation guard aborts producers if opening a session is cancelled before metadata arrives; session drop aborts an established producer. PostgreSQL's connection-driving task has its own abort-on-drop guard.

An export worker owns each XLSX workbook and its worksheets. It executes on Tokio's blocking pool and uses the caller/runtime handle for query consumption; it does not block the JavaScript event loop. It applies final cell formats before a streaming row flush. Workbook output is staged in the destination directory and published after successful finalization. There is no multi-file atomic transaction.

The C ABI owns a process-lifetime Tokio runtime, monotonically assigned handle registry, and explicitly freed output buffers. It exposes C-layout pointer/length buffers rather than Rust allocations by value. Node uses napi-rs and a separate DLL entry point. The C ABI DLL does not depend on Node.

Styles use immutable configuration, named-scope resolution, property overlay, and backend format deduplication. The current API intentionally rejects unknown features; future changes must update schema, documentation, declarations, and contract tests together.
