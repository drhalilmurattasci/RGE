#Requires -Version 5.1
<#
.SYNOPSIS
    Standalone live terminal-window watcher for an active AI dispatch run.

.DESCRIPTION
    Tails a dispatch-<ID>/ run-dir and prints stage transitions with
    timing, color-coded output, and periodic heartbeats. Designed for a
    second PowerShell window kept open alongside the running dispatch, so
    you can see progress without checking files by hand.

    Detects three terminal states and exits cleanly:

      - BRANCH AHEAD          Queue runner committed work on
                              ai-dispatch/<ID> (branch ahead of origin/main).
                              Exit code 0.
      - HALT SENTINEL         .ai/dispatch.auto-halt was written.
                              Exit code 1.
      - DISPATCH-FAILED LABEL Issue has ai-dispatch-failed (polled via gh).
                              Exit code 2.

    During the run, heartbeats highlight a possible hang:

      - silent_for >= 600s    Red — likely hung; investigate.
      - silent_for >= 180s    Yellow — long quiet, but plausible.
      - silent_for <  180s    Dark gray — normal.

.PARAMETER DispatchId
    Dispatch ID (e.g. ISSUE-91 or MY-PROJECT-TASK-001). If omitted, the
    most-recently-modified .ai/dispatch-* directory is auto-detected.

.PARAMETER RepoRoot
    Repository root. Defaults to the current working directory. Must be a
    git repo with the .ai/ subdir.

.PARAMETER BranchName
    Branch to compare against origin/main for the "branch ahead" success
    signal. Defaults to ai-dispatch/<DispatchId>.

.PARAMETER HeartbeatSeconds
    Heartbeat interval. Default 30s. Lower = noisier but more responsive;
    higher = quieter but slower to surface a true hang.

.PARAMETER PollSeconds
    File-system poll interval for stage transitions. Default 3s. The
    heartbeat tick uses HeartbeatSeconds independently.

.PARAMETER LabelPollMinutes
    How often to poll GitHub for the issue's labels (looking for
    ai-dispatch-failed). Default 5 min. Set to 0 to disable label polling
    (no `gh` invocations).

.PARAMETER NoColor
    Disable colored output (use plain Write-Output instead of Write-Host).
    Useful if you're piping the output to a file.

.EXAMPLE
    .\Watch-DispatchStages.ps1 -DispatchId ISSUE-91

.EXAMPLE
    .\Watch-DispatchStages.ps1   # auto-detect latest

.EXAMPLE
    .\Watch-DispatchStages.ps1 -DispatchId MY-PROJ-001 -HeartbeatSeconds 60 -LabelPollMinutes 0

.NOTES
    Read-only watcher. Touches nothing in the repo. Safe to run alongside
    a live dispatch or to point at a completed one for retrospective
    review.
#>

[CmdletBinding()]
param(
    [string]$DispatchId,
    [string]$RepoRoot = (Get-Location).Path,
    [string]$BranchName,
    [int]$HeartbeatSeconds = 30,
    [int]$PollSeconds = 3,
    [int]$LabelPollMinutes = 5,
    [switch]$NoColor
)

$ErrorActionPreference = 'Stop'

# -------------------------------------------------------------------------
# Preflight
# -------------------------------------------------------------------------

if (-not (Test-Path -LiteralPath (Join-Path $RepoRoot '.git'))) {
    Write-Error "Not a git repo: $RepoRoot"
    exit 1
}
Set-Location -LiteralPath $RepoRoot

# Auto-detect latest dispatch ID
if (-not $DispatchId) {
    $dispatchDirs = Get-ChildItem -Path '.ai' -Directory -Filter 'dispatch-*' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending
    if (-not $dispatchDirs) {
        Write-Error "No .ai/dispatch-* directories found in $RepoRoot. Pass -DispatchId explicitly."
        exit 1
    }
    $DispatchId = $dispatchDirs[0].Name -replace '^dispatch-', ''
}

$DispatchDir = Join-Path $RepoRoot ".ai/dispatch-$DispatchId"
if (-not (Test-Path -LiteralPath $DispatchDir)) {
    Write-Error "Dispatch dir not found: $DispatchDir"
    exit 1
}

if (-not $BranchName) {
    $BranchName = "ai-dispatch/$DispatchId"
}

$HaltSentinel = Join-Path $RepoRoot '.ai/dispatch.auto-halt'

# -------------------------------------------------------------------------
# Helpers
# -------------------------------------------------------------------------

function Out-ColoredLine {
    param([string]$Text, [string]$Color = 'White')
    if ($NoColor) {
        Write-Output $Text
    } else {
        try {
            Write-Host $Text -ForegroundColor $Color
        } catch {
            # Fallback if host doesn't support the color
            Write-Output $Text
        }
    }
}

function Format-Elapsed {
    param([datetime]$From)
    $span = (Get-Date) - $From
    $m = [int][Math]::Floor($span.TotalMinutes)
    $s = $span.Seconds
    return ('{0}m{1:00}s' -f $m, $s)
}

function Get-FileSize {
    param([string]$Path)
    try {
        return (Get-Item -LiteralPath $Path -ErrorAction Stop).Length
    } catch {
        return 0
    }
}

function Get-FileMtime {
    param([string]$Path)
    try {
        return (Get-Item -LiteralPath $Path -ErrorAction Stop).LastWriteTime
    } catch {
        return $null
    }
}

function Test-Native {
    param([string]$Name)
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Invoke-NativeQuiet {
    # Run a native command with EAP isolation (PS 5.1 stderr trap).
    # Returns @{Code, Stdout}. Failures do not throw.
    param([string]$Exe, [string[]]$ArgList)
    $tmp = [System.IO.Path]::GetTempFileName()
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & $Exe @ArgList > $tmp 2>&1
    } finally {
        $ErrorActionPreference = $prev
    }
    $code = $LASTEXITCODE
    $out = (Get-Content -Raw -LiteralPath $tmp -ErrorAction SilentlyContinue)
    if ($null -eq $out) { $out = '' }
    Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
    return @{ Code = $code; Stdout = $out }
}

# -------------------------------------------------------------------------
# Stage definitions — file marker -> label, in approximate order of
# appearance. The watcher reports each file as it first appears.
# -------------------------------------------------------------------------

$Stages = @(
    @{ File = 'claude.ready.envelope.json';    Label = 'ready'                       },
    @{ File = 'codex.plan.rev0.log';           Label = 'codex plan rev 0'            },
    @{ File = 'codex.plan.rev1.log';           Label = 'codex plan rev 1'            },
    @{ File = 'codex.plan.rev2.log';           Label = 'codex plan rev 2'            },
    @{ File = 'claude.plan_gate.rev0.md';      Label = 'claude plan-gate rev 0'      },
    @{ File = 'claude.plan_gate.rev1.md';      Label = 'claude plan-gate rev 1'      },
    @{ File = 'claude.plan_gate.rev2.md';      Label = 'claude plan-gate rev 2'      },
    @{ File = 'claude.execute.round0.md';      Label = 'claude execute round 0'     },
    @{ File = 'verification.round0.log';       Label = 'verification round 0'        },
    @{ File = 'codex.control.round0.log';      Label = 'codex control round 0 (active)' },
    @{ File = 'codex.control.round0.json';     Label = 'codex control round 0 FINALIZED' },
    @{ File = 'codex.correct.round0.log';      Label = 'codex CORRECTION round 0'    },
    @{ File = 'claude.execute.round1.md';      Label = 'claude execute round 1'      },
    @{ File = 'verification.round1.log';       Label = 'verification round 1'        },
    @{ File = 'codex.control.round1.log';      Label = 'codex control round 1 (active)' },
    @{ File = 'codex.control.round1.json';     Label = 'codex control round 1 FINALIZED' },
    @{ File = 'codex.correct.round1.log';      Label = 'codex CORRECTION round 1'    },
    @{ File = 'claude.execute.round2.md';      Label = 'claude execute round 2'      },
    @{ File = 'verification.round2.log';       Label = 'verification round 2'        },
    @{ File = 'codex.control.round2.log';      Label = 'codex control round 2 (active)' },
    @{ File = 'codex.control.round2.json';     Label = 'codex control round 2 FINALIZED' }
)

$EnvelopeFiles = @(
    'claude.execute.round0.envelope.json',
    'claude.execute.round1.envelope.json',
    'claude.execute.round2.envelope.json'
)

# -------------------------------------------------------------------------
# Resolve anchor time
# -------------------------------------------------------------------------

$readyFile = Join-Path $DispatchDir 'claude.ready.envelope.json'
$startTime = Get-FileMtime $readyFile
if (-not $startTime) {
    $startTime = (Get-Item -LiteralPath $DispatchDir).LastWriteTime
}

$gitAvailable = Test-Native 'git'
$ghAvailable = Test-Native 'gh'
if (-not $gitAvailable) {
    Out-ColoredLine "WARN: git not on PATH — branch-ahead success signal disabled." 'Yellow'
}
if (-not $ghAvailable -and $LabelPollMinutes -gt 0) {
    Out-ColoredLine "WARN: gh not on PATH — label-poll failure signal disabled." 'Yellow'
    $LabelPollMinutes = 0
}

# -------------------------------------------------------------------------
# Initial banner
# -------------------------------------------------------------------------

Out-ColoredLine ('=' * 78) 'DarkGray'
Out-ColoredLine "Dispatch watcher  --  $DispatchId" 'Cyan'
Out-ColoredLine ("Repo:       {0}" -f $RepoRoot) 'Gray'
Out-ColoredLine ("Run dir:    .ai/dispatch-{0}" -f $DispatchId) 'Gray'
Out-ColoredLine ("Branch:     {0}" -f $BranchName) 'Gray'
Out-ColoredLine ("Anchor:     {0:yyyy-MM-dd HH:mm:ss}" -f $startTime) 'Gray'
Out-ColoredLine ("Heartbeat:  every {0}s   poll: every {1}s   label-poll: every {2}m" -f $HeartbeatSeconds, $PollSeconds, $LabelPollMinutes) 'Gray'
Out-ColoredLine ('=' * 78) 'DarkGray'

# -------------------------------------------------------------------------
# Initial sweep — announce already-completed stages
# -------------------------------------------------------------------------

$Seen = @{}
foreach ($stage in $Stages) {
    $path = Join-Path $DispatchDir $stage.File
    if (Test-Path -LiteralPath $path) {
        $Seen[$stage.File] = $true
        $sz = Get-FileSize $path
        $mt = Get-FileMtime $path
        $line = "[{0}]  ok    {1,-42}  ({2} B at {3:HH:mm:ss})" -f (Format-Elapsed $startTime), $stage.Label, $sz, $mt
        Out-ColoredLine $line 'DarkGray'
    }
}
Out-ColoredLine ('-' * 78) 'DarkGray'

# -------------------------------------------------------------------------
# Main loop
# -------------------------------------------------------------------------

$lastHeartbeat = Get-Date
$lastLabelPoll = Get-Date
$lastEnvSize = @{}
foreach ($f in $EnvelopeFiles) {
    $lastEnvSize[$f] = Get-FileSize (Join-Path $DispatchDir $f)
}

while ($true) {
    # Terminal 1 — halt sentinel
    if (Test-Path -LiteralPath $HaltSentinel) {
        Out-ColoredLine ('-' * 78) 'DarkGray'
        Out-ColoredLine ("[{0}]  HALT SENTINEL appeared at {1}" -f (Format-Elapsed $startTime), $HaltSentinel) 'Red'
        try {
            Get-Content -LiteralPath $HaltSentinel | ForEach-Object { Out-ColoredLine ("    {0}" -f $_) 'Red' }
        } catch { }
        exit 1
    }

    # Terminal 2 — branch ahead of origin/main
    if ($gitAvailable) {
        $r = Invoke-NativeQuiet -Exe 'git' -ArgList @('rev-list', '--count', "origin/main..$BranchName")
        if ($r.Code -eq 0) {
            $ahead = 0
            [int]::TryParse($r.Stdout.Trim(), [ref]$ahead) | Out-Null
            if ($ahead -gt 0) {
                $h = Invoke-NativeQuiet -Exe 'git' -ArgList @('log', '--format=%h %s', '-1', $BranchName)
                $headStr = if ($h.Code -eq 0) { $h.Stdout.Trim() } else { '?' }
                Out-ColoredLine ('-' * 78) 'DarkGray'
                Out-ColoredLine ("[{0}]  BRANCH AHEAD: {1} @ {2} ({3} commit(s) ahead of origin/main)" -f (Format-Elapsed $startTime), $BranchName, $headStr, $ahead) 'Green'
                exit 0
            }
        }
    }

    # Terminal 3 — ai-dispatch-failed label on the issue (slower poll)
    if ($LabelPollMinutes -gt 0 -and ((Get-Date) - $lastLabelPoll).TotalMinutes -ge $LabelPollMinutes) {
        $lastLabelPoll = Get-Date
        # Try to parse issue number from DispatchId (e.g. ISSUE-91 -> 91)
        $issueNum = 0
        if ($DispatchId -match 'ISSUE-(\d+)') {
            $issueNum = [int]$matches[1]
        }
        if ($issueNum -gt 0) {
            $g = Invoke-NativeQuiet -Exe 'gh' -ArgList @('issue', 'view', $issueNum.ToString(), '--json', 'labels', '--jq', '[.labels[].name]')
            if ($g.Code -eq 0 -and $g.Stdout) {
                try {
                    $labels = $g.Stdout | ConvertFrom-Json
                    if ($labels -contains 'ai-dispatch-failed') {
                        Out-ColoredLine ('-' * 78) 'DarkGray'
                        Out-ColoredLine ("[{0}]  ai-dispatch-failed LABEL on issue #{1} -- queue runner reported failure" -f (Format-Elapsed $startTime), $issueNum) 'Red'
                        exit 2
                    }
                } catch { }
            }
        }
    }

    # Stage-file appearance detection
    foreach ($stage in $Stages) {
        $path = Join-Path $DispatchDir $stage.File
        if ((Test-Path -LiteralPath $path) -and -not $Seen.ContainsKey($stage.File)) {
            $Seen[$stage.File] = $true
            $sz = Get-FileSize $path
            $line = "[{0}]  >>>   {1,-42}  ({2} B)" -f (Format-Elapsed $startTime), $stage.Label, $sz
            Out-ColoredLine $line 'Cyan'

            # Parse Codex control verdicts when finalized
            if ($stage.File -like 'codex.control.round*.json') {
                try {
                    $content = Get-Content -Raw -LiteralPath $path -ErrorAction Stop | ConvertFrom-Json
                    $verdictColor = switch ($content.verdict) {
                        'pass'          { 'Green' }
                        'needs_changes' { 'Yellow' }
                        'block'         { 'Red' }
                        default         { 'Gray' }
                    }
                    Out-ColoredLine ("       verdict          = {0}" -f $content.verdict) $verdictColor
                    Out-ColoredLine ("       commit_readiness = {0}" -f $content.commit_readiness) $verdictColor
                    if ($content.required_fixes -and $content.required_fixes.Count -gt 0) {
                        Out-ColoredLine ("       required_fixes:") 'Yellow'
                        foreach ($fix in $content.required_fixes) {
                            Out-ColoredLine ("         - {0}" -f $fix) 'Yellow'
                        }
                    }
                } catch {
                    Out-ColoredLine ("       (could not parse control JSON: {0})" -f $_.Exception.Message) 'Yellow'
                }
            }
        }
    }

    # Envelope-size growth (claude returned signal)
    foreach ($f in $EnvelopeFiles) {
        $path = Join-Path $DispatchDir $f
        if (Test-Path -LiteralPath $path) {
            $sz = Get-FileSize $path
            $prev = if ($lastEnvSize.ContainsKey($f)) { $lastEnvSize[$f] } else { 0 }
            if ($sz -gt 0 -and $prev -eq 0) {
                Out-ColoredLine ("[{0}]  >>>   {1} ENVELOPE non-empty ({2} B) -- claude returned" -f (Format-Elapsed $startTime), $f, $sz) 'Green'
            }
            $lastEnvSize[$f] = $sz
        }
    }

    # Heartbeat
    if (((Get-Date) - $lastHeartbeat).TotalSeconds -ge $HeartbeatSeconds) {
        $latest = Get-ChildItem -LiteralPath $DispatchDir -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($latest) {
            $silent = [int]((Get-Date) - $latest.LastWriteTime).TotalSeconds
            $color = if ($silent -ge 600) { 'Red' }
                     elseif ($silent -ge 180) { 'Yellow' }
                     else { 'DarkGray' }
            $hint = ''
            if ($silent -ge 600) {
                $hint = '  [HANG SUSPECTED]'
            }
            $line = "[{0}]  hb    latest={1}  size={2} B  silent_for={3}s{4}" -f (Format-Elapsed $startTime), $latest.Name, $latest.Length, $silent, $hint
            Out-ColoredLine $line $color
        } else {
            Out-ColoredLine ("[{0}]  hb    (no files in run-dir yet)" -f (Format-Elapsed $startTime)) 'DarkGray'
        }
        $lastHeartbeat = Get-Date
    }

    Start-Sleep -Seconds $PollSeconds
}
