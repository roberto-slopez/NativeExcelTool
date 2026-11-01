# Remaining implementation and release plan

This tracked checklist completes the roadmap with concrete closure steps. It distinguishes shipped code from work that still requires implementation or external verification. It does not certify unfinished capabilities as complete.

## Completed in the telemetry increment

- [x] Export telemetry disabled by default, with explicit per-job absolute log paths.
- [x] Sanitized JSONL lifecycle/sampled events, including terminal flush.
- [x] Optional Prometheus textfile snapshots and a Rust custom-sink interface.
- [x] An actual Rust command with unconditional progress feedback and opt-in console telemetry.
- [x] Latest-value async progress subscriptions and telemetry error counters.
- [x] Tests for default-off behavior, destination validation, redaction, metrics, custom sinks, failures, and CLI flag gating.

## Ordered closure steps

| Step | Deliverable | Acceptance gate |
| --- | --- | --- |
| R01 | Incremental caller-input sessions and bounded batches in Rust, Node, and C ABI | Slow writers backpressure input; early close/disposal frees every session; the same batch fixture exports through all bindings |
| R02 | Streaming structured tables through a proven bounded backend or narrowly scoped OOXML adapter | Table metadata does not grow per data cell; Excel opens the output without repair; no implicit buffered fallback |
| R03 | Worksheet/workbook overflow splitting | Headers and styles repeat; every source record appears exactly once; generated names and paths cannot collide |
| R04 | Declarative style rules and explicit row/cell overrides | Deterministic precedence/reset behavior; bounded rule depth/style count; rules work before streaming rows flush |
| R05 | Complete layout and formatting capability matrix | Printing, gridlines, themes, per-edge borders and fill patterns have fixtures, validation, and mode-specific docs |
| R06 | Configurable timezone/precision/binary policies | DST/boundary cases and exact values round-trip; unsupported or lossy conversions fail unless explicitly requested |
| R07 | Private CA/client-certificate and authentication matrix | Trusted test certificates succeed; bad identities fail; every documented auth mode passes on its supported platforms |
| R08 | Isolation, lock/idle timeouts, direct/portal modes and optional pooling | Correct consistency scope; cancelled/partially consumed connections are discarded; unsupported options fail |
| R09 | Shared decoded-byte permits, spool quotas and bounded multi-workbook concurrency | Slow-consumer and low-disk tests enforce budgets across all active jobs, including finalization |
| R10 | Portable progress iterators and richer error context across all bindings | Bounded/coalesced delivery, terminal states, config paths and row/field identifiers; no secrets in telemetry/errors |
| R11 | Buffered edit builders and append-only public writers | Production APIs exercise reference-like cell writing and incremental typed records without exposing backend internals |
| R12 | Optional database feature builds and domain/adapter crate separation | No-DB, PostgreSQL-only, SQL Server-only and both-driver builds pass; public API compatibility is documented |
| R13 | Generated C/TypeScript declarations and ABI/schema compatibility checks | CI detects declaration drift; exact calling conventions, sizes, enums, ownership and additive schema changes are verified |
| R14 | Full dependency-license bundle, SBOM and compatibility manifest | Every shipped dependency has attribution/license evidence; artifacts have verified checksums and compiler/target metadata |
| R15 | Cross-platform packages, installation tests and signing | Clean consumers load the correct target binary; Windows/Linux/macOS smoke jobs pass; signing follows the chosen release policy |
| R16 | Live database certification | Disposable PostgreSQL and SQL Server instances cover multiple versions, TLS/auth, zero rows, aliases, decimals, cancellation and midstream failures |
| R17 | Performance and resilience acceptance | Published 10k/100k/1m-row RSS/disk/throughput results; slow disks/consumers, finalization, cancellation and failure injection pass |
| R18 | Desktop interoperability and complete cookbook | Excel/LibreOffice opens have no repair messages; every documented public option has an executable example |
| R19 | Optional native OTLP exporter and telemetry rotation policies | Explicit activation, bounded delivery/shutdown, endpoint configuration, no sensitive attributes, exporter failure tests; existing telemetry remains off by default |
| R20 | Release candidate and registry publication | All required gates above are evidenced; known exclusions published; actual Cargo/npm/NuGet publication is an explicit release action |

R01-R05 and R09-R13 can proceed with deterministic local fixtures. R07-R08 and R16 require designated test servers/certificates. R15 and R18 require the corresponding runners and spreadsheet applications. Publication/signing credentials must not be embedded in source or logs.

The existing live database tests remain explicitly ignored until test connection strings are supplied. Access to Docker was previously unavailable in this environment. These are external verification limitations; they do not imply that all other roadmap items are complete.

The current implementation and telemetry contract are documented in `API.md`, `guides/typed-columns.md`, and `guides/telemetry.md`. Update this checklist and the implementation status as gates are actually met.

## Telemetry increment verification

23 Rust/C ABI tests and 6 Node tests passed. The C# example generated a report with JSONL and Prometheus outputs enabled by absolute paths. Clippy passed for all targets. Debug and optimized native libraries and the Windows command were built; the release command's separate progress/telemetry stderr and JSON stdout were checked, and release checksums were verified. Two live database tests remain ignored pending designated connections; these telemetry results do not replace the database/release gates above.
