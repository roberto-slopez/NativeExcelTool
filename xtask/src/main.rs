use sha2::{Digest, Sha256};
use std::io::Read;
use std::{path::PathBuf, process::Command};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("schema") {
        std::fs::create_dir_all(root.join("schemas"))?;
        std::fs::write(
            root.join("schemas/export-request.schema.json"),
            serde_json::to_string_pretty(&native_excel::request_schema())?,
        )?;
        return Ok(());
    }
    if args.first().map(String::as_str) != Some("build-all") {
        return Err("Usage: cargo xtask schema | build-all [--release]".into());
    }
    let release = args.iter().any(|s| s == "--release");
    if std::env::consts::OS != "windows" || std::env::consts::ARCH != "x86_64" {
        return Err("Initial build-all packaging supports Windows x64 only".into());
    }
    let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.current_dir(&root).args([
        "build",
        "-p",
        "native-excel",
        "-p",
        "native-excel-node",
        "-p",
        "native-excel-ffi",
        "--locked",
    ]);
    if release {
        cmd.arg("--release");
    }
    if !cmd.status()?.success() {
        return Err("Cargo build failed".into());
    }
    let profile = if release { "release" } else { "debug" };
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let artifacts = target.join(profile);
    let dist = root.join("dist").join(profile);
    std::fs::create_dir_all(&dist)?;
    for (from, to) in [
        ("native_excel_node.dll", "native_excel.node"),
        ("native_excel_ffi.dll", "native_excel_ffi.dll"),
        ("native-excel.exe", "native-excel.exe"),
        ("libnative_excel.rlib", "libnative_excel.rlib"),
    ] {
        std::fs::copy(artifacts.join(from), dist.join(to))?;
    }
    std::fs::write(
        dist.join("export-request.schema.json"),
        serde_json::to_string_pretty(&native_excel::request_schema())?,
    )?;
    for file in [
        "bindings/node/index.cjs",
        "bindings/node/index.d.ts",
        "bindings/c/native_excel.h",
        "bindings/dotnet/NativeExcelNative.cs",
        "bindings/node/package.json",
        "LICENSE",
        "THIRD_PARTY_NOTICES.md",
    ] {
        std::fs::copy(
            root.join(file),
            dist.join(PathBuf::from(file).file_name().unwrap()),
        )?;
    }
    std::fs::write(
        dist.join("manifest.json"),
        serde_json::to_string_pretty(
            &serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"abiVersion":1,"schemaVersion":1,"profile":profile,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"nodeApi":6}),
        )?,
    )?;
    std::fs::create_dir_all(dist.join("licenses"))?;
    std::fs::copy(
        root.join("references/LICENSE"),
        dist.join("licenses/msexcel-builder.LICENSE"),
    )?;
    let mut checksums = String::new();
    let mut files: Vec<_> = std::fs::read_dir(&dist)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && e.file_name() != "checksums.txt")
        .collect();
    files.sort_by_key(|e| e.file_name());
    for entry in files {
        let mut input = std::fs::File::open(entry.path())?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        checksums.push_str(&format!(
            "{}  {}\n",
            hash.finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            entry.file_name().to_string_lossy()
        ));
    }
    std::fs::write(dist.join("checksums.txt"), checksums)?;
    println!("Artifacts: {}", dist.display());
    Ok(())
}
