#Requires -Version 5.1
<#
.SYNOPSIS
    Aggregate .ai/dispatch-trace/*.jsonl timing events into a plain-text
    dispatch trend report with optional alert exit code.

.DESCRIPTION
    Read-only, local-only CLI. Walks existing JSONL trace lines emitted by
    Invoke-AiDispatchAuto.ps1 and Invoke-AiDispatchQueue.ps1 and pairs
    start/done messages within each trace file (one trace file = one PID) to
    produce samples / average / p50 / p95 / max durations for the standard
    automation phases. Malformed JSON lines are tallied without aborting and
    are reported per file plus in total.

    No trace files are modified. No git, gh, network, or scheduler calls.
    Default exit code is 0 even when alerts fire; pass -FailOnAlert to exit
    non-zero after the full report is printed whenever any alert was emitted.

.PARAMETER RepoRoot
    Repository root. Defaults to the directory containing this script.

.PARAMETER TraceDir
    Trace directory. Relative paths resolve under -RepoRoot. Defaults to
    `.ai\dispatch-trace`.

.PARAMETER SinceHours
    Only consider events whose timestamp is within the last N hours. 0 or
    unset means all parsed events.

.PARAMETER WarnEmptyCapGapSec
    Alert threshold for the `empty-cap gap` span (seconds).

.PARAMETER WarnQueueLoopSec
    Alert threshold for the `queue loop` span (seconds).

.PARAMETER WarnPublishSec
    Alert threshold for the `queue publish block` span (seconds).

.PARAMETER WarnGithubFinalizeSec
    Alert threshold for the `queue GitHub finalize` span (seconds).

.PARAMETER WarnInvalidJsonLines
    Alert threshold for the total count of malformed JSONL lines.

.PARAMETER FailOnAlert
    Print the report normally, then exit non-zero if any alert was emitted.

.EXAMPLE
    .\Get-AiDispatchTrends.ps1

.EXAMPLE
    .\Get-AiDispatchTrends.ps1 -SinceHours 24 -FailOnAlert
#>
[CmdletBinding()]
param(
    [string]$RepoRoot = $PSScriptRoot,
    [string]$TraceDir = '.ai\dispatch-trace',
    [int]$SinceHours = 0,
    [double]$WarnEmptyCapGapSec    = 30,
    [double]$WarnQueueLoopSec      = 1800,
    [double]$WarnPublishSec        = 60,
    [double]$WarnGithubFinalizeSec = 30,
    [int]$WarnInvalidJsonLines     = 0,
    [switch]$FailOnAlert
)

$ErrorActionPreference = 'Stop'

if (-not $RepoRoot) { $RepoRoot = (Get-Location).Path }
if (Test-Path -LiteralPath $RepoRoot) {
    $RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path
}

if ([System.IO.Path]::IsPathRooted($TraceDir)) {
    $resolvedTraceDir = $TraceDir
} else {
    $resolvedTraceDir = Join-Path $RepoRoot $TraceDir
}

# Each phase is one start/done message pair. Pairing is FIFO within a single
# trace file (one PID per file), so nested or overlapping pairs are matched in
# order. Threshold/AlertLabel only set on the spans that should produce alerts.
$phases = @(
    [pscustomobject]@{ Name = 'auto tick total';       StartPattern = '^auto\.tick:\s*start\b';                    DonePattern = '^auto\.tick:\s*end\b';                       Threshold = $null;                  AlertLabel = '' },
    [pscustomobject]@{ Name = 'empty-cap gap';         StartPattern = '^auto\.queue-check:\s*primary\s+done\b';    DonePattern = '^auto\.cap-check:\s*start\b';                Threshold = $WarnEmptyCapGapSec;    AlertLabel = 'empty-cap gap' },
    [pscustomobject]@{ Name = 'auto queue invocation'; StartPattern = '^auto\.tick:\s*queue-invocation\s+start\b'; DonePattern = '^auto\.tick:\s*queue-invocation\s+done\b';   Threshold = $null;                  AlertLabel = '' },
    [pscustomobject]@{ Name = 'queue loop';            StartPattern = '^queue\.loop:\s*start\b';                   DonePattern = '^queue\.loop:\s*done\b';                     Threshold = $WarnQueueLoopSec;      AlertLabel = 'queue loop' },
    [pscustomobject]@{ Name = 'queue publish block';   StartPattern = '^queue\.publish:\s*block-entry\b';          DonePattern = '^queue\.publish:\s*block-exit\b';            Threshold = $WarnPublishSec;        AlertLabel = 'queue publish block' },
    [pscustomobject]@{ Name = 'queue GitHub finalize'; StartPattern = '^queue\.github:\s*comment\s+start\b';       DonePattern = '^queue\.github:\s*relabel\s+done\b';         Threshold = $WarnGithubFinalizeSec; AlertLabel = 'queue GitHub finalize' }
)

# --- Read trace files --------------------------------------------------------

$traceDirExists = Test-Path -LiteralPath $resolvedTraceDir
$traceFiles = @()
if ($traceDirExists) {
    $traceFiles = @(Get-ChildItem -LiteralPath $resolvedTraceDir -File -Filter '*.jsonl' -ErrorAction SilentlyContinue)
}

$cutoff = $null
if ($SinceHours -gt 0) {
    $cutoff = (Get-Date).AddHours(-$SinceHours)
}

$durations = [ordered]@{}
foreach ($p in $phases) {
    $durations[$p.Name] = New-Object System.Collections.Generic.List[double]
}

$invalidByFile    = [ordered]@{}
$invalidTotal     = 0
$linesRead        = 0
$linesParsed      = 0
$linesAfterCutoff = 0
$earliestEvent    = $null
$latestEvent      = $null

foreach ($file in $traceFiles) {
    $invalidByFile[$file.Name] = 0

    $rawLines = $null
    try {
        $rawLines = @(Get-Content -LiteralPath $file.FullName -ErrorAction Stop)
    } catch {
        # Treat an unreadable file as one parse failure; do not abort the run.
        $invalidByFile[$file.Name] += 1
        $invalidTotal += 1
        continue
    }

    $fileEvents = New-Object System.Collections.Generic.List[object]
    foreach ($line in $rawLines) {
        $linesRead++
        if ($null -eq $line) { continue }
        $trim = $line.Trim()
        if ($trim.Length -eq 0) { continue }

        $obj = $null
        try {
            $obj = $trim | ConvertFrom-Json -ErrorAction Stop
        } catch {
            $invalidByFile[$file.Name] += 1
            $invalidTotal += 1
            continue
        }
        if ($null -eq $obj) {
            $invalidByFile[$file.Name] += 1
            $invalidTotal += 1
            continue
        }

        $linesParsed++

        $ts = $null
        if ($obj.PSObject.Properties['timestamp'] -and $obj.timestamp) {
            try { $ts = [DateTimeOffset]::Parse([string]$obj.timestamp) } catch { $ts = $null }
        }
        if ($cutoff -and $ts -and $ts.LocalDateTime -lt $cutoff) { continue }
        $linesAfterCutoff++

        if ($ts) {
            if ($null -eq $earliestEvent -or $ts -lt $earliestEvent) { $earliestEvent = $ts }
            if ($null -eq $latestEvent   -or $ts -gt $latestEvent)   { $latestEvent   = $ts }
        }

        $msg = ''
        if ($obj.PSObject.Properties['message']) { $msg = [string]$obj.message }
        if (-not $msg) { continue }

        $elapsed = $null
        if ($obj.PSObject.Properties['elapsed_seconds'] -and $null -ne $obj.elapsed_seconds) {
            try { $elapsed = [double]$obj.elapsed_seconds } catch { $elapsed = $null }
        }

        $fileEvents.Add([pscustomobject]@{
            Message = $msg
            Elapsed = $elapsed
            Time    = $ts
        }) | Out-Null
    }

    foreach ($p in $phases) {
        $openStarts = New-Object System.Collections.Generic.Queue[object]
        foreach ($ev in $fileEvents) {
            if ($ev.Message -match $p.StartPattern) {
                $openStarts.Enqueue($ev) | Out-Null
                continue
            }
            if ($ev.Message -match $p.DonePattern) {
                if ($openStarts.Count -gt 0) {
                    $startEv = $openStarts.Dequeue()
                    $dur = $null
                    if ($null -ne $ev.Elapsed -and $null -ne $startEv.Elapsed) {
                        $dur = [double]$ev.Elapsed - [double]$startEv.Elapsed
                    } elseif ($ev.Time -and $startEv.Time) {
                        $dur = ($ev.Time - $startEv.Time).TotalSeconds
                    }
                    if ($null -ne $dur -and $dur -ge 0) {
                        $durations[$p.Name].Add([double]$dur) | Out-Null
                    }
                }
            }
        }
    }
}

# --- Stats helpers -----------------------------------------------------------

function Get-Percentile {
    param([double[]]$Sorted, [double]$P)
    if ($null -eq $Sorted -or $Sorted.Length -eq 0) { return $null }
    if ($Sorted.Length -eq 1) { return $Sorted[0] }
    $rank = [int][math]::Ceiling(($P / 100.0) * $Sorted.Length)
    if ($rank -lt 1)              { $rank = 1 }
    if ($rank -gt $Sorted.Length) { $rank = $Sorted.Length }
    return $Sorted[$rank - 1]
}

function Format-Seconds {
    param($Value)
    if ($null -eq $Value) { return '      n/a' }
    return ('{0,8:N2}s' -f [double]$Value)
}

# --- Compose report ----------------------------------------------------------

$report = New-Object System.Collections.Generic.List[string]
$alerts = New-Object System.Collections.Generic.List[string]

$report.Add('') | Out-Null
$report.Add('Summary') | Out-Null
$report.Add(('  Trace dir:           {0}' -f $resolvedTraceDir)) | Out-Null
if (-not $traceDirExists) {
    $report.Add('  Trace dir present:   no  (no-data report)') | Out-Null
} else {
    $report.Add('  Trace dir present:   yes') | Out-Null
}
$report.Add(('  Trace files scanned: {0}' -f $traceFiles.Count)) | Out-Null
$report.Add(('  Lines read:          {0}' -f $linesRead)) | Out-Null
$report.Add(('  Events parsed:       {0}' -f $linesParsed)) | Out-Null
if ($SinceHours -gt 0) {
    $report.Add(('  Since-hours filter:  {0}h (cutoff {1:yyyy-MM-dd HH:mm:ss})' -f $SinceHours, $cutoff)) | Out-Null
    $report.Add(('  Events in window:    {0}' -f $linesAfterCutoff)) | Out-Null
}
$report.Add(('  Invalid JSON lines:  {0}' -f $invalidTotal)) | Out-Null
if ($invalidTotal -gt 0) {
    foreach ($name in $invalidByFile.Keys) {
        $n = [int]$invalidByFile[$name]
        if ($n -gt 0) {
            $report.Add(('    {0}: {1}' -f $name, $n)) | Out-Null
        }
    }
}
if ($earliestEvent) {
    $report.Add(('  Earliest event:      {0}' -f $earliestEvent.ToString('yyyy-MM-dd HH:mm:ss zzz'))) | Out-Null
}
if ($latestEvent) {
    $report.Add(('  Latest event:        {0}' -f $latestEvent.ToString('yyyy-MM-dd HH:mm:ss zzz'))) | Out-Null
}
if ($linesParsed -eq 0) {
    $report.Add('  No parseable events found; phase durations and alerts will be empty.') | Out-Null
}

$report.Add('') | Out-Null
$report.Add('Phase Durations') | Out-Null
$rowFmt = '  {0,-24} samples={1,3}  avg={2}  p50={3}  p95={4}  max={5}'
foreach ($p in $phases) {
    $vals = @($durations[$p.Name])
    $count = $vals.Count
    if ($count -eq 0) {
        $report.Add(('  {0,-24} samples=  0  (no complete pairs)' -f $p.Name)) | Out-Null
        continue
    }
    $arr = [double[]]$vals
    [Array]::Sort($arr)
    $avg  = ($arr | Measure-Object -Average).Average
    $p50  = Get-Percentile -Sorted $arr -P 50
    $p95  = Get-Percentile -Sorted $arr -P 95
    $maxv = $arr[$arr.Length - 1]
    $report.Add(($rowFmt -f $p.Name, $count, (Format-Seconds $avg), (Format-Seconds $p50), (Format-Seconds $p95), (Format-Seconds $maxv))) | Out-Null
}

# --- Alerts ------------------------------------------------------------------

if ($invalidTotal -gt $WarnInvalidJsonLines) {
    $alerts.Add(('ALERT: invalid JSON lines: {0} (threshold {1})' -f $invalidTotal, $WarnInvalidJsonLines)) | Out-Null
}

foreach ($p in $phases) {
    if ($null -eq $p.Threshold) { continue }
    $vals = @($durations[$p.Name])
    if ($vals.Count -eq 0) { continue }
    $maxv = ($vals | Measure-Object -Maximum).Maximum
    if ($maxv -gt $p.Threshold) {
        $alerts.Add(('ALERT: {0} max {1:N2}s exceeds threshold {2:N2}s' -f $p.AlertLabel, $maxv, $p.Threshold)) | Out-Null
    }
}

$report.Add('') | Out-Null
$report.Add('Alerts') | Out-Null
if ($alerts.Count -eq 0) {
    $report.Add('  (no alerts)') | Out-Null
} else {
    foreach ($a in $alerts) {
        $report.Add(('  {0}' -f $a)) | Out-Null
    }
}
$report.Add('') | Out-Null

foreach ($line in $report) { Write-Output $line }

if ($FailOnAlert -and $alerts.Count -gt 0) {
    exit 1
}
exit 0
