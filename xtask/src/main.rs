use sha2::{Digest, Sha256};
use std::io::Read;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn collect_files(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(base, &path, out)?;
        } else if path.is_file()
            && path.file_name().and_then(|s| s.to_str()) != Some("checksums.txt")
        {
            out.push(path.strip_prefix(base).unwrap().to_path_buf());
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut input = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("schema") => {
            if args.len() > 1 {
                return Err("Usage: cargo xtask schema".into());
            }
            std::fs::create_dir_all(root.join("schemas"))?;
            std::fs::write(
                root.join("schemas/export-request.schema.json"),
                serde_json::to_string_pretty(&native_excel::request_schema())?,
            )?;
            return Ok(());
        }
        Some("build-all") => {}
        _ => {
            return Err(
                "Usage: cargo xtask schema | build-all [--release] [--target x86_64-pc-windows-msvc]"
                    .into(),
            );
        }
    }

    let mut release = false;
    let mut target_triple: Option<String> = None;
    let mut idx = 1;
    while idx < args.len() {
        match args[idx].as_str() {
            "--release" => {
                release = true;
                idx += 1;
            }
            "--target" => {
                let value = args
                    .get(idx + 1)
                    .ok_or("--target requires a target triple")?;
                if value != "x86_64-pc-windows-msvc" {
                    return Err(format!(
                        "Unsupported target {value}; only x86_64-pc-windows-msvc is supported"
                    )
                    .into());
                }
                target_triple = Some(value.clone());
                idx += 2;
            }
            other => {
                return Err(format!(
                    "Unknown argument '{other}'. Usage: cargo xtask schema | build-all [--release] [--target x86_64-pc-windows-msvc]"
                )
                .into());
            }
        }
    }

    if std::env::consts::OS != "windows" || std::env::consts::ARCH != "x86_64" {
        return Err("Initial build-all packaging supports Windows x64 only".into());
    }
    let cargo_bin = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(&cargo_bin);
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
    if let Some(triple) = &target_triple {
        cmd.args(["--target", triple]);
    }
    if !cmd.status()?.success() {
        return Err("Cargo build failed".into());
    }
    let profile = if release { "release" } else { "debug" };
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let artifacts = match &target_triple {
        Some(triple) => target.join(triple).join(profile),
        None => target.join(profile),
    };
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
    std::fs::create_dir_all(dist.join("licenses"))?;
    let tracked_license = root.join("licenses/msexcel-builder.LICENSE");
    let reference_license = root.join("msexcel-builder/LICENSE");
    let license_source = if tracked_license.exists() {
        tracked_license
    } else if reference_license.exists() {
        reference_license
    } else {
        return Err(
            "Missing msexcel-builder license file in licenses/msexcel-builder.LICENSE".into(),
        );
    };
    std::fs::copy(
        license_source,
        dist.join("licenses/msexcel-builder.LICENSE"),
    )?;

    let mut artifact_hashes = serde_json::Map::new();
    for entry in [
        "native_excel.node",
        "native_excel_ffi.dll",
        "native-excel.exe",
        "libnative_excel.rlib",
        "export-request.schema.json",
        "licenses/msexcel-builder.LICENSE",
    ] {
        artifact_hashes.insert(
            entry.to_string(),
            serde_json::Value::String(sha256_file(&dist.join(entry))?),
        );
    }

    std::fs::write(
        dist.join("manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "abiVersion": 1,
            "schemaVersion": 1,
            "profile": profile,
            "target": target_triple.as_deref().unwrap_or("x86_64-pc-windows-msvc"),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "nodeApi": 6,
            "dbms": ["postgresql", "sqlserver"],
            "artifacts": artifact_hashes,
        }))?,
    )?;

    let mut rel_files = Vec::new();
    collect_files(&dist, &dist, &mut rel_files)?;
    rel_files.sort_by_key(|p| p.to_string_lossy().replace('\\', "/"));
    let mut checksums = String::new();
    for rel in rel_files {
        let digest = sha256_file(&dist.join(&rel))?;
        let normalized = rel.to_string_lossy().replace('\\', "/");
        checksums.push_str(&format!("{digest}  {normalized}\n"));
    }
    std::fs::write(dist.join("checksums.txt"), checksums)?;
    println!("Artifacts: {}", dist.display());
    Ok(())
}
