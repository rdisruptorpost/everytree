param(
    [switch]$SkipTests,
    [switch]$Check,
    [string]$ExpectedTag = ''
)
$ErrorActionPreference = 'Stop'
if ($Check -and $SkipTests) { throw '-Check cannot be combined with -SkipTests.' }
$projectRoot = Split-Path -Parent $PSScriptRoot
$version = & (Join-Path $PSScriptRoot 'version.ps1') -ExpectedTag $ExpectedTag
$target = 'x86_64-pc-windows-msvc'
& (Join-Path $PSScriptRoot 'setup.ps1')
Push-Location $projectRoot
try {
    if ($Check) {
        cargo fmt --check
        if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed.' }
        cargo clippy --locked --release --all-targets --target $target -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
    }
    if (-not $SkipTests) {
        cargo test --locked --release --target $target
        if ($LASTEXITCODE -ne 0) { throw 'Tests failed.' }
    }
    cargo build --locked --release --target $target
    if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }

    $distDir = Join-Path $projectRoot "dist\$version"
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null
    $packageSources = @(
        "target\$target\release\everytree.exe",
        'vendor\everything-sdk\dll\Everything64.dll',
        'README.md',
        'BENCHMARKS.md',
        'LICENSE',
        'THIRD_PARTY_NOTICES.txt',
        'assets\everytree.ico'
    )
    $packagePaths = @()
    foreach ($source in $packageSources) {
        $destination = Join-Path $distDir (Split-Path -Leaf $source)
        Copy-Item -LiteralPath (Join-Path $projectRoot $source) -Destination $destination -Force
        $packagePaths += $destination
    }
    $executable = Join-Path $distDir 'everytree.exe'
    if ((Get-Item -LiteralPath $executable).VersionInfo.ProductVersion -cne $version) {
        throw 'The executable version does not match Cargo.toml.'
    }
    $helpText = & $executable --help | Out-String
    if ($LASTEXITCODE -ne 0 -or $helpText -notmatch '^everytree\r?\n') {
        throw 'The packaged executable failed its startup check.'
    }
    # Explicit package contents keep stale files from previous builds out of releases.
    $zipPath = Join-Path $projectRoot "dist\everytree-$version-windows-x64.zip"
    Compress-Archive -LiteralPath $packagePaths -DestinationPath $zipPath -CompressionLevel Optimal -Force
    $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    # LF is required so GNU sha256sum on the publishing runner reads the filename correctly.
    [System.IO.File]::WriteAllText("$zipPath.sha256", "$hash *$(Split-Path -Leaf $zipPath)`n", [System.Text.Encoding]::ASCII)
    Write-Host "Portable build: $executable"
    Write-Host "Release archive: $zipPath"
    Write-Host "SHA-256 checksum: $zipPath.sha256"

    # Keep the familiar launch path current without stopping a running app.
    $stableDir = Join-Path $projectRoot 'dist'
    try {
        foreach ($file in $packagePaths) {
            Copy-Item -LiteralPath $file -Destination (Join-Path $stableDir (Split-Path -Leaf $file)) -Force
        }
        Write-Host "Latest build: $stableDir\everytree.exe"
    } catch {
        Write-Warning "Could not update the original launch path (it may be running). Open $executable instead. $($_.Exception.Message)"
    }
} finally { Pop-Location }
