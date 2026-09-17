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
    # Keys are paths inside the package; values are paths in the source checkout.
    $packageSources = [ordered]@{
        'everytree.exe' = "target\$target\release\everytree.exe"
        'Everything64.dll' = 'vendor\everything-sdk\dll\Everything64.dll'
        'README.md' = 'README.md'
        'assets/screenshot.png' = 'assets\screenshot.png'
        'BENCHMARKS.md' = 'BENCHMARKS.md'
        'LICENSE' = 'LICENSE'
        'THIRD_PARTY_NOTICES.txt' = 'THIRD_PARTY_NOTICES.txt'
        'everytree.ico' = 'assets\everytree.ico'
    }
    foreach ($entry in $packageSources.GetEnumerator()) {
        $destination = Join-Path $distDir $entry.Key
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $projectRoot $entry.Value) -Destination $destination -Force
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
    # Preserve relative paths without including unrelated files in an existing dist folder.
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipStream = [System.IO.File]::Open($zipPath, [System.IO.FileMode]::Create)
    try {
        $archive = [System.IO.Compression.ZipArchive]::new($zipStream, [System.IO.Compression.ZipArchiveMode]::Create, $true)
        try {
            foreach ($relativePath in $packageSources.Keys) {
                [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                    $archive, (Join-Path $distDir $relativePath), $relativePath,
                    [System.IO.Compression.CompressionLevel]::Optimal
                ) | Out-Null
            }
        } finally { $archive.Dispose() }
    } finally { $zipStream.Dispose() }
    $hash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
    # LF is required so GNU sha256sum on the publishing runner reads the filename correctly.
    [System.IO.File]::WriteAllText("$zipPath.sha256", "$hash *$(Split-Path -Leaf $zipPath)`n", [System.Text.Encoding]::ASCII)
    Write-Host "Portable build: $executable"
    Write-Host "Release archive: $zipPath"
    Write-Host "SHA-256 checksum: $zipPath.sha256"

    # Keep the familiar launch path current without stopping a running app.
    $stableDir = Join-Path $projectRoot 'dist'
    try {
        foreach ($relativePath in $packageSources.Keys) {
            $destination = Join-Path $stableDir $relativePath
            New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $distDir $relativePath) -Destination $destination -Force
        }
        Write-Host "Latest build: $stableDir\everytree.exe"
    } catch {
        Write-Warning "Could not update the original launch path (it may be running). Open $executable instead. $($_.Exception.Message)"
    }
} finally { Pop-Location }
