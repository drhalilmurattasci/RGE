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

.PARAMETER TraceTiming
    Emit timing trace lines for automation phase diagnosis. Can also be enabled
    by setting RGE_AI_DISPATCH_TRACE_TIMING=1.

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
    [int]$MaxCorrectionRounds = 2,

    [switch]$TraceTiming,

    [switch]$EnablePreflightAudit
)

$ErrorActionPreference = 'Stop'

$script:TraceTimingEnabled = [bool]$TraceTiming -or ($env:RGE_AI_DISPATCH_TRACE_TIMING -match '^(1|true|yes|on)$')
$script:TraceTimingStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$script:TraceTimingScriptLeaf = 'Invoke-AiDispatchQueue.ps1'
$script:TraceTimingJsonlPath = $null
$script:TraceTimingJsonlInitialized = $false

function Initialize-TimingTraceJsonl {
    if ($script:TraceTimingJsonlInitialized) { return }
    $script:TraceTimingJsonlInitialized = $true
    try {
        $traceDir = Join-Path $PSScriptRoot '.ai\dispatch-trace'
        if (-not (Test-Path -LiteralPath $traceDir)) {
            New-Item -ItemType Directory -Path $traceDir -Force | Out-Null
        }
        $leaf  = [System.IO.Path]::GetFileNameWithoutExtension($script:TraceTimingScriptLeaf)
        $stamp = (Get-Date).ToString('yyyyMMdd-HHmmss-fff')
        $script:TraceTimingJsonlPath = Join-Path $traceDir "$leaf-$stamp-$PID.jsonl"
    } catch {
        $script:TraceTimingJsonlPath = $null
    }
}

function Write-TimingTrace {
    param([string]$Message)
    if (-not $script:TraceTimingEnabled) { return }
    $now = Get-Date
    $elapsedSeconds = $script:TraceTimingStopwatch.Elapsed.TotalSeconds
    $elapsed = '{0:n3}' -f $elapsedSeconds
    Write-Output "[TRACE $($now.ToString('HH:mm:ss.fff')) +${elapsed}s] $Message"

    # Best-effort JSONL persistence; never throws and never affects exit code.
    try {
        Initialize-TimingTraceJsonl
        if (-not $script:TraceTimingJsonlPath) { return }

        $eventName = $Message
        $colonIdx  = $Message.IndexOf(':')
        if ($colonIdx -gt 0) {
            $eventName = $Message.Substring(0, $colonIdx)
        } else {
            $wsIdx = $Message.IndexOfAny(@(' ', "`t"))
            if ($wsIdx -gt 0) { $eventName = $Message.Substring(0, $wsIdx) }
        }

        $repo = [ordered]@{}
        if ($script:RepoRoot) { $repo.root = [string]$script:RepoRoot }
        if ($script:repoSlug) { $repo.slug = [string]$script:repoSlug }

        $entry = [ordered]@{
            timestamp       = $now.ToString('o')
            elapsed_seconds = $elapsedSeconds
            script          = $script:TraceTimingScriptLeaf
            pid             = $PID
            event           = $eventName
            message         = $Message
            repo            = $repo
        }
        $json = $entry | ConvertTo-Json -Compress -Depth 5
        [System.IO.File]::AppendAllText(
            $script:TraceTimingJsonlPath,
            $json + "`n",
            [System.Text.UTF8Encoding]::new($false))
    } catch {
        # Swallow: JSONL trace persistence must never block dispatch progress.
    }
}

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
- Dispatch: ``$Id``
- Issue: #$($Issue.number) - $($Issue.title)
- Issue URL: $($Issue.url)
- Branch: ``$Branch``
- Loop exit code: ``$LoopExit``
- Codex control verdict: ``$Verdict``
- Loop log: ``$LoopLog``

## Process Trace

1. Queue selected the oldest open $QueueLabel issue.
2. Queue labelled the issue $runLabel.
3. Queue created branch $Branch.
4. ``Invoke-AiDispatchLoop.ps1`` ran Codex plan, Claude gate, Claude execute, and Codex control.
5. Queue wrote this detailed log before staging, committing, merging, or pushing.
6. If and only if exit code is 0 and Codex control verdict is ``pass``, queue will fast-forward ``main`` and push ``origin/main``.

## Files Changed / Added / Deleted

``git status --short --untracked-files=all`` before the queue commit:

~~~text
$status
~~~

``git diff --name-status`` before the queue commit:

~~~text
$nameStatus
~~~

``git diff --stat`` before the queue commit:

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

# --- Queue scope guard helpers --------------------------------------------
# Validate that, after Write-DispatchLog returns and before `git add -A`, the
# only changed or untracked paths in the worktree are this dispatch's own
# allowed artifacts plus the positive surface declared in the active TASK
# packet's `### MAY edit` / `### MAY add new files` sections. This blocks the
# queue from accidentally staging stray work outside the dispatch's scope.

function Convert-ToRepoRelativePath {
    # Normalize an already-relative path string to repo-relative,
    # forward-slash form so it lines up with `git status` output on Windows.
    param([string]$Path)
    if (-not $Path) { return '' }
    $p = ($Path -replace '\\', '/').Trim()
    if ($p.Length -ge 2 -and $p.StartsWith('"') -and $p.EndsWith('"')) {
        $p = $p.Substring(1, $p.Length - 2)
    }
    while ($p.StartsWith('./')) { $p = $p.Substring(2) }
    return $p.TrimStart('/')
}

function Convert-GlobToRegexForQueueGuard {
    # Glob to anchored regex: `**` -> `.*`, `*` -> `[^/]*`, `?` -> `[^/]`.
    # `*` stays segment-bounded so a TASK token like `foo/*.md` cannot
    # accidentally cover `foo/sub/x.md`.
    param([string]$Glob)
    $sb = [System.Text.StringBuilder]::new()
    [void]$sb.Append('^')
    $i = 0
    while ($i -lt $Glob.Length) {
        $c = $Glob[$i]
        if ($c -eq '*') {
            if ($i + 1 -lt $Glob.Length -and $Glob[$i + 1] -eq '*') {
                [void]$sb.Append('.*')
                $i += 2
            } else {
                [void]$sb.Append('[^/]*')
                $i++
            }
        } elseif ($c -eq '?') {
            [void]$sb.Append('[^/]')
            $i++
        } else {
            [void]$sb.Append([regex]::Escape([string]$c))
            $i++
        }
    }
    [void]$sb.Append('$')
    return $sb.ToString()
}

function Test-TaskTokenMatchesPath {
    # Match a repo-relative path against one positive TASK token. Glob tokens
    # go through Convert-GlobToRegexForQueueGuard. Non-glob tokens match
    # exactly OR as a path-boundary directory prefix, so `some/dir` allows
    # `some/dir/file.txt` without leaking to `some/dir2/file.txt`.
    param([string]$Path, [string]$Token)
    if (-not $Token) { return $false }
    if ($Token -match '[*?]') {
        return [regex]::IsMatch($Path, (Convert-GlobToRegexForQueueGuard -Glob $Token))
    }
    $tok = $Token.TrimEnd('/')
    if (-not $tok) { return $false }
    if ($Path -eq $tok) { return $true }
    return $Path.StartsWith($tok + '/')
}

function Test-LooksLikePathToken {
    # Decide whether a backtick-quoted token in a MAY section is a path
    # candidate. Bare identifiers like `Write-DispatchLog` or `git add` are
    # not paths and must not enter the allowlist.
    param([string]$Token)
    if (-not $Token) { return $false }
    if ($Token -match '\s') { return $false }
    if ($Token.Contains('/')) { return $true }
    if ($Token.Contains('*')) { return $true }
    if ($Token -match '\.[A-Za-z0-9]{1,8}$') { return $true }
    return $false
}

function Get-ActiveTaskPacketPathForQueueGuard {
    # Return the newest TASK packet for this dispatch under ai_handoffs/, or
    # $null. Sorting by Name picks the lexicographically latest timestamp,
    # which is also the latest in time given new-handoff.ps1's filename shape.
    param([string]$DispatchId)
    $handoffDir = Join-Path $script:RepoRoot 'ai_handoffs'
    if (-not (Test-Path -LiteralPath $handoffDir)) { return $null }
    $packet = Get-ChildItem -LiteralPath $handoffDir -File -Filter "${DispatchId}_TASK_*.md" -ErrorAction SilentlyContinue |
        Sort-Object Name |
        Select-Object -Last 1
    if ($packet) { return $packet.FullName }
    return $null
}

function Get-TaskPositiveAllowedTokens {
    # Parse only the active TASK packet's `### MAY edit` and
    # `### MAY add new files` sections. Extract backtick-quoted path tokens,
    # quarantine anything under `ai_handoffs/` or `ai_dispatch_logs/` (those
    # trees are governed exclusively by the hard-coded artifact rules and the
    # exact just-written queue log), and skip fenced code blocks so a `#`
    # inside a code sample is not misread as a section heading.
    param([string]$TaskPath)
    if (-not (Test-Path -LiteralPath $TaskPath)) { return @() }
    $content = Get-Content -Raw -LiteralPath $TaskPath
    if (-not $content) { return @() }
    $lines = @($content -split "`r?`n")
    $tokens = @()
    $inAllowedSection = $false
    $inFence = $false
    foreach ($line in $lines) {
        if ($line -match '^[ \t]*(```|~~~)') {
            $inFence = -not $inFence
            continue
        }
        if ($inFence) { continue }
        if ($line -match '^#{1,6}\s') {
            $inAllowedSection = ($line -match '^###\s+MAY\s+(edit|add\s+new\s+files)\s*$')
            continue
        }
        if (-not $inAllowedSection) { continue }
        foreach ($m in [regex]::Matches($line, '`([^`]+)`')) {
            $raw = $m.Groups[1].Value.Trim()
            if (-not (Test-LooksLikePathToken $raw)) { continue }
            $norm = Convert-ToRepoRelativePath $raw
            if (-not $norm) { continue }
            if ($norm -match '^(ai_handoffs|ai_dispatch_logs)(/|$)') { continue }
            $tokens += $norm
        }
    }
    return @($tokens | Select-Object -Unique)
}

function Test-ActiveDispatchArtifactPath {
    # Allow only one-segment basenames directly under ai_handoffs/. A broad
    # -like 'ai_handoffs/ISSUE-180_EXEC_*.md' would wrongly cover nested
    # paths like ai_handoffs/ISSUE-180_EXEC_x/nested.md, because `*` there
    # matches `/`. The explicit `^ai_handoffs/([^/]+)$` shape rules that out.
    # The basename must additionally carry the `new-handoff.ps1` timestamp
    # shape `yyyy-MM-dd_HH-mm-ss<+|->HHMM`, so arbitrary suffixes like
    # `ISSUE-180_EXEC_x.md` or `ISSUE-180_EXEC_x.meta.json` are rejected.
    param([string]$Path, [string]$DispatchId)
    if ($Path -notmatch '^ai_handoffs/([^/]+)$') { return $false }
    $base = $Matches[1]
    $idEsc = [regex]::Escape($DispatchId)
    $tsPat = '\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}[+-]\d{4}'
    return ($base -match "^${idEsc}_(TASK|EXEC|CORRECT)_${tsPat}\.(md|meta\.json)$")
}

function Test-ExactQueueLogPath {
    # Allow exactly the dispatch log path Write-DispatchLog just returned,
    # and only when it lives directly under ai_dispatch_logs/ as log_*.md.
    # A TASK token like `ai_dispatch_logs/log_*.md` is quarantined elsewhere
    # so it cannot broaden this single-path allowance.
    param([string]$Path, [string]$LogPath)
    if (-not $LogPath) { return $false }
    $logRel = Convert-ToRepoRelativePath (Get-RepoRelativePathForQueue $LogPath)
    if (-not $logRel) { return $false }
    if ($logRel -notmatch '^ai_dispatch_logs/log_[^/]+\.md$') { return $false }
    return ($Path -eq $logRel)
}

function Get-QueueStatusEntries {
    # Parse `git status --short --untracked-files=all` into entries. Capture
    # stdout and stderr separately so git stderr noise (e.g., permission
    # warnings for $HOME/.config/git/ignore) is never fed into path
    # validation. Each surviving stdout line must additionally match the
    # porcelain short-status shape (`XY <path>` with X, Y from the documented
    # status-code alphabet ' MADRCUT?!') as defense in depth. core.quotepath=false
    # keeps non-ASCII bytes raw rather than octal-escaped so the path strings
    # compare directly with TASK tokens. Rename and copy entries carry both
    # an old and a new repo-relative path; every other status carries one
    # path. A non-zero git exit still fails closed. A stdout line with
    # porcelain short-status shape (space at column 3) but an unrecognized
    # status code in columns 1-2 is also failed closed so future real status
    # codes cannot be silently dropped before broad staging.
    $tmpOut = [System.IO.Path]::GetTempFileName()
    $tmpErr = [System.IO.Path]::GetTempFileName()
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    $exitCode = 0
    $stdoutText = ''
    $stderrText = ''
    try {
        # PS 5.1 note: keep stderr in its own file (no `2>&1` merge) so a
        # warning line on stderr never looks like a porcelain record. EAP is
        # Continue here so native stderr writes do not raise a terminating
        # error before the exit code is read.
        & 'git' '-c' 'core.quotepath=false' 'status' '--short' '--untracked-files=all' 1> $tmpOut 2> $tmpErr
        $exitCode = $LASTEXITCODE
        $stdoutText = Get-Content -Raw -LiteralPath $tmpOut -ErrorAction SilentlyContinue
        $stderrText = Get-Content -Raw -LiteralPath $tmpErr -ErrorAction SilentlyContinue
        if ($null -eq $stdoutText) { $stdoutText = '' }
        if ($null -eq $stderrText) { $stderrText = '' }
    } finally {
        $ErrorActionPreference = $prevEap
        Remove-Item -LiteralPath $tmpOut -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $tmpErr -Force -ErrorAction SilentlyContinue
    }
    if ($exitCode -ne 0) {
        $msg = "Queue scope guard: 'git status --short --untracked-files=all' failed (exit $exitCode)"
        if ($stderrText) { $msg += ":`n$stderrText" }
        Fail $msg
    }
    $entries = @()
    foreach ($line in @($stdoutText -split "`r?`n")) {
        if (-not $line -or $line.Length -lt 4) { continue }
        # Strict porcelain short-status shape: two status-code chars from the
        # documented set (' ', 'M', 'A', 'D', 'R', 'C', 'U', 'T', '?', '!')
        # and a space at column 3. 'T' covers a regular-file -> symlink (or
        # equivalent) type change which git status reports just like a
        # modification but which would otherwise be dropped from guard
        # validation before `git add -A`.
        if ($line.Substring(2, 1) -ne ' ') {
            # Not porcelain shape (no separator at column 3). Skip - this is
            # not a status record. Stderr is captured separately, so warning
            # text reaching here would be malformed in some other way; the
            # length-4 guard above also weeds out very short noise.
            continue
        }
        $statusCode = $line.Substring(0, 2)
        if ($statusCode -notmatch '^[ MADRCUT?!][ MADRCUT?!]$') {
            # Porcelain-shaped but with an unknown status code. Fail closed
            # so a status alphabet expansion in a future git release cannot
            # bypass the scope guard and have its path silently staged by
            # the broad `git add -A` that follows.
            Fail ("Queue scope guard: 'git status --short --untracked-files=all' returned " +
                "a porcelain-shaped record with an unrecognized status code: '$line'. " +
                "Refusing to stage or commit until the status alphabet is updated.")
        }
        $rest = $line.Substring(3)
        $hasRenameOrCopy = ($statusCode.IndexOf('R') -ge 0 -or $statusCode.IndexOf('C') -ge 0)
        if ($hasRenameOrCopy -and $rest -match '^(.+?)\s+->\s+(.+)$') {
            $oldPath = Convert-ToRepoRelativePath $Matches[1]
            $newPath = Convert-ToRepoRelativePath $Matches[2]
            $entries += [pscustomobject]@{ Code = $statusCode; Paths = @($oldPath, $newPath) }
        } else {
            $entries += [pscustomobject]@{ Code = $statusCode; Paths = @((Convert-ToRepoRelativePath $rest)) }
        }
    }
    return ,$entries
}

function Invoke-QueueScopeGuard {
    # Block the queue from staging or publishing anything outside the active
    # dispatch's allowed surface. Allowed surfaces:
    #   * Active-dispatch TASK / EXEC / CORRECT packets and matching
    #     .meta.json sidecars, single basename directly under ai_handoffs/.
    #   * The exact queue log path that Write-DispatchLog just returned.
    #   * Positive path tokens parsed from the active TASK packet's
    #     `### MAY edit` and `### MAY add new files` sections.
    # Fails closed if the active TASK packet cannot be located or yields no
    # positive tokens. Renames and copies require BOTH paths to be allowed.
    param([string]$DispatchId, [string]$DispatchLogPath)

    $taskPath = Get-ActiveTaskPacketPathForQueueGuard -DispatchId $DispatchId
    if (-not $taskPath) {
        Fail ("Queue scope guard: no active TASK packet for $DispatchId found under " +
            "ai_handoffs/; refusing to stage or commit.")
    }

    $tokens = Get-TaskPositiveAllowedTokens -TaskPath $taskPath
    if ($tokens.Count -eq 0) {
        Fail ("Queue scope guard: active TASK packet '$(Get-RepoRelativePathForQueue $taskPath)' " +
            "declares no positive allowed-path tokens (no path-like tokens in " +
            "'### MAY edit' or '### MAY add new files'); failing closed before staging.")
    }

    $entries = Get-QueueStatusEntries
    $disallowed = @()
    foreach ($entry in $entries) {
        foreach ($p in $entry.Paths) {
            if (-not $p) { continue }
            $allowed = $false
            if (Test-ActiveDispatchArtifactPath -Path $p -DispatchId $DispatchId) {
                $allowed = $true
            } elseif (Test-ExactQueueLogPath -Path $p -LogPath $DispatchLogPath) {
                $allowed = $true
            } else {
                foreach ($t in $tokens) {
                    if (Test-TaskTokenMatchesPath -Path $p -Token $t) {
                        $allowed = $true
                        break
                    }
                }
            }
            if (-not $allowed) {
                $disallowed += "  [$($entry.Code)] $p"
            }
        }
    }
    if ($disallowed.Count -gt 0) {
        Fail ("Queue scope guard: $($disallowed.Count) changed or untracked path(s) " +
            "fall outside the active TASK packet's positive allowed surface for " +
            "$DispatchId. Refusing to stage, commit, merge, push, or publish.`n" +
            "Disallowed paths:`n" + ($disallowed -join "`n") +
            "`nActive TASK packet: $(Get-RepoRelativePathForQueue $taskPath)" +
            "`nDispatch log path: $(Get-RepoRelativePathForQueue $DispatchLogPath)")
    }
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

function Get-ExecutionStatus {
    # Claude's execute wrapper writes EXEC_STATUS into claude.execute.round<N>.md.
    # If that marker is absent, mirror the dispatch loop's fallback to the
    # canonical EXEC packet footer. A deliberate "blocked" status means a halt
    # condition fired and should not be retried as if it were an accidental
    # execution failure.
    param([string]$RunDir, [string]$DispatchId)
    $exec = Get-NewestRoundFile -RunDir $RunDir -Filter 'claude.execute.round*.md'
    if ($exec) {
        try {
            $marker = Select-String -LiteralPath $exec.FullName -Pattern '^EXEC_STATUS:\s*(\S+)\s*$' -ErrorAction Stop |
                Select-Object -Last 1
            if ($marker -and $marker.Matches.Count -gt 0) {
                return [string]$marker.Matches[0].Groups[1].Value
            }
        } catch {
            # Fall through to the canonical packet footer below.
        }
    }

    if ($DispatchId) {
        $handoffDir = Join-Path $script:RepoRoot 'ai_handoffs'
        $packet = Get-ChildItem -LiteralPath $handoffDir -File -Filter "$DispatchId`_EXEC_*.md" -ErrorAction SilentlyContinue |
            Sort-Object Name |
            Select-Object -Last 1
        if ($packet) {
            try {
                $text = Get-Content -Raw -LiteralPath $packet.FullName
                $handoff = [regex]::Match($text, '(?m)^HANDOFF_STATUS:\s*(\S+)\s*$').Groups[1].Value
                $packetStatus = [regex]::Match($text, '(?m)^STATUS:\s*(\S+)\s*$').Groups[1].Value
                $exitRaw = [regex]::Match($text, '(?m)^EXIT_CODE:\s*(-?\d+)\s*$').Groups[1].Value
                $handoffNorm = if ($handoff) { $handoff.ToUpperInvariant() } else { '' }
                $packetStatusNorm = if ($packetStatus) { $packetStatus.ToUpperInvariant() } else { '' }
                $exitCode = $null
                if ($exitRaw) { $exitCode = [int]$exitRaw }

                if ($handoffNorm -eq 'COMPLETE' -and $exitCode -eq 0) { return 'executed' }
                if ($handoffNorm -in @('BLOCKED', 'NEEDS_HUMAN') -or $packetStatusNorm -in @('BLOCKED', 'NEEDS_HUMAN')) { return 'blocked' }
                if ($handoffNorm -eq 'FAILED' -or $packetStatusNorm -eq 'FAILED' -or ($null -ne $exitCode -and $exitCode -ne 0)) { return 'failed' }
            } catch {
                return 'unknown'
            }
        }
    }
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

function Get-FailureTaxonomyLabels {
    # Classify a terminal failed dispatch run using outcomes the queue has
    # already computed after the loop and publish decision. Returns the
    # taxonomy label set to apply alongside ai-dispatch-failed; the set is
    # never empty, so unknown failures still land in ai-dispatch-failure-unknown.
    # Callers must invoke this only when the run is terminal (runFailed=true,
    # willRetry=false). The helper does not read or write files, call gh/git,
    # or alter loop output -- it is a pure text classifier over loop output.
    param(
        [string]$LoopText,
        [string]$ExecStatus,
        [bool]$PublishHardFailed
    )

    # Order matters: more specific signals win over more generic ones, so a
    # publish-pipeline failure beats any loop-text wording, a blocked execution
    # beats timeout wording, and a Codex watchdog stall is never demoted to a
    # generic timeout.
    if ($PublishHardFailed) {
        return @('ai-dispatch-failure-publish')
    }
    if ($ExecStatus -eq 'blocked') {
        return @('ai-dispatch-failure-blocked')
    }
    $text = [string]$LoopText
    if ($text -match '(?i)codex exec stalled' -or $text -match '(?i)no log growth') {
        return @('ai-dispatch-failure-stall')
    }
    if ($text -match '(?i)timed out' -or $text -match '(?i)timeout') {
        return @('ai-dispatch-failure-timeout')
    }
    if ($text -match '(?i)verification gate failed' -or
        $text -match '(?i)verification round \d+:\s*fail') {
        return @('ai-dispatch-failure-verification')
    }
    if ($text -match '(?i)codex control blocked' -or
        $text -match '(?i)codex requested changes' -or
        $text -match '(?i)maxcorrectionrounds=\d+ is exhausted') {
        return @('ai-dispatch-failure-control')
    }
    return @('ai-dispatch-failure-unknown')
}

# --- Environment -----------------------------------------------------------

# Testability seam: when RGE_AI_DISPATCH_QUEUE_SKIP_MAIN is set, return
# before any top-level dispatch flow. Pester (tools/dispatch-tests/**) dot-
# sources this script with that env var so the function definitions above
# (Write-DispatchLog, Git-Step, ...) load without requiring gh / codex /
# claude on PATH, a real GitHub remote, or the queue lock. Direct
# invocation never sets the env var, so production queue behavior is
# unchanged.
if ($env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -eq '1') {
    return
}

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
        @{ Name = $retryLabel; Color = 'd4c5f9'; Desc = 'AI dispatch re-queued for one retry' },
        @{ Name = 'ai-dispatch-failure-stall';        Color = 'd93f0b'; Desc = 'AI dispatch terminal failure: Codex watchdog stall' },
        @{ Name = 'ai-dispatch-failure-timeout';      Color = 'd93f0b'; Desc = 'AI dispatch terminal failure: generic timeout' },
        @{ Name = 'ai-dispatch-failure-blocked';      Color = 'd93f0b'; Desc = 'AI dispatch terminal failure: executor reported blocked' },
        @{ Name = 'ai-dispatch-failure-verification'; Color = 'd93f0b'; Desc = 'AI dispatch terminal failure: verification gate failed' },
        @{ Name = 'ai-dispatch-failure-control';      Color = 'd93f0b'; Desc = 'AI dispatch terminal failure: Codex control failed' },
        @{ Name = 'ai-dispatch-failure-publish';      Color = 'd93f0b'; Desc = 'AI dispatch terminal failure: publish pipeline failed' },
        @{ Name = 'ai-dispatch-failure-unknown';      Color = 'd93f0b'; Desc = 'AI dispatch terminal failure: class not matched' }
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
    Write-TimingTrace "queue.loop: start (dispatch=$id, branch=$branch)"
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        $loopArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $loopScript,
            '-DispatchId', $id, '-GoalFile', $goalFile,
            '-MaxPlanRevisions', $MaxPlanRevisions,
            '-MaxCorrectionRounds', $MaxCorrectionRounds)
        if ($EnablePreflightAudit) { $loopArgs += '-EnablePreflightAudit' }
        & powershell.exe @loopArgs 2>&1 | Tee-Object -FilePath $loopLog
    } finally {
        $ErrorActionPreference = $prevEap
    }
    $loopExit = $LASTEXITCODE
    Write-Output "----------------------------------------------------------------"
    Write-Output "Dispatch loop exited with code $loopExit."
    Write-TimingTrace "queue.loop: done (exit=$loopExit)"

    $loopText = (Get-Content -Raw -LiteralPath $loopLog -ErrorAction SilentlyContinue)
    # Read the Codex control verdict from the structured run-dir JSON the loop
    # writes (schema-validated), not by scraping loop stdout. Newest round wins.
    $runDir = Join-Path $script:RepoRoot (Join-Path '.ai' "dispatch-$id")
    $verdict = Get-ControlVerdict -RunDir $runDir
    $execStatus = Get-ExecutionStatus -RunDir $runDir -DispatchId $id
    Write-TimingTrace "queue.control: verdict-read (verdict=$verdict, execStatus=$execStatus)"

    # --- Write detailed audit log, then commit the branch ------------------

    Write-TimingTrace "queue.commit: dispatch-log start"
    $dispatchLogPath = Write-DispatchLog -Id $id -Issue $issue -Branch $branch `
        -LoopLog $loopLog -LoopText ([string]$loopText) -LoopExit $loopExit -Verdict $verdict
    Write-Output "Detailed dispatch log written: $(Get-RepoRelativePathForQueue $dispatchLogPath)"
    Write-TimingTrace "queue.commit: dispatch-log done"

    # Scope guard: validate the worktree against the active TASK packet
    # BEFORE any broad staging, commit, checkout-to-main, merge, push, or
    # publish step. Stray work outside the dispatch's declared surface aborts
    # the run here -- nothing is staged, committed, or published.
    Write-TimingTrace "queue.guard: scope-check start"
    Invoke-QueueScopeGuard -DispatchId $id -DispatchLogPath $dispatchLogPath
    Write-TimingTrace "queue.guard: scope-check done"

    Write-TimingTrace "queue.commit: git-add start"
    Git-Step @('add', '-A') | Out-Null
    Write-TimingTrace "queue.commit: git-add done"
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
        Write-TimingTrace "queue.commit: git-commit start"
        Git-Step @('commit', '-F', $msgFile) | Out-Null
        Write-TimingTrace "queue.commit: git-commit done"
        Remove-Item -LiteralPath $msgFile -Force -ErrorAction SilentlyContinue
        $commitSha = (Git-Step @('rev-parse', '--short', 'HEAD')).Trim()
        $committed = $true
        Write-TimingTrace "queue.commit: committed (sha=$commitSha)"
    } else {
        Write-TimingTrace "queue.commit: no staged changes"
    }

    Write-TimingTrace "queue.commit: checkout-main start"
    Git-Step @('checkout', 'main') | Out-Null
    Write-TimingTrace "queue.commit: checkout-main done"
    if (-not $committed) {
        Write-TimingTrace "queue.commit: delete-empty-branch start"
        Git-Step @('branch', '-D', $branch) | Out-Null
        Write-TimingTrace "queue.commit: delete-empty-branch done"
    }

    # --- Restore the parked untracked clutter ------------------------------

    $stashWarning = ''
    if ($stashed) {
        Write-TimingTrace "queue.stash: restore start"
        $pop = Invoke-Tool -Exe 'git' -CmdArgs @('stash', 'pop')
        Write-TimingTrace "queue.stash: restore done (exit=$($pop.Code))"
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
        Write-TimingTrace "queue.publish: block-entry; eligibleForPublish=true"

        Write-TimingTrace "queue.publish: git-fetch start"
        $fetch = Invoke-Tool -Exe 'git' -CmdArgs @('fetch', '--quiet', 'origin', '+main:refs/remotes/origin/main')
        Write-TimingTrace "queue.publish: git-fetch done (exit=$($fetch.Code))"
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
                Write-TimingTrace "queue.publish: ff-merge start (branch=$branch)"
                $merge = Invoke-Tool -Exe 'git' -CmdArgs @('merge', '--ff-only', $branch)
                Write-TimingTrace "queue.publish: ff-merge done (exit=$($merge.Code))"
                if ($merge.Code -ne 0) {
                    $publishFailed = $true
                    $publishHardFailed = $true
                    $publishDetail = "git merge --ff-only $branch failed (exit $($merge.Code)): $($merge.Text)"
                } else {
                    Write-TimingTrace "queue.publish: git-push start"
                    $push = Invoke-Tool -Exe 'git' -CmdArgs @('push', 'origin', 'main')
                    Write-TimingTrace "queue.publish: git-push done (exit=$($push.Code))"
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
                        Write-TimingTrace "queue.publish: published as $publishedSha; branch-delete start"
                        $delete = Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-d', $branch)
                        Write-TimingTrace "queue.publish: branch-delete done (exit=$($delete.Code))"
                        if ($delete.Code -ne 0) {
                            $publishDetail += "`nWARNING: published, but could not delete local branch $branch (exit $($delete.Code)): $($delete.Text)"
                        }
                    }
                }
            }
        }
        Write-TimingTrace "queue.publish: block-exit (published=$published, publishFailed=$publishFailed, publishHardFailed=$publishHardFailed)"
    } elseif ($eligibleForPublish -and $NoPublish) {
        $publishDetail = "NoPublish set; kept $branch local."
        Write-TimingTrace "queue.publish: skipped (NoPublish=true, eligibleForPublish=true)"
    } elseif ($committed) {
        $publishFailed = $true
        $publishDetail = "Not published because loop exit code was $loopExit and Codex control verdict was '$verdict'."
        Write-TimingTrace "queue.publish: skipped (eligibleForPublish=false, loopExit=$loopExit, verdict=$verdict)"
    } else {
        $publishDetail = "No branch commit was created."
        Write-TimingTrace "queue.publish: skipped (committed=false)"
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
    # risks the diverged-main corruption. A deliberate EXEC_STATUS=blocked is
    # also terminal: the executor hit a task-defined halt condition, so a retry
    # would just ask the next run to "fix" a scope boundary it was told to obey.
    # Only accidental dispatch-execution failures get the one automatic retry.
    $runBlocked = ($execStatus -eq 'blocked')
    $willRetry = ($runFailed -and -not $runBlocked -and -not $isRetry -and -not $publishHardFailed)
    # Classify terminal failures into a durable taxonomy label so a human (or
    # later policy analysis) can triage them without re-reading loop output.
    # The taxonomy is layered on top of ai-dispatch-failed, not a replacement
    # for it, and is only applied to non-retry terminal failures.
    $taxonomyLabels = @()
    if ($runFailed -and -not $willRetry) {
        $taxonomyLabels = @(Get-FailureTaxonomyLabels `
            -LoopText ([string]$loopText) `
            -ExecStatus $execStatus `
            -PublishHardFailed $publishHardFailed)
    }
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
    } elseif ($runBlocked) {
        'BLOCKED'
    } elseif ($willRetry) {
        'FAILED (auto-retry queued)'
    } else {
        'FAILED'
    }
    $retryNote = if ($willRetry) {
        "`n- Re-queued for one automatic retry; the next run gets the prior-attempt feedback."
    } elseif ($isRetry -and $runFailed) {
        "`n- This was the retry attempt; the issue is now marked ``$failLabel`` for human review."
    } elseif ($runBlocked -and $runFailed) {
        "`n- Executor reported ``EXEC_STATUS: blocked``; no automatic retry was queued."
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
    Write-TimingTrace "queue.github: comment start"
    $comment = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'comment', "$($issue.number)", '--repo', $repoSlug, '--body-file', $commentFile)
    Write-TimingTrace "queue.github: comment done (exit=$($comment.Code))"
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
        if ($isRetry) { $relabel += @('--remove-label', $retryLabel) }
        if ($runFailed) { $relabel += @('--add-label', $failLabel) }
        foreach ($tl in $taxonomyLabels) {
            $relabel += @('--add-label', $tl)
        }
    }
    Write-TimingTrace "queue.github: relabel start"
    $rl = Invoke-Tool -Exe 'gh' -CmdArgs $relabel
    Write-TimingTrace "queue.github: relabel done (exit=$($rl.Code))"
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
                           ($nowLabels -notcontains $retryLabel) -and
                           ((-not $runFailed) -or ($nowLabels -contains $failLabel))
                if ($labelOk -and $runFailed) {
                    foreach ($tl in $taxonomyLabels) {
                        if ($nowLabels -notcontains $tl) { $labelOk = $false; break }
                    }
                }
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
        Write-TimingTrace "queue.github: close done (exit=$($close.Code))"
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
