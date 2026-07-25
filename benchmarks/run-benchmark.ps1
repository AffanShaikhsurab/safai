<#
.SYNOPSIS
    Compares Safai's Rust directory-sizing engine against the tools Windows
    ships with, on the same folder tree, doing the same job.

.DESCRIPTION
    Every contender answers one identical question:

        "How many bytes does this directory tree contain, in total?"

    That is the operation that dominates a Safai scan, so it is the honest thing
    to measure. Three contenders:

      1. Get-ChildItem  - the idiomatic PowerShell way (Measure-Object -Sum)
      2. cmd /c dir /s  - the classic Win32 console tool
      3. Safai          - crates/safai-core's `dir_size`, via the walk_bench example

.NOTES
    Methodology, because a filesystem benchmark is easy to get wrong:

    * Each contender gets one untimed WARM-UP pass before its timed runs, so no
      one pays for populating the NTFS metadata cache while the others don't.
      Without this the winner is simply whoever ran second.
    * Results report min / median / mean. `min` is the headline: the run least
      disturbed by unrelated system activity.
    * Byte totals are printed for every tool so you can confirm they actually
      did equivalent work. They will not match to the byte - see the README's
      "Why the totals differ slightly" note - and a large divergence means the
      comparison is invalid, not that Safai is fast.
    * Safai must be built with --release. A debug build is several times slower
      and would make this meaningless.
    * Numbers are machine-specific. Re-run it on yours.

.PARAMETER Root
    Directory tree to measure. Defaults to your user profile: large, deeply
    nested, representative of real developer junk, and readable without admin.

.PARAMETER Iterations
    Timed runs per contender (after the warm-up). Default 3.

.PARAMETER SkipDir
    Skip the `cmd /c dir /s` contender.

    Worth knowing: on a multi-million-file tree `dir /s` is slow enough to
    dominate the whole run - it did not finish two passes over a 5.3M-file
    profile in 25 minutes on the reference machine. Pass -SkipDir for a quick
    Safai-vs-PowerShell comparison, and leave it on only when you are willing to
    wait.

.EXAMPLE
    ./benchmarks/run-benchmark.ps1
    ./benchmarks/run-benchmark.ps1 -Root C:\ -Iterations 3
#>
[CmdletBinding()]
param(
    [string] $Root = $env:USERPROFILE,
    [ValidateRange(1, 25)]
    [int] $Iterations = 3,
    [switch] $SkipDir
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
    throw "Not a directory: $Root"
}
$Root = (Resolve-Path -LiteralPath $Root).Path
$repoRoot = Split-Path -Parent $PSScriptRoot

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

function Format-Bytes {
    param([double] $Bytes)
    $units = 'B', 'KB', 'MB', 'GB', 'TB'
    $i = 0
    while ($Bytes -ge 1024 -and $i -lt $units.Count - 1) { $Bytes /= 1024; $i++ }
    if ($i -eq 0) { return "$([int]$Bytes) B" }
    return ('{0:N1} {1}' -f $Bytes, $units[$i])
}

# Runs $Work once untimed (warm-up), then $Iterations times with the stopwatch.
# $Work must return the total byte count it measured.
function Measure-Contender {
    param(
        [Parameter(Mandatory)] [string]   $Name,
        [Parameter(Mandatory)] [scriptblock] $Work
    )

    Write-Host "  $Name" -NoNewline -ForegroundColor Cyan
    Write-Host ' warming up…' -NoNewline -ForegroundColor DarkGray

    $bytes = 0
    try {
        $bytes = & $Work
    } catch {
        Write-Host ' failed' -ForegroundColor Red
        return [pscustomobject]@{ Tool = $Name; Failed = $true; Error = $_.Exception.Message }
    }

    $timings = New-Object System.Collections.Generic.List[double]
    for ($i = 1; $i -le $Iterations; $i++) {
        Write-Host " $i" -NoNewline -ForegroundColor DarkGray
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $bytes = & $Work
        $sw.Stop()
        $timings.Add($sw.Elapsed.TotalMilliseconds)
    }
    Write-Host ' done' -ForegroundColor Green

    $sorted = @($timings | Sort-Object)
    $median = if ($sorted.Count % 2 -eq 0) {
        ($sorted[$sorted.Count / 2 - 1] + $sorted[$sorted.Count / 2]) / 2
    } else {
        $sorted[[math]::Floor($sorted.Count / 2)]
    }

    [pscustomobject]@{
        Tool     = $Name
        Failed   = $false
        Bytes    = [double]$bytes
        MinMs    = $sorted[0]
        MedianMs = $median
        MeanMs   = ($timings | Measure-Object -Average).Average
        MaxMs    = $sorted[$sorted.Count - 1]
    }
}

# ---------------------------------------------------------------------------
# Build Safai first, so its compile time is never counted as run time
# ---------------------------------------------------------------------------

Write-Host ''
Write-Host 'Safai directory-sizing benchmark' -ForegroundColor White
Write-Host ('=' * 60) -ForegroundColor DarkGray
Write-Host "Root       : $Root"
Write-Host "Iterations : $Iterations (plus 1 untimed warm-up each)"
Write-Host "CPU        : $($env:NUMBER_OF_PROCESSORS) logical cores"
Write-Host "OS         : $([System.Environment]::OSVersion.VersionString)"
Write-Host ''

Write-Host 'Building walk_bench (release)…' -ForegroundColor DarkGray
& cargo build --release -p safai-core --example walk_bench --quiet
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$benchExe = Join-Path $repoRoot 'target\release\examples\walk_bench.exe'
if (-not (Test-Path -LiteralPath $benchExe)) {
    throw "walk_bench.exe not found at $benchExe"
}

Write-Host ''
Write-Host 'Running contenders:' -ForegroundColor White

$results = New-Object System.Collections.Generic.List[object]

# -- 1. Get-ChildItem ------------------------------------------------------
# -Force includes hidden/system entries so the traversal is comparable.
# -ErrorAction SilentlyContinue skips the reparse points and protected folders
# that exist in every user profile; without it the pipeline aborts partway.
$results.Add(
    (Measure-Contender -Name 'Get-ChildItem -Recurse' -Work {
        $sum = Get-ChildItem -LiteralPath $Root -Recurse -File -Force -ErrorAction SilentlyContinue |
            Measure-Object -Property Length -Sum
        if ($null -eq $sum.Sum) { 0 } else { $sum.Sum }
    })
)

# -- 2. cmd /c dir /s ------------------------------------------------------
# `dir /s` prints a grand total; parse the last "File(s)" summary line.
#
# The filtering is done *inside* cmd, by findstr, and the result lands in a temp
# file. Capturing `& cmd /c dir /s` into a PowerShell variable instead would
# force PowerShell to materialise one object per output line — over five million
# of them on a large profile — and the run would then be dominated by
# PowerShell's pipeline overhead rather than by `dir`. That would slander the
# baseline and inflate Safai's advantage, so keep the line handling native.
#
# Locale dependent: a parse failure is reported rather than silently scored 0.
if (-not $SkipDir) {
    $dirTmp = Join-Path ([System.IO.Path]::GetTempPath()) 'safai-dir-bench.txt'
    $results.Add(
        (Measure-Contender -Name 'cmd /c dir /s' -Work {
            & cmd.exe /c "dir `"$Root`" /s /a-d 2>nul | findstr /C:`"File(s)`" > `"$dirTmp`"" | Out-Null
            $totalLine = Get-Content -LiteralPath $dirTmp -Tail 5 |
                Select-String -Pattern '\d[\d.,]*\s+File\(s\)' |
                Select-Object -Last 1
            if (-not $totalLine) { throw 'could not parse the dir /s grand total (non-English locale?)' }
            $digits = ([regex]::Matches($totalLine.Line, '[\d.,]+') |
                ForEach-Object { $_.Value }) | Select-Object -Last 1
            [double](($digits -replace '[.,]', ''))
        })
    )
    Remove-Item -LiteralPath $dirTmp -ErrorAction SilentlyContinue
}

# -- 3. Safai --------------------------------------------------------------
# The Rust side does its own warm-up + timing and reports JSON; run it once
# here and adopt its numbers directly rather than timing the process launch.
Write-Host '  Safai (Rust, parallel)' -NoNewline -ForegroundColor Cyan
Write-Host ' running…' -NoNewline -ForegroundColor DarkGray
$json = & $benchExe $Root $Iterations
if ($LASTEXITCODE -ne 0) { throw "walk_bench failed: $json" }
$safai = $json | ConvertFrom-Json
Write-Host ' done' -ForegroundColor Green

$results.Add([pscustomobject]@{
    Tool     = 'Safai (Rust, parallel)'
    Failed   = $false
    Bytes    = [double]$safai.bytes
    MinMs    = [double]$safai.minMs
    MedianMs = [double]$safai.medianMs
    MeanMs   = [double]$safai.meanMs
    MaxMs    = [double]$safai.maxMs
})

# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------

$ok = @($results | Where-Object { -not $_.Failed })
$baseline = $ok | Where-Object { $_.Tool -ne 'Safai (Rust, parallel)' } |
    Sort-Object MinMs | Select-Object -First 1
$safaiRow = $ok | Where-Object { $_.Tool -eq 'Safai (Rust, parallel)' }

Write-Host ''
Write-Host 'Results' -ForegroundColor White
Write-Host ('=' * 60) -ForegroundColor DarkGray

$table = $ok | ForEach-Object {
    $speedup = if ($_.MinMs -gt 0 -and $safaiRow) {
        '{0:N1}x' -f ($_.MinMs / $safaiRow.MinMs)
    } else { '-' }
    [pscustomobject]@{
        Tool       = $_.Tool
        'Best'     = '{0:N2} s' -f ($_.MinMs / 1000)
        'Median'   = '{0:N2} s' -f ($_.MedianMs / 1000)
        'Total'    = Format-Bytes $_.Bytes
        'vs Safai' = $speedup
    }
}
$table | Format-Table -AutoSize

foreach ($f in @($results | Where-Object { $_.Failed })) {
    Write-Host "  ! $($f.Tool) failed: $($f.Error)" -ForegroundColor Yellow
}

if ($baseline -and $safaiRow) {
    $factor = $baseline.MinMs / $safaiRow.MinMs
    Write-Host ''
    Write-Host ('Safai is {0:N1}x faster than the quickest Windows built-in ({1}).' -f $factor, $baseline.Tool) -ForegroundColor Green

    # Sanity check: if the tools disagree wildly they did not do the same work,
    # so the speed comparison means nothing.
    if ($baseline.Bytes -gt 0) {
        $drift = [math]::Abs($safaiRow.Bytes - $baseline.Bytes) / $baseline.Bytes
        if ($drift -gt 0.10) {
            Write-Host ('WARNING: byte totals differ by {0:P1} — the contenders did not measure the same set of files, so this comparison is not valid.' -f $drift) -ForegroundColor Red
        } else {
            Write-Host ('Byte totals agree within {0:P2} — the contenders measured equivalent work.' -f $drift) -ForegroundColor DarkGray
        }
    }
}

# Machine-readable output, handy for pasting into the README or a CI artifact.
$outFile = Join-Path $PSScriptRoot 'results.json'
[pscustomobject]@{
    root       = $Root
    iterations = $Iterations
    cpuCores   = [int]$env:NUMBER_OF_PROCESSORS
    os         = [System.Environment]::OSVersion.VersionString
    measuredAt = (Get-Date).ToString('o')
    results    = $ok
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $outFile -Encoding UTF8

Write-Host ''
Write-Host "Wrote $outFile" -ForegroundColor DarkGray
Write-Host ''
