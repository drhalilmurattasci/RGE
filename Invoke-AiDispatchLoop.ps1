#Requires -Version 5.1
<#
.SYNOPSIS
    Run a Codex-plans, configurable-executor, Codex-controls dispatch loop.

.DESCRIPTION
    This is a thin orchestration layer over the canonical ai_handoffs/
    packet protocol. It automates model routing, but it does not commit or
    push. Human authorization remains required for any git publish step.

    Flow:
      1. Scaffold TASK packet.
      2. Ask Codex to fill the TASK packet from the supplied goal.
      3. Ask the selected executor to review the TASK as an executor gate.
      4. If the executor gate approves, finalize the TASK sidecar.
      5. Ask the selected executor to execute and write/finalize an
         EXECUTION_REPORT. The default executor is Codex; `-Executor claude`
         is an explicit opt-in.
      6. Run the verification gate (.ai/dispatch.verify.ps1). A non-zero
         exit fails the round before any control review runs.
      7. Ask Codex to perform a read-only control review of the diff,
         packets, and verification results.

    If Codex control returns needs_changes and MaxCorrectionRounds is greater
    than zero, the script asks Codex to write a CORRECTION_PACKET and routes
    that packet back to the selected executor for another execution round.

    With -ResumeApprovedTask, steps 1-4 are skipped: the loop locates the
    already-approved, finalized TASK packet for the given DispatchId and runs
    only the execution and control phase (steps 5-6).

.EXAMPLE
    .\Invoke-AiDispatchLoop.ps1 `
      -DispatchId POSTV0-HANDOFF-ARTIFACT-TRIAGE-001 `
      -Goal "Audit untracked handoff artifacts and recommend cleanup. No edits."

.EXAMPLE
    # Resume mode: skip planning, execute an already-approved + finalized TASK
    # packet (one that has a .meta.json sidecar) without scaffolding a new one.
    .\Invoke-AiDispatchLoop.ps1 `
      -DispatchId POSTV0-HANDOFF-ARTIFACT-TRIAGE-004 `
      -ResumeApprovedTask

.NOTES
    Requires local `codex`, `git`, `.mcp.json`, `new-handoff.ps1`, and the
    ai_handoffs packet templates. `claude` is required only when
    `-Executor claude` is explicitly selected.
#>
[CmdletBinding(DefaultParameterSetName = 'GoalText')]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$DispatchId,

    [Parameter(Mandatory, ParameterSetName = 'GoalText')]
    [string]$Goal,

    [Parameter(Mandatory, ParameterSetName = 'GoalFile')]
    [string]$GoalFile,

    [ValidateRange(0, 5)]
    [int]$MaxPlanRevisions = 2,

    [ValidateRange(0, 5)]
    [int]$MaxCorrectionRounds = 1,

    [ValidateRange(0, 5)]
    [int]$MutationRetryCount = 1,

    [ValidateSet('acceptEdits', 'auto', 'bypassPermissions', 'default', 'dontAsk', 'plan')]
    [string]$ClaudePermissionMode = 'acceptEdits',

    [ValidateSet('claude', 'codex')]
    [string]$Executor = 'codex',

    [switch]$CodexExecutorExternalScratch,

    [string]$CodexModel = '',

    [string]$ClaudeModel = '',

    [switch]$AllowDirtyTracked,

    [switch]$PlanOnly,

    [string]$VerifyScript = '',

    [switch]$SkipVerification,

    [ValidateRange(60, 7200)]
    [int]$ModelTimeoutSec = 1800,

    [ValidateRange(120, 14400)]
    [int]$VerifyTimeoutSec = 3600,

    [ValidateRange(0, 3600)]
    [int]$CodexStallThresholdSec = 0,

    [Parameter(Mandatory, ParameterSetName = 'ResumeTask')]
    [switch]$ResumeApprovedTask,

    [switch]$EnablePreflightAudit
)

$ErrorActionPreference = 'Stop'

$script:PlanRevisionGateContextMaxChars = 20000

# When $true, Fail throws instead of exiting so an eligible read-only model
# review phase (Claude plan gate / Codex control) can be wrapped by a
# same-phase retry. Reset to $false outside the retry wrapper so unrelated
# Fail calls still exit the process with the original final wording.
$script:RetryableFailEnabled = $false

function Fail {
    param([string]$Message)
    if ($script:RetryableFailEnabled) {
        throw $Message
    }
    [Console]::Error.WriteLine($Message)
    exit 1
}

function Invoke-WithSamePhaseRetry {
    # Same-phase retry for the two read-only model review phases
    # (Invoke-ClaudePlanGate and Invoke-CodexControl). Enables the
    # RetryableFailEnabled flag so a Fail surfaced from the model-call path
    # propagates as a catchable exception instead of exiting the process.
    # On a first failure the listed CleanupPaths are removed so a partial
    # first attempt cannot be mistaken for the retry's result, then the
    # action runs up to RetryCount more times. RetryCount=0 preserves the old
    # single-attempt behavior. On retry exhaustion the flag is cleared and
    # Fail is re-invoked with the final attempt's message so the original
    # failure path (including the Codex stall watchdog wording) is preserved.
    param(
        [Parameter(Mandatory)] [string]$PhaseLabel,
        [Parameter(Mandatory)] [scriptblock]$Action,
        [ValidateRange(0, 5)]
        [int]$RetryCount = 1,
        [string[]]$CleanupPaths = @()
    )

    $script:RetryableFailEnabled = $true
    try {
        $maxAttempts = 1 + $RetryCount
        for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
            try {
                $result = & $Action
                if ($attempt -gt 1) {
                    Write-Host "[retry] $PhaseLabel succeeded on attempt $attempt of $maxAttempts."
                }
                return $result
            } catch {
                $message = [string]$_.Exception.Message
                if ($attempt -ge $maxAttempts) {
                    if ($RetryCount -gt 0) {
                        Write-Host "[retry] $PhaseLabel same-phase retry exhausted ($maxAttempts/$maxAttempts); failing via the original failure path."
                    }
                    $script:RetryableFailEnabled = $false
                    Fail $message
                }
                Write-Host "[retry] $PhaseLabel attempt $attempt failed: $message"
                foreach ($p in $CleanupPaths) {
                    if ($p -and (Test-Path -LiteralPath $p)) {
                        Remove-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue
                    }
                }
                $nextAttempt = $attempt + 1
                Write-Host "[retry] $PhaseLabel retrying once (attempt $nextAttempt of $maxAttempts)..."
            }
        }
    } finally {
        $script:RetryableFailEnabled = $false
    }
}

function New-MutationSnapshot {
    # Capture a phase-entry snapshot of the worktree.
    #
    # Tracked changes (staged + unstaged) go into a `git stash create`
    # commit, pinned via a dedicated ref under refs/dispatch-snapshot/ so a
    # stray `git gc` cannot reap it during the retry window. A clean tracked
    # state yields an empty SHA and no ref is pinned.
    #
    # Untracked non-ignored files are captured into an in-memory byte map.
    # This avoids `git stash create -u`, whose untracked parent tree is not
    # reliably extracted by `git stash apply` on Git for Windows, and keeps
    # ignored paths (the .ai/dispatch-*/ run dir, *.log, build outputs)
    # outside the snapshot so external caches survive across retries.
    param([Parameter(Mandatory)] [string]$Label)

    $rootResult = Invoke-GitCapture @('rev-parse', '--show-toplevel')
    if ($rootResult.Code -ne 0 -or -not $rootResult.Lines) {
        Fail "Mutation snapshot: cannot resolve git repository root."
    }
    $repoRoot = ($rootResult.Lines -join "`n").Trim()
    # git on Windows returns forward slashes; normalize so Join-Path with
    # platform separators produces a single canonical path.
    $repoRoot = $repoRoot -replace '/', [System.IO.Path]::DirectorySeparatorChar

    $shaResult = Invoke-GitCapture @('stash', 'create')
    if ($shaResult.Code -ne 0) {
        Fail "Mutation snapshot: 'git stash create' failed (exit $($shaResult.Code))."
    }
    $sha = ($shaResult.Lines -join "`n").Trim()
    if (-not $sha) { $sha = $null }

    $refName = $null
    if ($sha) {
        $safe = ($Label -replace '[^A-Za-z0-9._\-]', '-')
        $refName = "refs/dispatch-snapshot/$safe"
        $update = Invoke-GitCapture @('update-ref', $refName, $sha)
        if ($update.Code -ne 0) {
            Fail "Mutation snapshot: 'git update-ref $refName $sha' failed (exit $($update.Code))."
        }
    }

    $lsResult = Invoke-GitCapture @('ls-files', '--others', '--exclude-standard')
    if ($lsResult.Code -ne 0) {
        Fail "Mutation snapshot: 'git ls-files --others' failed (exit $($lsResult.Code))."
    }
    # Ordered hashtable so restore is deterministic in path order.
    $untracked = [ordered]@{}
    foreach ($rel in $lsResult.Lines) {
        $rel = [string]$rel
        if (-not $rel) { continue }
        $full = Join-Path $repoRoot ($rel -replace '/', [System.IO.Path]::DirectorySeparatorChar)
        if (Test-Path -LiteralPath $full -PathType Leaf) {
            $untracked[$rel] = [System.IO.File]::ReadAllBytes($full)
        }
    }

    return [pscustomobject]@{
        Sha       = $sha
        Ref       = $refName
        Untracked = $untracked
        RepoRoot  = $repoRoot
    }
}

function Restore-MutationSnapshot {
    # Restore the worktree to the state captured by New-MutationSnapshot:
    #   1. `git reset --hard HEAD`  -- discard staged + unstaged tracked
    #      changes from the failed attempt.
    #   2. `git clean -fd`           -- delete untracked non-ignored files
    #      and directories created by the failed attempt. Ignored files
    #      (run-dir transcripts, *.log, /target/) are preserved.
    #   3. `git stash apply --index` -- replay the snapshot's tracked
    #      changes back onto the working tree and index.
    #   4. Rewrite the captured untracked manifest so phase-entry untracked
    #      non-ignored files come back with their original bytes.
    # All git invocations are scoped to the repository root captured at
    # snapshot time; no paths outside the repository are touched.
    param([Parameter(Mandatory)] [pscustomobject]$Snapshot)

    $reset = Invoke-GitCapture @('reset', '--hard', '--quiet', 'HEAD')
    if ($reset.Code -ne 0) {
        Fail "Mutation snapshot restore: 'git reset --hard HEAD' failed (exit $($reset.Code))."
    }
    $clean = Invoke-GitCapture @('clean', '-fd', '--quiet')
    if ($clean.Code -ne 0) {
        Fail "Mutation snapshot restore: 'git clean -fd' failed (exit $($clean.Code))."
    }
    if ($Snapshot.Sha) {
        $apply = Invoke-GitCapture @('stash', 'apply', '--index', '--quiet', $Snapshot.Sha)
        if ($apply.Code -ne 0) {
            Fail "Mutation snapshot restore: 'git stash apply --index $($Snapshot.Sha)' failed (exit $($apply.Code))."
        }
    }
    if ($Snapshot.Untracked -and $Snapshot.Untracked.Count -gt 0) {
        foreach ($entry in $Snapshot.Untracked.GetEnumerator()) {
            $rel = [string]$entry.Key
            $bytes = [byte[]]$entry.Value
            $full = Join-Path $Snapshot.RepoRoot ($rel -replace '/', [System.IO.Path]::DirectorySeparatorChar)
            $parent = Split-Path -Parent $full
            if ($parent -and -not (Test-Path -LiteralPath $parent)) {
                New-Item -ItemType Directory -Path $parent -Force | Out-Null
            }
            [System.IO.File]::WriteAllBytes($full, $bytes)
        }
    }
}

function Remove-MutationSnapshot {
    # Drop the pinned snapshot ref so the stash commit is no longer kept
    # alive and a later `git gc` can reclaim it. Safe to call with a null
    # snapshot or one that recorded no changes.
    param([pscustomobject]$Snapshot)

    if ($Snapshot -and $Snapshot.Ref) {
        Invoke-GitCapture @('update-ref', '-d', $Snapshot.Ref) | Out-Null
    }
}

function Invoke-WithMutationRetry {
    # Snapshot-backed same-phase retry for the two dispatch-loop mutation
    # phases that can edit the repository (Invoke-ClaudeExecute and
    # Invoke-CorrectionPacket). RetryCount=0 short-circuits to the previous
    # single-attempt behavior: no snapshot, no restore, no retry output, and
    # Fail still exits the process directly so unrelated callers see the
    # original failure path. With RetryCount > 0, a phase-entry snapshot is
    # captured once; on each attempt failure the worktree is restored from
    # that snapshot before the next attempt runs. On retry exhaustion the
    # original failure path is taken via the existing Fail helper, so the
    # terminal wording the queue runner relies on is preserved.
    param(
        [Parameter(Mandatory)] [string]$PhaseLabel,
        [Parameter(Mandatory)] [scriptblock]$Action,
        [ValidateRange(0, 5)]
        [int]$RetryCount = 1
    )

    if ($RetryCount -le 0) {
        return & $Action
    }

    $label = "{0}-{1}" -f $DispatchId, [System.IO.Path]::GetRandomFileName().Substring(0, 8)
    $snapshot = New-MutationSnapshot -Label $label
    $script:RetryableFailEnabled = $true
    try {
        $maxAttempts = 1 + $RetryCount
        for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
            try {
                $result = & $Action
                if ($attempt -gt 1) {
                    Write-Host "[mutation-retry] $PhaseLabel succeeded on attempt $attempt of $maxAttempts."
                }
                return $result
            } catch {
                $message = [string]$_.Exception.Message
                if ($attempt -ge $maxAttempts) {
                    Write-Host "[mutation-retry] $PhaseLabel same-phase retry exhausted ($maxAttempts/$maxAttempts); failing via the original failure path."
                    $script:RetryableFailEnabled = $false
                    Fail $message
                    return
                }
                Write-Host "[mutation-retry] $PhaseLabel attempt $attempt failed: $message"
                Write-Host "[mutation-retry] $PhaseLabel restoring worktree to phase-entry snapshot before retry..."
                try {
                    Restore-MutationSnapshot -Snapshot $snapshot
                } catch {
                    $restoreErr = [string]$_.Exception.Message
                    $script:RetryableFailEnabled = $false
                    Fail "Mutation retry restore failed for $PhaseLabel after attempt ${attempt}: $restoreErr"
                    return
                }
                Write-Host "[mutation-retry] $PhaseLabel worktree restored to phase-entry state; retrying (attempt $($attempt + 1) of $maxAttempts)..."
            }
        }
    } finally {
        $script:RetryableFailEnabled = $false
        Remove-MutationSnapshot -Snapshot $snapshot
    }
}

function Require-Command {
    param([string]$Name)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Fail "Required command not found on PATH: $Name"
    }
}

function Write-TextFile {
    param([string]$Path, [string]$Text)
    $parent = Split-Path -Parent $Path
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Read-JsonFile {
    param([string]$Path)
    try {
        return (Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json)
    } catch {
        Fail "Could not parse JSON at $Path. Error: $($_.Exception.Message)"
    }
}

function Get-RepoRelativePath {
    param([string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    $root = [System.IO.Path]::GetFullPath($script:RepoRoot).TrimEnd('\', '/')
    if ($full.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
        return (($full.Substring($root.Length)).TrimStart('\', '/') -replace '\\', '/')
    }
    return ($full -replace '\\', '/')
}

function Get-LatestPacket {
    param([string]$PacketType)
    $filter = "${DispatchId}_${PacketType}_*.md"
    return Get-ChildItem -LiteralPath $script:HandoffDir -Filter $filter -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc, Name |
        Select-Object -Last 1
}

function Invoke-NewPacket {
    param([string]$PacketType, [string]$Author)
    $global:LASTEXITCODE = 0
    $output = & $script:NewHandoff -DispatchId $DispatchId -PacketType $PacketType -Author $Author
    if ($LASTEXITCODE -ne 0) {
        Fail "new-handoff.ps1 failed while creating $PacketType packet."
    }
    $packetPath = ($output | Select-Object -First 1)
    if (-not $packetPath -or -not (Test-Path -LiteralPath $packetPath)) {
        Fail "Could not determine created $PacketType packet path."
    }
    return (Get-Item -LiteralPath $packetPath)
}

function Test-PacketFinalizeDryRun {
    param([System.IO.FileInfo]$Packet, [string]$LogPath)
    $global:LASTEXITCODE = 0
    & $script:NewHandoff -Finalize -PacketPath $Packet.FullName -DryRun > $LogPath 2>&1
    if ($LASTEXITCODE -ne 0) {
        Fail "Packet did not pass finalize dry-run validation: $(Get-RepoRelativePath $Packet.FullName). See $LogPath"
    }
}

function Finalize-Packet {
    param([System.IO.FileInfo]$Packet)
    $sidecarPath = $Packet.FullName -replace '\.md$', '.meta.json'
    if (Test-Path -LiteralPath $sidecarPath) {
        return (Get-Item -LiteralPath $sidecarPath)
    }
    $global:LASTEXITCODE = 0
    $output = & $script:NewHandoff -Finalize -PacketPath $Packet.FullName
    if ($LASTEXITCODE -ne 0) {
        Fail "Finalizing packet failed: $(Get-RepoRelativePath $Packet.FullName)"
    }
    $created = ($output | Select-Object -First 1)
    if (-not $created -or -not (Test-Path -LiteralPath $created)) {
        Fail "Could not determine sidecar path after finalizing $(Get-RepoRelativePath $Packet.FullName)"
    }
    return (Get-Item -LiteralPath $created)
}

function Invoke-WithTimeout {
    # Run a native command with a hard timeout. The command runs inside a
    # child powershell so that, on timeout, its whole process tree can be
    # killed (taskkill /T) -- a hung cargo/codex/claude cannot then sit until
    # the scheduled task kills the entire queue. Arguments are passed via a
    # clixml params file so the call operator quotes them correctly, including
    # a multi-line prompt argument. stdout goes to OutFile; stderr to ErrFile
    # if given, else merged into OutFile. Returns @{ Code; TimedOut; Stalled }.
    #
    # Optional Codex-only stall watchdog: when -StallThresholdSec is greater
    # than 0, the child is polled and OutFile is watched for progress. The
    # watchdog only arms once OutFile becomes non-empty, then treats output
    # activity as an increase in OutFile.Length. If the size does not advance
    # for StallThresholdSec, the same process tree is killed and the result
    # becomes @{ Code = 125; TimedOut = $true; Stalled = $true }. When
    # -StallThresholdSec is 0 or omitted, the legacy hard-timeout control flow
    # runs unchanged and the additive Stalled field remains $false.
    param(
        [string]$Exe,
        [string[]]$Arguments,
        [string]$OutFile,
        [int]$TimeoutSec,
        [string]$StdinFile = '',
        [string]$ErrFile = '',
        [ValidateRange(0, 3600)]
        [int]$StallThresholdSec = 0,
        [ValidateRange(1, 600)]
        [int]$PollIntervalSec = 5
    )
    $base = [System.IO.Path]::GetTempFileName()
    $paramsFile = "$base.params.xml"
    $launcherFile = "$base.launcher.ps1"
    @{
        Exe = $Exe; Arguments = $Arguments; OutFile = $OutFile
        StdinFile = $StdinFile; ErrFile = $ErrFile
    } | Export-Clixml -LiteralPath $paramsFile
    $launcher = @'
param([string]$ParamsFile)
$ErrorActionPreference = 'Continue'
$p = Import-Clixml -LiteralPath $ParamsFile
$a = @($p.Arguments)
$o = [string]$p.OutFile
$e = [string]$p.ErrFile
$s = [string]$p.StdinFile
if ($s) {
    if ($e) { Get-Content -Raw -LiteralPath $s | & $p.Exe @a > $o 2> $e }
    else    { Get-Content -Raw -LiteralPath $s | & $p.Exe @a > $o 2>&1 }
} else {
    if ($e) { & $p.Exe @a > $o 2> $e }
    else    { & $p.Exe @a > $o 2>&1 }
}
exit $LASTEXITCODE
'@
    [System.IO.File]::WriteAllText($launcherFile, $launcher, [System.Text.UTF8Encoding]::new($false))
    $timedOut = $false
    $stalled = $false
    $code = 0
    try {
        $proc = Start-Process -FilePath 'powershell.exe' -PassThru -NoNewWindow -ArgumentList @(
            '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $launcherFile, $paramsFile)
        # Touch .Handle so the Process object caches it; without this a
        # -PassThru process returns $null from .ExitCode after it exits.
        $null = $proc.Handle
        if ($StallThresholdSec -le 0) {
            if ($proc.WaitForExit($TimeoutSec * 1000)) {
                $proc.WaitForExit()
                $code = $proc.ExitCode
            } else {
                $timedOut = $true
                & taskkill /T /F /PID $proc.Id *> $null
                $code = 124
            }
        } else {
            # Stall watchdog path: poll OutFile for progress and kill on
            # whichever fires first -- the existing hard timeout or the new
            # stall threshold. The watchdog arms only after OutFile reaches
            # non-zero size, so quiet startup time does not count as a stall.
            $hardDeadline = [DateTime]::UtcNow.AddSeconds($TimeoutSec)
            $armed = $false
            $lastSize = [long]0
            $stallDeadline = [DateTime]::MaxValue
            while ($true) {
                if ($proc.HasExited) {
                    $proc.WaitForExit()
                    $code = $proc.ExitCode
                    break
                }
                $now = [DateTime]::UtcNow
                if ($now -ge $hardDeadline) {
                    $timedOut = $true
                    & taskkill /T /F /PID $proc.Id *> $null
                    $code = 124
                    break
                }
                $fi = $null
                if ($OutFile -and (Test-Path -LiteralPath $OutFile)) {
                    $fi = Get-Item -LiteralPath $OutFile -ErrorAction SilentlyContinue
                }
                if ($fi -and $fi.Length -gt 0) {
                    $curSize = [long]$fi.Length
                    if (-not $armed) {
                        $armed = $true
                        $lastSize = $curSize
                        $stallDeadline = $now.AddSeconds($StallThresholdSec)
                    } elseif ($curSize -gt $lastSize) {
                        $lastSize = $curSize
                        $stallDeadline = $now.AddSeconds($StallThresholdSec)
                    } elseif ($now -ge $stallDeadline) {
                        $stalled = $true
                        $timedOut = $true
                        & taskkill /T /F /PID $proc.Id *> $null
                        $code = 125
                        break
                    }
                }
                # Cap each sleep by the nearest pending deadline so the kill
                # is prompt and we do not overshoot the hard timeout by a
                # full poll interval. 50 ms is the lower bound so we never
                # spin in a tight polling loop.
                $nextDeadline = $hardDeadline
                if ($armed -and $stallDeadline -lt $nextDeadline) {
                    $nextDeadline = $stallDeadline
                }
                $msPoll = $PollIntervalSec * 1000
                $msUntil = ($nextDeadline - $now).TotalMilliseconds
                $msSleep = if ($msUntil -lt $msPoll) { $msUntil } else { $msPoll }
                if ($msSleep -lt 50) { $msSleep = 50 }
                Start-Sleep -Milliseconds ([int]$msSleep)
            }
        }
    } finally {
        Remove-Item -LiteralPath $base, $paramsFile, $launcherFile -Force -ErrorAction SilentlyContinue
    }
    return [pscustomobject]@{ Code = $code; TimedOut = $timedOut; Stalled = $stalled }
}

function Invoke-CodexPrompt {
    param(
        [string]$Prompt,
        [ValidateSet('read-only', 'workspace-write', 'danger-full-access')]
        [string]$Sandbox,
        [string]$LogPath,
        [string]$OutputSchema = '',
        [string]$OutputPath = ''
    )

    $promptPath = Join-Path $script:RunDir 'codex.prompt.md'
    Write-TextFile $promptPath $Prompt

    $args = @('exec', '--cd', $script:RepoRoot, '--sandbox', $Sandbox)
    if ($CodexModel) { $args += @('--model', $CodexModel) }
    if ($OutputSchema) {
        $args += @('--output-schema', $OutputSchema, '--output-last-message', $OutputPath)
    }
    $args += '-'

    $r = Invoke-WithTimeout -Exe 'codex' -Arguments $args -StdinFile $promptPath `
        -OutFile $LogPath -TimeoutSec $ModelTimeoutSec -StallThresholdSec $CodexStallThresholdSec
    if ($r.Stalled) {
        Fail "codex exec stalled: no log growth for ${CodexStallThresholdSec}s after first output. Killed process tree. See $LogPath"
    }
    if ($r.TimedOut) {
        Fail "codex exec timed out after ${ModelTimeoutSec}s (terminal infrastructure failure). See $LogPath"
    }
    if ($r.Code -ne 0) {
        Fail "codex exec failed. See $LogPath"
    }
}

function Get-CodexExecutorSandbox {
    if ($CodexExecutorExternalScratch) { return 'danger-full-access' }
    return 'workspace-write'
}

function Limit-PlanRevisionGateContext {
    # Plan-gate reviews are model prose and can become very large when the
    # gate enumerates repository evidence. A revision prompt only needs the
    # actionable review tail; keep the task goal untouched and cap this
    # prior-gate context before feeding it back to Codex.
    param(
        [AllowNull()]
        [string]$Text,
        [ValidateRange(1000, 1000000)]
        [int]$MaxChars = $script:PlanRevisionGateContextMaxChars
    )

    if (-not $Text) { return $Text }
    if ($Text.Length -le $MaxChars) { return $Text }

    $kept = $Text.Substring($Text.Length - $MaxChars, $MaxChars)
    return @"
[dispatch prior-gate context truncated: original $($Text.Length) chars; kept last $MaxChars chars. The omitted text was executor preflight review prose only.]

$kept
"@
}

function Invoke-CodexPreflightAudit {
    # Opt-in pitfall audit: run Codex in a read-only sandbox, extract a
    # marker-delimited Markdown body from the current audit output, validate
    # the required headings and stable Pn / Vn IDs, write
    # .ai/dispatch-<id>/codex.preflight.md only after validation succeeds,
    # and return the validated checklist text for prompt injection.
    #
    # On any failure, Fail is called -- no codex.preflight.md is written,
    # so a stale pre-existing checklist cannot be reused by a later step.
    param([System.IO.FileInfo]$TaskPacket)

    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $log = Join-Path $script:RunDir 'codex.preflight.log'
    $checklistPath = Join-Path $script:RunDir 'codex.preflight.md'
    $beginMarker = '<<<PREFLIGHT_AUDIT_BEGIN>>>'
    $endMarker = '<<<PREFLIGHT_AUDIT_END>>>'

    $prompt = @"
You are Codex Pre-flight Auditor for an automated RGE dispatch loop.

Read this TASK packet:

$taskRel

Your job is to identify scope-preserving pitfalls and verification focus items
that will help the selected executor carry out the TASK and Codex control
review the result.
You may inspect repository context needed to identify pitfalls. You must not
edit files, stage, commit, or push. You must not expand scope: the TASK
packet remains authoritative; this checklist is advisory only.

Emit exactly one Markdown block between the two markers below. Do not write
any other prose, headings, fences, or commentary outside the markers. Each
marker must appear on its own line, anchored at column 1, exactly as shown.

$beginMarker
# Pre-flight Audit

TASK_PACKET: $taskRel

## Boundary

The TASK packet remains authoritative. This checklist is advisory and must not
expand scope.

## Pitfall Checklist

- [ ] P1: <scope-preserving pitfall text>
- [ ] P2: <scope-preserving pitfall text>

## Verification Checklist

- [ ] V1: <verification focus text>
- [ ] V2: <verification focus text>
$endMarker

Rules:
- Use stable IDs P1, P2, P3, ... for the Pitfall Checklist items.
- Use stable IDs V1, V2, V3, ... for the Verification Checklist items.
- IDs must be unique within the audit (no duplicate Pn or Vn values).
- Provide at least one Pn item and at least one Vn item.
- Keep the four headings exactly as shown: ``# Pre-flight Audit``,
  ``## Boundary``, ``## Pitfall Checklist``, ``## Verification Checklist``.
- Do not authorize edits, tests, or scope changes beyond the TASK packet.
"@

    Invoke-CodexPrompt -Prompt $prompt -Sandbox 'read-only' -LogPath $log

    if (-not (Test-Path -LiteralPath $log)) {
        Fail "Pre-flight audit produced no log file. See $(Get-RepoRelativePath $log)"
    }
    $logText = Get-Content -Raw -LiteralPath $log
    if (-not $logText) {
        Fail "Pre-flight audit log is empty. See $(Get-RepoRelativePath $log)"
    }

    # LastIndexOf so any earlier marker mention in Codex reasoning cannot
    # outrank the final emitted block.
    $startIdx = $logText.LastIndexOf($beginMarker)
    if ($startIdx -lt 0) {
        Fail "Pre-flight audit output is missing the begin marker '$beginMarker'. See $(Get-RepoRelativePath $log)"
    }
    $bodyStart = $startIdx + $beginMarker.Length
    $endIdx = $logText.IndexOf($endMarker, $bodyStart)
    if ($endIdx -lt 0) {
        Fail "Pre-flight audit output is missing the end marker '$endMarker' after the begin marker. See $(Get-RepoRelativePath $log)"
    }
    $body = $logText.Substring($bodyStart, $endIdx - $bodyStart).Trim()
    if (-not $body) {
        Fail "Pre-flight audit body between markers is empty. See $(Get-RepoRelativePath $log)"
    }

    foreach ($required in @(
        '# Pre-flight Audit',
        '## Boundary',
        '## Pitfall Checklist',
        '## Verification Checklist'
    )) {
        $pattern = '(?m)^' + [regex]::Escape($required) + '\s*$'
        if ($body -notmatch $pattern) {
            Fail "Pre-flight audit body is missing required heading '$required'. See $(Get-RepoRelativePath $log)"
        }
    }

    # Slice the body by '## ' section starts so item parsing cannot leak
    # between sections.
    $sections = [regex]::Split($body, '(?m)^(?=## )')
    $pitfallText = ($sections | Where-Object { $_ -match '^## Pitfall Checklist\b' } | Select-Object -First 1)
    $verifyText = ($sections | Where-Object { $_ -match '^## Verification Checklist\b' } | Select-Object -First 1)
    if (-not $pitfallText -or -not $verifyText) {
        Fail "Pre-flight audit body is missing a required checklist section. See $(Get-RepoRelativePath $log)"
    }

    $pItems = @()
    foreach ($line in ($pitfallText -split "`r?`n")) {
        if ($line -match '^\s*-\s*\[\s*\]\s*(P\d+)\s*:') {
            $pItems += $matches[1]
        }
    }
    $vItems = @()
    foreach ($line in ($verifyText -split "`r?`n")) {
        if ($line -match '^\s*-\s*\[\s*\]\s*(V\d+)\s*:') {
            $vItems += $matches[1]
        }
    }

    if ($pItems.Count -lt 1) {
        Fail "Pre-flight audit has no '- [ ] Pn:' items in the Pitfall Checklist. See $(Get-RepoRelativePath $log)"
    }
    if ($vItems.Count -lt 1) {
        Fail "Pre-flight audit has no '- [ ] Vn:' items in the Verification Checklist. See $(Get-RepoRelativePath $log)"
    }

    $pDupes = @($pItems | Group-Object | Where-Object { $_.Count -gt 1 })
    if ($pDupes.Count -gt 0) {
        $names = ($pDupes | ForEach-Object { $_.Name }) -join ', '
        Fail "Pre-flight audit has duplicate P-number IDs: $names. See $(Get-RepoRelativePath $log)"
    }
    $vDupes = @($vItems | Group-Object | Where-Object { $_.Count -gt 1 })
    if ($vDupes.Count -gt 0) {
        $names = ($vDupes | ForEach-Object { $_.Name }) -join ', '
        Fail "Pre-flight audit has duplicate V-number IDs: $names. See $(Get-RepoRelativePath $log)"
    }

    Write-TextFile $checklistPath $body
    return $body
}

function Test-PacketForbidsSidecar {
    param([System.IO.FileInfo]$Packet)

    $text = Get-Content -Raw -LiteralPath $Packet.FullName
    return (
        $text -match '(?is)MUST NOT add new files.*sidecar\s+`?\.meta\.json`?' -or
        $text -match '(?is)Do not create,\s*finalize,\s*repair,\s*delete,\s*or\s*regenerate handoff sidecars' -or
        $text -match '(?is)MUST NOT.*sidecar\s+`?\.meta\.json`?'
    )
}

function Get-MarkerValue {
    # Extract a line-anchored 'NAME: value' marker from free-form model output.
    # Tolerates leading list/quote/emphasis decoration and surrounding backticks
    # or quotes; preserves interior characters (path separators, underscores).
    # Returns the value of the last matching line, or $null if none is present.
    param([string]$Text, [string]$Name)

    $value = $null
    $pattern = '^' + [regex]::Escape($Name) + '\s*:\s*(.+)$'
    foreach ($line in ($Text -split "`r?`n")) {
        $norm = ($line -replace '`', '').Trim()
        $norm = ($norm -replace '^[>\-\*\+\s]+', '').Trim()
        if ($norm -match $pattern) {
            $candidate = ($matches[1] -replace '^[\*"''\s]+', '' -replace '[\*"''\s.,;]+$', '')
            if ($candidate) { $value = $candidate }
        }
    }
    return $value
}

function Resolve-GateVerdictFallback {
    param([string]$Text)

    foreach ($line in ($Text -split "`r?`n")) {
        $norm = ($line -replace '`', '').Trim()
        $norm = ($norm -replace '^[>\-\*\+\#\s]+', '').Trim()
        $norm = ($norm -replace '[\*_]+', '').Trim()
        if ($norm -notmatch '^(?:Gate\s+)?Verdict\s*:\s*(.+)$') {
            continue
        }

        $raw = $matches[1].Trim().ToLowerInvariant()
        if ($raw -match '^(approve|approved|pass)\b') {
            return 'approve'
        }
        if ($raw -match '^(needs[_\-\s]?changes?|changes[_\-\s]?needed)\b') {
            return 'needs_changes'
        }
        if ($raw -match '^(block|blocked)\b') {
            return 'block'
        }
    }

    return $null
}

function Resolve-ExecStatusFromPacket {
    param([System.IO.FileInfo]$Packet)

    if (-not $Packet -or -not (Test-Path -LiteralPath $Packet.FullName)) {
        return $null
    }

    $text = Get-Content -Raw -LiteralPath $Packet.FullName
    $handoff = Get-MarkerValue -Text $text -Name 'HANDOFF_STATUS'
    $packetStatus = Get-MarkerValue -Text $text -Name 'STATUS'
    $exitRaw = Get-MarkerValue -Text $text -Name 'EXIT_CODE'

    $handoffNorm = if ($handoff) { $handoff.ToUpperInvariant() } else { '' }
    $packetStatusNorm = if ($packetStatus) { $packetStatus.ToUpperInvariant() } else { '' }
    $exitCode = $null
    if ($exitRaw -and $exitRaw -match '^-?\d+$') {
        $exitCode = [int]$exitRaw
    }

    if ($handoffNorm -eq 'COMPLETE' -and $exitCode -eq 0) {
        return 'executed'
    }
    if ($handoffNorm -in @('BLOCKED', 'NEEDS_HUMAN') -or $packetStatusNorm -in @('BLOCKED', 'NEEDS_HUMAN')) {
        return 'blocked'
    }
    if ($handoffNorm -eq 'FAILED' -or $packetStatusNorm -eq 'FAILED' -or ($null -ne $exitCode -and $exitCode -ne 0)) {
        return 'failed'
    }

    return $null
}

function Invoke-ClaudeMarker {
    # Run a Claude step and extract line-anchored markers from its prose output.
    # The verbatim response is saved to OutputPath as the record and as Codex
    # revision context. No JSON parsing or schema validation on the critical
    # path: each marker in $Markers maps a NAME to either an allowed-values
    # array (required, enum-checked) or $null (optional, free-form).
    param(
        [string]$Prompt,
        [hashtable]$Markers,
        [string]$OutputPath,
        [ValidateSet('acceptEdits', 'auto', 'bypassPermissions', 'default', 'dontAsk', 'plan')]
        [string]$PermissionMode
    )

    $base = $OutputPath -replace '\.[^.\\/]+$', ''
    $envelopePath = "$base.envelope.json"
    $stderrPath = "$base.stderr.txt"

    $args = @(
        '-p',
        '--mcp-config', $script:McpConfig,
        '--permission-mode', $PermissionMode,
        '--output-format', 'json'
    )
    if ($ClaudeModel) { $args += @('--model', $ClaudeModel) }

    $r = Invoke-WithTimeout -Exe 'claude' -Arguments ($args + $Prompt) `
        -OutFile $envelopePath -ErrFile $stderrPath -TimeoutSec $ModelTimeoutSec
    if ($r.TimedOut) {
        Fail "claude timed out after ${ModelTimeoutSec}s (terminal infrastructure failure). See $stderrPath"
    }
    if ($r.Code -ne 0) {
        Fail "claude failed. See $stderrPath"
    }
    if (-not (Test-Path -LiteralPath $envelopePath) -or (Get-Item -LiteralPath $envelopePath).Length -eq 0) {
        Fail "claude produced no output. See $stderrPath"
    }

    $envelope = Read-JsonFile $envelopePath
    $props = @($envelope.PSObject.Properties.Name)
    if (($props -contains 'is_error') -and $envelope.is_error) {
        Fail "claude reported an error: $($envelope.result). See $envelopePath"
    }
    if (-not ($props -contains 'result')) {
        Fail "claude did not return a result payload. See $envelopePath"
    }

    $resultText = [string]$envelope.result
    Write-TextFile $OutputPath $resultText

    # Tail-anchor: required markers must appear at the end of the response
    # (GATE_VERDICT as the final line; EXEC_STATUS/EXEC_PACKET as the final
    # two). Scanning only the last few non-empty lines stops a marker quoted
    # mid-prose from being read as the verdict.
    $tailText = (@($resultText -split "`r?`n" | Where-Object { $_.Trim() }) |
        Select-Object -Last 3) -join "`n"

    $extracted = @{}
    foreach ($name in @($Markers.Keys)) {
        $allowed = $Markers[$name]
        $value = Get-MarkerValue -Text $tailText -Name $name
        if ($null -eq $value) {
            if ($name -eq 'GATE_VERDICT') {
                $value = Resolve-GateVerdictFallback -Text $resultText
            }
            if ($allowed) {
                if ($null -eq $value) {
                    Fail "claude response is missing the required '${name}:' marker line. See $OutputPath"
                }
            }
        } elseif ($allowed -and ($allowed -notcontains $value)) {
            Fail "claude '${name}:' marker value '$value' is not one of: $($allowed -join ', '). See $OutputPath"
        }
        $extracted[$name] = $value
    }

    return [pscustomobject]@{
        Text    = $resultText
        Markers = $extracted
    }
}

function Test-ClaudeCliReady {
    $probeOut = Join-Path $script:RunDir 'claude.ready.envelope.json'
    $probeErr = Join-Path $script:RunDir 'claude.ready.stderr.txt'

    $r = Invoke-WithTimeout -Exe 'claude' -OutFile $probeOut -ErrFile $probeErr -TimeoutSec 180 `
        -Arguments @('-p', '--output-format', 'json', 'Return exactly: ready')
    if ($r.TimedOut) {
        Fail "claude readiness probe timed out after 180s. Check the Claude CLI / auth, then retry."
    }

    if (-not (Test-Path -LiteralPath $probeOut) -or (Get-Item -LiteralPath $probeOut).Length -eq 0) {
        if ($r.Code -ne 0) {
            Fail "claude readiness probe failed. See $probeErr"
        }
        Fail "claude readiness probe produced no JSON output. See $probeErr"
    }

    $probe = Read-JsonFile $probeOut
    $props = @($probe.PSObject.Properties.Name)
    if (($props -contains 'is_error') -and $probe.is_error) {
        Fail "claude is not ready: $($probe.result). Run Claude Code login/auth setup, then retry."
    }
    if ($r.Code -ne 0) {
        Fail "claude readiness probe failed. See $probeErr"
    }
}

function Invoke-PlanFill {
    param(
        [System.IO.FileInfo]$TaskPacket,
        [int]$RevisionNumber,
        [string]$PriorExecutorGatePath
    )

    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $gateContext = 'No prior executor gate.'
    if ($PriorExecutorGatePath -and (Test-Path -LiteralPath $PriorExecutorGatePath)) {
        $gateContext = Limit-PlanRevisionGateContext -Text (Get-Content -Raw -LiteralPath $PriorExecutorGatePath)
    }

    $prompt = @"
You are Planner / OpenAI Codex in the RGE repository.

Fill or revise this TASK_PACKET only:

$taskRel

User goal:

$script:GoalText

Revision number: $RevisionNumber

Prior executor gate result, if any:

$gateContext

Rules:
- Edit only the TASK_PACKET above.
- Do not edit source, docs, schemas, scripts, .gitignore, or any other packet.
- Replace every placeholder.
- Make scope precise: MAY edit, MUST NOT edit, deliverables, gates, halt conditions.
- If the task is audit-only, make that explicit and set MAY edit to none.
- The TASK packet must preserve every top-level header field and the full
  machine-readable completion footer even if you cannot read
  ai_handoffs/templates/TASK_PACKET.md during planning. The rules below are
  authoritative for that header/footer contract.
- The TASK header at the top of the packet body must contain all of these
  fields, in this order, each on its own line with a non-empty value:
  DISPATCH_ID, AUTHOR, TIMESTAMP, RELATED_FILES, STATUS. RELATED_FILES is
  a bulleted list of repo-relative paths or globs that follows the field
  label on the next lines. The finalizer
  ``new-handoff.ps1 -Finalize -DryRun`` rejects any TASK packet that omits
  one of these header fields and names the missing field in its failure
  text.
- The machine-readable completion footer at the bottom of the packet must
  contain all of these fields, each on its own line with a non-empty value:
  HANDOFF_STATUS, DISPATCH_ID, AUTHOR, NEXT_ROLE, EXIT_CODE. The footer
  must be preserved in full even if the template file is unavailable.
- Every path or glob token listed under ``### MAY edit`` and
  ``### MAY add new files`` must be wrapped in Markdown backticks so the queue
  scope guard can parse it as an explicit code token. Example of a valid
  bullet: ``- ``Invoke-AiDispatchLoop.ps1`` ``.
- Bare-bulleted paths or globs in ``### MAY edit`` and ``### MAY add new files``
  (for example ``- Invoke-AiDispatchLoop.ps1`` with no backticks) are invalid
  for the queue scope guard and must not appear in the generated TASK packet.
- If the TASK template contains an ADR-121 ``<!-- handoff:envelope v1 -->``
  block, fill it as advisory machine-readable scope: mirror the concrete
  positive edit surface from ``### MAY edit`` and ``### MAY add new files`` into
  ``MAY_EDIT``; mirror the concrete negative surface from ``### MUST NOT edit``
  and ``### MUST NOT add new files`` into ``MUST_NOT_EDIT``; set
  ``INCIDENTAL_OK`` to ``true`` only when incidental ``Cargo.lock`` /
  ``*.meta.json`` outputs are explicitly acceptable. Envelope entries are raw
  repo-relative paths or globs without Markdown backticks, and must not use
  brace expansion such as ``crates/{a,b}/**``. If the dispatch is read-only or
  the scope cannot be represented safely, delete the envelope block or leave
  both lists empty so advisory validation reports ``UNCHECKED``.
- Footer must be:
  HANDOFF_STATUS: COMPLETE
  DISPATCH_ID: <same as header>
  AUTHOR: <same as header, e.g. Planner / OpenAI Codex>
  NEXT_ROLE: EXECUTOR_AI
  EXIT_CODE: 0
- The packet must pass new-handoff.ps1 -Finalize -DryRun.
"@

    $log = Join-Path $script:RunDir ("codex.plan.rev{0}.log" -f $RevisionNumber)
    Invoke-CodexPrompt -Prompt $prompt -Sandbox 'workspace-write' -LogPath $log
    Test-PacketFinalizeDryRun -Packet $TaskPacket -LogPath (Join-Path $script:RunDir ("task.finalize-dryrun.rev{0}.log" -f $RevisionNumber))
}

function Invoke-ClaudePlanGate {
    param([System.IO.FileInfo]$TaskPacket, [int]$RevisionNumber)
    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $out = Join-Path $script:RunDir ("claude.plan_gate.rev{0}.md" -f $RevisionNumber)
    $prompt = @"
You are Claude acting as Executor preflight gate for RGE.

Review the TASK_PACKET:

$taskRel

You must not edit files. Read the packet, inspect only the repo context needed
to decide whether the plan is executable, bounded, and protocol-safe.

Write your review as free-form prose. Cover, in whatever structure you prefer:
- the verdict reasoning,
- any blocking reasons,
- recommended changes to the TASK packet,
- the commands you actually ran.

Apply Protocol Rule 8 (negative current-state claims require falsification) as a
gate criterion. Enumerate every claim in the TASK packet that asserts absence,
unchanged-ness, or zero reachability of current repository state (for example
"no call sites", "zero matches", "feature absent", "X is unchanged", "not
wired"). For each such claim, confirm the packet carries an explicit,
re-runnable falsifying search -- an rg / git grep command with its path set --
and the observed result that grounds the claim. A negative claim with no
attached falsifying search is an invalid premise: return needs_changes so Codex
adds the falsifying search (or restates the claim as an open question) before
execution. You must not run edits to repair the packet yourself.

End your response with exactly one line, by itself, anchored at column 1:

GATE_VERDICT: approve

Substitute one of these values for 'approve':
- approve        the task is safe to execute as written.
- needs_changes  Codex should revise the TASK packet first.
- block          execution must not proceed without human arbitration.

That GATE_VERDICT line must be the final line of your response. Do not wrap it
in Markdown, quotes, or a code block.
"@
    $res = Invoke-ClaudeMarker -Prompt $prompt `
        -Markers @{ 'GATE_VERDICT' = @('approve', 'needs_changes', 'block') } `
        -OutputPath $out -PermissionMode 'default'
    return [pscustomobject]@{ verdict = $res.Markers['GATE_VERDICT']; review = $res.Text }
}

function Invoke-CodexPlanGate {
    param([System.IO.FileInfo]$TaskPacket, [int]$RevisionNumber)
    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $out = Join-Path $script:RunDir ("codex.plan_gate.rev{0}.md" -f $RevisionNumber)
    $prompt = @"
You are Codex acting as Executor preflight gate for RGE.

Review the TASK_PACKET:

$taskRel

You must not edit files. Read the packet, inspect only the repo context needed
to decide whether the plan is executable, bounded, and protocol-safe.

Write your review as free-form prose. Cover, in whatever structure you prefer:
- the verdict reasoning,
- any blocking reasons,
- recommended changes to the TASK packet,
- the commands you actually ran.

Apply Protocol Rule 8 (negative current-state claims require falsification) as a
gate criterion. Enumerate every claim in the TASK packet that asserts absence,
unchanged-ness, or zero reachability of current repository state (for example
"no call sites", "zero matches", "feature absent", "X is unchanged", "not
wired"). For each such claim, confirm the packet carries an explicit,
re-runnable falsifying search -- an rg / git grep command with its path set --
and the observed result that grounds the claim. A negative claim with no
attached falsifying search is an invalid premise: return needs_changes so Codex
adds the falsifying search (or restates the claim as an open question) before
execution. You must not run edits to repair the packet yourself.

End your response with exactly one line, by itself, anchored at column 1:

GATE_VERDICT: approve

Substitute one of these values for 'approve':
- approve        the task is safe to execute as written.
- needs_changes  Codex should revise the TASK packet first.
- block          execution must not proceed without human arbitration.

That GATE_VERDICT line must be the final line of your response. Do not wrap it
in Markdown, quotes, or a code block.
"@
    Invoke-CodexPrompt -Prompt $prompt -Sandbox 'read-only' -LogPath $out
    if (-not (Test-Path -LiteralPath $out)) {
        Fail "codex plan gate produced no log file. See $out"
    }
    $text = Get-Content -Raw -LiteralPath $out
    $verdict = Get-MarkerValue -Text $text -Name 'GATE_VERDICT'
    if (-not $verdict) {
        $verdict = Resolve-GateVerdictFallback -Text $text
    }
    if (-not $verdict) {
        Fail "codex response is missing the required 'GATE_VERDICT:' marker line. See $out"
    }
    if (@('approve', 'needs_changes', 'block') -notcontains $verdict) {
        Fail "codex 'GATE_VERDICT:' marker value '$verdict' is not one of: approve, needs_changes, block. See $out"
    }
    return [pscustomobject]@{ verdict = $verdict; review = $text }
}

function Invoke-ClaudeExecute {
    param(
        [System.IO.FileInfo]$ActivePacket,
        [string]$PacketKind,
        [int]$Round,
        [string]$PreflightChecklist = ''
    )

    $packetRel = Get-RepoRelativePath $ActivePacket.FullName
    $out = Join-Path $script:RunDir ("claude.execute.round{0}.md" -f $Round)

    # Round 0 only: advisory pre-flight checklist. Corrections (rounds > 0)
    # must follow the CORRECTION_PACKET directly without re-injecting the
    # initial audit.
    $preflightBlock = ''
    if ($Round -eq 0 -and $PreflightChecklist) {
        $preflightBlock = @"

Advisory Codex pre-flight checklist (read-only audit aid, round 0 only):

$PreflightChecklist

The TASK packet above remains authoritative. This pre-flight checklist is
advisory only. It does not expand scope, does not authorize edits or tests
outside the TASK packet's allowed surface, and must be ignored wherever it
conflicts with the TASK packet. Preserve the Pn and Vn IDs when you refer to
items so control can match them.
"@
    }

    $prompt = @"
You are Executor / Claude in the RGE repository.

Read and execute this $PacketKind packet:

$packetRel
$preflightBlock
Protocol rules:
- Execute only the enumerated scope.
- Do not commit.
- Do not push.
- If a halt condition triggers, stop and write an EXECUTION_REPORT with
  STATUS: BLOCKED or NEEDS_HUMAN as appropriate.
- If execution proceeds, write an EXECUTION_REPORT using:
  .\new-handoff.ps1 -DispatchId $DispatchId -PacketType EXEC -Author "Executor / Claude"
- Fill the EXEC packet completely.
- If the active packet allows sidecar creation, run:
  .\new-handoff.ps1 -Finalize -PacketPath <exec packet path>
- If the active packet forbids sidecar .meta.json creation, do not finalize
  the EXEC packet; note that deliberate skip in your summary.

Write a free-form prose summary of the execution: what changed, the
verification commands you ran and their results, the final git status, and
any notes for the reviewer.

End your response with exactly these two lines, by themselves, anchored at
column 1:

EXEC_STATUS: executed
EXEC_PACKET: ai_handoffs/<EXECUTION_REPORT file name>.md

Substitute one EXEC_STATUS value for 'executed':
- executed  the enumerated scope was carried out.
- blocked   a halt condition stopped execution.
- failed    execution was attempted but did not complete.

For EXEC_PACKET give the repo-relative path to the EXECUTION_REPORT you wrote,
or the single word none if no report was written. These two lines must be the
final lines of your response. Do not wrap them in Markdown, quotes, or a code
block.
"@
    $res = Invoke-ClaudeMarker -Prompt $prompt `
        -Markers @{ 'EXEC_STATUS' = $null; 'EXEC_PACKET' = $null } `
        -OutputPath $out -PermissionMode $ClaudePermissionMode
    $status = $res.Markers['EXEC_STATUS']
    if ($status -and (@('executed', 'blocked', 'failed') -notcontains $status)) {
        Fail "claude 'EXEC_STATUS:' marker value '$status' is not one of: executed, blocked, failed. See $out"
    }
    $packet = $res.Markers['EXEC_PACKET']
    if ($packet -and ($packet -match '^(none|<none>|n/?a|null|-)$')) { $packet = $null }
    if (-not $status) {
        $fallbackPacket = $null
        if ($packet) {
            $candidate = Join-Path $script:RepoRoot (($packet -replace '/', '\'))
            if (Test-Path -LiteralPath $candidate) {
                $fallbackPacket = Get-Item -LiteralPath $candidate
            }
        }
        if (-not $fallbackPacket) {
            $fallbackPacket = Get-LatestPacket -PacketType 'EXEC'
        }
        if ($fallbackPacket) {
            $status = Resolve-ExecStatusFromPacket -Packet $fallbackPacket
            if ($status -and -not $packet) {
                $packet = Get-RepoRelativePath $fallbackPacket.FullName
            }
        }
        if (-not $status) {
            Fail "claude response is missing the required 'EXEC_STATUS:' marker line and no canonical EXEC packet footer could be used as fallback. See $out"
        }
    }
    # When the executor claims success, its EXEC packet must be the canonical
    # ai_handoffs/<DispatchId>_EXEC_*.md -- not a stale or unrelated packet.
    if ($status -eq 'executed') {
        $execName = if ($packet) { Split-Path -Leaf $packet } else { '' }
        if (-not $packet -or ($execName -notlike "${DispatchId}_EXEC_*.md")) {
            Fail "claude reported EXEC_STATUS: executed but EXEC_PACKET '$packet' is not the canonical ai_handoffs/${DispatchId}_EXEC_*.md packet for this dispatch. See $out"
        }
    }
    return [pscustomobject]@{
        status      = $status
        exec_packet = $packet
        report      = $res.Text
    }
}

function Invoke-CodexExecute {
    param(
        [System.IO.FileInfo]$ActivePacket,
        [string]$PacketKind,
        [int]$Round,
        [string]$PreflightChecklist = ''
    )

    $packetRel = Get-RepoRelativePath $ActivePacket.FullName
    $out = Join-Path $script:RunDir ("codex.execute.round{0}.md" -f $Round)

    # Round 0 only: advisory pre-flight checklist. Corrections (rounds > 0)
    # must follow the CORRECTION_PACKET directly without re-injecting the
    # initial audit.
    $preflightBlock = ''
    if ($Round -eq 0 -and $PreflightChecklist) {
        $preflightBlock = @"

Advisory Codex pre-flight checklist (read-only audit aid, round 0 only):

$PreflightChecklist

The TASK packet above remains authoritative. This pre-flight checklist is
advisory only. It does not expand scope, does not authorize edits or tests
outside the TASK packet's allowed surface, and must be ignored wherever it
conflicts with the TASK packet. Preserve the Pn and Vn IDs when you refer to
items so control can match them.
"@
    }

    $prompt = @"
You are Executor / Codex in the RGE repository.

Read and execute this $PacketKind packet:

$packetRel
$preflightBlock
Protocol rules:
- Execute only the enumerated scope.
- Do not commit.
- Do not push.
- If a halt condition triggers, stop and write an EXECUTION_REPORT with
  STATUS: BLOCKED or NEEDS_HUMAN as appropriate.
- If execution proceeds, write an EXECUTION_REPORT using:
  .\new-handoff.ps1 -DispatchId $DispatchId -PacketType EXEC -Author "Executor / Codex"
- Fill the EXEC packet completely.
- If the active packet allows sidecar creation, run:
  .\new-handoff.ps1 -Finalize -PacketPath <exec packet path>
- If the active packet forbids sidecar .meta.json creation, do not finalize
  the EXEC packet; note that deliberate skip in your summary.

Write a free-form prose summary of the execution: what changed, the
verification commands you ran and their results, the final git status, and
any notes for the reviewer.

End your response with exactly these two lines, by themselves, anchored at
column 1:

EXEC_STATUS: executed
EXEC_PACKET: ai_handoffs/<EXECUTION_REPORT file name>.md

Substitute one EXEC_STATUS value for 'executed':
- executed  the enumerated scope was carried out.
- blocked   a halt condition stopped execution.
- failed    execution was attempted but did not complete.

For EXEC_PACKET give the repo-relative path to the EXECUTION_REPORT you wrote,
or the single word none if no report was written. These two lines must be the
final lines of your response. Do not wrap them in Markdown, quotes, or a code
block.
"@
    Invoke-CodexPrompt -Prompt $prompt -Sandbox (Get-CodexExecutorSandbox) -LogPath $out
    if (-not (Test-Path -LiteralPath $out)) {
        Fail "codex execution produced no log file. See $out"
    }
    $text = Get-Content -Raw -LiteralPath $out
    $status = Get-MarkerValue -Text $text -Name 'EXEC_STATUS'
    if ($status -and (@('executed', 'blocked', 'failed') -notcontains $status)) {
        Fail "codex 'EXEC_STATUS:' marker value '$status' is not one of: executed, blocked, failed. See $out"
    }
    $packet = Get-MarkerValue -Text $text -Name 'EXEC_PACKET'
    if ($packet -and ($packet -match '^(none|<none>|n/?a|null|-)$')) { $packet = $null }
    if (-not $status) {
        $fallbackPacket = $null
        if ($packet) {
            $candidate = Join-Path $script:RepoRoot (($packet -replace '/', '\'))
            if (Test-Path -LiteralPath $candidate) {
                $fallbackPacket = Get-Item -LiteralPath $candidate
            }
        }
        if (-not $fallbackPacket) {
            $fallbackPacket = Get-LatestPacket -PacketType 'EXEC'
        }
        if ($fallbackPacket) {
            $status = Resolve-ExecStatusFromPacket -Packet $fallbackPacket
            if ($status -and -not $packet) {
                $packet = Get-RepoRelativePath $fallbackPacket.FullName
            }
        }
        if (-not $status) {
            Fail "codex response is missing the required 'EXEC_STATUS:' marker line and no canonical EXEC packet footer could be used as fallback. See $out"
        }
    }
    # When the executor claims success, its EXEC packet must be the canonical
    # ai_handoffs/<DispatchId>_EXEC_*.md -- not a stale or unrelated packet.
    if ($status -eq 'executed') {
        $execName = if ($packet) { Split-Path -Leaf $packet } else { '' }
        if (-not $packet -or ($execName -notlike "${DispatchId}_EXEC_*.md")) {
            Fail "codex reported EXEC_STATUS: executed but EXEC_PACKET '$packet' is not the canonical ai_handoffs/${DispatchId}_EXEC_*.md packet for this dispatch. See $out"
        }
    }
    return [pscustomobject]@{
        status      = $status
        exec_packet = $packet
        report      = $text
    }
}

function Invoke-CodexControl {
    param(
        [System.IO.FileInfo]$TaskPacket,
        [System.IO.FileInfo]$ExecPacket,
        [int]$Round,
        [string]$VerificationLog = '',
        [string]$PreflightChecklist = ''
    )

    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $verifyNote = ''
    if ($VerificationLog -and (Test-Path -LiteralPath $VerificationLog)) {
        $verifyNote = "`n- $(Get-RepoRelativePath $VerificationLog) -- the orchestrator already ran the canonical CI verification gate (format, architecture lints, supply chain, workspace tests) and it passed; corroborate it rather than trusting EXECUTION_REPORT prose alone"
    }
    $execRel = if ($ExecPacket) { Get-RepoRelativePath $ExecPacket.FullName } else { '<none>' }
    $schema = Join-Path $script:AiDir 'codex_control.schema.json'
    $out = Join-Path $script:RunDir ("codex.control.round{0}.json" -f $Round)
    $log = Join-Path $script:RunDir ("codex.control.round{0}.log" -f $Round)

    # Advisory checklist injected on every control round when present.
    $preflightBlock = ''
    if ($PreflightChecklist) {
        $preflightBlock = @"

Advisory Codex pre-flight checklist (read-only audit aid):

$PreflightChecklist

The TASK packet above remains authoritative. This pre-flight checklist is
advisory only. It does not expand scope, does not authorize edits or tests
outside the TASK packet's allowed surface, and must be ignored wherever it
conflicts with the TASK packet. Reference items by their Pn and Vn IDs.
"@
    }

    $prompt = @"
You are Codex Controller / Reviewer for an automated RGE dispatch loop.

Review without editing anything.

Task packet:
$taskRel

Latest execution report:
$execRel
$preflightBlock
Also inspect:
- git status --short --branch
- git diff
- relevant changed files
- verification claims in the EXECUTION_REPORT$verifyNote
- ai_handoffs/AI_HANDOFF_PROTOCOL.md if protocol interpretation matters

Return schema-compliant JSON only. Use:
- verdict=pass only if the work is ready for queue commit/publish.
- verdict=needs_changes if Codex should write a CORRECTION_PACKET and route it
  back to Claude.
- verdict=block if human arbitration is required.

Do not edit files. Do not stage. Do not commit. Do not push.
"@
    Invoke-CodexPrompt -Prompt $prompt -Sandbox 'read-only' -LogPath $log -OutputSchema $schema -OutputPath $out
    return (Read-JsonFile $out)
}

function Invoke-Verification {
    # Run the canonical verification gate (.ai/dispatch.verify.ps1 by default).
    # Exit 0 means the working tree passes the same checks CI enforces; a
    # non-zero exit fails the round before any control review or publish.
    param([int]$Round)

    $log = Join-Path $script:RunDir ("verification.round{0}.log" -f $Round)
    if ($SkipVerification) {
        Write-TextFile $log "Verification skipped: -SkipVerification was set."
        return [pscustomobject]@{ Passed = $true; Skipped = $true; ExitCode = 0; Log = $log }
    }

    $oldDispatchEnv = $env:RGE_AI_DISPATCH_ID
    $oldRoundEnv = $env:RGE_AI_DISPATCH_ROUND
    $env:RGE_AI_DISPATCH_ID = $DispatchId
    $env:RGE_AI_DISPATCH_ROUND = [string]$Round
    try {
        $r = Invoke-WithTimeout -Exe 'powershell.exe' -OutFile $log -TimeoutSec $VerifyTimeoutSec `
            -Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $script:VerifyScriptPath)
    } finally {
        if ($null -ne $oldDispatchEnv) { $env:RGE_AI_DISPATCH_ID = $oldDispatchEnv }
        else { Remove-Item Env:RGE_AI_DISPATCH_ID -ErrorAction SilentlyContinue }
        if ($null -ne $oldRoundEnv) { $env:RGE_AI_DISPATCH_ROUND = $oldRoundEnv }
        else { Remove-Item Env:RGE_AI_DISPATCH_ROUND -ErrorAction SilentlyContinue }
    }
    if ($r.TimedOut) {
        Add-Content -LiteralPath $log -Value "`n[orchestrator] verification timed out after ${VerifyTimeoutSec}s; process tree killed."
    }
    return [pscustomobject]@{
        Passed   = ((-not $r.TimedOut) -and ($r.Code -eq 0))
        Skipped  = $false
        TimedOut = $r.TimedOut
        ExitCode = $r.Code
        Log      = $log
    }
}

function Invoke-CorrectionPacket {
    param([object]$ControlResult, [object]$Verification, [int]$Round)
    $packet = Invoke-NewPacket -PacketType 'CORRECT' -Author 'Planner / OpenAI Codex'
    $packetRel = Get-RepoRelativePath $packet.FullName

    if ($ControlResult) {
        $reviewContext = "Codex control review result (JSON):`n`n" +
            ($ControlResult | ConvertTo-Json -Depth 16)
    } elseif ($Verification) {
        $verifyTail = ''
        if ($Verification.Log -and (Test-Path -LiteralPath $Verification.Log)) {
            $verifyTail = (Get-Content -LiteralPath $Verification.Log -Tail 120 -ErrorAction SilentlyContinue) -join "`n"
        }
        $reviewContext = @"
The post-execution verification gate FAILED (exit code $($Verification.ExitCode)).
The dispatch cannot pass until verification does. Verification runs the
canonical CI checks: format, architecture lints, supply chain, workspace
tests and doctests. Tail of the verification log:

$verifyTail
"@
    } else {
        $reviewContext = '(no review context was supplied)'
    }

    $prompt = @"
You are Planner / OpenAI Codex in the RGE repository.

Write a CORRECTION_PACKET only. Edit only this file:

$packetRel

$reviewContext

Rules:
- Enumerate only the fixes needed to make the dispatch pass review and the
  verification gate.
- Do not expand scope.
- Do not edit any source, docs, schemas, scripts, or other packets.
- Fill every placeholder.
- Footer must be:
  HANDOFF_STATUS: COMPLETE
  NEXT_ROLE: EXECUTOR_AI
  EXIT_CODE: 0
"@
    $log = Join-Path $script:RunDir ("codex.correct.round{0}.log" -f $Round)
    Invoke-CodexPrompt -Prompt $prompt -Sandbox 'workspace-write' -LogPath $log
    Test-PacketFinalizeDryRun -Packet $packet -LogPath (Join-Path $script:RunDir ("correct.finalize-dryrun.round{0}.log" -f $Round))
    Finalize-Packet -Packet $packet | Out-Null
    return $packet
}

function Invoke-GitCapture {
    # Run git with PS 5.1 EAP isolation: native git stderr (e.g. benign
    # warnings about the user ignore file) under EAP=Stop would otherwise
    # terminate the script. stderr is dropped; the exit code still reflects
    # success. Returns [pscustomobject]@{ Code; Lines }.
    param([string[]]$GitArgs)
    $global:LASTEXITCODE = 0
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $out = & git @GitArgs 2>$null
    } finally {
        $ErrorActionPreference = $prevEap
    }
    return [pscustomobject]@{ Code = $LASTEXITCODE; Lines = @($out) }
}

function Get-WorktreeChangeSet {
    # Snapshot of `git status --porcelain` lines, for detecting what a
    # workspace-write Codex step changed.
    return @((Invoke-GitCapture @('status', '--porcelain=v1')).Lines |
        Where-Object { ([string]$_).Trim() })
}

function Assert-PlannerScopeClean {
    # A Codex plan/correction step runs under the workspace-write sandbox.
    # Compare the worktree before vs after and fail if the step touched any
    # path other than its own packet -- a prompt injection in untrusted issue
    # text could otherwise make the planner edit source files.
    param([string[]]$Before, [System.IO.FileInfo]$Packet, [string]$StepName)
    $allowed = @(
        (Get-RepoRelativePath $Packet.FullName),
        (Get-RepoRelativePath ($Packet.FullName -replace '\.md$', '.meta.json'))
    )
    $beforeSet = @{}
    foreach ($b in $Before) { $beforeSet[$b] = $true }
    $stray = @()
    foreach ($line in (Get-WorktreeChangeSet)) {
        if ($beforeSet[$line]) { continue }
        $t = [string]$line
        $p = if ($t.Length -gt 3) { $t.Substring(3) } else { $t }
        $p = ($p.Trim().Trim('"')) -replace '\\', '/'
        if ($p -match '->\s*(.+)$') { $p = ($matches[1].Trim().Trim('"')) }
        if ($allowed -notcontains $p) { $stray += $t }
    }
    if ($stray.Count -gt 0) {
        Fail ("$StepName changed files outside its packet (possible prompt-" +
            "injection scope escape):`n" + ($stray -join "`n"))
    }
}

Require-Command git
Require-Command codex
if ($CodexExecutorExternalScratch -and $Executor -ne 'codex') {
    Fail "-CodexExecutorExternalScratch is only valid with -Executor codex; it does not apply to Claude execution."
}
if ($Executor -eq 'claude') {
    Require-Command claude
}

if ($ResumeApprovedTask -and $PlanOnly) {
    Fail "-PlanOnly cannot be combined with -ResumeApprovedTask; resume mode runs the execution loop on an already-approved TASK."
}

$repoRootGit = Invoke-GitCapture @('rev-parse', '--show-toplevel')
$script:RepoRoot = ($repoRootGit.Lines -join "`n").Trim()
if ($repoRootGit.Code -ne 0 -or -not $script:RepoRoot) {
    Fail "Not inside a git repository."
}
Set-Location -LiteralPath $script:RepoRoot

$script:AiDir = Join-Path $script:RepoRoot '.ai'
$script:HandoffDir = Join-Path $script:RepoRoot 'ai_handoffs'
$script:NewHandoff = Join-Path $script:RepoRoot 'new-handoff.ps1'
$script:McpConfig = Join-Path $script:RepoRoot '.mcp.json'
$script:RunDir = Join-Path $script:AiDir ("dispatch-{0}" -f $DispatchId)

foreach ($path in @(
    $script:NewHandoff,
    $script:McpConfig,
    (Join-Path $script:AiDir 'codex_control.schema.json'),
    (Join-Path $script:HandoffDir 'AI_HANDOFF_PROTOCOL.md')
)) {
    if (-not (Test-Path -LiteralPath $path)) {
        Fail "Required file missing: $path"
    }
}

$script:VerifyScriptPath = if ($VerifyScript) {
    if ([System.IO.Path]::IsPathRooted($VerifyScript)) { $VerifyScript }
    else { Join-Path $script:RepoRoot $VerifyScript }
} else {
    Join-Path $script:AiDir 'dispatch.verify.ps1'
}
if (-not $SkipVerification -and -not (Test-Path -LiteralPath $script:VerifyScriptPath)) {
    Fail "Verification script not found: $script:VerifyScriptPath. Create it, pass -VerifyScript <path>, or run with -SkipVerification (an unverified dispatch can then publish - not recommended)."
}

if ($ResumeApprovedTask) {
    $script:GoalText = ''
} elseif ($GoalFile) {
    if (-not (Test-Path -LiteralPath $GoalFile)) {
        Fail "Goal file not found: $GoalFile"
    }
    $script:GoalText = Get-Content -Raw -LiteralPath $GoalFile
} else {
    $script:GoalText = $Goal
}

New-Item -ItemType Directory -Path $script:RunDir -Force | Out-Null

$aheadGit = Invoke-GitCapture @('rev-list', '--left-right', '--count', 'origin/main...HEAD')
$aheadBehind = ($aheadGit.Lines -join "`n").Trim()
if ($aheadGit.Code -eq 0 -and $aheadBehind -ne "0`t0") {
    Fail "Branch is not synced with origin/main: $aheadBehind"
}

$statusGit = Invoke-GitCapture @('status', '--porcelain=v1')
if ($statusGit.Code -ne 0) {
    Fail "git status --porcelain failed (exit $($statusGit.Code))."
}
$statusLines = $statusGit.Lines
$trackedDirty = @($statusLines | Where-Object { $_ -notmatch '^\?\? ' })
if ($trackedDirty.Count -gt 0 -and -not $AllowDirtyTracked) {
    Fail "Tracked files are already dirty. Re-run with -AllowDirtyTracked only if this is intentional."
}

if ($Executor -eq 'claude') {
    Test-ClaudeCliReady
}

Write-Output "AI dispatch loop: $DispatchId"
Write-Output "Repo: $script:RepoRoot"
Write-Output "Run dir: $(Get-RepoRelativePath $script:RunDir)"
if ($CodexExecutorExternalScratch) {
    Write-Output "Codex executor external scratch enabled: Codex execution sandbox is danger-full-access."
}

if ($ResumeApprovedTask) {
    $taskPacket = Get-LatestPacket -PacketType 'TASK'
    if (-not $taskPacket) {
        Fail "No TASK packet found for dispatch '$DispatchId' in $(Get-RepoRelativePath $script:HandoffDir)."
    }
    $taskSidecar = $taskPacket.FullName -replace '\.md$', '.meta.json'
    if (-not (Test-Path -LiteralPath $taskSidecar)) {
        Fail "TASK packet has no .meta.json sidecar, so it was never approved and finalized: $(Get-RepoRelativePath $taskPacket.FullName). Run a planning dispatch for this DispatchId first."
    }
    Write-Output "Resuming approved TASK: $(Get-RepoRelativePath $taskPacket.FullName)"
} else {
    $taskPacket = Invoke-NewPacket -PacketType 'TASK' -Author 'Planner / OpenAI Codex'
    Write-Output "TASK scaffolded: $(Get-RepoRelativePath $taskPacket.FullName)"

    $gateAgent = if ($Executor -eq 'codex') { 'Codex' } else { 'Claude' }
    $gateFilePrefix = if ($Executor -eq 'codex') { 'codex' } else { 'claude' }
    $gate = $null
    $gatePath = ''
    $approved = $false
    for ($i = 0; $i -le $MaxPlanRevisions; $i++) {
        $beforePlan = Get-WorktreeChangeSet
        Invoke-PlanFill -TaskPacket $taskPacket -RevisionNumber $i -PriorExecutorGatePath $gatePath
        Assert-PlannerScopeClean -Before $beforePlan -Packet $taskPacket -StepName "Plan-fill rev $i"
        $gatePath = Join-Path $script:RunDir ("{0}.plan_gate.rev{1}.md" -f $gateFilePrefix, $i)
        $gateCleanupPaths = @($gatePath)
        if ($Executor -eq 'claude') {
            $gateCleanupPaths += ($gatePath -replace '\.md$', '.envelope.json')
            $gateCleanupPaths += ($gatePath -replace '\.md$', '.stderr.txt')
        }
        $gate = Invoke-WithSamePhaseRetry `
            -PhaseLabel "$gateAgent plan gate rev $i" `
            -CleanupPaths $gateCleanupPaths `
            -Action {
                if ($Executor -eq 'codex') {
                    Invoke-CodexPlanGate -TaskPacket $taskPacket -RevisionNumber $i
                } else {
                    Invoke-ClaudePlanGate -TaskPacket $taskPacket -RevisionNumber $i
                }
            }
        Write-Output "$gateAgent plan gate rev ${i}: $($gate.verdict)"
        if ($gate.verdict -eq 'approve') {
            $approved = $true
            break
        }
        if ($gate.verdict -eq 'block') {
            Fail "$gateAgent blocked the plan. See $(Get-RepoRelativePath $gatePath)"
        }
    }

    if (-not $approved) {
        Fail "$gateAgent did not approve the plan within MaxPlanRevisions=$MaxPlanRevisions. See $(Get-RepoRelativePath $gatePath)"
    }

    Finalize-Packet -Packet $taskPacket | Out-Null
    Write-Output "TASK finalized."

    if ($PlanOnly) {
        Write-Output "PlanOnly requested. Stopping after approved TASK."
        exit 0
    }
}

$preflightChecklist = ''
if ($EnablePreflightAudit) {
    Write-Output "Running Codex pre-flight pitfall audit (read-only)..."
    $preflightChecklist = Invoke-CodexPreflightAudit -TaskPacket $taskPacket
    $checklistRel = Get-RepoRelativePath (Join-Path $script:RunDir 'codex.preflight.md')
    Write-Output "Pre-flight checklist written: $checklistRel"
}

$activePacket = $taskPacket
$activeKind = 'TASK'
$lastExecPacket = $null
$finalControl = $null
$verification = $null
$executorLabel = if ($Executor -eq 'codex') { 'Codex' } else { 'Claude' }

for ($round = 0; $round -le $MaxCorrectionRounds; $round++) {
    # Mutation retry wraps ONLY the selected executor invocation: this
    # covers retryable contract/invocation failures (timeouts, missing
    # markers, malformed envelopes, the canonical-packet check). The
    # EXEC_STATUS semantic check below is intentionally outside the wrapper
    # so blocked/failed verdicts remain terminal and are never retried.
    $execResult = Invoke-WithMutationRetry `
        -PhaseLabel "$executorLabel execution round $round" `
        -RetryCount $MutationRetryCount `
        -Action {
            if ($Executor -eq 'codex') {
                Invoke-CodexExecute -ActivePacket $activePacket -PacketKind $activeKind -Round $round `
                    -PreflightChecklist $preflightChecklist
            } else {
                Invoke-ClaudeExecute -ActivePacket $activePacket -PacketKind $activeKind -Round $round `
                    -PreflightChecklist $preflightChecklist
            }
        }
    Write-Output "$executorLabel execution round ${round}: $($execResult.status)"

    if ($execResult.status -ne 'executed') {
        Fail "$executorLabel execution round ${round} did not complete (EXEC_STATUS: $($execResult.status)). A blocked or failed execution is not eligible to verify or publish; resolve it before re-running."
    }

    $lastExecPacket = $null
    if ($execResult.exec_packet) {
        $candidate = Join-Path $script:RepoRoot (($execResult.exec_packet -replace '/', '\'))
        if (Test-Path -LiteralPath $candidate) {
            $lastExecPacket = Get-Item -LiteralPath $candidate
        }
    }
    if (-not $lastExecPacket) {
        $lastExecPacket = Get-LatestPacket -PacketType 'EXEC'
    }
    if ($lastExecPacket) {
        $sidecar = $lastExecPacket.FullName -replace '\.md$', '.meta.json'
        if (Test-PacketForbidsSidecar -Packet $activePacket) {
            Write-Output "EXEC sidecar finalization skipped; active packet forbids sidecar creation."
        } elseif (-not (Test-Path -LiteralPath $sidecar)) {
            Finalize-Packet -Packet $lastExecPacket | Out-Null
        }
    }

    # Hard verification gate: the working tree must pass the canonical CI
    # checks before Codex control runs. A non-zero exit cannot become a pass.
    $verification = Invoke-Verification -Round $round
    if ($verification.Skipped) {
        Write-Output "Verification round ${round}: SKIPPED (-SkipVerification)"
    } elseif ($verification.TimedOut) {
        Write-Output "Verification round ${round}: TIMED OUT (over ${VerifyTimeoutSec}s)"
    } elseif ($verification.Passed) {
        Write-Output "Verification round ${round}: pass (exit 0)"
    } else {
        Write-Output "Verification round ${round}: FAIL (exit $($verification.ExitCode)) - see $(Get-RepoRelativePath $verification.Log)"
    }

    if ($verification.TimedOut) {
        Fail "Verification timed out (over ${VerifyTimeoutSec}s) - terminal infrastructure failure, not a correctable task. See $(Get-RepoRelativePath $verification.Log)"
    }
    if (-not $verification.Passed) {
        if ($round -ge $MaxCorrectionRounds) {
            Fail "Verification gate failed (exit $($verification.ExitCode)) and MaxCorrectionRounds=$MaxCorrectionRounds is exhausted. See $(Get-RepoRelativePath $verification.Log)"
        }
        $beforeCorrect = Get-WorktreeChangeSet
        # Mutation retry covers both the correction-packet write AND the
        # planner scope-clean guard. If the planner's workspace-write step
        # edits any path outside its own packet, the snapshot restore wipes
        # those stray edits before the retry attempt.
        $activePacket = Invoke-WithMutationRetry `
            -PhaseLabel "Correction (verification) round $round" `
            -RetryCount $MutationRetryCount `
            -Action {
                $pkt = Invoke-CorrectionPacket -Verification $verification -Round $round
                Assert-PlannerScopeClean -Before $beforeCorrect -Packet $pkt -StepName "Correction (verification) round $round"
                return $pkt
            }
        $activeKind = 'CORRECTION'
        Write-Output "CORRECTION finalized (verification failure): $(Get-RepoRelativePath $activePacket.FullName)"
        continue
    }

    # Hand Codex the verification log only when verification actually ran:
    # supplying it asserts the gate passed, which is false under
    # -SkipVerification (the log exists but records a skip).
    $verifyLogForControl = if ($verification.Skipped) { '' } else { $verification.Log }
    $controlOutPath = Join-Path $script:RunDir ("codex.control.round{0}.json" -f $round)
    $controlLogPath = Join-Path $script:RunDir ("codex.control.round{0}.log" -f $round)
    $finalControl = Invoke-WithSamePhaseRetry `
        -PhaseLabel "Codex control round $round" `
        -CleanupPaths @($controlOutPath, $controlLogPath) `
        -Action {
            Invoke-CodexControl -TaskPacket $taskPacket -ExecPacket $lastExecPacket `
                -Round $round -VerificationLog $verifyLogForControl `
                -PreflightChecklist $preflightChecklist
        }
    Write-Output "Codex control round ${round}: $($finalControl.verdict)"

    if ($finalControl.verdict -eq 'pass') {
        break
    }
    if ($finalControl.verdict -eq 'block') {
        Fail "Codex control blocked the dispatch. See $(Get-RepoRelativePath (Join-Path $script:RunDir ("codex.control.round{0}.json" -f $round)))"
    }
    if ($round -ge $MaxCorrectionRounds) {
        Fail "Codex requested changes, but MaxCorrectionRounds=$MaxCorrectionRounds is exhausted."
    }

    $beforeCorrect = Get-WorktreeChangeSet
    # Mutation retry as above: the planner scope-clean guard runs inside
    # the retried action so an out-of-scope correction attempt is restored
    # to the phase-entry snapshot before the next attempt.
    $activePacket = Invoke-WithMutationRetry `
        -PhaseLabel "Correction (control) round $round" `
        -RetryCount $MutationRetryCount `
        -Action {
            $pkt = Invoke-CorrectionPacket -ControlResult $finalControl -Round $round
            Assert-PlannerScopeClean -Before $beforeCorrect -Packet $pkt -StepName "Correction (control) round $round"
            return $pkt
        }
    $activeKind = 'CORRECTION'
    Write-Output "CORRECTION finalized: $(Get-RepoRelativePath $activePacket.FullName)"
}

Write-Output ""
Write-Output "Dispatch loop finished."
Write-Output "Task: $(Get-RepoRelativePath $taskPacket.FullName)"
if ($lastExecPacket) {
    Write-Output "Latest EXEC: $(Get-RepoRelativePath $lastExecPacket.FullName)"
}
if ($verification) {
    if ($verification.Skipped) {
        Write-Output "Verification: skipped (-SkipVerification)"
    } else {
        Write-Output "Verification: pass (exit 0)"
    }
}
if ($finalControl) {
    Write-Output "Codex control verdict: $($finalControl.verdict)"
    Write-Output "Commit readiness: $($finalControl.commit_readiness)"
}
Write-Output "No commit or push was performed."
