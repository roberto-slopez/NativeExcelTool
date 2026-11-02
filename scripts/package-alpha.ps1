$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $root 'dist/release/manifest.json') -Raw | ConvertFrom-Json
if ($manifest.version -notmatch '^\d+\.\d+\.\d+-alpha\.\d+$' -or $manifest.profile -ne 'release' -or $manifest.target -ne 'x86_64-pc-windows-msvc') {
    throw 'Expected a Windows x64 release build with an alpha version'
}
$name = "NativeExcelTool-v$($manifest.version)-windows-x64"
$packages = Join-Path $root 'dist/packages'
$stage = Join-Path $packages ([guid]::NewGuid().ToString('N'))
$bundle = Join-Path $stage $name
New-Item -ItemType Directory -Path $bundle -Force | Out-Null
foreach ($relative in @('dist/release', 'bindings/dotnet', 'examples/node')) {
    $destination = Join-Path $bundle (Split-Path -Parent $relative)
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $root $relative) -Destination $destination -Recurse
}
$scripts = Join-Path $bundle 'scripts'
New-Item -ItemType Directory -Path $scripts -Force | Out-Null
foreach ($file in @('run-examples.ps1', 'verify-examples.ps1')) {
    Copy-Item -LiteralPath (Join-Path $root "scripts/$file") -Destination $scripts
}
$dotnet = Join-Path $bundle 'examples/dotnet'
New-Item -ItemType Directory -Path $dotnet -Force | Out-Null
foreach ($file in @('Program.cs', 'Example.csproj')) {
    Copy-Item -LiteralPath (Join-Path $root "examples/dotnet/$file") -Destination $dotnet
}
Copy-Item -LiteralPath (Join-Path $root 'docs/ALPHA_QUICKSTART.md') -Destination (Join-Path $bundle 'README.md')
foreach ($file in @('LICENSE', 'THIRD_PARTY_NOTICES.md')) {
    Copy-Item -LiteralPath (Join-Path $root $file) -Destination $bundle
}
$samples = Join-Path $bundle 'samples'
New-Item -ItemType Directory -Path $samples -Force | Out-Null
foreach ($file in @('orders-node.xlsx', 'orders-dotnet.xlsx')) {
    Copy-Item -LiteralPath (Join-Path $root "output/$file") -Destination $samples
}
& (Join-Path $root 'scripts/verify-examples.ps1') -OutputDirectory $samples
$archive = Join-Path $packages "$name.zip"
Compress-Archive -Path (Join-Path $bundle '*') -DestinationPath $archive -Force
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $name.zip" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
Write-Host "Alpha bundle: $archive"
