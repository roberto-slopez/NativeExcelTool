# Third-party notices

The optional local `msexcel-builder/` reference directory contains msexcel-builder by Charlie Zheng under the MIT license. This directory is excluded from Git. Its original license notice is tracked in `licenses/msexcel-builder.LICENSE` so clean checkouts can package attribution without the local reference directory. The Rust implementation uses msexcel-builder as behavioral guidance; it does not execute the legacy JavaScript or CoffeeScript implementation.

Rust dependencies and their exact versions are recorded in `Cargo.lock`. Their declared licenses can be inspected with `cargo metadata --locked --format-version 1`. Primary dependencies include rust_xlsxwriter, Tokio, tokio-postgres, tokio-postgres-rustls, Tiberius, napi-rs, Serde, rustls, rust_decimal, and tempfile. Their original licenses apply to their respective sources.

The alpha packages include the project license and the tracked msexcel-builder notice. A complete transitive dependency license bundle and SBOM remain open release work; the existing notices do not constitute a complete dependency license audit. See `docs/REMAINING_PLAN.md` for the remaining release gates.
