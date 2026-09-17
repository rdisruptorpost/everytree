$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$vendorDir = Join-Path $projectRoot 'vendor'
$sdkDir = Join-Path $vendorDir 'everything-sdk'
$dllPath = Join-Path $sdkDir 'dll\Everything64.dll'
if (-not (Test-Path -LiteralPath $dllPath)) {
    New-Item -ItemType Directory -Force -Path $vendorDir | Out-Null
    $zipPath = Join-Path $vendorDir 'Everything-SDK.zip'
    Write-Host 'Downloading the official Everything SDK from voidtools.com...'
    Invoke-WebRequest -Uri 'https://www.voidtools.com/Everything-SDK.zip' -OutFile $zipPath
    Expand-Archive -LiteralPath $zipPath -DestinationPath $sdkDir -Force
}
if (-not (Test-Path -LiteralPath $dllPath)) { throw 'The SDK archive did not contain Everything64.dll.' }
Write-Host "SDK ready: $dllPath"
