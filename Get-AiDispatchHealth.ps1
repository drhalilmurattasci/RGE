#Requires -Version 5.1
<#
.SYNOPSIS
    Dispatch-health readout -- pass rate, correction rounds, plan revisions,
    retries, and duration across the AI dispatch runs recorded under
    .ai/dispatch-*/.

.DESCRIPTION
    Every Invoke-AiDispatchLoop.ps1 run leaves a run directory,
    .ai/dispatch-<DispatchId>/, holding the round-numbered artefacts of that
    dispatch: the Codex plan log(s), the Claude plan-gate envelope(s), the
    Claude execution envelope(s), the verification log(s), and the Codex
    control verdict JSON(s).

    This script reads those directories and reports, per run and in aggregate:

      * the final Codex control verdict (pass / needs_changes / none),
      * plan revisions      -- how often Claude bounced the Codex plan,
      * correction rounds   -- how often Codex control bounced the result,
      * verification runs   -- how many times the canonical gate ran,
      * wall-clock duration -- earliest to latest artefact write.

    It turns "is the dispatch loop working well" into numbers you can watch: a
    healthy loop passes on the first try with zero plan revisions and zero
    correction rounds. Rising correction rounds mean the loop is struggling.

    Run directories are gitignored local scratch, so this reflects the runs
    still retained on this machine -- not necessarily every run in repo history.

.PARAMETER RepoRoot
    Repository root. Defaults to the directory containing this script.

.PARAMETER IncludeIncomplete
    Also list run directories that never reached a Codex control review
    (planning-only or aborted runs). Off by default.

.PARAMETER MainRef
    Local git ref used only to detect merged ai-dispatch commits newer than
    the newest retained run dir (drives the stale-readout banner). Defaults to
    `main`. Purely advisory: if git is unavailable or the ref has no
    ai-dispatch commits, the banner simply does not fire and the readout is
    never failed or blocked.

.EXAMPLE
    .\Get-AiDispatchHealth.ps1

.EXAMPLE
    .\Get-AiDispatchHealth.ps1 -IncludeIncomplete
#>
[CmdletBinding()]
param(
    [string]$RepoRoot = $PSScriptRoot,
    [switch]$IncludeIncomplete,
    [string]$MainRef = 'main'
)

$ErrorActionPreference = 'Stop'

# cwd-default guard (mirrors Get-AiDispatchTrends.ps1): under
# `powershell -File .\Get-AiDispatchHealth.ps1` invocation paths $PSScriptRoot
# can be empty, which would make Join-Path throw on an empty/null base. Fall
# back to the current location and resolve it to a full path.
if (-not $RepoRoot) { $RepoRoot = (Get-Location).Path }
if (Test-Path -LiteralPath $RepoRoot) {
    $RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path
}

$aiDir = Join-Path $RepoRoot '.ai'
if (-not (Test-Path -LiteralPath $aiDir)) {
    [Console]::Error.WriteLine("No .ai directory under: $RepoRoot")
    exit 1
}

$runDirs = @(Get-ChildItem -LiteralPath $aiDir -Directory -Filter 'dispatch-*' -ErrorAction SilentlyContinue)
if ($runDirs.Count -eq 0) {
    Write-Output 'No dispatch run directories (.ai/dispatch-*/) found.'
    exit 0
}

# Highest N across file names matching <Pattern> (one (\d+) capture); -1 if none.
function Get-MaxIndex {
    param([string[]]$Names, [string]$Pattern)
    $found = @($Names | ForEach-Object { if ($_ -match $Pattern) { [int]$matches[1] } })
    if ($found.Count -eq 0) { return -1 }
    ($found | Measure-Object -Maximum).Maximum
}

function Format-Pct {
    param([int]$Num, [int]$Den)
    if ($Den -gt 0) { '{0:N0}%' -f (100.0 * $Num / $Den) } else { 'n/a' }
}

$rows = foreach ($d in $runDirs) {
    $files = @(Get-ChildItem -LiteralPath $d.FullName -File -ErrorAction SilentlyContinue)
    $names = @($files | ForEach-Object { $_.Name })

    $planMax   = Get-MaxIndex $names 'plan_gate\.rev(\d+)\.'
    $execMax   = Get-MaxIndex $names 'execute\.round(\d+)\.'
    $ctrlMax   = Get-MaxIndex $names 'control\.round(\d+)\.json$'
    $verifyMax = Get-MaxIndex $names 'verification\.round(\d+)\.log$'

    $verdict = ''
    $summary = ''
    if ($ctrlMax -ge 0) {
        $ctrlPath = Join-Path $d.FullName ('codex.control.round{0}.json' -f $ctrlMax)
        try {
            $j = Get-Content -LiteralPath $ctrlPath -Raw -ErrorAction Stop | ConvertFrom-Json
            if ($j.verdict) { $verdict = [string]$j.verdict }
            if ($j.summary) { $summary = [string]$j.summary }
        } catch {
            $verdict = 'unreadable'
        }
    }

    $outcome = 'INCOMPLETE'
    if     ($verdict -eq 'pass')          { $outcome = 'PASS' }
    elseif ($verdict -eq 'needs_changes') { $outcome = 'FAIL' }
    elseif ($verdict -ne '')              { $outcome = $verdict.ToUpper() }

    $started = $null
    $durMin  = $null
    if ($files.Count -gt 0) {
        $sorted  = @($files | Sort-Object LastWriteTime)
        $started = $sorted[0].LastWriteTime
        $durMin  = [Math]::Round((New-TimeSpan -Start $started -End $sorted[-1].LastWriteTime).TotalMinutes, 1)
    }

    [pscustomobject]@{
        Run         = $d.Name -replace '^dispatch-', ''
        Started     = $started
        Outcome     = $outcome
        Verdict     = $verdict
        Summary     = $summary
        PlanRevs    = [Math]::Max(0, $planMax)
        Corrections = [Math]::Max(0, $execMax)
        VerifyRuns  = $verifyMax + 1
        IsRetry     = ($d.Name -match '\.attempt\d+$')
        DurationMin = $durMin
    }
}

$rows  = @($rows | Sort-Object { if ($_.Started) { $_.Started } else { [DateTime]::MaxValue } })
$shown = if ($IncludeIncomplete) { $rows } else { @($rows | Where-Object { $_.Outcome -ne 'INCOMPLETE' }) }

# --- Stale-readout banner ----------------------------------------------------
# Run dirs are gitignored local scratch (ISSUE-231 leaves the loop's run dir
# inside the disposable worktree), so this machine can be missing later runs
# that have already merged. Fire a loud banner when the newest retained run is
# stale (>24h since its last artefact) AND git history shows a later merged
# ai-dispatch ISSUE-<n> than the newest retained run id. Both signals are
# failure-tolerant: if git is unavailable or returns nothing, the banner does
# not fire. The banner never changes the exit code.
$newestActivity = $null
foreach ($d in $runDirs) {
    $rdFiles = @(Get-ChildItem -LiteralPath $d.FullName -File -ErrorAction SilentlyContinue)
    if ($rdFiles.Count -gt 0) {
        $rdNewest = (@($rdFiles | ForEach-Object { $_.LastWriteTime }) | Measure-Object -Maximum).Maximum
        if ($null -eq $newestActivity -or $rdNewest -gt $newestActivity) { $newestActivity = $rdNewest }
    }
}

$newestRetainedId = -1
foreach ($r in $rows) {
    if ($r.Run -match 'ISSUE-(\d+)') {
        $thisId = [int]$matches[1]
        if ($thisId -gt $newestRetainedId) { $newestRetainedId = $thisId }
    }
}

$maxMergedId = -1
$subjects = $null
try {
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    # NOTE PS5.1: do not pipe native git stderr through 2>&1 (NativeCommandError
    # trap). Use 2>$null and guard on $LASTEXITCODE; otherwise treat as no data.
    $subjects = & git -C $RepoRoot log $MainRef --pretty=%s 2>$null
    $ErrorActionPreference = $prevEap
    if ($LASTEXITCODE -ne 0) { $subjects = $null }
} catch {
    $subjects = $null
    $ErrorActionPreference = 'Stop'
}
if ($subjects) {
    foreach ($subj in @($subjects)) {
        if ($subj -match 'ai-dispatch ISSUE-(\d+)') {
            $mid = [int]$matches[1]
            if ($mid -gt $maxMergedId) { $maxMergedId = $mid }
        }
    }
}

$stale     = ($null -ne $newestActivity) -and ((New-TimeSpan -Start $newestActivity -End (Get-Date)).TotalHours -gt 24)
$haveLater = ($maxMergedId -gt $newestRetainedId)
if ($stale -and $haveLater) {
    $ageHours = (New-TimeSpan -Start $newestActivity -End (Get-Date)).TotalHours
    Write-Output ('!!! STALE READOUT ' + ('=' * 56))
    Write-Output ('!!! Newest retained run is {0:N1}h old (last activity {1:yyyy-MM-dd HH:mm}).' -f $ageHours, $newestActivity)
    Write-Output ('!!! Later dispatches have merged since: newest retained ISSUE-{0}, newest merged ISSUE-{1} on `{2}`.' -f $newestRetainedId, $maxMergedId, $MainRef)
    Write-Output '!!! Run dirs are gitignored local scratch -- this machine is missing later runs.'
    Write-Output ('!!! ' + ('=' * 70))
    Write-Output ''
}

Write-Output ''
Write-Output 'RGE AI Dispatch -- Health Readout'
Write-Output ('Source: {0}\dispatch-*\   ({1} run dir(s), {2} shown)' -f $aiDir, $rows.Count, $shown.Count)
Write-Output ''

$fmt = '{0,-22} {1,-11} {2,-10} {3,4} {4,5} {5,6} {6,9}'
Write-Output ($fmt -f 'Run', 'Started', 'Outcome', 'Plan', 'Corr', 'Verify', 'Dur(min)')
Write-Output ($fmt -f ('-' * 22), ('-' * 11), ('-' * 10), '----', '-----', '------', '---------')
foreach ($r in $shown) {
    $started = if ($r.Started) { $r.Started.ToString('MM-dd HH:mm') } else { '?' }
    $dur     = if ($null -ne $r.DurationMin) { '{0}' -f $r.DurationMin } else { '?' }
    # 0 verification runs means the gate did not run (run predates it, or the
    # run failed before reaching it) -- show '-', not a misleading count of 0.
    $verify  = if ($r.VerifyRuns -gt 0) { [string]$r.VerifyRuns } else { '-' }
    Write-Output ($fmt -f $r.Run, $started, $r.Outcome, $r.PlanRevs, $r.Corrections, $verify, $dur)
}
Write-Output ''

# --- Aggregate over runs that reached a Codex control review -----------------
$rated     = @($rows | Where-Object { $_.Outcome -eq 'PASS' -or $_.Outcome -eq 'FAIL' })
$passed    = @($rated | Where-Object { $_.Outcome -eq 'PASS' })
$failed    = @($rated | Where-Object { $_.Outcome -eq 'FAIL' })
$retries   = @($rows | Where-Object { $_.IsRetry })
$clean     = @($passed | Where-Object { $_.PlanRevs -eq 0 -and $_.Corrections -eq 0 })

Write-Output 'Summary'
if ($rated.Count -eq 0) {
    Write-Output '  No control-reviewed runs retained on this machine yet.'
} else {
    $totalCorr = [int]($rated | Measure-Object -Property Corrections -Sum).Sum
    $totalPlan = [int]($rated | Measure-Object -Property PlanRevs -Sum).Sum
    Write-Output ('  Reviewed runs:      {0}' -f $rated.Count)
    Write-Output ('  Passed:             {0}   ({1})' -f $passed.Count, (Format-Pct $passed.Count $rated.Count))
    Write-Output ('  Failed:             {0}   (control never reached pass)' -f $failed.Count)
    Write-Output ('  First-pass-clean:   {0} / {1}   (passed, 0 plan revisions, 0 correction rounds)' -f $clean.Count, $rated.Count)
    Write-Output ('  Retries archived:   {0}   (.attemptN run dir = a failed run that was re-dispatched)' -f $retries.Count)
    Write-Output ('  Correction rounds:  {0} total   (avg {1:N2} per run)' -f $totalCorr, ($totalCorr / [double]$rated.Count))
    Write-Output ('  Plan revisions:     {0} total   (avg {1:N2} per run)' -f $totalPlan, ($totalPlan / [double]$rated.Count))
    $durRuns = @($rated | Where-Object { $null -ne $_.DurationMin })
    if ($durRuns.Count -gt 0) {
        Write-Output ('  Avg duration:       {0:N1} min' -f ($durRuns | Measure-Object -Property DurationMin -Average).Average)
    }
}
Write-Output ''

# --- Why the non-passing runs did not pass -----------------------------------
$notes = @($shown | Where-Object { $_.Outcome -ne 'PASS' -and $_.Summary })
if ($notes.Count -gt 0) {
    Write-Output 'Non-pass runs -- Codex control summary'
    foreach ($r in $notes) {
        $s = $r.Summary
        if ($s.Length -gt 240) { $s = $s.Substring(0, 237) + '...' }
        Write-Output ('  {0}  [{1}]' -f $r.Run, $r.Verdict)
        Write-Output ('    {0}' -f $s)
    }
    Write-Output ''
}
