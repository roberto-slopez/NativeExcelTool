# Third-party notices

The local `msexcel-builder` reference directory (formerly `msexcel-builder`, excluded from Git) contains msexcel-builder by Charlie Zheng under the MIT license. Its original license notice is tracked in `licenses/msexcel-builder.LICENSE` so clean checkouts can package attribution without requiring the ignored `oldexcel` directory. The new Rust source uses the reference as behavioral guidance; the legacy JavaScript/CoffeeScript implementation is not executed.

Rust dependencies and their exact versions are recorded in `Cargo.lock`. Their declared licenses can be inspected with `cargo metadata --locked --format-version 1`. Primary dependencies include rust_xlsxwriter, Tokio, tokio-postgres, tokio-postgres-rustls, Tiberius, napi-rs, Serde, rustls, rust_decimal, and tempfile. Their original licenses apply to their respective sources.

A complete redistributable license bundle and SBOM must be generated and reviewed before publishing binary packages. The current local build output is not a certified public distribution.
