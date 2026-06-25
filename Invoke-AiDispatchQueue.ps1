#Requires -Version 5.1
<#
.SYNOPSIS
    Pull the next `ai-dispatch`-labelled GitHub issue and run it through the
    dispatch loop on a local branch, unattended.

.DESCRIPTION
    Work source : open GitHub issues labelled `ai-dispatch`, oldest first.
    Execution   : Invoke-AiDispatchLoop.ps1 on a per-issue branch
                  `ai-dispatch/ISSUE-<n>`, run as an isolated child process.
                  The default executor is Codex; `-Executor claude` is an
                  explicit opt-in passed through to the loop.
    Publish     : if the dispatch exits 0 and Codex control says pass, the
                  default is `pr` mode: push the dispatch branch and open a
                  GitHub pull request targeting main for human review. Use
                  `-PublishMode main` to opt in to the auto-publish path that
                  fast-forwards origin/main, or `-NoPublish` / `-PublishMode
                  branch` to keep the branch local. Failed / blocked runs
                  remain local for inspection.
    Bookkeeping : the issue is relabelled (running -> done, plus failed on a
                  non-zero loop exit) and a result comment is posted.

    Exactly one issue is processed per invocation. This is meant to be fired
    on a recurring schedule (e.g. Claude Code `/loop`). A temp-dir lock file
    prevents overlapping runs from colliding. An ADR-121 handoff claim also
    prevents another actor from executing the same dispatch id concurrently.

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

.PARAMETER SkipHandoffClaim
    Skip ADR-121 handoff claim acquisition and release. This is an operator
    escape hatch for diagnosing the claim helper; normal queue runs should keep
    the default claim lifecycle enabled.

.PARAMETER HandoffClaimTtlSeconds
    ADR-121 handoff claim TTL in seconds. The default is 12 hours so the live
    claim covers long normal queue runs across model calls, verification, and
    correction rounds without requiring a background renewer.

.NOTES
    Requires local `git`, `gh` (authenticated), `codex`, `powershell.exe`, and
    Invoke-AiDispatchLoop.ps1 in the repo root. `claude` is required only when
    `-Executor claude` is explicitly selected.
    Pushes only successful, Codex-control-passed dispatch commits. Three
    publish modes are supported:
      pr     (default) push the dispatch branch and open a GitHub pull
                       request targeting main without merging or pushing
                       origin/main and without closing the source issue;
      main             fast-forward origin/main and push (explicit opt-in
                       for delegated-human auto-publish batches);
      branch           keep the dispatch branch local for human review.
    Legacy -NoPublish is preserved and is equivalent to -PublishMode branch.
#>
[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$QueueLabel = 'ai-dispatch',

    [ValidateRange(10, 1440)]
    [int]$StaleLockMinutes = 180,

    [switch]$DryRun,

    [switch]$NoPublish,

    [ValidateSet('', 'main', 'branch', 'pr')]
    [string]$PublishMode = '',

    # Default-OFF surface-split publishing: when set, a control-passed run's effective
    # publish mode is derived from its changed paths (Get-DispatchSurfaceRouting) --
    # low-risk auto-merges to main, any high-risk path opens a PR for human merge.
    # Absent => the resolved -PublishMode is used unchanged (current behavior).
    [switch]$SurfaceSplitPublish,

    # Default-OFF diff-size cap (0 = unlimited). A main-routed publish whose diff
    # exceeds either cap is downgraded to a human-merged PR (fail-closed).
    [int]$MaxDiffFiles = 0,
    [int]$MaxDiffLines = 0,

    # Default-OFF brief ride-along. STRICT by default: a changeset that touches the
    # dispatch brief (a re-arm) routes to a human-merged PR even alongside low-risk
    # work. Set to let the brief re-arm ride along with low-risk docs/tests to main.
    [switch]$AllowBriefRideAlong,

    [ValidateRange(0, 5)]
    [int]$MaxPlanRevisions = 2,

    [ValidateRange(0, 5)]
    [int]$MaxCorrectionRounds = 2,

    [ValidateSet('claude', 'codex')]
    [string]$Executor = 'codex',

    [switch]$CodexExecutorExternalScratch,

    [switch]$SkipHandoffClaim,

    [ValidateRange(3600, 604800)]
    [int]$HandoffClaimTtlSeconds = 43200,

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
    # Mirror to stdout so terminal failures surface in captured queue output
    # even when the caller pipes only stdout to a log file (the scheduled-
    # task wrapper, Invoke-AiDispatchAuto.ps1, and `.\Invoke-AiDispatchQueue
    # > log.txt` all default to stdout-only capture). Without this mirror,
    # a Fail thrown from the post-dispatch-log / pre-publish-decision window
    # looked like a silent stall: the "Detailed dispatch log written" line
    # showed up but no publish-decision progress comment line ever did.
    Write-Output $Message
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
        [string]$Verdict,
        [string]$WorktreeRoot,
        [ValidateSet('claude', 'codex')]
        [string]$Executor = 'codex'
    )

    # ISSUE-231: when an isolated worktree is supplied, route the audit log
    # file and the diff/status snapshots through it so the log captures the
    # dispatch's own changes (and lands in the worktree's
    # `ai_dispatch_logs/` so it can be committed onto the dispatch branch).
    $logBase = if ($WorktreeRoot) { $WorktreeRoot } else { $script:RepoRoot }
    $logDir = Join-Path $logBase 'ai_dispatch_logs'
    if (-not (Test-Path -LiteralPath $logDir)) {
        New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    }

    $stamp = (Get-Date).ToString('yyyy-MM-dd_HH-mm-sszzz').Replace(':', '')
    $logPath = Join-Path $logDir "log_$stamp.md"
    $runDir = Join-Path $logBase (Join-Path '.ai' "dispatch-$Id")

    # Scope the diff/status snapshots to the worktree when one is supplied so
    # they describe the dispatch's own working tree, not the primary's.
    $gitScope = if ($WorktreeRoot) { @('-C', $WorktreeRoot) } else { @() }
    $status = (Git-Step ($gitScope + @('status', '--short', '--untracked-files=all'))).Trim()
    if (-not $status) { $status = '(clean)' }
    $nameStatus = (Git-Step ($gitScope + @('diff', '--name-status'))).Trim()
    if (-not $nameStatus) { $nameStatus = '(no tracked diff)' }
    $stat = (Git-Step ($gitScope + @('diff', '--stat'))).Trim()
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

    # ISSUE-231: when the dispatch ran inside an isolated worktree, embed a
    # durable "Isolated Worktree" section into the committed audit log so the
    # log itself records WHERE the run lived plus the deterministic
    # inspection/removal commands a human needs. Using $WorktreeRoot only as
    # log location / git scope is not sufficient -- the on-branch audit
    # artifact has to name the worktree path explicitly, since the dispatch
    # branch (or its `.attempt<N>` / `.interrupt<N>` archive) is the
    # surviving handle to this run after the worktree itself is removed,
    # archived, or preserved.
    $worktreeAuditSection = ''
    if ($WorktreeRoot) {
        $worktreeAuditSection = "`n" + (Format-DispatchWorktreeAuditSection -WorktreePath $WorktreeRoot) + "`n"
    }

    $executorLabel = if ($Executor -eq 'codex') { 'Codex' } else { 'Claude' }

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
$worktreeAuditSection
## Process Trace

1. Queue selected the oldest open $QueueLabel issue.
2. Queue labelled the issue $runLabel.
3. Queue created branch $Branch.
4. ``Invoke-AiDispatchLoop.ps1`` ran Codex plan, Claude gate, $executorLabel execute, and Codex control.
5. Queue wrote this detailed log before staging, committing, merging, or pushing.
6. If and only if exit code is 0 and Codex control verdict is ``pass``, queue will publish per the resolved ``-PublishMode``: ``pr`` (default) pushes ``$Branch`` and opens a PR targeting ``main`` without pushing ``origin/main`` or closing the source issue; ``main`` (explicit opt-in) fast-forwards ``main`` and pushes ``origin/main``; ``branch`` / ``-NoPublish`` leaves the work on ``$Branch``.

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
    # Normalize a path to repo-relative, forward-slash form. While a dispatch
    # is in flight ($script:DispatchWorktreeRoot is set), paths are emitted
    # relative to the isolated worktree so the audit log, scope guard, and
    # comment bullets all key off the same view of the dispatch's repo root.
    # Outside a dispatch, paths normalize against the primary repo root.
    param([string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    $rootBase = if ($script:DispatchWorktreeRoot) {
        $script:DispatchWorktreeRoot
    } else {
        $script:RepoRoot
    }
    $root = [System.IO.Path]::GetFullPath($rootBase).TrimEnd('\', '/')
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
    #
    # Accept criteria (any one is sufficient):
    #   * contains a `/` (multi-segment repo path)
    #   * contains a `*` (glob)
    #   * ends in a short extension `.<1-8 alnum>` (regular file)
    #   * is a strict leading-dot repo path token: starts with `.`, has at
    #     least one non-`.` character, and is composed only of
    #     [A-Za-z0-9._-] (optionally with one or more `/` segments where the
    #     same rule applies per segment). This covers `.gitattributes`,
    #     `.gitignore`, `.env`, `.envrc`, and `.cargo/config.toml` without
    #     accepting bare `.` / `..` or whitespace-bearing strings.
    param([string]$Token)
    if (-not $Token) { return $false }
    if ($Token -match '\s') { return $false }
    if ($Token.Contains('/')) { return $true }
    if ($Token.Contains('*')) { return $true }
    if ($Token -match '\.[A-Za-z0-9]{1,8}$') { return $true }
    if ($Token -match '^\.[A-Za-z0-9_-][A-Za-z0-9._-]*$') { return $true }
    return $false
}

function Get-ActiveTaskPacketPathForQueueGuard {
    # Return the newest TASK packet for this dispatch under ai_handoffs/, or
    # $null. Sorting by Name picks the lexicographically latest timestamp,
    # which is also the latest in time given new-handoff.ps1's filename shape.
    # When a dispatch is in flight the search is rooted at the isolated
    # worktree (where the TASK packet was scaffolded for this run), not the
    # primary checkout.
    param([string]$DispatchId)
    $handoffBase = if ($script:DispatchWorktreeRoot) { $script:DispatchWorktreeRoot } else { $script:RepoRoot }
    $handoffDir = Join-Path $handoffBase 'ai_handoffs'
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

function Test-HandoffClaimEventPath {
    # ADR-121 claim events live under ai_handoffs/claims/, not directly under
    # ai_handoffs/. Allow only this dispatch's helper-generated event JSON
    # files; keep every other nested handoff path outside the hard-coded
    # artifact allowance.
    param([string]$Path, [string]$DispatchId)
    $idEsc = [regex]::Escape($DispatchId)
    $tsPat = '\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}-\d{7}[+-]\d{4}'
    $eventPat = '(claim|renew|release|expire|reclaim)'
    return ($Path -match "^ai_handoffs/claims/${idEsc}_${tsPat}_${eventPat}(\.\d+)?\.json$")
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
    # Scope the status call to the dispatch's isolated worktree when one is
    # in flight; otherwise fall back to the current working directory so the
    # helper still works outside a dispatch (e.g. ad-hoc invocation).
    $gitArgs = @('-c', 'core.quotepath=false')
    if ($script:DispatchWorktreeRoot) {
        $gitArgs += @('-C', $script:DispatchWorktreeRoot)
    }
    $gitArgs += @('status', '--short', '--untracked-files=all')
    try {
        # PS 5.1 note: keep stderr in its own file (no `2>&1` merge) so a
        # warning line on stderr never looks like a porcelain record. EAP is
        # Continue here so native stderr writes do not raise a terminating
        # error before the exit code is read.
        & 'git' @gitArgs 1> $tmpOut 2> $tmpErr
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
    #   * Active-dispatch ADR-121 claim event JSON under ai_handoffs/claims/.
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
            } elseif (Test-HandoffClaimEventPath -Path $p -DispatchId $DispatchId) {
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
        $handoffBase = if ($script:DispatchWorktreeRoot) { $script:DispatchWorktreeRoot } else { $script:RepoRoot }
        $handoffDir = Join-Path $handoffBase 'ai_handoffs'
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

function Format-DispatchOrphanRecoveryComment {
    # ISSUE-231: build the GitHub issue comment text for an orphan-recovery
    # action. Pure helper: same inputs always return the same string; no I/O,
    # no git/gh calls. The four supported stages mirror the orphan-recovery
    # branches in Invoke-OrphanRecovery and are the only stages the queue
    # ever posts to a GitHub issue from that path. When the recovery
    # archived a worktree, the comment names the archive path and gives the
    # deterministic inspection/removal commands a human needs to recover the
    # preserved state. When no worktree was archived, the comment still
    # carries the original status text so existing operators are not
    # surprised by a missing line. Covered by Pester under
    # tools/dispatch-tests/**.
    [CmdletBinding()]
    [OutputType([string])]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('interrupted', 'already-published', 'interrupted-publish')]
        [string]$Stage,

        [Parameter(Mandatory = $true)]
        [ValidatePattern('^[A-Za-z0-9._-]+$')]
        [string]$DispatchId,

        [Parameter(Mandatory = $true)]
        [string]$Branch,

        [AllowEmptyString()]
        [string]$QueueLabel = '',

        [AllowEmptyString()]
        [string]$ArchivePath = '',

        [AllowEmptyString()]
        [string]$PreservedPath = '',

        [AllowEmptyString()]
        [string]$PublishedShortSha = ''
    )

    # Build the "where to inspect the surviving worktree" block. The block is
    # opt-in: it only appears when there is an actual path to point a human
    # at, so the original orphan-recovery messages stay intact for callers
    # that did not archive or preserve a worktree.
    $inspectBlock = ''
    if ($ArchivePath) {
        $inspectBlock = @"

The isolated worktree from the interrupted run was archived to ``$ArchivePath``. Inspect with ``git -C "$ArchivePath" status --short --branch`` (or ``git -C "$ArchivePath" log --oneline -5``) and remove it manually with ``git worktree remove "$ArchivePath"`` once you no longer need the preserved state.
"@
    } elseif ($PreservedPath) {
        $inspectBlock = @"

The isolated worktree from the interrupted run was preserved at ``$PreservedPath``. Inspect with ``git -C "$PreservedPath" status --short --branch`` (or ``git -C "$PreservedPath" log --oneline -5``) and remove it manually with ``git worktree remove "$PreservedPath"`` once you no longer need the preserved state.
"@
    }

    switch ($Stage) {
        'interrupted' {
            $label = if ($QueueLabel) { $QueueLabel } else { 'the queue' }
            return "An AI dispatch run for this issue was interrupted before it finished. The queue has reset it to ``$label`` and will pick it up again.$inspectBlock"
        }
        'already-published' {
            $shaDisplay = if ($PublishedShortSha) { " ($PublishedShortSha)" } else { '' }
            return "A prior AI dispatch run published this work$shaDisplay but was interrupted before cleanup; the queue has marked it done.$inspectBlock"
        }
        'interrupted-publish' {
            return "An AI dispatch run for this issue was interrupted between the local merge and the push to origin. The control-passed work is preserved on branch ``$Branch``; review and ``git push`` it by hand. Local main was reset to origin/main.$inspectBlock"
        }
    }
}

function Save-OrphanDispatchWorktree {
    # ISSUE-231: archive a leftover dispatch worktree and its branch under
    # `.interrupt<N>` so the next tick for the same issue can claim a fresh
    # worktree path/branch name without clobbering possibly-useful interrupted
    # state. Branches and worktree directories are archived in lockstep so a
    # single .interrupt<N> slot covers both. Returns a pscustomobject:
    #   Archived       (bool)   - true when anything was archived;
    #   ArchiveBranch  (string) - new branch name (.interrupt<N>) if branch existed;
    #   ArchivePath    (string) - new worktree path (.interrupt<N>) if worktree existed;
    #   WorktreeMoved  (bool)   - true when the worktree directory was archived
    #                             successfully; false on failure (path may still
    #                             survive at its original location) or absence.
    # The ArchivePath is the durable handle the orphan-recovery comment and the
    # dispatch audit log report to a human; callers must surface it whenever it
    # is non-empty. Best-effort: a rename/move failure warns but does not throw,
    # and the returned struct still names the archive slot that was attempted.
    param([string]$WorktreePath, [string]$Branch)
    $empty = [pscustomobject]@{
        Archived      = $false
        ArchiveBranch = ''
        ArchivePath   = ''
        WorktreeMoved = $false
    }
    if (-not $WorktreePath -or -not $Branch) { return $empty }
    $hasWt = (Test-Path -LiteralPath $WorktreePath)
    $hasBr = ((Invoke-Tool -Exe 'git' -CmdArgs @('branch', '--list', $Branch)).Text.Trim())
    if (-not $hasWt -and -not $hasBr) { return $empty }

    $n = 1
    while ((Invoke-Tool -Exe 'git' -CmdArgs @('branch', '--list', "$Branch.interrupt$n")).Text.Trim() -or
           (Test-Path -LiteralPath "$WorktreePath.interrupt$n")) { $n++ }
    $archBranch = "$Branch.interrupt$n"
    $archWt     = "$WorktreePath.interrupt$n"

    $branchArchived = $false
    if ($hasBr) {
        $rn = Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-m', $Branch, $archBranch)
        if ($rn.Code -eq 0) {
            Write-Output "  archived interrupted branch '$Branch' -> '$archBranch'."
            $branchArchived = $true
        } else {
            Write-Output "  WARNING: could not archive interrupted branch '$Branch' (exit $($rn.Code)): $($rn.Text)"
        }
    }
    $worktreeArchived = $false
    if ($hasWt) {
        $mv = Invoke-Tool -Exe 'git' -CmdArgs @('worktree', 'move', $WorktreePath, $archWt)
        if ($mv.Code -eq 0) {
            Write-Output "  archived interrupted worktree '$WorktreePath' -> '$archWt'."
            $worktreeArchived = $true
        } else {
            Write-Output "  WARNING: could not archive interrupted worktree '$WorktreePath' (exit $($mv.Code)): $($mv.Text)"
        }
    }
    return [pscustomobject]@{
        Archived      = ($branchArchived -or $worktreeArchived)
        ArchiveBranch = if ($branchArchived) { $archBranch } else { '' }
        ArchivePath   = if ($worktreeArchived) { $archWt } else { '' }
        WorktreeMoved = $worktreeArchived
    }
}

function Release-OrphanHandoffClaim {
    # Best-effort cleanup for queue-owned ADR-121 live claims left behind by
    # an interrupted queue process. Orphan recovery has already established
    # that no queue run holds the process lock, so a queue-owned live claim for
    # the same dispatch is stale even when its long TTL has not expired.
    param(
        [Parameter(Mandatory = $true)][string]$DispatchId,
        [Parameter(Mandatory = $true)][string]$Branch,
        [AllowEmptyString()][string]$EventRoot = '',
        [AllowEmptyString()][string]$Reason = ''
    )

    if ($SkipHandoffClaim) { return }
    if (-not $script:RepoRoot) { return }

    $claimDir = Join-Path (Join-Path $script:RepoRoot '.ai\handoff-claims') $DispatchId
    $claimPath = Join-Path $claimDir 'claim.json'
    if (-not (Test-Path -LiteralPath $claimPath)) { return }

    try {
        $record = Get-Content -Raw -LiteralPath $claimPath | ConvertFrom-Json
    } catch {
        Write-Output "  WARNING: could not parse ADR-121 claim for ${DispatchId}: $($_.Exception.Message)"
        return
    }

    $harness = [string]$record.harness
    if ($harness -ne 'Invoke-AiDispatchQueue.ps1') {
        Write-Output "  WARNING: leaving non-queue ADR-121 claim for ${DispatchId} intact (harness=$harness)."
        return
    }

    $actor = [string]$record.actor
    if ([string]::IsNullOrWhiteSpace($actor)) {
        Write-Output "  WARNING: leaving ADR-121 claim for ${DispatchId} intact because its actor is empty."
        return
    }

    $claimBranch = [string]$record.branch
    if ([string]::IsNullOrWhiteSpace($claimBranch)) { $claimBranch = $Branch }

    $eventRootForRelease = $script:RepoRoot
    if (-not [string]::IsNullOrWhiteSpace($EventRoot) -and (Test-Path -LiteralPath $EventRoot)) {
        $eventRootForRelease = $EventRoot
    }

    $reasonText = if ($Reason) { " ($Reason)" } else { '' }
    $claimHelper = if ($claimScript) { $claimScript } else { Join-Path $script:RepoRoot 'Invoke-HandoffClaim.ps1' }
    if (-not (Test-Path -LiteralPath $claimHelper)) {
        Write-Output "  WARNING: could not release ADR-121 claim for ${DispatchId}; helper missing: $claimHelper"
        return
    }

    $claimArgs = New-HandoffClaimArguments -ClaimScript $claimHelper -Action 'Release' `
        -DispatchId $DispatchId -Actor $actor -Harness 'Invoke-AiDispatchQueue.ps1' `
        -Branch $claimBranch -Root $eventRootForRelease -LiveRoot $script:RepoRoot `
        -TtlSeconds $HandoffClaimTtlSeconds
    $release = Invoke-Tool -Exe 'powershell.exe' -CmdArgs $claimArgs
    if ($release.Code -ne 0) {
        Write-Output "  WARNING: could not release ADR-121 claim for ${DispatchId}$reasonText (exit $($release.Code)): $($release.Text)"
        return
    }

    try {
        $result = $release.Text | ConvertFrom-Json
    } catch {
        Write-Output "  WARNING: ADR-121 claim release for ${DispatchId}$reasonText returned unparseable JSON: $($_.Exception.Message)"
        return
    }

    if ($result.status -eq 'RELEASED') {
        Write-Output "  released stale ADR-121 claim for ${DispatchId}$reasonText."
    } elseif ($result.status -eq 'AVAILABLE') {
        Write-Output "  ADR-121 claim for ${DispatchId} was already available$reasonText."
    } else {
        Write-Output "  WARNING: ADR-121 claim cleanup for ${DispatchId}$reasonText returned status=$($result.status)."
    }
}

function Get-QueueHandoffClaimOwnerPid {
    param([AllowNull()]$Record)
    if ($null -eq $Record) { return 0 }
    $actor = [string]$Record.actor
    if ($actor -match '^Invoke-AiDispatchQueue\.ps1:(\d+)$') {
        return [int]$matches[1]
    }
    return 0
}

function Test-QueueHandoffClaimOwnerLive {
    # Actor, not record.pid, is the durable queue owner. record.pid is the
    # short-lived Invoke-HandoffClaim.ps1 helper process and normally exits
    # immediately after writing the claim.
    param([AllowNull()]$Record)

    $ownerPid = Get-QueueHandoffClaimOwnerPid -Record $Record
    if ($ownerPid -le 0) {
        return [pscustomobject]@{
            Live  = $false
            Pid   = $ownerPid
            Reason = 'missing queue actor pid'
        }
    }

    $proc = Get-CimInstance Win32_Process -Filter "ProcessId = $ownerPid" -ErrorAction SilentlyContinue
    if (-not $proc) {
        return [pscustomobject]@{
            Live  = $false
            Pid   = $ownerPid
            Reason = "owner pid $ownerPid is not running"
        }
    }

    $cmd = [string]$proc.CommandLine
    if ($cmd -notmatch 'Invoke-AiDispatchQueue\.ps1') {
        return [pscustomobject]@{
            Live  = $false
            Pid   = $ownerPid
            Reason = "owner pid $ownerPid is not an Invoke-AiDispatchQueue.ps1 process"
        }
    }

    $claimStamp = $null
    try {
        $rawStamp = [string]$Record.timestamp
        if (-not [string]::IsNullOrWhiteSpace($rawStamp)) {
            $claimStamp = [DateTimeOffset]::Parse($rawStamp)
        }
    } catch {
        $claimStamp = $null
    }

    if ($claimStamp) {
        $created = $null
        try {
            if ($proc.CreationDate -is [datetime]) {
                $created = [DateTimeOffset]::new([datetime]$proc.CreationDate)
            } elseif ($proc.CreationDate) {
                $createdDate = [System.Management.ManagementDateTimeConverter]::ToDateTime([string]$proc.CreationDate)
                $created = [DateTimeOffset]::new($createdDate)
            }
        } catch {
            $created = $null
        }

        if ($created -and $created -gt $claimStamp.AddSeconds(5)) {
            return [pscustomobject]@{
                Live  = $false
                Pid   = $ownerPid
                Reason = "owner pid $ownerPid was recycled after the claim timestamp"
            }
        }
    }

    return [pscustomobject]@{
        Live  = $true
        Pid   = $ownerPid
        Reason = "owner queue process $ownerPid is live"
    }
}

function Invoke-StaleQueueHandoffClaimSweep {
    # Queue-start repair hook: after this process has acquired the single-run
    # queue lock, no other queue process should own ADR-121 claims. Sweep
    # queue-owned claim records whose actor process is dead or clearly not the
    # queue owner, so a long TTL cannot strand later ticks on BLOCKED claims.
    param([AllowEmptyString()][string]$Reason = 'queue-start stale-claim sweep')

    if ($SkipHandoffClaim) { return }
    if (-not $script:RepoRoot) { return }

    $claimRoot = Join-Path $script:RepoRoot '.ai\handoff-claims'
    if (-not (Test-Path -LiteralPath $claimRoot)) { return }

    $dirs = @(Get-ChildItem -LiteralPath $claimRoot -Directory -ErrorAction SilentlyContinue)
    if ($dirs.Count -eq 0) { return }

    foreach ($dir in $dirs) {
        $claimPath = Join-Path $dir.FullName 'claim.json'
        if (-not (Test-Path -LiteralPath $claimPath)) { continue }

        try {
            $record = Get-Content -Raw -LiteralPath $claimPath | ConvertFrom-Json
        } catch {
            Write-Output "  WARNING: stale-claim sweep could not parse '$claimPath': $($_.Exception.Message)"
            continue
        }

        if ([string]$record.harness -ne 'Invoke-AiDispatchQueue.ps1') { continue }

        $dispatchId = [string]$record.dispatch_id
        if ([string]::IsNullOrWhiteSpace($dispatchId)) { $dispatchId = [string]$dir.Name }
        if ($dispatchId -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
            Write-Output "  WARNING: stale-claim sweep skipping invalid dispatch id '$dispatchId'."
            continue
        }

        $owner = Test-QueueHandoffClaimOwnerLive -Record $record
        if ($owner.Live) { continue }

        $branch = [string]$record.branch
        if ([string]::IsNullOrWhiteSpace($branch)) { $branch = "ai-dispatch/$dispatchId" }
        Write-Output "Stale ADR-121 claim sweep: releasing $dispatchId ($($owner.Reason))."
        Release-OrphanHandoffClaim -DispatchId $dispatchId -Branch $branch -Reason $Reason
    }
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
                # ISSUE-231: if a worktree from the interrupted run is still
                # bound to the ahead-of-origin dispatch branch, archive both
                # (worktree + branch) under .interrupt<N> so the human can
                # inspect / push from the archived branch by hand. The
                # comment then carries the archive path as required by the
                # ISSUE-231 reporting contract; when no worktree existed,
                # ArchivePath stays empty and the comment falls back to its
                # legacy text.
                $aheadWtPath = Resolve-DispatchWorktreePath -RepoRoot $script:RepoRoot -DispatchId $aheadId
                $aheadBranch = "ai-dispatch/$aheadId"
                $aheadCommentBranch = $aheadBranch
                $aheadArchive = ''
                if (Test-Path -LiteralPath $aheadWtPath) {
                    # The branch was renamed by the ahead-of-origin reset above
                    # only via the `gh` edit (label changes only); the underlying
                    # local branch is still `ai-dispatch/$aheadId`. Save-OrphanDispatchWorktree
                    # archives BOTH branch and worktree atomically.
                    $saveResult = Save-OrphanDispatchWorktree -WorktreePath $aheadWtPath -Branch $aheadBranch
                    if ($saveResult.ArchivePath) { $aheadArchive = $saveResult.ArchivePath }
                    if ($saveResult.ArchiveBranch) { $aheadCommentBranch = $saveResult.ArchiveBranch }
                }
                Release-OrphanHandoffClaim -DispatchId $aheadId -Branch $aheadBranch `
                    -EventRoot $aheadArchive -Reason 'orphan recovery: interrupted publish'
                $aheadBody = Format-DispatchOrphanRecoveryComment `
                    -Stage 'interrupted-publish' `
                    -DispatchId $aheadId `
                    -Branch $aheadCommentBranch `
                    -ArchivePath $aheadArchive
                Invoke-Tool -Exe 'gh' -CmdArgs @('issue', 'comment', $aheadNum, '--repo', $RepoSlug,
                    '--body', $aheadBody) | Out-Null
                Write-Output "  issue #$aheadNum marked '$FailLabel'; its work is on branch $aheadCommentBranch."
                if ($aheadArchive) {
                    Write-Output "  isolated worktree archived to '$aheadArchive' (inspect with: git -C `"$aheadArchive`" status --short --branch)."
                }
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
            # ISSUE-231: when a worktree is still bound to the dispatch
            # branch, deleting the branch outright is impossible AND would
            # destroy the worktree's state. Archive both so a human can
            # inspect later; otherwise (no worktree) keep the legacy
            # `branch -D` so a stale local branch is cleaned up.
            $owtPath = Resolve-DispatchWorktreePath -RepoRoot $script:RepoRoot -DispatchId $oid
            $archivePath = ''
            if (Test-Path -LiteralPath $owtPath) {
                $saveResult = Save-OrphanDispatchWorktree -WorktreePath $owtPath -Branch $obranch
                if ($saveResult.ArchivePath) { $archivePath = $saveResult.ArchivePath }
            } elseif ((Invoke-Tool -Exe 'git' -CmdArgs @('branch', '--list', $obranch)).Text.Trim()) {
                Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-D', $obranch) | Out-Null
            }
            Release-OrphanHandoffClaim -DispatchId $oid -Branch $obranch `
                -EventRoot $archivePath -Reason 'orphan recovery: already published'
            Invoke-Tool -Exe 'gh' -CmdArgs @(
                'issue', 'edit', "$($o.number)", '--repo', $RepoSlug,
                '--remove-label', $RunLabel, '--remove-label', $QueueLabel,
                '--add-label', $DoneLabel) | Out-Null
            $publishedBody = Format-DispatchOrphanRecoveryComment `
                -Stage 'already-published' `
                -DispatchId $oid `
                -Branch $obranch `
                -PublishedShortSha $short `
                -ArchivePath $archivePath
            Invoke-Tool -Exe 'gh' -CmdArgs @(
                'issue', 'close', "$($o.number)", '--repo', $RepoSlug,
                '--comment', $publishedBody) | Out-Null
            if ($archivePath) {
                Write-Output "  isolated worktree archived to '$archivePath' (inspect with: git -C `"$archivePath`" status --short --branch)."
            }
            continue
        }

        # Not on origin/main -- genuinely interrupted; reset for a fresh run.
        # ISSUE-231: if a worktree is bound to this dispatch's branch the
        # interrupted state lives inside that worktree. Archive both (worktree
        # + branch) under .interrupt<N> so the retry tick can claim a fresh
        # path AND the human can still inspect / recover the partial work.
        # When no worktree exists (legacy interrupted run or no-worktree-
        # ever-created edge case) fall back to the destructive `branch -D`
        # because the partial work would be lost regardless.
        $owtPath = Resolve-DispatchWorktreePath -RepoRoot $script:RepoRoot -DispatchId $oid
        $archivePath = ''
        if (Test-Path -LiteralPath $owtPath) {
            $saveResult = Save-OrphanDispatchWorktree -WorktreePath $owtPath -Branch $obranch
            if ($saveResult.ArchivePath) { $archivePath = $saveResult.ArchivePath }
        } elseif ((Invoke-Tool -Exe 'git' -CmdArgs @('branch', '--list', $obranch)).Text.Trim()) {
            Write-Output "  deleting interrupted branch $obranch."
            Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-D', $obranch) | Out-Null
        }
        Release-OrphanHandoffClaim -DispatchId $oid -Branch $obranch `
            -EventRoot $archivePath -Reason 'orphan recovery: interrupted run'
        # Archive the interrupted run's primary-side scratch dir if one is
        # still there. With ISSUE-231 worktree isolation the run dir lives
        # inside the worktree (covered by Save-OrphanDispatchWorktree above),
        # so this only catches pre-ISSUE-231 leftovers.
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
            $interruptedBody = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId $oid `
                -Branch $obranch `
                -QueueLabel $QueueLabel `
                -ArchivePath $archivePath
            Invoke-Tool -Exe 'gh' -CmdArgs @(
                'issue', 'comment', "$($o.number)", '--repo', $RepoSlug,
                '--body', $interruptedBody) | Out-Null
            if ($archivePath) {
                Write-Output "  isolated worktree archived to '$archivePath' (inspect with: git -C `"$archivePath`" status --short --branch)."
            }
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
    # Plan-gate exhaustion / block (Invoke-AiDispatchLoop.ps1:1748,1753). Placed
    # AFTER stall/timeout so a Codex stall *during* plan-fill still classifies as
    # stall; the pure "did not approve the plan" / "blocked the plan" wording has
    # no stall/timeout/verification/control keywords so it would otherwise fall to
    # 'unknown' (non-recoverable). This is a flaky stochastic gate (Rule-8 NACK),
    # so it joins the bounded-recoverable set in Invoke-AiDispatchAuto.ps1.
    if ($text -match '(?i)did not approve the plan within maxplanrevisions' -or
        $text -match '(?i)blocked the plan\b') {
        return @('ai-dispatch-failure-plan-gate')
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

function Test-PendingIssueSuperseded {
    # PURE: a pending dispatch issue is SUPERSEDED when a newer ai-auto issue exists
    # (a higher number, any state). The self-rearm loop files one task at a time, so a
    # pending issue that is not the newest carries a STALE body (the brief was amended /
    # a fresh task filed after it was queued). Mirrors Get-RecoveryDecision's
    # supersession guard for the selection path. MaxAutoIssueNumber 0 => unknown =>
    # not superseded (the published-SHA guard remains the backstop).
    param(
        [Parameter(Mandatory)][int]$IssueNumber,
        [int]$MaxAutoIssueNumber = 0
    )
    return [bool]($MaxAutoIssueNumber -gt $IssueNumber)
}

function Get-StaleReplayPublishedShaArgs {
    # PURE: build the `git log` args that detect THIS dispatch's own already-published
    # commit on origin/main ("ai-dispatch ISSUE-N:"). Time-floored at the issue's
    # creation: a genuine publish of ISSUE-N is necessarily NEWER than ISSUE-N, so the
    # floor never drops a real match -- but WITHOUT it the grep also matches MIGRATED
    # old-repo commits that reuse the same "ai-dispatch ISSUE-N:" subject. After a repo
    # migration the new repo restarts issue numbering, so low new issue numbers collide
    # with imported history (e.g. an ancient "ai-dispatch ISSUE-4:" commit), producing a
    # false "already published" that wrongly skips the dispatch. The `--since` floor
    # prevents that collision. When CreatedAt is empty (defensive), fall back to the
    # unscoped grep (legacy behavior).
    param(
        [Parameter(Mandatory)][string]$IssueId,
        [AllowEmptyString()][AllowNull()][string]$CreatedAt
    )
    $a = @('log', 'origin/main', '-n', '1', '--fixed-strings',
        "--grep=ai-dispatch ${IssueId}:", '--format=%H')
    if (-not [string]::IsNullOrWhiteSpace($CreatedAt)) {
        $a += "--since=$CreatedAt"
    }
    return , $a
}

function Get-DispatchSurfaceRouting {
    # PURE classifier for surface-split publishing (default-OFF autonomy feature).
    # Given the changed paths of a dispatch, decide whether the change is low-risk
    # enough to AUTO-MERGE to main, or must open a PR for a human merge.
    # FAIL-CLOSED: a change auto-merges ONLY when it is non-empty AND every path
    # matches the explicit low-risk allowlist; anything else routes to a PR --
    # product source (crates/**), the automation scripts (*.ps1 incl. the verify
    # gate), .github/**, Cargo.*, schemas, or any unrecognized path. No side
    # effects, so it is unit-testable from the queue's skip-main seam.
    #
    # Low-risk allowlist (auto-merge):
    #   *.md                       docs / brief / runbook / handoff markdown
    #   *.Tests.ps1                PowerShell test files
    #   tools/dispatch-tests/**    dispatch test suite
    #   ai_handoffs/**             generated handoff packets
    #   ai_dispatch_logs/**        generated dispatch logs
    # NOTE: the dispatch automation *.ps1 and the verify gate are deliberately
    # HIGH-risk (human-merged) even though "scripts" reads as low-risk -- a loop
    # that auto-merges its own controller is high blast-radius. Broaden the
    # allowlist here if that posture is ever relaxed.
    param(
        [string[]]$ChangedPaths,
        # Default $false = STRICT: the dispatch brief is a control surface, so its mere
        # presence forces a human-merged PR (a change that can re-arm the loop is never
        # auto-merged). $true relaxes this so a brief re-arm may ride along with
        # genuine low-risk work to main (operator opt-in via -AllowBriefRideAlong).
        [bool]$AllowBriefRideAlong = $false
    )
    $paths = @($ChangedPaths |
        Where-Object { $_ -and $_.ToString().Trim() } |
        ForEach-Object { ($_ -replace '\\', '/').Trim() })
    $result = [pscustomobject]@{
        Routing       = 'pr'
        HighRiskPaths = @()
        Reason        = ''
    }
    if ($paths.Count -eq 0) {
        $result.Reason = 'no changed paths; nothing to auto-merge'
        return $result
    }
    # The dispatch brief is the loop's CONTROL surface: a change to it can re-arm the
    # loop (author the next task), so it is never a low-risk green light on its own.
    # STRICT (default): any brief change is high-risk -> PR. RIDE-ALONG (opt-in): the
    # brief is excluded from the low/high decision, so a re-arm riding along with
    # genuine low-risk work auto-merges, while a brief-only changeset still -> PR.
    $isControl = {
        param($p)
        ($p -like '.ai/dispatch.tasks.md') -or
        ($p -like '.ai/dispatch.tasks.archive.md')
    }
    $isLowRisk = {
        param($p)
        ($p -like '*.md') -or
        ($p -like '*.Tests.ps1') -or
        ($p -like 'tools/dispatch-tests/*') -or
        ($p -like 'ai_handoffs/*') -or
        ($p -like 'ai_dispatch_logs/*')
    }
    $control = @($paths | Where-Object { & $isControl $_ })
    $rest    = @($paths | Where-Object { -not (& $isControl $_) })
    $high    = @($rest | Where-Object { -not (& $isLowRisk $_) })
    if (-not $AllowBriefRideAlong) {
        # Strict: the brief's presence alone is a high-risk control-surface change.
        $high = @($high + $control)
    }
    if ($high.Count -gt 0) {
        $result.HighRiskPaths = $high
        $result.Reason = "$($high.Count) high-risk path(s) require a human-merged PR: " + (($high | Select-Object -First 5) -join ', ')
    } elseif ($rest.Count -eq 0) {
        # Only the control brief changed -> a re-arm with no substantive work; the
        # loop's own controller does not auto-merge itself.
        $result.Reason = 'only the dispatch brief (control surface) changed; routing to a human-merged PR'
    } else {
        $result.Routing = 'main'
        $riderNote = if ($control.Count -gt 0) { ' plus the brief re-arm' } else { '' }
        $result.Reason  = "all $($rest.Count) substantive change(s) are low-risk (docs/tests/artifacts)$riderNote; eligible for auto-merge"
    }
    return $result
}

function Test-DiffSizeWithinCap {
    # PURE diff-size cap for auto-merge. A cap of 0 means "unlimited" (disabled).
    # Within = (MaxFiles == 0 OR FilesChanged <= MaxFiles) AND
    #          (MaxLines == 0 OR LinesChanged <= MaxLines).
    # Used FAIL-CLOSED: a main-routed publish whose diff exceeds the cap is
    # downgraded to a human-merged PR (a large change always gets human eyes).
    param(
        [int]$FilesChanged,
        [int]$LinesChanged,
        [int]$MaxFiles = 0,
        [int]$MaxLines = 0
    )
    $result = [pscustomobject]@{ Within = $true; Reason = '' }
    if ($MaxFiles -gt 0 -and $FilesChanged -gt $MaxFiles) {
        $result.Within = $false
        $result.Reason = "files changed $FilesChanged > cap $MaxFiles"
        return $result
    }
    if ($MaxLines -gt 0 -and $LinesChanged -gt $MaxLines) {
        $result.Within = $false
        $result.Reason = "lines changed $LinesChanged > cap $MaxLines"
        return $result
    }
    $result.Reason = "within cap (files=$FilesChanged/$MaxFiles, lines=$LinesChanged/$MaxLines)"
    return $result
}

function Get-DispatchTerminalLabelPlan {
    # Build the deterministic terminal label add/remove plan for the queue's
    # final `gh issue edit` mutation. Consumes already-computed queue state;
    # makes no decisions about issue selection, retry eligibility, taxonomy
    # classification, publish behavior, branch archival, or scheduler/auto
    # behavior. Three terminal states are reconciled:
    #
    #   * Terminal success (RunFailed=$false, WillRetry=$false): adds the
    #     done label and removes queue/running/retry/failed and every known
    #     failure-taxonomy label, so a passing run cannot inherit stale
    #     failure markers from an earlier attempt.
    #   * Terminal failure (RunFailed=$true, WillRetry=$false): adds done,
    #     failed, and the caller-selected taxonomy labels; removes
    #     queue/running/retry and every non-selected failure-taxonomy label,
    #     so an issue cannot carry contradictory failure classifications.
    #   * Retry (WillRetry=$true): keeps or adds the queue and retry labels,
    #     removes running/done/failed and every failure-taxonomy label,
    #     since taxonomy labels describe a terminal failure outcome, not
    #     queued retry state.
    #
    # Returns [pscustomobject]@{ Add = @(...); Remove = @(...) } with each
    # list de-duplicated and ordered by first occurrence. Pure and
    # side-effect-free: covered by Pester under tools/dispatch-tests/**.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][bool]$WillRetry,
        [Parameter(Mandatory = $true)][bool]$RunFailed,
        [Parameter(Mandatory = $true)][string]$QueueLabel,
        [Parameter(Mandatory = $true)][string]$RunLabel,
        [Parameter(Mandatory = $true)][string]$DoneLabel,
        [Parameter(Mandatory = $true)][string]$FailLabel,
        [Parameter(Mandatory = $true)][string]$RetryLabel,
        [AllowNull()][AllowEmptyCollection()][string[]]$TaxonomyLabels,
        [AllowNull()][AllowEmptyCollection()][string[]]$KnownFailureTaxonomyLabels
    )

    if ($null -eq $TaxonomyLabels)             { $TaxonomyLabels = @() }
    if ($null -eq $KnownFailureTaxonomyLabels) { $KnownFailureTaxonomyLabels = @() }

    $add    = New-Object System.Collections.Generic.List[string]
    $remove = New-Object System.Collections.Generic.List[string]

    if ($WillRetry) {
        $add.Add($QueueLabel)
        $add.Add($RetryLabel)
        $remove.Add($RunLabel)
        $remove.Add($DoneLabel)
        $remove.Add($FailLabel)
        foreach ($t in $KnownFailureTaxonomyLabels) { $remove.Add($t) }
    } elseif ($RunFailed) {
        $add.Add($DoneLabel)
        $add.Add($FailLabel)
        foreach ($t in $TaxonomyLabels) { $add.Add($t) }
        $remove.Add($QueueLabel)
        $remove.Add($RunLabel)
        $remove.Add($RetryLabel)
        $selected = @{}
        foreach ($t in $TaxonomyLabels) { $selected[$t] = $true }
        foreach ($t in $KnownFailureTaxonomyLabels) {
            if (-not $selected.ContainsKey($t)) { $remove.Add($t) }
        }
    } else {
        $add.Add($DoneLabel)
        $remove.Add($QueueLabel)
        $remove.Add($RunLabel)
        $remove.Add($RetryLabel)
        $remove.Add($FailLabel)
        foreach ($t in $KnownFailureTaxonomyLabels) { $remove.Add($t) }
    }

    return [pscustomobject]@{
        Add    = @($add    | Select-Object -Unique)
        Remove = @($remove | Select-Object -Unique)
    }
}

# --- Worktree-isolation helpers --------------------------------------------
# ISSUE-231: a queue dispatch runs the inner Codex/Claude loop, scope guard,
# audit log, staging, and commit inside an isolated git worktree, while the
# primary checkout stays on `main`. The worktree convention follows the
# sibling shape documented in `AI_DISPATCH_PARALLEL.md`:
# `<parent-of-repo>/dispatch-worktrees/<DispatchId>`. The helpers below are
# small pure functions covered by Pester under `tools/dispatch-tests/**` so
# the path computation, cleanup decision, and status formatting can be
# exercised without any live git/gh/codex/claude calls.

function Resolve-DispatchWorktreePath {
    # Compute the deterministic isolated-worktree path for a single dispatch.
    # The path always sits in a `dispatch-worktrees` sibling of the primary
    # repo so it is outside the primary working tree (so `git status` on the
    # primary cannot see the dispatch's edits) and so multiple dispatches
    # share a single, scannable parent. Pure: no I/O, no git calls.
    [CmdletBinding()]
    [OutputType([string])]
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoRoot,

        [Parameter(Mandatory = $true)]
        [ValidatePattern('^[A-Za-z0-9._-]+$')]
        [string]$DispatchId
    )
    if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
        throw "Resolve-DispatchWorktreePath: -RepoRoot must be a non-empty path."
    }
    $trimmed = $RepoRoot.TrimEnd('\', '/')
    $parent  = [System.IO.Path]::GetDirectoryName($trimmed)
    if ([string]::IsNullOrWhiteSpace($parent)) {
        throw "Resolve-DispatchWorktreePath: cannot resolve parent directory of '$RepoRoot'."
    }
    # Use Path.Combine rather than Join-Path: this helper is pure and its
    # tests intentionally pass synthetic Windows drive paths. Join-Path
    # consults the PowerShell provider and fails when that drive is absent
    # on CI (for example `A:` on GitHub-hosted runners).
    return [System.IO.Path]::Combine($parent, 'dispatch-worktrees', $DispatchId)
}

function Copy-DispatchRunDirToPrimary {
    # ISSUE-231 left the loop's run dir (.ai/dispatch-<id>/) INSIDE the isolated
    # worktree, and that tree is gitignored so it is never committed. On a
    # successful run the queue then `git worktree remove`s the worktree, taking
    # the run dir's control verdict / plan-gate / execute / verification
    # artifacts with it -- so Get-AiDispatchHealth.ps1 (which reads only the
    # PRIMARY checkout's .ai/dispatch-*/) goes blind. Mirror the run dir out to
    # the primary .ai/ BEFORE removal so the health/trends readers keep seeing
    # the run. Best-effort by contract: any failure warns and returns; it must
    # never throw or change the dispatch exit code (a copy-out failure must not
    # turn a published success into a queue failure). Filesystem copy only, no
    # git/gh/network. Idempotent: re-copying overwrites the primary mirror in
    # place. The trace-jsonl copy is defensive -- today the queue writes its
    # trace jsonl straight to the primary .ai/dispatch-trace/, so a worktree-
    # local trace dir normally does not exist; copying it anyway keeps this
    # correct if a future loop change starts emitting worktree-local traces.
    param(
        [string]$WorktreeRoot,
        [string]$PrimaryRoot,
        [string]$DispatchId
    )
    if (-not $WorktreeRoot -or -not $PrimaryRoot -or -not $DispatchId) { return }
    try {
        $srcAi = Join-Path $WorktreeRoot '.ai'
        # Resolve to full paths so a worktree that happens to BE the primary
        # (defensive: should never occur for an isolated dispatch worktree)
        # short-circuits rather than copying a directory onto itself.
        $srcFull = [System.IO.Path]::GetFullPath($WorktreeRoot)
        $dstFull = [System.IO.Path]::GetFullPath($PrimaryRoot)
        if ($srcFull.TrimEnd('\','/') -ieq $dstFull.TrimEnd('\','/')) { return }
        if (-not (Test-Path -LiteralPath $srcAi)) { return }

        $primaryAi = Join-Path $PrimaryRoot '.ai'
        if (-not (Test-Path -LiteralPath $primaryAi)) {
            New-Item -ItemType Directory -Path $primaryAi -Force | Out-Null
        }

        # 1) The run dir: <worktree>/.ai/dispatch-<id>/ -> <primary>/.ai/dispatch-<id>/
        $srcRun = Join-Path $srcAi ("dispatch-{0}" -f $DispatchId)
        if (Test-Path -LiteralPath $srcRun) {
            $dstRun = Join-Path $primaryAi ("dispatch-{0}" -f $DispatchId)
            if (Test-Path -LiteralPath $dstRun) {
                Remove-Item -LiteralPath $dstRun -Recurse -Force -ErrorAction SilentlyContinue
            }
            Copy-Item -LiteralPath $srcRun -Destination $dstRun -Recurse -Force -ErrorAction Stop
            Write-Output "  copied dispatch run evidence -> $(Get-RepoRelativePathForQueue $dstRun)"
        }

        # 2) Defensive: any worktree-local trace jsonl -> primary dispatch-trace/
        $srcTrace = Join-Path $srcAi 'dispatch-trace'
        if (Test-Path -LiteralPath $srcTrace) {
            $dstTrace = Join-Path $primaryAi 'dispatch-trace'
            if (-not (Test-Path -LiteralPath $dstTrace)) {
                New-Item -ItemType Directory -Path $dstTrace -Force | Out-Null
            }
            foreach ($f in @(Get-ChildItem -LiteralPath $srcTrace -File -Filter '*.jsonl' -ErrorAction SilentlyContinue)) {
                Copy-Item -LiteralPath $f.FullName -Destination (Join-Path $dstTrace $f.Name) -Force -ErrorAction SilentlyContinue
            }
        }
    } catch {
        Write-Output "  WARNING: could not copy dispatch run evidence out of worktree before removal: $($_.Exception.Message)"
    }
}

function Test-DispatchWorktreeCleanupDecision {
    # Decide whether to remove, archive, or preserve the isolated worktree
    # after the dispatch run reaches a terminal state. The decision is layered
    # so that the most specific safety reason wins:
    #   * PublishHardFailed -> preserve (branch + worktree carry the only
    #     copy of control-passed work the queue could not publish);
    #   * RunBlocked        -> preserve (executor reported EXEC_STATUS=blocked,
    #     a designed halt that needs a human, not a retry);
    #   * WillRetry         -> archive (next attempt needs a fresh worktree
    #     path; archiving keeps the failed attempt's state intact);
    #   * RunFailed         -> preserve (terminal failure: human inspection
    #     against the original path is the most useful recovery surface);
    #   * default           -> remove (terminal success; branch ref keeps the
    #     commit, the worktree's scratch state is no longer needed).
    # Pure and side-effect-free: covered by Pester under
    # tools/dispatch-tests/**.
    [CmdletBinding()]
    [OutputType([pscustomobject])]
    param(
        [Parameter(Mandatory = $true)][bool]$RunFailed,
        [Parameter(Mandatory = $true)][bool]$RunBlocked,
        [Parameter(Mandatory = $true)][bool]$WillRetry,
        [Parameter(Mandatory = $true)][bool]$PublishHardFailed
    )

    if ($PublishHardFailed) {
        return [pscustomobject]@{
            Action = 'preserve'
            Reason = 'publish pipeline failed; branch and worktree kept for human recovery.'
        }
    }
    if ($RunBlocked) {
        return [pscustomobject]@{
            Action = 'preserve'
            Reason = 'executor reported EXEC_STATUS: blocked (designed halt); worktree kept for human review.'
        }
    }
    if ($WillRetry) {
        return [pscustomobject]@{
            Action = 'archive'
            Reason = 'run is retry-eligible; archive the failed worktree under .attemptN so the next attempt can claim a fresh path.'
        }
    }
    if ($RunFailed) {
        return [pscustomobject]@{
            Action = 'preserve'
            Reason = 'terminal failure; worktree kept for human inspection and recovery.'
        }
    }
    return [pscustomobject]@{
        Action = 'remove'
        Reason = 'terminal success; branch commit preserved on the dispatch branch and worktree is no longer needed.'
    }
}

function Format-DispatchWorktreeStatus {
    # Format a deterministic one-paragraph worktree-status report for use in
    # the result comment, the dispatch audit log, and local stdout. The
    # output describes the disposition of the isolated worktree and gives a
    # human enough information to inspect, recover, or remove it manually.
    # Pure: same inputs always return the same string; no I/O.
    [CmdletBinding()]
    [OutputType([string])]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('preserved', 'removed', 'archived')]
        [string]$Disposition,

        [Parameter(Mandatory = $true)]
        [string]$WorktreePath,

        [AllowEmptyString()]
        [string]$ArchivePath = '',

        [AllowEmptyString()]
        [string]$Reason = ''
    )

    if ([string]::IsNullOrWhiteSpace($WorktreePath)) {
        throw "Format-DispatchWorktreeStatus: -WorktreePath must be a non-empty path."
    }
    if ($Disposition -eq 'archived' -and [string]::IsNullOrWhiteSpace($ArchivePath)) {
        throw "Format-DispatchWorktreeStatus: -ArchivePath is required when -Disposition is 'archived'."
    }

    $line = switch ($Disposition) {
        'preserved' {
            "Isolated worktree preserved at ``$WorktreePath``. Inspect with ``git -C `"$WorktreePath`" status --short --branch`` and ``git -C `"$WorktreePath`" log --oneline -5``; remove it manually with ``git worktree remove `"$WorktreePath`"`` once you are done."
        }
        'removed' {
            "Isolated worktree at ``$WorktreePath`` was removed after the dispatch branch commit was preserved and the selected publish action completed."
        }
        'archived' {
            "Isolated worktree archived from ``$WorktreePath`` to ``$ArchivePath`` so the retry can claim a fresh path. Inspect with ``git -C `"$ArchivePath`" status --short --branch`` and ``git -C `"$ArchivePath`" log --oneline -5``; remove it manually with ``git worktree remove `"$ArchivePath`"``."
        }
    }
    if ($Reason) { $line += " ($Reason)" }
    return $line
}

function Format-DispatchWorktreeAuditSection {
    # ISSUE-231: build the "Isolated Worktree" markdown section that
    # `Write-DispatchLog` embeds into the committed dispatch audit log when
    # the run executed inside an isolated worktree. The section is the
    # durable on-branch record of where the run lived and how a human can
    # inspect, recover, or remove the worktree afterwards. Includes:
    #   * the worktree path (the deterministic sibling
    #     `<parent>/dispatch-worktrees/<DispatchId>`),
    #   * the inspection commands a human runs to look at the surviving
    #     state (`git -C <path> status --short --branch`, `git -C <path>
    #     log --oneline -5`),
    #   * the removal command a human runs once they are done
    #     (`git worktree remove <path>`).
    # Pure helper: same inputs always return the same markdown; no I/O.
    # Covered by Pester under tools/dispatch-tests/**.
    [CmdletBinding()]
    [OutputType([string])]
    param(
        [Parameter(Mandatory = $true)]
        [string]$WorktreePath
    )

    if ([string]::IsNullOrWhiteSpace($WorktreePath)) {
        throw "Format-DispatchWorktreeAuditSection: -WorktreePath must be a non-empty path."
    }

    return @"
## Isolated Worktree

- Worktree path: ``$WorktreePath``
- Inspect: ``git -C "$WorktreePath" status --short --branch``
- Recent history: ``git -C "$WorktreePath" log --oneline -5``
- Remove when done: ``git worktree remove "$WorktreePath"``

This dispatch ran the inner loop, scope guard, audit log, staging, and commit inside this isolated worktree (the primary checkout stayed on ``main``). On terminal success the queue removes the worktree after the publish action completes; on failure, blocked execution, publish-pipeline failure, or interruption the worktree is preserved or archived (``.attempt<N>`` for retry-eligible failures, ``.interrupt<N>`` for orphan-recovery archives) and the final issue comment carries the surviving path.
"@
}

# --- Publish-mode normalization and PR text helpers ------------------------
# ISSUE-239: the queue's mechanical default is `pr` -- a no-flag run pushes the
# dispatch branch and opens a GitHub pull request targeting main for human
# review rather than silently fast-forwarding origin/main. `main` remains
# available as an explicit opt-in (the delegated-human auto-publish posture
# from §18 of AI_DISPATCH_AUTOMATION.md), and `branch` keeps the branch local.
# Legacy `-NoPublish` is preserved as a branch-only alias.
#
# Resolve-DispatchPublishMode collapses the `-PublishMode` plus `-NoPublish`
# inputs into one internal mode string before the queue's progress comments,
# publish decisions, result comments, and terminal labels are computed. It is
# pure: it does not read or write files, call gh / git / codex / claude / the
# queue / the network, or look at any environment outside its arguments.
# Format-DispatchPrTitle and Format-DispatchPrBody are deterministic string
# formatters for the PR title and body that PR mode hands to `gh pr create`;
# they are equally pure. The helpers are covered by Pester under
# tools/dispatch-tests/**.

function Resolve-DispatchPublishMode {
    # Combine `-PublishMode <main|branch|pr>` and `-NoPublish` into one
    # internal mode string. The rules:
    #   * No -PublishMode and no -NoPublish        -> 'pr' (default).
    #   * -NoPublish alone                          -> 'branch'.
    #   * -PublishMode main|branch|pr               -> that mode.
    #   * -NoPublish + -PublishMode branch          -> 'branch' (compatible).
    #   * -NoPublish + -PublishMode main|pr         -> throws (conflict).
    # An invalid -PublishMode value also throws. The helper is pure: no I/O,
    # no external commands, no environment lookups -- callers are responsible
    # for passing the exact parameter values they observed.
    [CmdletBinding()]
    param(
        [AllowEmptyString()]
        [string]$PublishMode = '',

        [bool]$NoPublish = $false
    )

    $valid = @('', 'main', 'branch', 'pr')
    if ($valid -notcontains $PublishMode) {
        throw "Resolve-DispatchPublishMode: invalid -PublishMode value '$PublishMode'. Allowed: main, branch, pr."
    }

    if ($NoPublish) {
        if ($PublishMode -eq '' -or $PublishMode -eq 'branch') {
            return 'branch'
        }
        throw ("Resolve-DispatchPublishMode: -NoPublish is incompatible with " +
            "-PublishMode '$PublishMode'. -NoPublish means branch-only mode; " +
            "either drop -NoPublish or pass -PublishMode branch explicitly.")
    }

    if ($PublishMode -eq '') { return 'pr' }
    return $PublishMode
}

function Format-DispatchPrTitle {
    # Build the deterministic PR title for PR mode. Short and informative,
    # leading with the dispatch id so a glance at the PR list shows what
    # produced it. Pure: same inputs always return the same string.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidatePattern('^[A-Za-z0-9._-]+$')]
        [string]$DispatchId,

        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string]$IssueTitle
    )
    $title = if ($IssueTitle -and $IssueTitle.Trim()) { $IssueTitle.Trim() } else { '(no title)' }
    return "ai-dispatch ${DispatchId}: $title"
}

function Format-DispatchPrBody {
    # Build the deterministic PR body for PR mode. The body links to the
    # source issue with `Refs #<n>` (never `Closes #<n>` -- PR mode leaves
    # the issue open for a human to close), and includes the dispatch id,
    # branch, commit SHA, detailed log path, and Codex control verdict so a
    # reviewer can audit the run without leaving the PR page. Pure: same
    # inputs always return the same string.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [int]$IssueNumber,

        [Parameter(Mandatory = $true)]
        [AllowEmptyString()]
        [string]$IssueTitle,

        [Parameter(Mandatory = $true)]
        [ValidatePattern('^[A-Za-z0-9._-]+$')]
        [string]$DispatchId,

        [Parameter(Mandatory = $true)]
        [string]$Branch,

        [Parameter(Mandatory = $true)]
        [string]$CommitSha,

        [Parameter(Mandatory = $true)]
        [string]$DispatchLogPath,

        [Parameter(Mandatory = $true)]
        [string]$Verdict
    )

    $titleDisplay = if ($IssueTitle -and $IssueTitle.Trim()) { $IssueTitle.Trim() } else { '(no title)' }

    return @"
**AI dispatch pull request** - ``$DispatchId``

- Source issue: #$IssueNumber - $titleDisplay
- Dispatch id: ``$DispatchId``
- Branch: ``$Branch``
- Commit SHA: ``$CommitSha``
- Detailed log: ``$DispatchLogPath``
- Codex control verdict: ``$Verdict``

Refs #$IssueNumber

_Posted by Invoke-AiDispatchQueue.ps1 in PR publish mode. PR mode pushes the dispatch branch and opens this pull request, but does not merge, push ``origin/main``, or close the source issue. A human reviews and merges this PR._
"@
}

function Resolve-DispatchPrViewMetadata {
    # Pure parser for `gh pr view --json number,url` output. PR-mode success
    # requires BOTH a PR number and a PR URL; missing, unparseable, or partial
    # metadata is publish-pipeline failure so the queue cannot claim PR
    # success without the metadata the final issue comment needs.
    #
    # Returns a pscustomobject with:
    #   Success       (bool)   - true only when ExitCode=0, JSON parses, and
    #                            both number and url are populated.
    #   PrNumber      (int)    - parsed PR number, 0 on failure.
    #   PrUrl         (string) - parsed PR URL, '' on failure.
    #   FailureReason (string) - human-readable explanation when Success=false;
    #                            empty string when Success=true.
    #
    # Pure: no file I/O, no gh/git/network calls, deterministic for the same
    # inputs.
    [CmdletBinding()]
    [OutputType([pscustomobject])]
    param(
        [Parameter(Mandatory = $true)]
        [int]$ExitCode,

        [AllowNull()]
        [AllowEmptyString()]
        [string]$Text
    )

    $result = [pscustomobject]@{
        Success       = $false
        PrNumber      = 0
        PrUrl         = ''
        FailureReason = ''
    }

    if ($ExitCode -ne 0) {
        $snippet = if ($Text) { $Text.Trim() } else { '' }
        $result.FailureReason = "gh pr view --json number,url failed (exit $ExitCode): $snippet"
        return $result
    }

    if ([string]::IsNullOrWhiteSpace($Text)) {
        $result.FailureReason = "gh pr view --json number,url returned empty output"
        return $result
    }

    $info = $null
    try {
        $info = $Text | ConvertFrom-Json
    } catch {
        $result.FailureReason = "gh pr view --json number,url returned unparseable JSON: $($_.Exception.Message)"
        return $result
    }

    if ($null -eq $info) {
        $result.FailureReason = "gh pr view --json number,url returned a null payload"
        return $result
    }

    $parsedNumber = 0
    if ($null -ne $info.number) {
        try { $parsedNumber = [int]$info.number } catch { $parsedNumber = 0 }
    }
    $parsedUrl = if ($null -ne $info.url) { [string]$info.url } else { '' }

    if ($parsedNumber -le 0 -or [string]::IsNullOrWhiteSpace($parsedUrl)) {
        $result.FailureReason = "gh pr view --json number,url returned incomplete PR metadata (number='$parsedNumber', url='$parsedUrl')"
        return $result
    }

    $result.PrNumber = $parsedNumber
    $result.PrUrl    = $parsedUrl
    $result.Success  = $true
    return $result
}

# --- Mid-run progress-comment helpers --------------------------------------
# ISSUE-229: post a small, deterministic progress comment to the GitHub issue
# at the four major queue/orchestrator stage boundaries (issue claimed, inner
# loop starting, inner loop finished, publish decision) so a human watching
# the issue thread can see where an active dispatch is without reading
# local run-dir logs. Progress comments are quality-of-life observability
# only:
#
#   * The existing final result comment, terminal label reconciliation,
#     publish semantics, retry semantics, and failure taxonomy remain
#     authoritative and unchanged.
#   * Progress-comment failures are best-effort: a gh failure emits a
#     `WARNING:` line and continues the dispatch -- it never fails, retries,
#     relabels, publishes, or otherwise alters the run outcome.
#   * Comments stay short. They identify the issue, dispatch id, branch, and
#     stable local log/audit identifiers where available, and they never
#     include full logs, loop-output tails, model transcripts, diffs, or
#     control JSON. The final result comment remains the only comment that
#     carries the loop-output tail.
#
# Format-DispatchProgressComment is pure and side-effect-free. It never reads
# or writes files, calls gh / git / codex / claude / the queue / the
# scheduler, or touches the network -- covered by Pester under
# tools/dispatch-tests/**. Send-DispatchProgressComment is the best-effort
# wrapper that actually posts via `gh issue comment`.

function Format-DispatchProgressComment {
    # Build deterministic progress-comment markdown for a single stage. Pure:
    # given the same inputs the function returns the same output, with no
    # timestamps, process ids, or external lookups.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('issue-claimed', 'loop-starting', 'loop-finished', 'publish-decision')]
        [string]$Stage,

        [Parameter(Mandatory = $true)]
        [int]$IssueNumber,

        [Parameter(Mandatory = $true)]
        [ValidatePattern('^[A-Za-z0-9._-]+$')]
        [string]$DispatchId,

        [Parameter(Mandatory = $true)]
        [string]$Branch,

        [AllowEmptyString()]
        [string]$LoopLogPath = '',

        [AllowEmptyString()]
        [string]$LoopExit = '',

        [AllowEmptyString()]
        [string]$Verdict = '',

        [ValidateSet('', 'auto-publish', 'branch', 'pr', 'not-eligible', 'no-commit')]
        [string]$PublishMode = '',

        [ValidateSet('claude', 'codex')]
        [string]$Executor = 'codex'
    )

    $issueRef = "#$IssueNumber"
    $footer = '_Posted by Invoke-AiDispatchQueue.ps1 as a non-terminal progress marker. Progress-comment failures warn but do not alter dispatch outcome; the final result comment and terminal labels remain authoritative._'

    switch ($Stage) {
        'issue-claimed' {
            return @"
**AI dispatch progress** - ``$DispatchId`` - issue claimed

- Issue: $issueRef
- Dispatch id: ``$DispatchId``
- Branch: ``$Branch``
- Stage: queue runner claimed the issue and is preparing the dispatch.

$footer
"@
        }
        'loop-starting' {
            $executorLabel = if ($Executor -eq 'codex') { 'Codex' } else { 'Claude' }
            $logLine = if ($LoopLogPath) {
                "- Loop log: ``$LoopLogPath``"
            } else {
                '- Loop log: (path not yet available)'
            }
            return @"
**AI dispatch progress** - ``$DispatchId`` - inner loop starting

- Issue: $issueRef
- Dispatch id: ``$DispatchId``
- Branch: ``$Branch``
$logLine
- Stage: invoking Invoke-AiDispatchLoop.ps1 (Codex plan, Claude gate, $executorLabel execute, Codex control).

$footer
"@
        }
        'loop-finished' {
            $exitDisplay = if ($LoopExit -ne '') { $LoopExit } else { 'unknown' }
            $verdictDisplay = if ($Verdict)      { $Verdict }  else { 'unknown' }
            return @"
**AI dispatch progress** - ``$DispatchId`` - inner loop finished

- Issue: $issueRef
- Dispatch id: ``$DispatchId``
- Branch: ``$Branch``
- Loop exit code: ``$exitDisplay``
- Codex control verdict: ``$verdictDisplay``
- Stage: dispatch loop returned; queue is reconciling commit and publish decision.

$footer
"@
        }
        'publish-decision' {
            $modeLine = switch ($PublishMode) {
                'auto-publish' { 'auto-publish - attempting fast-forward into origin/main.' }
                'branch'       { '-NoPublish branch mode - committing to the dispatch branch for human review; no auto-merge, push, or PR publish.' }
                'pr'           { 'pr mode - pushing the dispatch branch and opening a GitHub pull request targeting ``main``; no auto-merge, no push to ``origin/main``, no automatic issue close.' }
                'not-eligible' { 'skipped - not eligible to publish (loop exit code was non-zero or Codex control verdict was not ``pass``).' }
                'no-commit'    { 'skipped - loop produced no committable changes; nothing to publish.' }
                default        { 'unknown.' }
            }
            return @"
**AI dispatch progress** - ``$DispatchId`` - publish decision

- Issue: $issueRef
- Dispatch id: ``$DispatchId``
- Branch: ``$Branch``
- Publish mode: $modeLine

$footer
"@
        }
    }
}

function Send-DispatchProgressComment {
    # Best-effort GitHub issue comment poster for non-terminal progress
    # markers. A gh failure emits a clear WARNING line and continues; nothing
    # else (publish gates, retry eligibility, terminal labels, failure
    # taxonomy, dispatch outcome) is affected. Never throws.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][int]$IssueNumber,
        [Parameter(Mandatory = $true)][string]$RepoSlug,
        [Parameter(Mandatory = $true)][string]$Stage,
        [Parameter(Mandatory = $true)][string]$Body
    )

    Write-TimingTrace "queue.github: progress-comment start (stage=$Stage)"
    $commentFile = Join-Path $env:TEMP "rge-ai-dispatch-progress-$IssueNumber-$Stage.txt"
    try {
        Write-Utf8 $commentFile $Body
        $r = Invoke-Tool -Exe 'gh' -CmdArgs @(
            'issue', 'comment', "$IssueNumber", '--repo', $RepoSlug,
            '--body-file', $commentFile)
        Write-TimingTrace "queue.github: progress-comment done (stage=$Stage, exit=$($r.Code))"
        if ($r.Code -ne 0) {
            Write-Output ("WARNING: progress comment for stage '$Stage' on issue #$IssueNumber " +
                "failed to post (gh exit $($r.Code)); continuing dispatch. Final result comment " +
                "and terminal labels remain authoritative.`n$($r.Text)")
        }
    } catch {
        Write-Output ("WARNING: progress comment for stage '$Stage' on issue #$IssueNumber " +
            "raised an exception; continuing dispatch. $($_.Exception.Message)")
    } finally {
        Remove-Item -LiteralPath $commentFile -Force -ErrorAction SilentlyContinue
    }
}

function New-DispatchLoopArguments {
    # Build the exact child powershell.exe argument vector the queue uses to
    # invoke Invoke-AiDispatchLoop.ps1. Pure helper: no process launch, no git,
    # no gh, no network. Pester uses this to dry-run executor plumbing without
    # running a live dispatch.
    [CmdletBinding()]
    [OutputType([string[]])]
    param(
        [Parameter(Mandatory = $true)]
        [string]$LoopScript,

        [Parameter(Mandatory = $true)]
        [string]$DispatchId,

        [Parameter(Mandatory = $true)]
        [string]$GoalFile,

        [ValidateRange(0, 5)]
        [int]$MaxPlanRevisions = 2,

        [ValidateRange(0, 5)]
        [int]$MaxCorrectionRounds = 2,

        [ValidateSet('claude', 'codex')]
        [string]$Executor = 'codex',

        [bool]$CodexExecutorExternalScratch = $false,

        [bool]$EnablePreflightAudit = $false
    )

    $args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $LoopScript,
        '-DispatchId', $DispatchId, '-GoalFile', $GoalFile,
        '-MaxPlanRevisions', $MaxPlanRevisions,
        '-MaxCorrectionRounds', $MaxCorrectionRounds,
        '-Executor', $Executor)
    if ($CodexExecutorExternalScratch) { $args += '-CodexExecutorExternalScratch' }
    if ($EnablePreflightAudit) { $args += '-EnablePreflightAudit' }
    return ,$args
}

function New-HandoffClaimArguments {
    # Build the child powershell.exe argument vector used to call the
    # standalone ADR-121 claim helper. Pure helper: no process launch, no git,
    # no gh, no network. Queue integration passes -Root as the isolated
    # worktree (committed event JSON) and -LiveRoot as the primary checkout
    # (shared live lock).
    [CmdletBinding()]
    [OutputType([string[]])]
    param(
        [Parameter(Mandatory = $true)]
        [string]$ClaimScript,

        [Parameter(Mandatory = $true)]
        [ValidateSet('Claim', 'Release', 'Reclaim')]
        [string]$Action,

        [Parameter(Mandatory = $true)]
        [string]$DispatchId,

        [Parameter(Mandatory = $true)]
        [string]$Actor,

        [Parameter(Mandatory = $true)]
        [string]$Harness,

        [Parameter(Mandatory = $true)]
        [string]$Branch,

        [Parameter(Mandatory = $true)]
        [string]$Root,

        [Parameter(Mandatory = $true)]
        [string]$LiveRoot,

        [ValidateRange(3600, 604800)]
        [int]$TtlSeconds = 43200
    )

    return ,@(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $ClaimScript,
        '-Action', $Action,
        '-DispatchId', $DispatchId,
        '-Actor', $Actor,
        '-Harness', $Harness,
        '-Branch', $Branch,
        '-Root', $Root,
        '-LiveRoot', $LiveRoot,
        '-TtlSeconds', $TtlSeconds,
        '-JsonOnly'
    )
}

function Invoke-QueueHandoffClaim {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('Claim', 'Release', 'Reclaim')]
        [string]$Action,

        [Parameter(Mandatory = $true)]
        [string]$DispatchId,

        [Parameter(Mandatory = $true)]
        [string]$Actor,

        [Parameter(Mandatory = $true)]
        [string]$Branch,

        [Parameter(Mandatory = $true)]
        [string]$WorktreeRoot,

        [Parameter(Mandatory = $true)]
        [string]$PrimaryRoot,

        [ValidateRange(3600, 604800)]
        [int]$TtlSeconds = 43200
    )

    $claimArgs = New-HandoffClaimArguments -ClaimScript $claimScript -Action $Action `
        -DispatchId $DispatchId -Actor $Actor -Harness 'Invoke-AiDispatchQueue.ps1' `
        -Branch $Branch -Root $WorktreeRoot -LiveRoot $PrimaryRoot -TtlSeconds $TtlSeconds
    $r = Invoke-Tool -Exe 'powershell.exe' -CmdArgs $claimArgs
    if ($r.Code -ne 0) {
        Fail "ADR-121 handoff claim action '$Action' failed (exit $($r.Code)):`n$($r.Text)"
    }
    try {
        return ($r.Text | ConvertFrom-Json)
    } catch {
        Fail "ADR-121 handoff claim action '$Action' returned unparseable JSON: $($_.Exception.Message)`n$($r.Text)"
    }
}

function Acquire-QueueHandoffClaim {
    param(
        [Parameter(Mandatory = $true)][string]$DispatchId,
        [Parameter(Mandatory = $true)][string]$Actor,
        [Parameter(Mandatory = $true)][string]$Branch,
        [Parameter(Mandatory = $true)][string]$WorktreeRoot,
        [Parameter(Mandatory = $true)][string]$PrimaryRoot,
        [ValidateRange(3600, 604800)][int]$TtlSeconds = 43200
    )

    $claim = Invoke-QueueHandoffClaim -Action 'Claim' -DispatchId $DispatchId `
        -Actor $Actor -Branch $Branch -WorktreeRoot $WorktreeRoot -PrimaryRoot $PrimaryRoot `
        -TtlSeconds $TtlSeconds
    if ($claim.status -eq 'STALE') {
        Write-Output "ADR-121 handoff claim is stale; reclaiming $DispatchId before execution."
        $claim = Invoke-QueueHandoffClaim -Action 'Reclaim' -DispatchId $DispatchId `
            -Actor $Actor -Branch $Branch -WorktreeRoot $WorktreeRoot -PrimaryRoot $PrimaryRoot `
            -TtlSeconds $TtlSeconds
    }

    return $claim
}

function Release-QueueHandoffClaim {
    param(
        [Parameter(Mandatory = $true)][string]$DispatchId,
        [Parameter(Mandatory = $true)][string]$Actor,
        [Parameter(Mandatory = $true)][string]$Branch,
        [Parameter(Mandatory = $true)][string]$WorktreeRoot,
        [Parameter(Mandatory = $true)][string]$PrimaryRoot,
        [ValidateRange(3600, 604800)][int]$TtlSeconds = 43200
    )

    $release = Invoke-QueueHandoffClaim -Action 'Release' -DispatchId $DispatchId `
        -Actor $Actor -Branch $Branch -WorktreeRoot $WorktreeRoot -PrimaryRoot $PrimaryRoot `
        -TtlSeconds $TtlSeconds
    if ($release.status -ne 'RELEASED') {
        Write-Output ("WARNING: ADR-121 handoff claim release for $DispatchId returned " +
            "status=$($release.status); live lock may remain until TTL expiry.")
    }
    return $release
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
if ($CodexExecutorExternalScratch -and $Executor -ne 'codex') {
    Fail "-CodexExecutorExternalScratch is only valid with -Executor codex; it does not apply to Claude execution."
}
if ($Executor -eq 'claude') {
    Require-Command claude
}
Require-Command powershell.exe

$loopScript = Join-Path $script:RepoRoot 'Invoke-AiDispatchLoop.ps1'
if (-not (Test-Path -LiteralPath $loopScript)) {
    Fail "Dispatch loop script not found: $loopScript"
}
$claimScript = Join-Path $script:RepoRoot 'Invoke-HandoffClaim.ps1'
if (-not $SkipHandoffClaim -and -not (Test-Path -LiteralPath $claimScript)) {
    Fail "Handoff claim helper not found: $claimScript"
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

# --- Resolve publish mode --------------------------------------------------
# Collapse -PublishMode and -NoPublish into one internal mode string ('main',
# 'branch', or 'pr'). Fails fast on a conflicting combination so progress
# comments, publish gates, retry classification, and the final result comment
# all key off a single, validated mode.
try {
    $script:ResolvedPublishMode = Resolve-DispatchPublishMode -PublishMode $PublishMode -NoPublish $NoPublish.IsPresent
} catch {
    Fail $_.Exception.Message
}

# --- Single-run lock -------------------------------------------------------

if (-not $DryRun) {
    if (-not (Acquire-Lock)) {
        Write-Output "A dispatch-queue run is already in progress; skipping this tick."
        exit 0
    }
    $script:LockHeld = $true
}

try {
    # --- Release stale queue-owned ADR-121 claims before issue selection ---
    if (-not $DryRun) {
        Invoke-StaleQueueHandoffClaimSweep -Reason 'queue-start owner-liveness sweep'
    }

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
    # ISSUE-231: the queue no longer stashes primary untracked clutter. The
    # dispatch runs in an isolated worktree, so primary untracked files are
    # outside the dispatch's working tree and cannot contaminate the commit.

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
        '--json', 'number,title,body,labels,url,createdAt')
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

    # Stale-issue replay guard: drop any pending issue whose work already reached
    # origin/main. A queued issue can carry a stale body (e.g. after a brief
    # amendment filed a fresh issue, or a terminal relabel that never stuck);
    # re-running it would replay/duplicate already-published work. Mirrors the
    # orphan-recovery published-SHA check -- search origin/main for the dispatch's
    # own commit "ai-dispatch ISSUE-N:".
    if ($pending.Count -gt 0) {
        # Highest ai-auto issue number across ALL states. The self-rearm loop files one
        # task at a time, so a pending issue below this is superseded -- its body is
        # stale after a brief amendment filed a newer task. Best-effort: if the lookup
        # fails, fall back to the max pending number, which still drops the older
        # pending issues (the common race) and leaves the published-SHA guard as the
        # cross-state backstop.
        $maxAuto = (@($pending) | Measure-Object -Property number -Maximum).Maximum
        $allAuto = Invoke-Tool -Exe 'gh' -CmdArgs @('issue', 'list', '--repo', $repoSlug,
            '--label', 'ai-auto', '--state', 'all', '--limit', '200', '--json', 'number')
        if ($allAuto.Code -eq 0 -and $allAuto.Text) {
            try {
                $gMax = ((@($allAuto.Text | ConvertFrom-Json) | ForEach-Object { [int]$_.number }) |
                    Measure-Object -Maximum).Maximum
                if ($gMax -gt $maxAuto) { $maxAuto = $gMax }
            } catch {
                Write-Output "Stale-replay guard: could not parse ai-auto issue list; using pending-max (#$maxAuto)."
            }
        } else {
            Write-Output "Stale-replay guard: ai-auto issue lookup failed (exit $($allAuto.Code)); using pending-max (#$maxAuto)."
        }
        $stillPending = @()
        foreach ($p in $pending) {
            if (Test-PendingIssueSuperseded -IssueNumber $p.number -MaxAutoIssueNumber $maxAuto) {
                Write-Output "Stale-replay guard: issue #$($p.number) superseded by a newer ai-auto issue (#$maxAuto); marking done, not dispatching its (possibly stale) body."
                Invoke-Tool -Exe 'gh' -CmdArgs @(
                    'issue', 'edit', "$($p.number)", '--repo', $repoSlug,
                    '--remove-label', $QueueLabel, '--remove-label', $runLabel,
                    '--add-label', $doneLabel) | Out-Null
                Invoke-Tool -Exe 'gh' -CmdArgs @(
                    'issue', 'close', "$($p.number)", '--repo', $repoSlug,
                    '--comment', "Superseded by a newer ai-auto issue (#$maxAuto); closed by the stale-issue replay guard without re-running its (possibly stale) body.") | Out-Null
                continue
            }
            $pIssueId = "ISSUE-$($p.number)"
            # Time-floored at the issue's creation so the grep cannot match migrated
            # old-repo "ai-dispatch ISSUE-N:" commits (post-migration issue-number reuse).
            $pubSha = (Git-Step (Get-StaleReplayPublishedShaArgs -IssueId $pIssueId -CreatedAt $p.createdAt)).Trim()
            if ($pubSha) {
                $short = $pubSha.Substring(0, [Math]::Min(8, $pubSha.Length))
                Write-Output "Stale-replay guard: issue #$($p.number) already published as $short; marking done, not dispatching."
                Invoke-Tool -Exe 'gh' -CmdArgs @(
                    'issue', 'edit', "$($p.number)", '--repo', $repoSlug,
                    '--remove-label', $QueueLabel, '--remove-label', $runLabel,
                    '--add-label', $doneLabel) | Out-Null
                Invoke-Tool -Exe 'gh' -CmdArgs @(
                    'issue', 'close', "$($p.number)", '--repo', $repoSlug,
                    '--comment', "Already published to origin/main as $short; closed by the stale-issue replay guard without re-running its (possibly stale) body.") | Out-Null
            } else {
                $stillPending += $p
            }
        }
        $pending = @($stillPending)
    }

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

    # ISSUE-231: every queue dispatch runs the inner loop inside an isolated
    # git worktree sibling to the primary repo (see AI_DISPATCH_AUTOMATION.md
    # for the run boundary, and AI_DISPATCH_PARALLEL.md for the sibling
    # convention). Compute the path early so the collision checks can refuse
    # to overwrite a leftover worktree from a prior interrupted, retry-
    # eligible, or terminal-failed dispatch.
    $worktreePath = Resolve-DispatchWorktreePath -RepoRoot $script:RepoRoot -DispatchId $id

    # Reconcile the worktree registry before the collision checks so a stale
    # entry (worktree dir already removed) does not block the later
    # `git worktree add`. Build-cache hygiene for the shared cargo target is
    # handled per-dispatch by the verify gate's step 0 (.ai/dispatch.verify.ps1).
    $null = Git-Step @('worktree', 'prune')

    # A branch with no terminal label means an earlier run was interrupted;
    # do not silently clobber it.
    if ((Git-Step @('branch', '--list', $branch)).Trim()) {
        Fail ("Branch '$branch' already exists but issue #$($issue.number) is not " +
            "labelled '$runLabel'/'$doneLabel'. Inconsistent state - resolve by hand.")
    }

    # A worktree path that already exists means a prior dispatch (interrupted,
    # terminal-failed, or human-owned) may still hold useful state. Refuse to
    # clobber: the human must inspect and archive or remove it manually
    # before this issue is re-queued.
    if (Test-Path -LiteralPath $worktreePath) {
        Fail ("Isolated worktree path '$worktreePath' already exists for $id. " +
            "A prior dispatch or human checkout may still hold work there. " +
            "Inspect with `"git -C `"$worktreePath`" status`", then archive or " +
            "remove it manually (`"git worktree remove `"$worktreePath`"`") " +
            "before re-queueing issue #$($issue.number).")
    }

    Release-OrphanHandoffClaim -DispatchId $id -Branch $branch `
        -Reason 'pre-claim queued issue cleanup'

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
        @{ Name = 'ai-dispatch-failure-plan-gate';    Color = 'd93f0b'; Desc = 'AI dispatch terminal failure: plan gate not approved within revisions (flaky-recoverable)' },
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

    # Progress comment: issue claimed. Best-effort; failures warn only.
    $progressBody = Format-DispatchProgressComment `
        -Stage 'issue-claimed' `
        -IssueNumber ([int]$issue.number) `
        -DispatchId $id `
        -Branch $branch
    Send-DispatchProgressComment `
        -IssueNumber ([int]$issue.number) `
        -RepoSlug $repoSlug `
        -Stage 'issue-claimed' `
        -Body $progressBody

    $goalBody = if ($issue.body -and $issue.body.Trim()) { [string]$issue.body } else { $title }
    $goalText = "GitHub issue #$($issue.number): $title`r`n`r`n$goalBody"
    if ($isRetry) {
        Write-Output "Retry run: issue carries '$retryLabel'; injecting prior-attempt feedback."

        # ISSUE-231: the prior retry-eligible failed dispatch should have
        # archived its worktree as `<worktreePath>.attempt<N>`. The run dir
        # lives inside that archive at `.ai/dispatch-<id>/`. Pick the highest-
        # numbered attempt.
        $priorRunDir = ''
        $worktreeParent = [System.IO.Path]::GetDirectoryName($worktreePath)
        if ($worktreeParent -and (Test-Path -LiteralPath $worktreeParent)) {
            $attempts = Get-ChildItem -LiteralPath $worktreeParent -Directory -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -match "^$([regex]::Escape($id))\.attempt(\d+)$" } |
                Sort-Object { [int]([regex]::Match($_.Name, 'attempt(\d+)$').Groups[1].Value) }
            $latestAttempt = $attempts | Select-Object -Last 1
            if ($latestAttempt) {
                $candidate = Join-Path $latestAttempt.FullName (Join-Path '.ai' "dispatch-$id")
                if (Test-Path -LiteralPath $candidate) {
                    $priorRunDir = $candidate
                    Write-Output "  reading prior-attempt feedback from $candidate"
                }
            }
        }

        # Legacy fallback: pre-ISSUE-231 runs archived the run dir under the
        # primary checkout's gitignored `.ai/dispatch-<id>.attemptN`. If no
        # worktree archive is found, fall back to that layout (and archive a
        # leftover live run dir if it is still present) so the transition
        # between flows does not drop feedback.
        if (-not $priorRunDir) {
            $liveRunDir = Join-Path $script:RepoRoot (Join-Path '.ai' "dispatch-$id")
            if (Test-Path -LiteralPath $liveRunDir) {
                $n = 1
                while (Test-Path -LiteralPath "$liveRunDir.attempt$n") { $n++ }
                $archiveDir = "$liveRunDir.attempt$n"
                try {
                    Move-Item -LiteralPath $liveRunDir -Destination $archiveDir -Force
                    $priorRunDir = $archiveDir
                    Write-Output "  archived legacy prior run dir -> $(Get-RepoRelativePathForQueue $archiveDir)"
                } catch {
                    Write-Output "  WARNING: could not archive legacy prior run dir: $($_.Exception.Message)"
                    $priorRunDir = $liveRunDir
                }
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

    # --- Create the isolated worktree and run the loop inside it -----------
    # ISSUE-231: the dispatch loop, scope guard, audit log, staging, and
    # commit all run against this isolated worktree, while the primary
    # checkout stays on `main`. The worktree is created off the synced
    # `HEAD` (which the preflight just confirmed matches `origin/main`) and
    # is on the per-issue branch immediately.

    Write-TimingTrace "queue.worktree: add start (path=$worktreePath, branch=$branch)"
    Git-Step @('worktree', 'add', '-b', $branch, $worktreePath, 'HEAD') | Out-Null
    Write-TimingTrace "queue.worktree: add done"
    $script:DispatchWorktreeRoot = $worktreePath

    $handoffClaimActor = "Invoke-AiDispatchQueue.ps1:$PID"
    $handoffClaimHeld = $false
    if (-not $SkipHandoffClaim) {
        Write-TimingTrace "queue.claim: acquire start (dispatch=$id)"
        $claimResult = Acquire-QueueHandoffClaim -DispatchId $id -Actor $handoffClaimActor `
            -Branch $branch -WorktreeRoot $worktreePath -PrimaryRoot $script:RepoRoot `
            -TtlSeconds $HandoffClaimTtlSeconds
        Write-TimingTrace "queue.claim: acquire done (status=$($claimResult.status))"
        if ($claimResult.status -notin @('CLAIMED', 'RECLAIMED', 'OWNED')) {
            Write-Output ("ADR-121 handoff claim blocked dispatch $id " +
                "(status=$($claimResult.status), actor=$($claimResult.actor), " +
                "harness=$($claimResult.harness), branch=$($claimResult.branch)).")
            Write-Output "Cleaning up empty isolated worktree after blocked claim."
            $rmClaimWt = Invoke-Tool -Exe 'git' -CmdArgs @('worktree', 'remove', $worktreePath)
            if ($rmClaimWt.Code -eq 0) {
                $script:DispatchWorktreeRoot = $null
                [void](Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-D', $branch))
            } else {
                Write-Output ("WARNING: could not remove claim-blocked worktree '$worktreePath' " +
                    "(exit $($rmClaimWt.Code)): $($rmClaimWt.Text)")
            }
            Fail ("ADR-121 handoff claim blocked dispatch $id; no execution was started.")
        }
        $handoffClaimHeld = $true
        Write-Output "ADR-121 handoff claim: $($claimResult.status) ($($claimResult.lock_path))"
    } else {
        Write-Output "ADR-121 handoff claim: skipped by -SkipHandoffClaim."
    }

    $loopLog = Join-Path $env:TEMP "rge-ai-dispatch-$id.log"

    # Progress comment: inner loop starting. Best-effort; failures warn only.
    $progressBody = Format-DispatchProgressComment `
        -Stage 'loop-starting' `
        -IssueNumber ([int]$issue.number) `
        -DispatchId $id `
        -Branch $branch `
        -LoopLogPath $loopLog `
        -Executor $Executor
    Send-DispatchProgressComment `
        -IssueNumber ([int]$issue.number) `
        -RepoSlug $repoSlug `
        -Stage 'loop-starting' `
        -Body $progressBody

    Write-Output ""
    Write-Output "Starting dispatch loop for $id in isolated worktree '$worktreePath'."
    Write-Output "Live loop output follows:"
    Write-Output "----------------------------------------------------------------"
    Write-TimingTrace "queue.loop: start (dispatch=$id, branch=$branch, worktree=$worktreePath)"
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    # Push-Location so the inner powershell.exe inherits cwd=worktree. The
    # loop resolves its own RepoRoot via `git rev-parse --show-toplevel`, so
    # cwd=worktree maps every loop-owned git operation onto the worktree
    # instead of the primary checkout.
    Push-Location -LiteralPath $worktreePath
    try {
        $loopArgs = New-DispatchLoopArguments -LoopScript $loopScript -DispatchId $id `
            -GoalFile $goalFile -MaxPlanRevisions $MaxPlanRevisions `
            -MaxCorrectionRounds $MaxCorrectionRounds -Executor $Executor `
            -CodexExecutorExternalScratch ([bool]$CodexExecutorExternalScratch) `
            -EnablePreflightAudit ([bool]$EnablePreflightAudit)
        & powershell.exe @loopArgs 2>&1 | Tee-Object -FilePath $loopLog
    } finally {
        Pop-Location
        $ErrorActionPreference = $prevEap
    }
    $loopExit = $LASTEXITCODE
    Write-Output "----------------------------------------------------------------"
    Write-Output "Dispatch loop exited with code $loopExit."
    Write-TimingTrace "queue.loop: done (exit=$loopExit)"

    if ($handoffClaimHeld) {
        Write-TimingTrace "queue.claim: release start (dispatch=$id)"
        $releaseResult = Release-QueueHandoffClaim -DispatchId $id -Actor $handoffClaimActor `
            -Branch $branch -WorktreeRoot $worktreePath -PrimaryRoot $script:RepoRoot `
            -TtlSeconds $HandoffClaimTtlSeconds
        Write-TimingTrace "queue.claim: release done (status=$($releaseResult.status))"
        Write-Output "ADR-121 handoff claim release: $($releaseResult.status)"
        $handoffClaimHeld = $false
    }

    $loopText = (Get-Content -Raw -LiteralPath $loopLog -ErrorAction SilentlyContinue)
    # Read the Codex control verdict from the structured run-dir JSON the loop
    # writes (schema-validated), not by scraping loop stdout. Newest round wins.
    # The run dir lives inside the worktree under `.ai/dispatch-<id>/`.
    $runDir = Join-Path $worktreePath (Join-Path '.ai' "dispatch-$id")
    $verdict = Get-ControlVerdict -RunDir $runDir
    $execStatus = Get-ExecutionStatus -RunDir $runDir -DispatchId $id
    Write-TimingTrace "queue.control: verdict-read (verdict=$verdict, execStatus=$execStatus)"

    # Progress comment: inner loop finished. Best-effort; failures warn only.
    $progressBody = Format-DispatchProgressComment `
        -Stage 'loop-finished' `
        -IssueNumber ([int]$issue.number) `
        -DispatchId $id `
        -Branch $branch `
        -LoopExit "$loopExit" `
        -Verdict $verdict
    Send-DispatchProgressComment `
        -IssueNumber ([int]$issue.number) `
        -RepoSlug $repoSlug `
        -Stage 'loop-finished' `
        -Body $progressBody

    # --- Write detailed audit log, then commit the branch ------------------

    Write-TimingTrace "queue.commit: dispatch-log start"
    $dispatchLogPath = Write-DispatchLog -Id $id -Issue $issue -Branch $branch `
        -LoopLog $loopLog -LoopText ([string]$loopText) -LoopExit $loopExit -Verdict $verdict `
        -WorktreeRoot $worktreePath -Executor $Executor
    # Capture the committed repo-relative dispatch-log path NOW, while
    # $script:DispatchWorktreeRoot is still set to the isolated worktree.
    # Worktree cleanup (main/pr publish remove, no-commit empty-worktree
    # remove, retry archive, or terminal cleanup-decision remove) clears
    # $script:DispatchWorktreeRoot, after which Get-RepoRelativePathForQueue
    # falls back to the primary repo root and -- since the dispatch log
    # lives at an absolute path inside the now-removed worktree directory
    # outside the primary checkout -- emits an absolute, removed-worktree
    # path instead of the committed `ai_dispatch_logs/log_*.md` relpath.
    # All final user-facing references (result comment, PR body, close
    # comment, commit message) must use this stable value.
    $dispatchLogRel = Get-RepoRelativePathForQueue $dispatchLogPath
    Write-Output "Detailed dispatch log written: $dispatchLogRel"
    Write-TimingTrace "queue.commit: dispatch-log done"

    # Scope guard: validate the worktree against the active TASK packet
    # BEFORE any broad staging, commit, merge, push, or publish step. The
    # scope guard reads status with `git -C $worktreePath` via
    # $script:DispatchWorktreeRoot. Stray work outside the dispatch's
    # declared surface aborts the run here -- nothing is staged, committed,
    # or published.
    #
    # Wrap the entire post-dispatch-log / pre-publish-decision window in a
    # try/catch that mirrors any non-Fail terminating error to Write-Output
    # before re-throwing. Fail itself already mirrors its message to stdout
    # via the Fail() helper, so a guard or Git-Step failure inside the
    # window surfaces in captured queue output. This catch covers the
    # remaining cases: a raw `throw`, a native command terminating error
    # under EAP=Stop, or any other generalized exception in this window --
    # so the queue no longer looks like a silent stall when the
    # publish-decision progress comment never lands. The body indentation
    # below stays at the outer-block level for review-friendly minimal diff;
    # PowerShell does not depend on indentation for parsing the try scope.
    try {
    Write-TimingTrace "queue.guard: scope-check start"
    Invoke-QueueScopeGuard -DispatchId $id -DispatchLogPath $dispatchLogPath
    Write-TimingTrace "queue.guard: scope-check done"

    Write-TimingTrace "queue.commit: git-add start"
    Git-Step @('-C', $worktreePath, 'add', '-A') | Out-Null
    Write-TimingTrace "queue.commit: git-add done"
    $staged = Invoke-Tool -Exe 'git' -CmdArgs @('-C', $worktreePath, 'diff', '--cached', '--quiet')
    $committed = $false
    $commitSha = ''
    if ($staged.Code -ne 0) {
        $outcome = if ($loopExit -eq 0) { 'ok' } else { "failed (exit $loopExit)" }
        $msg = @"
ai-dispatch $id`: $title

Unattended dispatch run via Invoke-AiDispatchQueue.ps1.
Loop exit code: $loopExit. Control verdict: $verdict. Outcome: $outcome.
Source: $($issue.url)
Detailed log: $dispatchLogRel

Publish policy: a passed run (loop exit 0, control verdict pass) publishes
per the resolved -PublishMode -- default pr opens a pull request targeting
main without pushing origin/main; explicit main fast-forwards and pushes
origin/main; branch / -NoPublish leaves the work on the dispatch branch.
Failed or blocked work remains local.

$(
    if ($Executor -eq 'codex') {
        'Co-Authored-By: OpenAI Codex <noreply@openai.com>'
    } else {
        'Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>'
    }
)
"@
        $msgFile = Join-Path $env:TEMP "rge-ai-dispatch-msg-$id.txt"
        Write-Utf8 $msgFile $msg
        Write-TimingTrace "queue.commit: git-commit start"
        Git-Step @('-C', $worktreePath, 'commit', '-F', $msgFile) | Out-Null
        Write-TimingTrace "queue.commit: git-commit done"
        Remove-Item -LiteralPath $msgFile -Force -ErrorAction SilentlyContinue
        $commitSha = (Git-Step @('-C', $worktreePath, 'rev-parse', '--short', 'HEAD')).Trim()
        $committed = $true
        Write-TimingTrace "queue.commit: committed (sha=$commitSha)"
    } else {
        Write-TimingTrace "queue.commit: no staged changes"
    }

    # The primary checkout never left `main`, so there is no checkout-back-
    # to-main step here, and the pre-ISSUE-231 stash/untracked-park dance is
    # gone too: the worktree is isolated, so primary untracked files never
    # contaminated the dispatch commit in the first place.

    # Track the disposition of the isolated worktree across the publish and
    # cleanup steps so the result comment / dispatch log can name where
    # surviving state lives.
    $worktreeDisposition = ''
    $worktreeArchivePath = ''
    $worktreeStatusLine = ''

    # If the loop produced no committable changes the worktree has nothing
    # worth preserving and the branch was created but never advanced. Remove
    # the worktree first (so the branch is no longer checked out anywhere),
    # then delete the empty branch. A failure here is non-terminal: the
    # worktree is preserved for human inspection and a warning is recorded.
    if (-not $committed) {
        Write-TimingTrace "queue.commit: remove-empty-worktree start"
        Copy-DispatchRunDirToPrimary -WorktreeRoot $worktreePath -PrimaryRoot $script:RepoRoot -DispatchId $id
        $rmwt = Invoke-Tool -Exe 'git' -CmdArgs @('worktree', 'remove', $worktreePath)
        Write-TimingTrace "queue.commit: remove-empty-worktree done (exit=$($rmwt.Code))"
        if ($rmwt.Code -eq 0) {
            $worktreeDisposition = 'removed'
            $script:DispatchWorktreeRoot = $null
            Write-TimingTrace "queue.commit: delete-empty-branch start"
            $delEmpty = Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-D', $branch)
            Write-TimingTrace "queue.commit: delete-empty-branch done (exit=$($delEmpty.Code))"
            if ($delEmpty.Code -ne 0) {
                Write-Output "WARNING: could not delete empty branch '$branch' (exit $($delEmpty.Code)): $($delEmpty.Text)"
            }
        } else {
            Write-Output ("WARNING: could not remove empty worktree '$worktreePath' " +
                "(exit $($rmwt.Code)): $($rmwt.Text). Branch '$branch' left in place.")
            $worktreeDisposition = 'preserved'
        }
    }

    # --- Publish passed work ------------------------------------------------

    $published = $false
    $publishFailed = $false
    $publishHardFailed = $false
    $publishDetail = ''
    $publishedSha = ''
    $prNumber = 0
    $prUrl = ''
    $eligibleForPublish = ($committed -and $loopExit -eq 0 -and $verdict -eq 'pass')

    # Default-OFF surface-split routing: when armed and the run is publishable, derive
    # the effective publish mode from the changed paths -- low-risk (docs/tests/
    # artifacts) auto-merges to main; ANY high-risk path opens a PR for human merge.
    # FAIL-CLOSED: an empty or uncomputable diff routes to PR, never main. The guard's
    # publish-confirmation (Test-PublishConfirmation) independently gates the actual
    # main push, so this routing cannot by itself land unverified work on main.
    # Surface-split applies ONLY to a main posture: it may DOWNGRADE a main publish to a
    # human-merged PR (any high-risk path, or a brief-only changeset), but it must never
    # PROMOTE an explicit branch/-NoPublish or pr posture up to main. Gating on 'main'
    # keeps the operator's lower-trust intent authoritative (fail-closed).
    if ($SurfaceSplitPublish -and $eligibleForPublish -and $script:ResolvedPublishMode -eq 'main') {
        $ssChanged = @()
        # Diff the dispatch BRANCH (not HEAD): Git-Step runs in the primary checkout's
        # cwd, whose HEAD is the parked origin/main -> 'origin/main...HEAD' would be empty.
        # The branch ref resolves correctly from any cwd (worktrees share the object store).
        $ssDiff = Git-Step @('diff', '--name-only', "origin/main...$branch")
        if ($ssDiff) { $ssChanged = @(($ssDiff -split "`r?`n") | Where-Object { $_.Trim() }) }
        $ssRouting = Get-DispatchSurfaceRouting -ChangedPaths $ssChanged -AllowBriefRideAlong:([bool]$AllowBriefRideAlong)
        if ($script:ResolvedPublishMode -ne $ssRouting.Routing) {
            Write-Output "Surface-split: routing this dispatch to '$($ssRouting.Routing)' (was '$($script:ResolvedPublishMode)') -- $($ssRouting.Reason)"
        }
        $script:ResolvedPublishMode = $ssRouting.Routing
    }

    # Default-OFF diff-size cap: a main-routed publish whose diff exceeds the cap is
    # downgraded to a human-merged PR (fail-closed: a large OR uncomputable diff
    # always gets human review). No-op when both caps are 0 (disabled).
    if ($eligibleForPublish -and $script:ResolvedPublishMode -eq 'main' -and ($MaxDiffFiles -gt 0 -or $MaxDiffLines -gt 0)) {
        # Diff the dispatch BRANCH (see surface-split note above): computing against the
        # primary checkout's HEAD would see 0 files/0 lines and FAIL OPEN (any diff judged
        # "within cap"), letting an oversized change auto-publish to main.
        $numstat = Git-Step @('diff', '--numstat', "origin/main...$branch")
        $dcFiles = 0; $dcLines = 0; $dcParseOk = $true
        if ($numstat) {
            foreach ($nl in ($numstat -split "`r?`n")) {
                if (-not $nl.Trim()) { continue }
                $cols = $nl -split "`t"
                if ($cols.Count -ge 2) {
                    $dcFiles++
                    $add = 0; $del = 0
                    [void][int]::TryParse($cols[0], [ref]$add)
                    [void][int]::TryParse($cols[1], [ref]$del)
                    $dcLines += ($add + $del)
                } else { $dcParseOk = $false }
            }
        }
        $cap = Test-DiffSizeWithinCap -FilesChanged $dcFiles -LinesChanged $dcLines -MaxFiles $MaxDiffFiles -MaxLines $MaxDiffLines
        if (-not $dcParseOk -or -not $cap.Within) {
            $dcReason = if (-not $dcParseOk) { 'diff numstat unparseable' } else { $cap.Reason }
            Write-Output "Diff-size cap: downgrading main publish to a human-merged PR -- $dcReason."
            $script:ResolvedPublishMode = 'pr'
        }
    }

    # Progress comment: publish decision. Best-effort; failures warn only.
    # The five-way mode mirrors the publish if/elseif chain below so the
    # comment authoritatively names which branch is about to run.
    $progressMode = if ($eligibleForPublish -and $script:ResolvedPublishMode -eq 'main') {
        'auto-publish'
    } elseif ($eligibleForPublish -and $script:ResolvedPublishMode -eq 'branch') {
        'branch'
    } elseif ($eligibleForPublish -and $script:ResolvedPublishMode -eq 'pr') {
        'pr'
    } elseif ($committed) {
        'not-eligible'
    } else {
        'no-commit'
    }
    $progressBody = Format-DispatchProgressComment `
        -Stage 'publish-decision' `
        -IssueNumber ([int]$issue.number) `
        -DispatchId $id `
        -Branch $branch `
        -PublishMode $progressMode
    } catch {
        # Mirror the caught exception into captured queue output before re-
        # throwing so any failure between `Detailed dispatch log written` and
        # the `publish-decision` progress comment is visible in a stdout-only
        # log (the prior silent-stall symptom). Re-throw preserves the
        # original exit behavior: the top-level catch (or PowerShell's
        # default terminating-error handling) still terminates the queue
        # with a non-zero exit. This is generalized -- it covers raw
        # `throw`, native command terminating errors under EAP=Stop, and
        # any other exception in this window, not only the dotfile guard
        # failure that surfaced on ISSUE-234.
        Write-Output ("ERROR: queue step failed between dispatch-log write and " +
            "publish-decision progress comment: $($_.Exception.Message)")
        throw
    }
    Send-DispatchProgressComment `
        -IssueNumber ([int]$issue.number) `
        -RepoSlug $repoSlug `
        -Stage 'publish-decision' `
        -Body $progressBody

    if ($eligibleForPublish -and $script:ResolvedPublishMode -eq 'main') {
        Write-Output "Codex control passed; publishing $branch to origin/main."
        Write-TimingTrace "queue.publish: block-entry; eligibleForPublish=true mode=main"

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
                        # Remove the isolated worktree FIRST: git refuses to
                        # delete a branch that is checked out by a linked
                        # worktree, so the worktree removal has to precede
                        # the branch delete. A worktree-remove failure is
                        # non-terminal -- the publish has already succeeded
                        # -- but it does prevent the branch delete: the
                        # branch is then preserved alongside the worktree
                        # for human cleanup rather than left dangling.
                        Write-TimingTrace "queue.publish: published as $publishedSha; worktree-remove start"
                        Copy-DispatchRunDirToPrimary -WorktreeRoot $worktreePath -PrimaryRoot $script:RepoRoot -DispatchId $id
                        $rmwt = Invoke-Tool -Exe 'git' -CmdArgs @('worktree', 'remove', $worktreePath)
                        Write-TimingTrace "queue.publish: worktree-remove done (exit=$($rmwt.Code))"
                        if ($rmwt.Code -eq 0) {
                            $worktreeDisposition = 'removed'
                            $script:DispatchWorktreeRoot = $null
                            Write-TimingTrace "queue.publish: branch-delete start"
                            $delete = Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-d', $branch)
                            Write-TimingTrace "queue.publish: branch-delete done (exit=$($delete.Code))"
                            if ($delete.Code -ne 0) {
                                $publishDetail += "`nWARNING: published, but could not delete local branch $branch (exit $($delete.Code)): $($delete.Text)"
                            }
                        } else {
                            $publishDetail += "`nWARNING: published, but could not remove worktree '$worktreePath' (exit $($rmwt.Code)): $($rmwt.Text). Branch '$branch' kept in place."
                            $worktreeDisposition = 'preserved'
                        }
                    }
                }
            }
        }
        Write-TimingTrace "queue.publish: block-exit (published=$published, publishFailed=$publishFailed, publishHardFailed=$publishHardFailed)"
    } elseif ($eligibleForPublish -and $script:ResolvedPublishMode -eq 'pr') {
        # PR publish path: push the dispatch branch to origin and open a pull
        # request targeting main. Never fast-forward, never merge, never push
        # origin/main, never close the source issue. Any failure in branch push
        # or PR creation is publish-pipeline failure (publishHardFailed=true);
        # the local branch is preserved for human recovery and the run is not
        # auto-retried. The local branch is also kept post-success so the human
        # reviewer can rebase or amend if needed before merging the PR.
        Write-Output "Codex control passed; PR mode publishing $branch and opening a pull request."
        Write-TimingTrace "queue.publish: block-entry; eligibleForPublish=true mode=pr"

        Write-TimingTrace "queue.publish: branch-push start (branch=$branch)"
        $branchPush = Invoke-Tool -Exe 'git' -CmdArgs @('push', '--set-upstream', 'origin', $branch)
        Write-TimingTrace "queue.publish: branch-push done (exit=$($branchPush.Code))"
        if ($branchPush.Code -ne 0) {
            $publishFailed = $true
            $publishHardFailed = $true
            $publishDetail = "git push origin $branch failed (exit $($branchPush.Code)): $($branchPush.Text)`n$branch kept for human recovery."
        } else {
            $prTitle = Format-DispatchPrTitle -DispatchId $id -IssueTitle $title
            $prBody  = Format-DispatchPrBody `
                -IssueNumber ([int]$issue.number) `
                -IssueTitle $title `
                -DispatchId $id `
                -Branch $branch `
                -CommitSha $commitSha `
                -DispatchLogPath $dispatchLogRel `
                -Verdict $verdict
            $prBodyFile = Join-Path $env:TEMP "rge-ai-dispatch-pr-body-$id.md"
            Write-Utf8 $prBodyFile $prBody
            Write-TimingTrace "queue.publish: gh-pr-create start"
            $prCreate = Invoke-Tool -Exe 'gh' -CmdArgs @(
                'pr', 'create', '--repo', $repoSlug,
                '--base', 'main', '--head', $branch,
                '--title', $prTitle, '--body-file', $prBodyFile)
            Write-TimingTrace "queue.publish: gh-pr-create done (exit=$($prCreate.Code))"
            Remove-Item -LiteralPath $prBodyFile -Force -ErrorAction SilentlyContinue
            if ($prCreate.Code -ne 0) {
                $publishFailed = $true
                $publishHardFailed = $true
                $publishDetail = "gh pr create for $branch failed (exit $($prCreate.Code)): $($prCreate.Text)`nBranch was pushed to origin; $branch kept locally for human recovery."
            } else {
                # PR-mode success requires BOTH a PR number and a PR URL so the
                # final issue comment can carry the authoritative reference.
                # Always canonicalize through `gh pr view --json number,url` --
                # the stdout of `gh pr create` is not load-bearing for the
                # success gate. Any failure here (non-zero exit, unparseable
                # JSON, or missing fields) is publish-pipeline failure:
                # publishHardFailed=true, $published stays false, the branch is
                # preserved for human recovery, and the run is not auto-retried.
                Write-TimingTrace "queue.publish: gh-pr-view start"
                $prView = Invoke-Tool -Exe 'gh' -CmdArgs @(
                    'pr', 'view', $branch, '--repo', $repoSlug, '--json', 'number,url')
                Write-TimingTrace "queue.publish: gh-pr-view done (exit=$($prView.Code))"
                $prMeta = Resolve-DispatchPrViewMetadata -ExitCode $prView.Code -Text $prView.Text
                if ($prMeta.Success) {
                    $prNumber     = $prMeta.PrNumber
                    $prUrl        = $prMeta.PrUrl
                    $published    = $true
                    $publishedSha = $commitSha
                    $publishDetail = "Pushed $branch to origin and opened PR #$prNumber ($prUrl)."
                    Write-TimingTrace "queue.publish: pr-opened (prNumber=$prNumber)"
                    # PR is opened and the dispatch branch is published on
                    # `origin`; the isolated worktree is no longer needed for
                    # the reviewer's path. Remove it now and keep the branch
                    # in place as the PR's head. A remove failure is non-
                    # terminal: the PR has already succeeded and the human
                    # can clean up the worktree by hand.
                    Write-TimingTrace "queue.publish: pr-worktree-remove start"
                    Copy-DispatchRunDirToPrimary -WorktreeRoot $worktreePath -PrimaryRoot $script:RepoRoot -DispatchId $id
                    $rmwt = Invoke-Tool -Exe 'git' -CmdArgs @('worktree', 'remove', $worktreePath)
                    Write-TimingTrace "queue.publish: pr-worktree-remove done (exit=$($rmwt.Code))"
                    if ($rmwt.Code -eq 0) {
                        $worktreeDisposition = 'removed'
                        $script:DispatchWorktreeRoot = $null
                    } else {
                        $publishDetail += "`nWARNING: PR opened, but could not remove worktree '$worktreePath' (exit $($rmwt.Code)): $($rmwt.Text). Inspect and remove it manually."
                        $worktreeDisposition = 'preserved'
                    }
                } else {
                    $publishFailed     = $true
                    $publishHardFailed = $true
                    $publishDetail     = "$($prMeta.FailureReason)`nBranch was pushed to origin and gh pr create reported success, but PR metadata is incomplete; $branch kept locally for human recovery."
                    Write-TimingTrace "queue.publish: pr-metadata-incomplete ($($prMeta.FailureReason))"
                }
            }
        }
        Write-TimingTrace "queue.publish: block-exit (published=$published, publishFailed=$publishFailed, publishHardFailed=$publishHardFailed, mode=pr)"
    } elseif ($eligibleForPublish -and $script:ResolvedPublishMode -eq 'branch') {
        $publishDetail = "branch mode set; kept $branch local."
        Write-TimingTrace "queue.publish: skipped (mode=branch, eligibleForPublish=true)"
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
            switch ($script:ResolvedPublishMode) {
                'pr' {
                    # PR-mode $published=true is gated on both $prNumber and
                    # $prUrl by the publish block above, so this branch can
                    # always emit the full reference. The structural guarantee
                    # is what the ISSUE-230 correction requires: PR-mode
                    # success is reported only with PR number AND URL.
                    "Pushed branch ``$branch`` (commit ``$commitSha``) to ``origin`` and opened pull request **#$prNumber**: $prUrl"
                }
                default {
                    "Published ``$publishedSha`` to ``origin/main`` from branch ``$branch``."
                }
            }
        } elseif ($script:ResolvedPublishMode -eq 'branch') {
            "Committed locally as ``$commitSha`` on branch ``$branch`` (branch mode; not pushed)."
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
        # (it holds the audit-log commit) AND the isolated worktree under
        # .attemptN so the retry can reuse both the branch name and the
        # worktree path, rather than destroying either. The branch and
        # worktree are archived in lockstep so a single attempt slot covers
        # both -- the branch's history and the worktree's working state are
        # both diagnostically useful and must travel together.
        $n = 1
        while (((Invoke-Tool -Exe 'git' -CmdArgs @('branch', '--list', "$branch.attempt$n")).Text.Trim()) -or
               (Test-Path -LiteralPath "$worktreePath.attempt$n")) { $n++ }
        $archiveBranch = "$branch.attempt$n"
        $archiveWorktree = "$worktreePath.attempt$n"
        # Rename the branch first; the linked worktree's HEAD follows the
        # rename automatically.
        $rename = Invoke-Tool -Exe 'git' -CmdArgs @('branch', '-m', $branch, $archiveBranch)
        if ($rename.Code -eq 0) {
            Write-Output "Archived failed branch $branch -> $archiveBranch."
        } else {
            Write-Output "WARNING: could not archive failed branch $branch (exit $($rename.Code)): $($rename.Text)"
        }
        # Move the worktree directory and update git's worktree registration
        # in one step. `git worktree move` refuses to clobber an existing
        # destination, so the .attemptN slot picked above must be free.
        $moveWt = Invoke-Tool -Exe 'git' -CmdArgs @('worktree', 'move', $worktreePath, $archiveWorktree)
        if ($moveWt.Code -eq 0) {
            Write-Output "Archived failed worktree '$worktreePath' -> '$archiveWorktree'."
            $worktreeDisposition  = 'archived'
            $worktreeArchivePath  = $archiveWorktree
            $script:DispatchWorktreeRoot = $null
        } else {
            Write-Output ("WARNING: could not archive failed worktree '$worktreePath' " +
                "(exit $($moveWt.Code)): $($moveWt.Text). The retry will collide on " +
                "the original worktree path until a human resolves it.")
            $worktreeDisposition = 'preserved'
        }
    }

    # --- Worktree disposition (terminal success, terminal failure) ---------
    # ISSUE-231: after the publish block and the retry archival, decide what
    # to do with any worktree that is still at its original path. The pure
    # `Test-DispatchWorktreeCleanupDecision` helper drives the call; the
    # queue performs the chosen action here. Terminal success in `branch`
    # mode (or any other path that did not already remove the worktree)
    # removes it now. Terminal failure / publish-pipeline failure paths
    # preserve it so a human can inspect, recover, or remove it manually.
    if (-not $worktreeDisposition -and (Test-Path -LiteralPath $worktreePath)) {
        $decision = Test-DispatchWorktreeCleanupDecision `
            -RunFailed $runFailed `
            -RunBlocked $runBlocked `
            -WillRetry $willRetry `
            -PublishHardFailed $publishHardFailed
        switch ($decision.Action) {
            'remove' {
                Write-TimingTrace "queue.worktree: cleanup-remove start"
                Copy-DispatchRunDirToPrimary -WorktreeRoot $worktreePath -PrimaryRoot $script:RepoRoot -DispatchId $id
                $rmwt = Invoke-Tool -Exe 'git' -CmdArgs @('worktree', 'remove', $worktreePath)
                Write-TimingTrace "queue.worktree: cleanup-remove done (exit=$($rmwt.Code))"
                if ($rmwt.Code -eq 0) {
                    $worktreeDisposition = 'removed'
                    $script:DispatchWorktreeRoot = $null
                } else {
                    Write-Output ("WARNING: could not remove worktree '$worktreePath' " +
                        "(exit $($rmwt.Code)): $($rmwt.Text). Inspect and remove it manually.")
                    $worktreeDisposition = 'preserved'
                }
            }
            'preserve' {
                $worktreeDisposition = 'preserved'
            }
            default {
                # 'archive' is handled inline by the retry path above; any
                # other state is treated as preserve so the worktree is not
                # silently dropped.
                $worktreeDisposition = 'preserved'
            }
        }
    }

    # Build the worktree status line for the result comment / local stdout.
    # Outside the disposition cases above (e.g. the worktree was never
    # created, or it was already removed during the publish block) the
    # status line stays empty so the comment does not carry a stale bullet.
    if ($worktreeDisposition) {
        $worktreeStatusLine = Format-DispatchWorktreeStatus `
            -Disposition $worktreeDisposition `
            -WorktreePath $worktreePath `
            -ArchivePath $worktreeArchivePath
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
    $footerLine = switch ($script:ResolvedPublishMode) {
        'branch' {
            '_Posted by Invoke-AiDispatchQueue.ps1 (branch mode): a passed run is committed to its branch for human review; nothing is auto-pushed._'
        }
        'pr' {
            '_Posted by Invoke-AiDispatchQueue.ps1 (PR mode): a passed run pushes its branch and opens a pull request targeting main; nothing is merged, origin/main is never pushed, and the source issue is not auto-closed._'
        }
        default {
            '_Posted by Invoke-AiDispatchQueue.ps1. Successful control-passed runs are auto-published to origin/main; failed or blocked runs remain local._'
        }
    }
    $worktreeBullet = if ($worktreeStatusLine) { "`n- $worktreeStatusLine" } else { '' }
    $commentBody = @"
**AI dispatch run $statusIcon** - dispatch ``$id``

- Loop exit code: ``$loopExit``
- Codex control verdict: ``$verdict``
- $branchLine
- Detailed log: ``$dispatchLogRel``$worktreeBullet$retryNote

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

    # Build the deterministic terminal label add/remove plan from already-
    # computed queue state. The helper is pure and side-effect-free; the queue
    # only consumes its output here, so retry eligibility, taxonomy
    # classification, publish behavior, branch archival, and scheduler/auto
    # behavior remain untouched.
    $knownFailureTaxonomyLabels = @($labelSpec |
        Where-Object { $_.Name -like 'ai-dispatch-failure-*' } |
        ForEach-Object { $_.Name })
    $labelPlan = Get-DispatchTerminalLabelPlan `
        -WillRetry $willRetry `
        -RunFailed $runFailed `
        -QueueLabel $QueueLabel `
        -RunLabel $runLabel `
        -DoneLabel $doneLabel `
        -FailLabel $failLabel `
        -RetryLabel $retryLabel `
        -TaxonomyLabels $taxonomyLabels `
        -KnownFailureTaxonomyLabels $knownFailureTaxonomyLabels
    $relabel = @('issue', 'edit', "$($issue.number)", '--repo', $repoSlug)
    foreach ($l in $labelPlan.Add)    { $relabel += @('--add-label', $l) }
    foreach ($l in $labelPlan.Remove) { $relabel += @('--remove-label', $l) }

    # Bounded, idempotent relabel with per-attempt verify + a REST fallback.
    # A partial gh edit (e.g. running removed but retry never added, or a stale
    # taxonomy label surviving a terminal success) would otherwise let the
    # autonomous driver loop forever or never halt. Re-issuing the SAME add/remove
    # plan is idempotent, so retrying is safe. `gh issue edit` uses GraphQL, which
    # can transiently 401 here even when REST works (see MEMORY dispatch-issue-
    # state-check), so the final attempt falls back to the REST labels endpoint.
    # A successful main-mode publish has ALREADY landed by this point, so we never
    # fail an already-published run on a label blip -- the stale-issue replay
    # guard at selection (published-SHA dedup) is the backstop that keeps a
    # mis-labelled, already-published issue from being re-dispatched.
    $labelOk = $false
    $maxRelabelAttempts = 3
    for ($attempt = 1; $attempt -le $maxRelabelAttempts -and -not $labelOk; $attempt++) {
        if ($attempt -gt 1) { Start-Sleep -Seconds ([Math]::Min(10, 2 * ($attempt - 1))) }
        Write-TimingTrace "queue.github: relabel attempt $attempt/$maxRelabelAttempts"
        if ($attempt -lt $maxRelabelAttempts) {
            $rl = Invoke-Tool -Exe 'gh' -CmdArgs $relabel
        } else {
            # Final attempt: REST fallback, bypassing the GraphQL mutation path.
            foreach ($l in $labelPlan.Add) {
                Invoke-Tool -Exe 'gh' -CmdArgs @(
                    'api', '--method', 'POST',
                    "repos/$repoSlug/issues/$($issue.number)/labels",
                    '-f', "labels[]=$l") | Out-Null
            }
            foreach ($l in $labelPlan.Remove) {
                Invoke-Tool -Exe 'gh' -CmdArgs @(
                    'api', '--method', 'DELETE',
                    "repos/$repoSlug/issues/$($issue.number)/labels/$l") | Out-Null
            }
            $rl = [pscustomobject]@{ Code = 0; Text = 'REST fallback labels applied' }
        }
        # Verify: every planned add present, every planned remove absent.
        $lv = Invoke-Tool -Exe 'gh' -CmdArgs @(
            'issue', 'view', "$($issue.number)", '--repo', $repoSlug, '--json', 'labels')
        if ($lv.Code -eq 0) {
            $nowLabels = @()
            try { $nowLabels = @(($lv.Text | ConvertFrom-Json).labels | ForEach-Object { $_.name }) } catch { }
            $labelOk = $true
            foreach ($l in $labelPlan.Add) {
                if ($nowLabels -notcontains $l) { $labelOk = $false; break }
            }
            if ($labelOk) {
                foreach ($l in $labelPlan.Remove) {
                    if ($nowLabels -contains $l) { $labelOk = $false; break }
                }
            }
        }
        Write-TimingTrace "queue.github: relabel attempt $attempt verify labelOk=$labelOk (exit=$($rl.Code))"
    }
    if (-not $labelOk) {
        Write-Output "WARNING: issue #$($issue.number) labels did not finalize to the expected set after $maxRelabelAttempts attempts (incl. REST fallback; last gh exit $($rl.Code)): $($rl.Text). The stale-issue replay guard (published-SHA dedup at selection) will keep a mis-labelled, already-published issue from being re-dispatched; inspect the issue's labels."
    }

    # Auto-close the source issue only after a successful `main`-mode publish.
    # `branch` mode and `pr` mode both leave issue closure to a human: branch
    # mode because the work is still awaiting human review/merge, and PR mode
    # because the human reviewer who merges the pull request also owns the
    # decision to close (or keep open) the source issue.
    if (-not $runFailed -and $script:ResolvedPublishMode -eq 'main') {
        $closeComment = if ($published) {
            "Auto-published to origin/main as $publishedSha. Detailed log: $dispatchLogRel"
        } else {
            "Dispatch completed with no committable changes. Detailed log: $dispatchLogRel"
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
    if (-not $runFailed -and $script:ResolvedPublishMode -eq 'main') {
        Write-Output "Issue #$($issue.number) closed after publish."
    }
    Write-Output "Loop log: $loopLog"
    if ($worktreeStatusLine) {
        Write-Output ($worktreeStatusLine -replace '`', '')
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
