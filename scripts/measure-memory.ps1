param(
    [string]$Executable = (Join-Path $PSScriptRoot '..\dist\everytree.exe'),
    [ValidateRange(1, 50000000)][int]$Files = 1000000,
    [ValidateRange(1, 60)][int]$SettleSeconds = 8,
    [ValidateRange(1, 30)][int]$Samples = 3
)
$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
# Use a fresh synthetic view so runs compare the same data without querying Everything.
# Only the process created by this script is closed; existing app instances are untouched.
$testProcess = Start-Process -FilePath $executablePath -ArgumentList '--demo', $Files -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Seconds $SettleSeconds
    for ($sample = 1; $sample -le $Samples; $sample++) {
        $testProcess.Refresh()
        if ($testProcess.HasExited) { throw 'The test application exited before measurement.' }
        $dedicatedBytes = $null
        $sharedBytes = $null
        try {
            $counters = (Get-Counter '\GPU Process Memory(*)\Dedicated Usage', '\GPU Process Memory(*)\Shared Usage').CounterSamples |
                Where-Object { $_.InstanceName -like "pid_$($testProcess.Id)_*" }
            if ($counters) {
                $dedicatedBytes = ($counters | Where-Object Path -Like '*\dedicated usage' | Measure-Object CookedValue -Sum).Sum
                $sharedBytes = ($counters | Where-Object Path -Like '*\shared usage' | Measure-Object CookedValue -Sum).Sum
            }
        } catch {
            Write-Warning "GPU counters unavailable (the English counter names may differ on localized Windows): $($_.Exception.Message)"
        }
        [pscustomobject]@{
            Executable = $executablePath
            Files = $Files
            Sample = $sample
            WorkingSetBytes = $testProcess.WorkingSet64
            PrivateBytes = $testProcess.PrivateMemorySize64
            DedicatedGpuBytes = $dedicatedBytes
            SharedGpuBytes = $sharedBytes
        }
        if ($sample -lt $Samples) { Start-Sleep -Seconds 1 }
    }
} finally {
    if (-not $testProcess.HasExited) { Stop-Process -Id $testProcess.Id }
    $testProcess.Dispose()
}
