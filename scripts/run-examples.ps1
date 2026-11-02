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
