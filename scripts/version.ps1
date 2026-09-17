param([string]$ExpectedTag = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $projectRoot 'Cargo.toml') -Raw
$package = [regex]::Match($manifest, '(?ms)^\[package\]\s*(.*?)(?=^\[|\z)').Groups[1].Value
$version = [regex]::Match($package, '(?m)^version\s*=\s*"([^"\r\n]+)"').Groups[1].Value
if ($version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
    throw 'A valid package version was not found in Cargo.toml.'
}
if ($ExpectedTag -and $ExpectedTag -cne "v$version") {
    throw "Release tag '$ExpectedTag' does not match Cargo.toml. Expected 'v$version'."
}
# Emit only the version, so callers can use it as a filename or workflow output.
Write-Output $version
