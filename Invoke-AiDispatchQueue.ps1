#Requires -Version 5.1
<#
.SYNOPSIS
    Pull the next `ai-dispatch`-labelled GitHub issue and run it through the
    dispatch loop on a local branch, unattended.

.DESCRIPTION
    Work source : open GitHub issues labelled `ai-dispatch`, oldest first.
    Execution   : Invoke-AiDispatchLoop.ps1 on a per-issue branch
                  `ai-dispatch/ISSUE-<n>`, run as an isolated child process.
    Publish     : if the dispatch exits 0 and Codex control says pass, the
                  branch is fast-forwarded into main and pushed to origin/main.
                  Failed / blocked runs remain local for inspection.
    Bookkeeping : the issue is relabelled (running -> done, plus failed on a
                  non-zero loop exit) and a result comment is posted.

    Exactly one issue is processed per invocation. This is meant to be fired
    on a recurring schedule (e.g. Claude Code `/loop`). A temp-dir lock file
    prevents overlapping runs from colliding.

    Pre-existing untracked files (unrelated working-tree clutter) are parked
    with `git stash --include-untracked` for the duration of the run so the
    branch commit captures only the dispatch's own output, then restored.

.EXAMPLE
    .\Invoke-AiDispatchQueue.ps1 -DryRun
    # Report which issue would be picked next; mutate nothing.

.EXAMPLE
    .\Invoke-AiDispatchQueue.ps1
    # Process the oldest queued issue end to end and publish if control passes.

.NOTES
    Requires local `git`, `gh` (authenticated), `codex`, `claude`,
    `powershell.exe`, and Invoke-AiDispatchLoop.ps1 in the repo root.
    Pushes only successful, Codex-control-passed dispatch commits. Use
    -NoPublish to keep the old local-branch-only behavior for a one-off run.
#>
[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$QueueLabel = 'ai-dispatch',

    [ValidateRange(10, 1440)]
    [int]$StaleLockMinutes = 180,

    [switch]$DryRun,

    [switch]$NoPublish,

    [ValidateRange(0, 5)]
    [int]$MaxPlanRevisions = 1,

    [ValidateRange(0, 5)]
    [int]$MaxCorrectionRounds = 2
)

$ErrorActionPreference = 'Stop'

$script:LockPath = Join-Path $env:TEMP 'rge-ai-dispatch-queue.lock'
$script:LockHeld = $false

function Release-Lock {
    if ($script:LockHeld) {
        Remove-Item -LiteralPath $script:LockPath -Force -ErrorAction SilentlyContinue
        $script:LockHeld = $false
    }
}

function Fail {
    param([string]$Message)
    Release-Lock
    [Console]::Error.WriteLine($Message)
    exit 1
}

function Finish {
    param([int]$Code = 0)
    Release-Lock
    exit $Code
}

function Require-Command {
    param([string]$Name)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Fail "Required command not found on PATH: $Name"
    }
}

function Write-Utf8 {
    param([string]$Path, [string]$Text)
    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Invoke-Capture {
    # Run a native command with PS 5.1 EAP isolation (native stderr under
    # EAP=Stop becomes a terminating error). Merges stdout+stderr into $File.
    # Returns the process exit code.
    param([string]$File, [string]$Exe, [string[]]$CmdArgs)
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & $Exe @CmdArgs > $File 2>&1
    } finally {
        $ErrorActionPreference = $prev
    }
    return $LASTEXITCODE
}

function Invoke-Tool {
    # Invoke-Capture into a temp file, return [pscustomobject]@{ Code; Text }.
    param([string]$Exe, [string[]]$CmdArgs)
    $tmp = [System.IO.Path]::GetTempFileName()
    try {
        $code = Invoke-Capture -File $tmp -Exe $Exe -CmdArgs $CmdArgs
        $text = (Get-Content -Raw -LiteralPath $tmp -ErrorAction SilentlyContinue)
        # Get-Content -Raw yields $null for empty output; normalize to '' so
        # callers can safely call .Trim()/string ops on the result.
        if ($null -eq $text) { $text = '' }
        return [pscustomobject]@{ Code = $code; Text = $text }
    } finally {
        Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
    }
}

function Git-Step {
    # Run a git command; fail hard with captured output on non-zero exit.
    param([string[]]$CmdArgs)
    $r = Invoke-Tool -Exe 'git' -CmdArgs $CmdArgs
    if ($r.Code -ne 0) {
        Fail "git $($CmdArgs -join ' ') failed (exit $($r.Code)):`n$($r.Text)"
    }
    return $r.Text
}

function Get-ShortOutput {
    param([string]$Text, [int]$MaxLines = 80)
    if (-not $Text) { return '' }
    $lines = @($Text -split "`r?`n")
    if ($lines.Count -le $MaxLines) {
        return ($lines -join "`n")
    }
    return (@("... output truncated to last $MaxLines lines ...") +
        ($lines | Select-Object -Last $MaxLines)) -join "`n"
}

function Write-DispatchLog {
    param(
        [string]$Id,
        [object]$Issue,
        [string]$Branch,
        [string]$LoopLog,
        [string]$LoopText,
        [int]$LoopExit,
        [string]$Verdict
    )

    $logDir = Join-Path $script:RepoRoot 'ai_dispatch_logs'
    if (-not (Test-Path -LiteralPath $logDir)) {
        New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    }

    $stamp = (Get-Date).ToString('yyyy-MM-dd_HH-mm-sszzz').Replace(':', '')
    $logPath = Join-Path $logDir "log_$stamp.md"
    $runDir = Join-Path $script:RepoRoot (Join-Path '.ai' "dispatch-$Id")

    $status = (Git-Step @('status', '--short', '--untracked-files=all')).Trim()
    if (-not $status) { $status = '(clean)' }
    $nameStatus = (Git-Step @('diff', '--name-status')).Trim()
    if (-not $nameStatus) { $nameStatus = '(no tracked diff)' }
    $stat = (Git-Step @('diff', '--stat')).Trim()
    if (-not $stat) { $stat = '(no tracked diff)' }

    $generated = '(run dir not found)'
    if (Test-Path -LiteralPath $runDir) {
        $generated = (Get-ChildItem -LiteralPath $runDir -File |
            Sort-Object LastWriteTime |
            ForEach-Object { "- $(Get-RepoRelativePathForQueue $_.FullName) ($($_.Length) bytes)" }) -join "`n"
        if (-not $generated) { $generated = '(no run-dir files)' }
    }

    $markerSummary = @()
    if (Test-Path -LiteralPath $runDir) {
        foreach ($md in Get-ChildItem -LiteralPath $runDir -File -Filter '*.md' | Sort-Object Name) {
            $markers = Select-String -LiteralPath $md.FullName -Pattern '^(GATE_VERDICT|EXEC_STATUS|EXEC_PACKET):' -ErrorAction SilentlyContinue
            if ($markers) {
                $markerSummary += "### $(Get-RepoRelativePathForQueue $md.FullName)"
                $markerSummary += '```text'
                $markerSummary += @($markers | ForEach-Object { $_.Line })
                $markerSummary += '```'
            }
        }
    }
    if ($markerSummary.Count -eq 0) { $markerSummary = @('(no Claude marker lines found)') }

    $controlSummary = '(no Codex control JSON found)'
    if (Test-Path -LiteralPath $runDir) {
        $control = Get-NewestRoundFile -RunDir $runDir -Filter 'codex.control.round*.json'
        if ($control) {
            $controlSummary = Get-Content -Raw -LiteralPath $control.FullName
        }
    }

    $body = @"
# AI Dispatch Log

- Timestamp: $((Get-Date).ToString('o'))
- Dispatch: `$Id`
- Issue: #$($Issue.number) - $($Issue.title)
- Issue URL: $($Issue.url)
- Branch: `$Branch`
- Loop exit code: `$LoopExit`
- Codex control verdict: `$Verdict`
- Loop log: `$LoopLog`

## Process Trace

1. Queue selected the oldest open $QueueLabel issue.
2. Queue labelled the issue $runLabel.
3. Queue created branch $Branch.
4. `Invoke-AiDispatchLoop.ps1` ran Codex plan, Claude gate, Claude execute, and Codex control.
5. Queue wrote this detailed log before staging, committing, merging, or pushing.
6. If and only if exit code is 0 and Codex control verdict is `pass`, queue will fast-forward `main` and push `origin/main`.

## Files Changed / Added / Deleted

`git status --short --untracked-files=all` before the queue commit:

~~~text
$status
~~~

`git diff --name-status` before the queue commit:

~~~text
$nameStatus
~~~

`git diff --stat` before the queue commit:

~~~text
$stat
~~~

This log file is also added by the queue before publish:

- $(Get-RepoRelativePathForQueue $logPath)

## Generated Run Files

$generated

## Claude Marker Summary

$($markerSummary -join "`n")

## Codex Control JSON

~~~json
$controlSummary
~~~

## Loop Output

~~~text
$(Get-ShortOutput -Text $LoopText -MaxLines 200)
~~~
"@

    Write-Utf8 $logPath $body
    return $logPath
}

function Get-RepoRelativePathForQueue {
    param([string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    $root = [System.IO.Path]::GetFullPath($script:RepoRoot).TrimEnd('\', '/')
    if ($full.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
        return (($full.Substring($root.Length)).TrimStart('\', '/') -replace '\\', '/')
    }
    return ($full -replace '\\', '/')
}

function Get-NewestRoundFile {
    # Pick the highest-numbered round file (codex.control.round<N>.json,
    # verification.round<N>.log, ...) by parsing the round number rather than
    # by mtime -- a stale artifact from an earlier run can carry a newer mtime
    # than the current round.
    param([string]$RunDir, [string]$Filter)
    if (-not (Test-Path -LiteralPath $RunDir)) { return $null }
    return Get-ChildItem -LiteralPath $RunDir -File -Filter $Filter -ErrorAction SilentlyContinue |
        Sort-Object { if ($_.Name -match 'round(\d+)') { [int]$matches[1] } else { -1 } } |
        Select-Object -Last 1
}

function Get-ControlVerdict {
    # The dispatch loop writes a schema-validated codex.control.round<N>.json
    # per control round. Return the highest round's verdict, read from that
    # structured artifact rather than scraped from loop stdout. Returns
    # 'unknown' when no control JSON exists (loop failed before any review).
    param([string]$RunDir)
    $control = Get-NewestRoundFile -RunDir $RunDir -Filter 'codex.control.round*.json'
    if (-not $control) { return 'unknown' }
    try {
        $obj = Get-Content -Raw -LiteralPath $control.FullName | ConvertFrom-Json
    } catch {
        return 'unknown'
    }
    if ($obj -and $obj.verdict) { return [string]$obj.verdict }
    return 'unknown'
}

function Get-ProcessStartTicks {
    param([int]$ProcessId)
    if ($ProcessId -le 0) { return [long]0 }
    $p = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($p) {
        try { return [long]$p.StartTime.Ticks } catch { return [long]0 }
    }
    return [long]0
}

function Get-LockInfo {
    # Parse the queue lock file. The owner pid plus the owner process start
    # time together distinguish a live owner from a stale lock or a recycled
    # pid. An old-format lock with no recorded procstart falls back to the age
    # window, so a recycled pid cannot pin the lock alive indefinitely.
    param([string]$Path, [int]$StaleLockMinutes)
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    $raw = (Get-Content -Raw -LiteralPath $Path -ErrorAction SilentlyContinue)
    $ownerPid = 0
    $ownerStart = [long]0
    if ($raw) {
        if ($raw -match 'pid=(\d+)')       { $ownerPid = [int]$matches[1] }
        if ($raw -match 'procstart=(\d+)') { $ownerStart = [long]$matches[1] }
    }
    $ageMin = ((Get-Date) - (Get-Item -LiteralPath $Path).LastWriteTime).TotalMinutes
    $alive = $false
    if ($ownerPid -gt 0) {
        $liveStart = Get-ProcessStartTicks -ProcessId $ownerPid
        if ($liveStart -ne 0) {
            if ($ownerStart -ne 0) {
                # New-format lock: owner is live only if the start time matches.
                $alive = ($liveStart -eq $ownerStart)
            } else {
                # Old-format lock (no procstart): the pid may be recycled, so
                # trust it only while the lock is still inside the age window.
                $alive = ($ageMin -lt $StaleLockMinutes)
            }
        }
    }
    return [pscustomobject]@{
        OwnerPid   = $ownerPid
        OwnerStart = $ownerStart
        Alive      = $alive
        AgeMin     = $ageMin
    }
}

function Acquire-Lock {
    # Atomically create the lock file: FileMode.CreateNew fails if it already
    # exists, so two racing starts cannot both win. A stale lock whose owner
    # process is gone is removed and the create retried once.
    $content = "pid=$PID started=$((Get-Date).ToString('o')) procstart=$(Get-ProcessStartTicks -ProcessId $PID)"
    for ($attempt = 0; $attempt -lt 2; $attempt++) {
        try {
            $fs = [System.IO.File]::Open($script:LockPath,
                [System.IO.FileMode]::CreateNew,
                [System.IO.FileAccess]::Write,
                [System.IO.FileShare]::None)
            try {
                $bytes = [System.Text.Encoding]::UTF8.GetBytes($content)
                $fs.Write($bytes, 0, $bytes.Length)
            } finally {
                $fs.Close()
            }
            return $true
        } catch [System.IO.IOException] {
            $info = Get-LockInfo -Path $script:LockPath -StaleLockMinutes $StaleLockMinutes
            if ($null -eq $info) { continue }
            if ($info.Alive) { return $false }
            if ($info.OwnerPid -le 0 -and $info.AgeMin -lt $StaleLockMinutes) {
                # Cannot confirm the owner died and the lock is recent: stay
                # conservative rather than risk a concurrent run.
                return $false
            }
            Write-Output "Lock is stale (owner pid $($info.OwnerPid) not running); overriding."
            Remove-Item -LiteralPath $script:LockPath -Force -ErrorAction SilentlyContinue
        }
    }
    return $false
}

function Get-PriorFeedback {
    # Build a feedback block from a previous failed run's artifacts in the
    # gitignored run dir, for injection into a retry's goal.
    param([string]$RunDir)
    if (-not (Test-Path -LiteralPath $RunDir)) { return '' }
    $parts = @()
    $control = Get-NewestRoundFile -RunDir $RunDir -Filter 'codex.control.round*.json'
    if ($control) {
        try {
            $c = Get-Content -Raw -LiteralPath $control.FullName | ConvertFrom-Json
            if ($c.verdict) { $parts += "Prior Codex control verdict: $($c.verdict)" }
            if ($c.summary) { $parts += "Prior control summary: $($c.summary)" }
            if ($c.required_fixes -and @($c.required_fixes).Count -gt 0) {
                $parts += 'Prior required fixes:'
                $parts += (@($c.required_fixes) | ForEach-Object { "  - $_" })
            }
        } catch { }
    }
    $verify = Get-NewestRoundFile -RunDir $RunDir -Filter 'verification.round*.log'
    if ($verify) {
        $vt = (Get-Content -LiteralPath $verify.FullName -Tail 40 -ErrorAction SilentlyContinue) -join "`n"
        if ($vt) { $parts += "Prior verification gate output (tail):`n$vt" }
    }
    if ($parts.Count -eq 0) { return '' }
    return ($parts -join "`n")
}

function Invoke-OrphanRecovery {
    # Recover from a dispatch run killed mid-flight: an issue stuck in
    # <label>-running with no live queue process, a leftover dispatch branch,
    # queue-parked stashes, or a non-main checkout. Resets such issues to the
    # queue so they are retried, and returns the repo to a clean main.
    # Resilient by design: a recovery failure warns but never aborts the tick.
    param([string]$RepoSlug, [string]$QueueLabel, [string]$RunLabel, [string]$DoneLabel, [string]$FailLabel)

    $list = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'list', '--repo', $RepoSlug, '--label', $RunLabel,
        '--state', 'open', '--limit', '100', '--json', 'number,title')
    if ($list.Code -ne 0) {
        Write-Output "WARNING: orphan recovery could not list '$RunLabel' issues (exit $($list.Code)); skipping recovery."
        return
    }
    $orphans = @()
    if ($list.Text -and $list.Text.Trim()) {
        try {
            $parsed = $list.Text | ConvertFrom-Json
            if ($null -ne $parsed) { $orphans = @($parsed) }
        } catch {
            Write-Output 'WARNING: orphan recovery could not parse issue JSON; skipping recovery.'
            return
        }
    }
    if ($orphans.Count -eq 0) { return }

    Write-Output "Orphan recovery: $($orphans.Count) issue(s) stuck in '$RunLabel' with no live run."

    # Return to a clean main, but only when the repo is on main or on a
    # queue-owned ai-dispatch/ISSUE-* branch -- that branch is the interrupted
    # run's own, and its partial edits never published, so discarding them is
    # safe. On any other branch the working tree may hold a human's
    # uncommitted work: stop and ask rather than force-clean over it.
    $curBranch = (Invoke-Tool -Exe 'git' -CmdArgs @('symbolic-ref', '--short', 'HEAD')).Text.Trim()
    if ($curBranch -and $curBranch -ne 'main') {
        if ($curBranch -match '^ai-dispatch/ISSUE-') {
            Write-Output "  repo left on queue branch '$curBranch'; forcing back to main (discarding interrupted work)."
            $co = Invoke-Tool -Exe 'git' -CmdArgs @('checkout', '-f', 'main')
            if ($co.Code -ne 0) {
                Write-Output "  WARNING: could not checkout main (exit $($co.Code)): $($co.Text)"
            }
        } else {
            Fail ("Orphan recovery found an interrupted dispatch, but the repo is on " +
                "branch '$curBranch' - not main, not a queue branch. That branch may " +
                "hold uncommitted work, so the queue will not force-clean it. Return " +
                "to a clean main by hand, then re-run.")
        }
    }

    # Restore any queue-parked stashes (pop one at a time; indices shift).
    for ($i = 0; $i -lt 20; $i++) {
        $stashList = (Invoke-Tool -Exe 'git' -CmdArgs @('stash', 'list')).Text
        $ref = $null
        foreach ($line in @($stashList -split "`r?`n")) {
            if ($line -match 'ai-dispatch-queue park:' -and $line -match '^(stash@\{\d+\})') {
                $ref = $matches[1]; break
            }
        }
        if (-not $ref) { break }
        Write-Output "  restoring parked stash $ref."
        $pop = Invoke-Tool -Exe 'git' -CmdArgs @('stash', 'pop', $ref)
        if ($pop.Code -ne 0) {
            Write-Output "  WARNING: 'git stash pop $ref' failed (exit $($pop.Code)); leaving it stashed."
            break
        }
    }

    # Make origin/main current so already-published work can be detected. The
    # explicit refspec guarantees refs/remotes/origin/main is updated; on a
    # fetch failure the published-vs-interrupted check cannot be trusted, so
    # leave the orphans untouched rather than risk requeuing published work.
    $orphanFetch = Invoke-Tool -Exe 'git' -CmdArgs @(
        'fetch', '--quiet', 'origin', '+main:refs/remotes/origin/main')
    if ($orphanFetch.Code -ne 0) {
        Write-Output ("WARNING: orphan recovery could not fetch origin/main " +
            "(exit $($orphanFetch.Code)); leaving '$RunLabel' issues untouched this tick.")
        return
    }

    # An interrupted publish (process killed between the ff-merge and the push)
    # leaves local main ahead of origin/main. Reset main back -- the commits
    # survive on their ai-dispatch/* branch -- and mark each affected issue
    # terminal so a human can push that branch by hand.
    $handledByAhead = @{}
    $ahead = (Invoke-Tool -Exe 'git' -CmdArgs @('rev-list', '--count', 'origin/main..main')).Text.Trim()
    if ($ahead -and $ahead -ne '0') {
        Write-Output "  local main is $ahead commit(s) ahead of origin/main (interrupted publish)."
        $aheadSubjects = (Invoke-Tool -Exe 'git' -CmdArgs @('log', 'origin/main..main', '--format=%s')).Text
        $reset = Invoke-Tool -Exe 'git' -CmdArgs @('reset', '--hard', 'origin/main')
        if ($reset.Code -ne 0) {
            Write-Output "  WARNING: could not reset local main (exit $($reset.Code)); resolve by hand."
        } else {
            Write-Output "  local main reset to origin/main; interrupted-publish commits remain on their branch."
        }
        foreach ($subj in @($aheadSubjects -split "`r?`n" | Where-Object { $_ })) {
            if ($subj -match 'ai-dispatch (ISSUE-(\d+)):') {
                $aheadId = $matches[1]
                $aheadNum = $matches[2]
                $handledByAhead[$aheadId] = $true
                Invoke-Tool -Exe 'gh' -CmdArgs @('issue', 'edit', $aheadNum, '--repo', $RepoSlug,
                    '--remove-label', $RunLabel, '--remove-label', $QueueLabel,
                    '--add-label', $DoneLabel, '--add-label', $FailLabel) | Out-Null
                Invoke-Tool -Exe 'gh' -CmdArgs @('issue', 'comment', $aheadNum, '--repo', $RepoSlug,
                    '--body', "An AI dispatch run for this issue was interrupted between the local merge and the push to origin. The control-passed work is preserved on branch ``ai-dispatch/$aheadId``; review and ``git push`` it by hand. Local main was reset to origin/main.") | Out-Null
                Write-Output "  issue #$aheadNum marked '$FailLabel'; its work is on branch ai-dispatch/$aheadId."
            }
        }
    }

    foreach ($o in $orphans) {
        $oid = "ISSUE-$($o.number)"
        $obranch = "ai-dispatch/$oid"
        if ($handledByAhead[$oid]) { continue }

        # If this dispatch's commit already reached origin/main, the run
        # completed and published and was only interrupted before label
        # cleanup. Re-running it would duplicate the work -- mark it done.
        $priorSha = (Invoke-Tool -Exe 'git' -CmdArgs @(
            'log', 'origin/main', '-n', '1', '--fixed-strings',
            "--grep=ai-dispatch ${oid}:", '--format=%H')).Text.Trim()
        if ($priorSha) {
            $short = $priorSha.Substring(0, [Math]::Min(8, $priorSha.Length))
            Write-Output "  issue #$($o.number) already published as $short; marking done, not requeuing."
            if ((Invoke-Tool -Exe 'git' -CmdArgs @('branch', '--list', $obranch)).Text.Trim()) {
                Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-D', $obranch) | Out-Null
            }
            Invoke-Tool -Exe 'gh' -CmdArgs @(
                'issue', 'edit', "$($o.number)", '--repo', $RepoSlug,
                '--remove-label', $RunLabel, '--remove-label', $QueueLabel,
                '--add-label', $DoneLabel) | Out-Null
            Invoke-Tool -Exe 'gh' -CmdArgs @(
                'issue', 'close', "$($o.number)", '--repo', $RepoSlug,
                '--comment', "A prior AI dispatch run published this work ($short) but was interrupted before cleanup; the queue has marked it done.") | Out-Null
            continue
        }

        # Not on origin/main -- genuinely interrupted; reset for a fresh run.
        if ((Invoke-Tool -Exe 'git' -CmdArgs @('branch', '--list', $obranch)).Text.Trim()) {
            Write-Output "  deleting interrupted branch $obranch."
            Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-D', $obranch) | Out-Null
        }
        # Archive the interrupted run's scratch dir so stale round artifacts
        # cannot mislead the fresh run.
        $orphanRunDir = Join-Path $script:RepoRoot (Join-Path '.ai' "dispatch-$oid")
        if (Test-Path -LiteralPath $orphanRunDir) {
            $rn = 1
            while (Test-Path -LiteralPath "$orphanRunDir.orphan$rn") { $rn++ }
            Move-Item -LiteralPath $orphanRunDir -Destination "$orphanRunDir.orphan$rn" -Force -ErrorAction SilentlyContinue
        }
        $relabel = Invoke-Tool -Exe 'gh' -CmdArgs @(
            'issue', 'edit', "$($o.number)", '--repo', $RepoSlug,
            '--remove-label', $RunLabel, '--add-label', $QueueLabel)
        if ($relabel.Code -eq 0) {
            Write-Output "  issue #$($o.number) reset to '$QueueLabel' for retry."
            Invoke-Tool -Exe 'gh' -CmdArgs @(
                'issue', 'comment', "$($o.number)", '--repo', $RepoSlug,
                '--body', "An AI dispatch run for this issue was interrupted before it finished. The queue has reset it to ``$QueueLabel`` and will pick it up again.") | Out-Null
        } else {
            Write-Output "  WARNING: could not relabel issue #$($o.number) (exit $($relabel.Code)): $($relabel.Text)"
        }
    }
}

# --- Environment -----------------------------------------------------------

$script:RepoRoot = $PSScriptRoot
Set-Location -LiteralPath $script:RepoRoot

Require-Command git
Require-Command gh
Require-Command codex
Require-Command claude
Require-Command powershell.exe

$loopScript = Join-Path $script:RepoRoot 'Invoke-AiDispatchLoop.ps1'
if (-not (Test-Path -LiteralPath $loopScript)) {
    Fail "Dispatch loop script not found: $loopScript"
}

$auth = Invoke-Tool -Exe 'gh' -CmdArgs @('auth', 'status')
if ($auth.Code -ne 0) {
    Fail "gh is not authenticated. Run 'gh auth login' first.`n$($auth.Text)"
}

$originUrl = (Git-Step @('remote', 'get-url', 'origin')).Trim()
if ($originUrl -notmatch 'github\.com[:/](.+?)(?:\.git)?/?$') {
    Fail "Could not parse an owner/name slug from origin URL: $originUrl"
}
$repoSlug = $matches[1]

$runLabel = "${QueueLabel}-running"
$doneLabel = "${QueueLabel}-done"
$failLabel = "${QueueLabel}-failed"
$retryLabel = "${QueueLabel}-retry"

# --- Single-run lock -------------------------------------------------------

if (-not $DryRun) {
    if (-not (Acquire-Lock)) {
        Write-Output "A dispatch-queue run is already in progress; skipping this tick."
        exit 0
    }
    $script:LockHeld = $true
}

try {
    # --- Recover any dispatch interrupted by a killed or crashed run -------
    if (-not $DryRun) {
        Invoke-OrphanRecovery -RepoSlug $repoSlug -QueueLabel $QueueLabel -RunLabel $runLabel -DoneLabel $doneLabel -FailLabel $failLabel
    }

    # --- Preflight: clean main, in sync with origin ------------------------

    $currentBranch = (Git-Step @('symbolic-ref', '--short', 'HEAD')).Trim()
    if ($currentBranch -ne 'main') {
        Fail ("Repository is on branch '$currentBranch', not 'main'. A previous run " +
            "may have been interrupted. Return to a clean 'main' before queueing.")
    }

    $porcelain = @((Git-Step @('status', '--porcelain=v1')) -split "`r?`n" | Where-Object { $_ })
    $trackedDirty = @($porcelain | Where-Object { $_ -notmatch '^\?\? ' })
    if ($trackedDirty.Count -gt 0) {
        Fail ("Tracked files are dirty on main; refusing to queue a dispatch:`n" +
            ($trackedDirty -join "`n"))
    }
    $hasUntracked = (@($porcelain | Where-Object { $_ -match '^\?\? ' }).Count -gt 0)

    Git-Step @('fetch', '--quiet', 'origin', '+main:refs/remotes/origin/main') | Out-Null
    $headSha = (Git-Step @('rev-parse', 'HEAD')).Trim()
    $originSha = (Git-Step @('rev-parse', 'origin/main')).Trim()
    if ($headSha -ne $originSha) {
        Fail ("Local main ($($headSha.Substring(0,8))) is not in sync with " +
            "origin/main ($($originSha.Substring(0,8))). Resolve before queueing.")
    }

    # --- Select the oldest unprocessed queued issue ------------------------

    $list = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'list', '--repo', $repoSlug, '--label', $QueueLabel,
        '--state', 'open', '--limit', '100',
        '--json', 'number,title,body,labels,url')
    if ($list.Code -ne 0) {
        Fail "gh issue list failed (exit $($list.Code)):`n$($list.Text)"
    }

    # PS 5.1 ConvertFrom-Json emits a JSON array as a single non-enumerated
    # object; assign first, then wrap, so an empty [] yields zero items.
    $issues = @()
    if ($list.Text -and $list.Text.Trim()) {
        try { $parsed = $list.Text | ConvertFrom-Json }
        catch { Fail "Could not parse gh issue list JSON: $($_.Exception.Message)" }
        if ($null -ne $parsed) { $issues = @($parsed) }
    }

    $pending = @($issues | Where-Object {
        $names = @($_.labels | ForEach-Object { $_.name })
        ($names -notcontains $runLabel) -and ($names -notcontains $doneLabel) -and
        ($names -notcontains $failLabel)
    } | Sort-Object number)

    if ($pending.Count -eq 0) {
        Write-Output "No queued '$QueueLabel' issues to process in $repoSlug."
        Finish 0
    }

    $issue = $pending[0]
    $id = "ISSUE-$($issue.number)"
    $branch = "ai-dispatch/$id"
    $title = if ($issue.title) { [string]$issue.title } else { '(no title)' }
    $issueLabelNames = @($issue.labels | ForEach-Object { $_.name })
    $isRetry = ($issueLabelNames -contains $retryLabel)

    Write-Output "Repo:     $repoSlug"
    Write-Output "Queued:   $($pending.Count) issue(s)"
    Write-Output "Next:     #$($issue.number) - $title$(if ($isRetry) { '  [RETRY]' } else { '' })"
    Write-Output "Dispatch: $id  ->  branch $branch"

    if ($DryRun) {
        Write-Output ""
        Write-Output "DryRun: no labels changed, no branch created, loop not run."
        Finish 0
    }

    # A branch with no terminal label means an earlier run was interrupted;
    # do not silently clobber it.
    if ((Git-Step @('branch', '--list', $branch)).Trim()) {
        Fail ("Branch '$branch' already exists but issue #$($issue.number) is not " +
            "labelled '$runLabel'/'$doneLabel'. Inconsistent state - resolve by hand.")
    }

    # --- Ensure bookkeeping labels exist (idempotent) ----------------------

    $labelSpec = @(
        @{ Name = $QueueLabel; Color = '0e8a16'; Desc = 'Queued for the AI dispatch loop' },
        @{ Name = $runLabel;   Color = 'fbca04'; Desc = 'AI dispatch in progress' },
        @{ Name = $doneLabel;  Color = '5319e7'; Desc = 'AI dispatch processed' },
        @{ Name = $failLabel;  Color = 'd93f0b'; Desc = 'AI dispatch run failed' },
        @{ Name = $retryLabel; Color = 'd4c5f9'; Desc = 'AI dispatch re-queued for one retry' }
    )
    foreach ($l in $labelSpec) {
        Invoke-Tool -Exe 'gh' -CmdArgs @(
            'label', 'create', $l.Name, '--repo', $repoSlug,
            '--color', $l.Color, '--description', $l.Desc, '--force') | Out-Null
    }

    # --- Mark running, build the goal --------------------------------------

    $edit = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'edit', "$($issue.number)", '--repo', $repoSlug, '--add-label', $runLabel)
    if ($edit.Code -ne 0) {
        Fail "Could not label issue #$($issue.number) '$runLabel' (exit $($edit.Code)):`n$($edit.Text)"
    }

    $goalBody = if ($issue.body -and $issue.body.Trim()) { [string]$issue.body } else { $title }
    $goalText = "GitHub issue #$($issue.number): $title`r`n`r`n$goalBody"
    if ($isRetry) {
        Write-Output "Retry run: issue carries '$retryLabel'; injecting prior-attempt feedback."
        $liveRunDir = Join-Path $script:RepoRoot (Join-Path '.ai' "dispatch-$id")
        # Archive the prior attempt's run dir so the retry's loop cannot
        # overwrite its artifacts. .ai/dispatch-*/ is gitignored, and so is
        # each .attemptN archive; pick the next free slot.
        $priorRunDir = ''
        if (Test-Path -LiteralPath $liveRunDir) {
            $n = 1
            while (Test-Path -LiteralPath "$liveRunDir.attempt$n") { $n++ }
            $archiveDir = "$liveRunDir.attempt$n"
            try {
                Move-Item -LiteralPath $liveRunDir -Destination $archiveDir -Force
                $priorRunDir = $archiveDir
                Write-Output "  archived prior run dir -> $(Get-RepoRelativePathForQueue $archiveDir)"
            } catch {
                Write-Output "  WARNING: could not archive prior run dir: $($_.Exception.Message)"
                $priorRunDir = $liveRunDir
            }
        }
        if ($priorRunDir) {
            $feedback = Get-PriorFeedback -RunDir $priorRunDir
            if ($feedback) {
                $goalText += "`r`n`r`n--- PRIOR ATTEMPT FAILED - ADDRESS THIS FEEDBACK ---`r`n$feedback"
            }
        }
    }
    $goalFile = Join-Path $env:TEMP "rge-ai-dispatch-goal-$id.txt"
    Write-Utf8 $goalFile $goalText

    # --- Park unrelated untracked clutter, branch, run the loop ------------

    $stashed = $false
    if ($hasUntracked) {
        Git-Step @('stash', 'push', '--include-untracked', '--message', "ai-dispatch-queue park: $id") | Out-Null
        $stashed = $true
    }

    Git-Step @('checkout', '-b', $branch) | Out-Null

    $loopLog = Join-Path $env:TEMP "rge-ai-dispatch-$id.log"
    Write-Output ""
    Write-Output "Starting dispatch loop for $id. Live loop output follows:"
    Write-Output "----------------------------------------------------------------"
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $loopScript `
            -DispatchId $id -GoalFile $goalFile `
            -MaxPlanRevisions $MaxPlanRevisions -MaxCorrectionRounds $MaxCorrectionRounds `
            2>&1 | Tee-Object -FilePath $loopLog
    } finally {
        $ErrorActionPreference = $prevEap
    }
    $loopExit = $LASTEXITCODE
    Write-Output "----------------------------------------------------------------"
    Write-Output "Dispatch loop exited with code $loopExit."

    $loopText = (Get-Content -Raw -LiteralPath $loopLog -ErrorAction SilentlyContinue)
    # Read the Codex control verdict from the structured run-dir JSON the loop
    # writes (schema-validated), not by scraping loop stdout. Newest round wins.
    $runDir = Join-Path $script:RepoRoot (Join-Path '.ai' "dispatch-$id")
    $verdict = Get-ControlVerdict -RunDir $runDir

    # --- Write detailed audit log, then commit the branch ------------------

    $dispatchLogPath = Write-DispatchLog -Id $id -Issue $issue -Branch $branch `
        -LoopLog $loopLog -LoopText ([string]$loopText) -LoopExit $loopExit -Verdict $verdict
    Write-Output "Detailed dispatch log written: $(Get-RepoRelativePathForQueue $dispatchLogPath)"

    Git-Step @('add', '-A') | Out-Null
    $staged = Invoke-Tool -Exe 'git' -CmdArgs @('diff', '--cached', '--quiet')
    $committed = $false
    $commitSha = ''
    if ($staged.Code -ne 0) {
        $outcome = if ($loopExit -eq 0) { 'ok' } else { "failed (exit $loopExit)" }
        $msg = @"
ai-dispatch $id`: $title

Unattended dispatch run via Invoke-AiDispatchQueue.ps1.
Loop exit code: $loopExit. Control verdict: $verdict. Outcome: $outcome.
Source: $($issue.url)
Detailed log: $(Get-RepoRelativePathForQueue $dispatchLogPath)

Publish policy: auto-push to origin/main only when loop exit code is 0 and
Codex control verdict is pass. Failed or blocked work remains local.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"@
        $msgFile = Join-Path $env:TEMP "rge-ai-dispatch-msg-$id.txt"
        Write-Utf8 $msgFile $msg
        Git-Step @('commit', '-F', $msgFile) | Out-Null
        Remove-Item -LiteralPath $msgFile -Force -ErrorAction SilentlyContinue
        $commitSha = (Git-Step @('rev-parse', '--short', 'HEAD')).Trim()
        $committed = $true
    }

    Git-Step @('checkout', 'main') | Out-Null
    if (-not $committed) {
        Git-Step @('branch', '-D', $branch) | Out-Null
    }

    # --- Restore the parked untracked clutter ------------------------------

    $stashWarning = ''
    if ($stashed) {
        $pop = Invoke-Tool -Exe 'git' -CmdArgs @('stash', 'pop')
        if ($pop.Code -ne 0) {
            $stashWarning = "WARNING: 'git stash pop' failed; parked untracked files " +
                "are still stashed. Run 'git stash pop' by hand.`n$($pop.Text)"
        }
    }

    # --- Publish passed work ------------------------------------------------

    $published = $false
    $publishFailed = $false
    $publishHardFailed = $false
    $publishDetail = ''
    $publishedSha = ''
    $eligibleForPublish = ($committed -and $loopExit -eq 0 -and $verdict -eq 'pass')

    if ($eligibleForPublish -and -not $NoPublish) {
        Write-Output "Codex control passed; publishing $branch to origin/main."

        $fetch = Invoke-Tool -Exe 'git' -CmdArgs @('fetch', '--quiet', 'origin', '+main:refs/remotes/origin/main')
        if ($fetch.Code -ne 0) {
            $publishFailed = $true
            $publishHardFailed = $true
            $publishDetail = "git fetch origin main failed (exit $($fetch.Code)): $($fetch.Text)"
        } else {
            $preMergeSha = (Git-Step @('rev-parse', 'main')).Trim()
            $originMainSha = (Git-Step @('rev-parse', 'origin/main')).Trim()
            if ($preMergeSha -ne $originMainSha) {
                $publishFailed = $true
                $publishHardFailed = $true
                $publishDetail = "origin/main moved during dispatch ($($originMainSha.Substring(0,8)) != local main $($preMergeSha.Substring(0,8))); leaving $branch local."
            } else {
                $merge = Invoke-Tool -Exe 'git' -CmdArgs @('merge', '--ff-only', $branch)
                if ($merge.Code -ne 0) {
                    $publishFailed = $true
                    $publishHardFailed = $true
                    $publishDetail = "git merge --ff-only $branch failed (exit $($merge.Code)): $($merge.Text)"
                } else {
                    $push = Invoke-Tool -Exe 'git' -CmdArgs @('push', 'origin', 'main')
                    if ($push.Code -ne 0) {
                        # Push failed AFTER the local ff-merge: local main is now
                        # ahead of origin. Reset it back so the next tick's
                        # preflight sync check is not permanently broken.
                        $publishFailed = $true
                        $publishHardFailed = $true
                        $reset = Invoke-Tool -Exe 'git' -CmdArgs @('reset', '--hard', $preMergeSha)
                        $resetNote = if ($reset.Code -eq 0) {
                            "local main reset to $($preMergeSha.Substring(0,8))"
                        } else {
                            "WARNING: could not reset local main (exit $($reset.Code)); it may be ahead of origin"
                        }
                        $publishDetail = "git push origin main failed (exit $($push.Code)): $($push.Text)`n$resetNote; $branch kept for review."
                    } else {
                        $published = $true
                        $publishedSha = (Git-Step @('rev-parse', '--short', 'HEAD')).Trim()
                        $publishDetail = "Published to origin/main as $publishedSha."
                        $delete = Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-d', $branch)
                        if ($delete.Code -ne 0) {
                            $publishDetail += "`nWARNING: published, but could not delete local branch $branch (exit $($delete.Code)): $($delete.Text)"
                        }
                    }
                }
            }
        }
    } elseif ($eligibleForPublish -and $NoPublish) {
        $publishDetail = "NoPublish set; kept $branch local."
    } elseif ($committed) {
        $publishFailed = $true
        $publishDetail = "Not published because loop exit code was $loopExit and Codex control verdict was '$verdict'."
    } else {
        $publishDetail = "No branch commit was created."
    }

    # --- Post the result comment and finalize labels -----------------------

    $branchLine = if ($committed) {
        if ($published) {
            "Published ``$publishedSha`` to ``origin/main`` from branch ``$branch``."
        } elseif ($NoPublish) {
            "Committed locally as ``$commitSha`` on branch ``$branch`` (NoPublish set; not pushed)."
        } else {
            "Committed locally as ``$commitSha`` on branch ``$branch`` but publish did not complete: $publishDetail"
        }
    } else {
        "The loop produced no committable changes; branch ``$branch`` was not kept."
    }
    $logTail = if ($loopText) {
        ($loopText -split "`r?`n" | Select-Object -Last 30) -join "`n"
    } else { '(no loop output captured)' }
    $runFailed = ($loopExit -ne 0 -or $publishFailed)
    # A publish-pipeline failure is terminal, not retryable: re-running it
    # risks the diverged-main corruption. Only dispatch-execution failures
    # (non-zero loop exit) get the one automatic retry.
    $willRetry = ($runFailed -and -not $isRetry -and -not $publishHardFailed)
    if ($willRetry -and $committed) {
        # First failure of a retry-eligible issue: archive the failed branch
        # (it holds the audit-log commit) under .attemptN so the retry can
        # reuse the branch name, rather than destroying it.
        $n = 1
        while ((Invoke-Tool -Exe 'git' -CmdArgs @('branch', '--list', "$branch.attempt$n")).Text.Trim()) { $n++ }
        $archiveBranch = "$branch.attempt$n"
        $rename = Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-m', $branch, $archiveBranch)
        if ($rename.Code -eq 0) {
            Write-Output "Archived failed branch $branch -> $archiveBranch."
        } else {
            Write-Output "WARNING: could not archive failed branch $branch (exit $($rename.Code)): $($rename.Text)"
        }
    }
    $statusIcon = if (-not $runFailed) {
        'succeeded'
    } elseif ($willRetry) {
        'FAILED (auto-retry queued)'
    } else {
        'FAILED'
    }
    $retryNote = if ($willRetry) {
        "`n- Re-queued for one automatic retry; the next run gets the prior-attempt feedback."
    } elseif ($isRetry -and $runFailed) {
        "`n- This was the retry attempt; the issue is now marked ``$failLabel`` for human review."
    } else {
        ''
    }
    $footerLine = if ($NoPublish) {
        '_Posted by Invoke-AiDispatchQueue.ps1 (branch mode): a passed run is committed to its branch for human review; nothing is auto-pushed._'
    } else {
        '_Posted by Invoke-AiDispatchQueue.ps1. Successful control-passed runs are auto-published to origin/main; failed or blocked runs remain local._'
    }
    $commentBody = @"
**AI dispatch run $statusIcon** - dispatch ``$id``

- Loop exit code: ``$loopExit``
- Codex control verdict: ``$verdict``
- $branchLine
- Detailed log: ``$(Get-RepoRelativePathForQueue $dispatchLogPath)``$retryNote

<details><summary>Dispatch loop output (tail)</summary>

``````
$logTail
``````
</details>

$footerLine
"@
    $commentFile = Join-Path $env:TEMP "rge-ai-dispatch-comment-$id.txt"
    Write-Utf8 $commentFile $commentBody
    $comment = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'comment', "$($issue.number)", '--repo', $repoSlug, '--body-file', $commentFile)
    Remove-Item -LiteralPath $commentFile -Force -ErrorAction SilentlyContinue
    if ($comment.Code -ne 0) {
        Write-Output "WARNING: could not post result comment (exit $($comment.Code)): $($comment.Text)"
    }

    if ($willRetry) {
        # Re-queue for one automatic retry: keep the queue label so the issue
        # is re-selected, drop running, add the retry marker. No done/failed.
        $relabel = @('issue', 'edit', "$($issue.number)", '--repo', $repoSlug,
            '--remove-label', $runLabel, '--add-label', $retryLabel)
    } else {
        $relabel = @('issue', 'edit', "$($issue.number)", '--repo', $repoSlug,
            '--remove-label', $runLabel, '--remove-label', $QueueLabel, '--add-label', $doneLabel)
        if ($runFailed) { $relabel += @('--add-label', $failLabel) }
    }
    $rl = Invoke-Tool -Exe 'gh' -CmdArgs $relabel
    # Verify the label mutation actually took. A partial gh edit (e.g. removed
    # running but did not add retry) would otherwise loop forever or never halt.
    $labelOk = $false
    if ($rl.Code -eq 0) {
        $lv = Invoke-Tool -Exe 'gh' -CmdArgs @(
            'issue', 'view', "$($issue.number)", '--repo', $repoSlug, '--json', 'labels')
        if ($lv.Code -eq 0) {
            $nowLabels = @()
            try { $nowLabels = @(($lv.Text | ConvertFrom-Json).labels | ForEach-Object { $_.name }) } catch { }
            if ($willRetry) {
                $labelOk = ($nowLabels -contains $retryLabel) -and
                           ($nowLabels -notcontains $runLabel) -and
                           ($nowLabels -contains $QueueLabel)
            } else {
                $labelOk = ($nowLabels -contains $doneLabel) -and
                           ($nowLabels -notcontains $runLabel) -and
                           ($nowLabels -notcontains $QueueLabel) -and
                           ((-not $runFailed) -or ($nowLabels -contains $failLabel))
            }
        }
    }
    if (-not $labelOk) {
        Write-Output "WARNING: issue #$($issue.number) labels did not finalize to the expected set (gh exit $($rl.Code)): $($rl.Text)"
    }

    if (-not $runFailed -and -not $NoPublish) {
        $closeComment = if ($published) {
            "Auto-published to origin/main as $publishedSha. Detailed log: $(Get-RepoRelativePathForQueue $dispatchLogPath)"
        } else {
            "Dispatch completed with no committable changes. Detailed log: $(Get-RepoRelativePathForQueue $dispatchLogPath)"
        }
        $close = Invoke-Tool -Exe 'gh' -CmdArgs @(
            'issue', 'close', "$($issue.number)", '--repo', $repoSlug,
            '--comment', $closeComment)
        if ($close.Code -ne 0) {
            Write-Output "WARNING: could not close issue #$($issue.number) (exit $($close.Code)): $($close.Text)"
        }
    }

    Remove-Item -LiteralPath $goalFile -Force -ErrorAction SilentlyContinue

    # --- Report ------------------------------------------------------------

    Write-Output ""
    Write-Output "Dispatch $id $statusIcon (loop exit $loopExit, verdict $verdict)."
    Write-Output $branchLine.Replace('`', '')
    if ($willRetry) {
        Write-Output "Issue #$($issue.number) re-queued for one automatic retry; result comment posted."
    } else {
        Write-Output "Issue #$($issue.number) relabelled; result comment posted."
    }
    if (-not $runFailed -and -not $NoPublish) {
        Write-Output "Issue #$($issue.number) closed after publish."
    }
    Write-Output "Loop log: $loopLog"
    if ($stashWarning) {
        Write-Output ""
        Write-Output $stashWarning
    }

    # A label finalization that cannot be verified must not exit 0: the
    # autonomous driver keys its halt and retry accounting off these labels,
    # and a partial relabel could otherwise loop or never halt.
    if (-not $labelOk) {
        Fail ("Dispatch $id could not be finalized to the expected label set " +
            "(gh edit/view failed or applied partially). Exiting non-zero so " +
            "the autonomous driver halts.")
    }
    Finish 0
} catch {
    Fail "Unexpected error: $($_.Exception.Message)"
}
