<#
.SYNOPSIS
Runs and verifies the Node.js and C# sales examples against a native build.
.DESCRIPTION
Creates both ten-order workbooks under output/ and verifies their table data.
The selected native profile must already exist under dist/. The script restores
the caller's working directory and NATIVE_EXCEL_PROFILE environment variable.
.PARAMETER Profile
Selects debug or release binaries. The default is release for extracted alpha bundles.
.EXAMPLE
./scripts/run-examples.ps1 -Profile debug
#>
param([ValidateSet('debug', 'release')][string]$Profile = 'release')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$previousProfile = $env:NATIVE_EXCEL_PROFILE
Push-Location -LiteralPath $root
try {
    $env:NATIVE_EXCEL_PROFILE = $Profile
    $configuration = if ($Profile -eq 'release') { 'Release' } else { 'Debug' }
    & node examples/node/export.cjs
    if ($LASTEXITCODE -ne 0) { throw 'Node.js example failed' }
    & dotnet run --project examples/dotnet --configuration $configuration
    if ($LASTEXITCODE -ne 0) { throw 'C# example failed' }
    & (Join-Path $PSScriptRoot 'verify-examples.ps1')
} finally {
    $env:NATIVE_EXCEL_PROFILE = $previousProfile
    Pop-Location
}
