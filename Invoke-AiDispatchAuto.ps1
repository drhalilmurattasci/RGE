#Requires -Version 5.1
<#
.SYNOPSIS
    Autonomous AI dispatch driver: Codex selects the next task, the hardened
    dispatch queue runs it. One task per invocation -- schedule it for a
    continuous, self-restarting loop.

.DESCRIPTION
    This is the "Codex decides what to do" layer on top of
    Invoke-AiDispatchQueue.ps1. Each tick:

      1. Halt check  - if any prior autonomous task carries 'ai-dispatch-
                       failed', stop and do nothing until a human clears it.
      2. Cap check   - stop once -MaxAutonomousTasks 'ai-auto' issues exist,
                       so a human reviews each batch before more run.
      3. Select      - when no 'ai-dispatch' issue is pending, Codex reads the
                       task brief (.ai/dispatch.tasks.md), picks the next
                       task, and a GitHub issue is filed for it (labels
                       'ai-dispatch' + 'ai-auto'). Codex picks the WHAT; the
                       issue is an internal record, not a human gate.
      4. Run         - Invoke-AiDispatchQueue.ps1 runs the pending issue
                       through the full hardened path: Codex plan -> Claude
                       gate -> selected executor -> verification gate ->
                       Codex control -> publish. The default executor remains
                       Claude; `-Executor codex` is an explicit opt-in.

    -PublishMode chooses what happens to a passed task:
      pr (default)    - the queue pushes the dispatch branch and opens a
                        GitHub pull request targeting main. Nothing is merged
                        or pushed to origin/main, and the source issue is not
                        auto-closed -- the human reviewer who merges the PR
                        also owns issue closure.
      branch          - work stays on its ai-dispatch/ISSUE-* branch and the
                        issue stays open; a human reviews and merges it.
      main            - the queue fast-forwards origin/main automatically
                        (explicit opt-in for delegated-human auto-publish
                        batches; never the unattended default).

    The loop is INERT until .ai/dispatch.tasks.md is populated with real
    tasks; an empty or instructions-only brief selects nothing.

.PARAMETER PublishMode
    'pr' (default, human reviews and merges the PR), 'branch' (human-gated
    publish on the local branch), or 'main' (explicit opt-in auto-publish).

.PARAMETER MaxAutonomousTasks
    Halt for human review once this many 'ai-auto' issues exist. Default 5.
    Raise it (or re-register the schedule with a higher value) to continue.

.PARAMETER TaskBrief
    Path to the task-selection brief. Default .ai/dispatch.tasks.md.

.PARAMETER DryRun
    Report the halt/cap state and the task Codex would select; create no
    issue and run no dispatch.

.PARAMETER TraceTiming
    Emit timing trace lines for automation phase diagnosis. Can also be enabled
    by setting RGE_AI_DISPATCH_TRACE_TIMING=1.

.EXAMPLE
    .\Invoke-AiDispatchAuto.ps1 -DryRun
    .\Invoke-AiDispatchAuto.ps1                      # pr mode (default)
    .\Invoke-AiDispatchAuto.ps1 -PublishMode branch  # human-gated branch mode
    .\Invoke-AiDispatchAuto.ps1 -PublishMode main    # auto-publish mode (opt-in)

.NOTES
    Requires git, gh (authenticated), codex, powershell.exe, and
    Invoke-AiDispatchQueue.ps1 in the repo root.
#>
[CmdletBinding()]
param(
    [ValidateSet('branch', 'main', 'pr')]
    [string]$PublishMode = 'pr',

    [ValidateRange(1, 200)]
    [int]$MaxAutonomousTasks = 5,

    [string]$TaskBrief = '',

    [ValidateRange(0, 5)]
    [int]$MaxPlanRevisions = 1,

    [ValidateRange(0, 5)]
    [int]$MaxCorrectionRounds = 2,

    [ValidateSet('claude', 'codex')]
    [string]$Executor = 'claude',

    [switch]$DryRun,

    [switch]$TraceTiming,

    [switch]$EnablePreflightAudit
)

$ErrorActionPreference = 'Stop'

$script:TraceTimingEnabled = [bool]$TraceTiming -or ($env:RGE_AI_DISPATCH_TRACE_TIMING -match '^(1|true|yes|on)$')
$script:TraceTimingStopwatch = [System.Diagnostics.Stopwatch]::StartNew()
$script:TraceTimingScriptLeaf = 'Invoke-AiDispatchAuto.ps1'
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

function Fail {
    param([string]$Message)
    [Console]::Error.WriteLine($Message)
    exit 1
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

function Invoke-Tool {
    # Run a native command with PS 5.1 EAP isolation (native stderr under
    # EAP=Stop becomes a terminating error). Returns @{ Code; Text }.
    param([string]$Exe, [string[]]$CmdArgs)
    $tmp = [System.IO.Path]::GetTempFileName()
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & $Exe @CmdArgs > $tmp 2>&1
    } finally {
        $ErrorActionPreference = $prev
    }
    $code = $LASTEXITCODE
    $text = (Get-Content -Raw -LiteralPath $tmp -ErrorAction SilentlyContinue)
    if ($null -eq $text) { $text = '' }
    Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
    return [pscustomobject]@{ Code = $code; Text = $text }
}

function Get-IssuesJson {
    # gh issue list --json ... -> array. Two PS 5.1 unrolling gotchas:
    #   1. ConvertFrom-Json may yield a single non-enumerated object for a JSON
    #      array, so the parse result is wrapped with @(...).
    #   2. `return $array` enumerates into the output stream, so a single-
    #      element array unrolls to its scalar at the call site. The scalar
    #      (a PSCustomObject) has no synthetic .Count in PS 5.1, so
    #      `$queue.Count -gt 0` evaluates `$null -gt 0` = $false and a
    #      single queued issue is misdetected as an empty queue. The comma
    #      operator on return prevents enumeration -- `return ,$items` emits
    #      the array itself as one stream object, so the caller always sees
    #      an array regardless of element count.
    param([string[]]$GhArgs)
    $r = Invoke-Tool -Exe 'gh' -CmdArgs $GhArgs
    if ($r.Code -ne 0) {
        Fail "gh issue list failed (exit $($r.Code)):`n$($r.Text)"
    }
    $items = @()
    if ($r.Text -and $r.Text.Trim()) {
        try { $parsed = $r.Text | ConvertFrom-Json }
        catch { Fail "Could not parse gh issue JSON: $($_.Exception.Message)" }
        if ($null -ne $parsed) { $items = @($parsed) }
    }
    return ,$items
}

function Get-OpenQueueIssuesRest {
    # Independent fallback for the queue-presence check. gh issue list can
    # intermittently report an empty label search while the REST issues
    # endpoint still sees the queued issue. Return Code/Text so callers can
    # fail closed instead of selecting fresh work on an ambiguous queue state.
    param([string]$RepoSlug, [string]$QueueLabel)
    $encodedLabel = [System.Uri]::EscapeDataString($QueueLabel)
    $endpoint = "repos/$RepoSlug/issues?state=open&labels=$encodedLabel&per_page=100"
    $r = Invoke-Tool -Exe 'gh' -CmdArgs @('api', $endpoint)
    $items = @()
    if ($r.Code -eq 0 -and $r.Text -and $r.Text.Trim()) {
        try { $parsed = $r.Text | ConvertFrom-Json }
        catch {
            return [pscustomobject]@{
                Code = 1
                Text = "Could not parse gh api issue JSON: $($_.Exception.Message)"
                Items = @()
            }
        }
        if ($null -ne $parsed) {
            $items = @($parsed | Where-Object { -not $_.pull_request })
        }
    }
    return [pscustomobject]@{ Code = $r.Code; Text = $r.Text; Items = $items }
}

function Get-BlockText {
    # Extract the text between two sentinel lines from free-form model output.
    # Last occurrence wins, so a sentinel echoed in reasoning cannot mask the
    # real answer block.
    param([string]$Text, [string]$BeginMark, [string]$EndMark)
    $pattern = [regex]::Escape($BeginMark) + '(.*?)' + [regex]::Escape($EndMark)
    $blocks = [regex]::Matches([string]$Text, $pattern,
        [System.Text.RegularExpressions.RegexOptions]::Singleline)
    if ($blocks.Count -gt 0) {
        return $blocks[$blocks.Count - 1].Groups[1].Value.Trim()
    }
    return ''
}

function Get-RecoveryDecision {
    # Pure decision helper for one-shot transient recovery. Given the list of
    # OPEN failed autonomous issues plus the label set this loop uses, return
    # the eligibility verdict and the exact intended label transition. No
    # GitHub side effects, so the same function is callable from a non-mutating
    # verification harness with hand-crafted inputs.
    #
    # Eligibility (fail-closed by default) requires ALL of:
    #   - exactly one open failed autonomous issue,
    #   - it has no 'ai-dispatch-recovered-transient' marker,
    #   - it has exactly one ai-dispatch-failure-* taxonomy label,
    #   - and that taxonomy label is one of the explicit transient labels.
    param(
        [object[]]$Issues,
        [string]$FailLabel,
        [string]$QueueLabel,
        [string]$DoneLabel,
        [string]$RetryLabel,
        [string]$RecoverLabel,
        [string[]]$TransientLabels
    )
    $decision = [pscustomobject]@{
        Eligible       = $false
        Reason         = ''
        Issue          = $null
        TransientLabel = $null
        LabelsToRemove = @()
        LabelsToAdd    = @()
    }
    $list = @($Issues)
    if ($list.Count -eq 0) {
        $decision.Reason = 'no open failed autonomous issues'
        return $decision
    }
    if ($list.Count -gt 1) {
        $decision.Reason = "$($list.Count) open failed autonomous issues (recovery requires exactly one)"
        return $decision
    }
    $cand = $list[0]
    $labels = @()
    if ($cand.labels) {
        $labels = @($cand.labels | ForEach-Object {
            if ($_ -is [string]) { $_ } else { $_.name }
        })
    }
    $taxonomy     = @($labels | Where-Object { $_ -like 'ai-dispatch-failure-*' })
    $transient    = @($labels | Where-Object { $TransientLabels -contains $_ })
    $nonTransient = @($taxonomy | Where-Object { $TransientLabels -notcontains $_ })
    $alreadyRecovered = ($labels -contains $RecoverLabel)

    if ($alreadyRecovered) {
        $decision.Reason = "issue #$($cand.number) already carries '$RecoverLabel'"
        return $decision
    }
    if ($taxonomy.Count -eq 0) {
        $decision.Reason = "issue #$($cand.number) has no failure taxonomy label"
        return $decision
    }
    if ($nonTransient.Count -gt 0) {
        $decision.Reason = "issue #$($cand.number) has non-transient taxonomy label(s): " + ($nonTransient -join ', ')
        return $decision
    }
    if ($taxonomy.Count -gt 1) {
        $decision.Reason = "issue #$($cand.number) has multiple taxonomy labels: " + ($taxonomy -join ', ')
        return $decision
    }
    if ($transient.Count -ne 1) {
        $decision.Reason = "issue #$($cand.number) has no transient taxonomy label"
        return $decision
    }

    $remove = @($FailLabel)
    if ($labels -contains $DoneLabel) { $remove += $DoneLabel }
    $add = @($QueueLabel, $RetryLabel, $RecoverLabel)

    $decision.Eligible       = $true
    $decision.Issue          = $cand
    $decision.TransientLabel = $transient[0]
    $decision.LabelsToRemove = $remove
    $decision.LabelsToAdd    = $add
    return $decision
}

function Get-PostRecoveryQueueState {
    # After a successful recovery label mutation, decide what queue state this
    # tick should use given whether the relabeled issue is visible under the
    # queue label search. Pure function so the verdict is exercisable from a
    # non-mutating verification harness.
    #
    #   'Drain'   -> Seed openQueue with the recovered issue and SKIP the
    #                primary queue label re-fetch this tick. A stale empty
    #                label-index read on the same recovered issue must not
    #                route the tick into new task selection.
    #   'EndTick' -> Visibility never confirmed within the poll budget. The
    #                caller MUST exit 0 immediately, before any cap check,
    #                Codex task selection, gh issue create, or queue
    #                invocation can run. A later tick will drain the issue
    #                once GitHub's label index catches up.
    param(
        [Parameter(Mandatory)] $RecoveredIssue,
        [bool] $VisibilityConfirmed,
        [int]  $VisibilityElapsedSeconds = 0
    )
    if ($VisibilityConfirmed) {
        $seed = @([pscustomobject]@{
            number = $RecoveredIssue.number
            title  = $RecoveredIssue.title
        })
        return [pscustomobject]@{
            Action      = 'Drain'
            SeededQueue = $seed
            ElapsedSecs = $VisibilityElapsedSeconds
            Reason      = "Recovered issue #$($RecoveredIssue.number) is listable after ${VisibilityElapsedSeconds}s; seeding queue from the recovery result and skipping the label re-fetch this tick."
        }
    }
    return [pscustomobject]@{
        Action      = 'EndTick'
        SeededQueue = @()
        ElapsedSecs = $VisibilityElapsedSeconds
        Reason      = "Recovered issue #$($RecoveredIssue.number) not visible to queue label search after ${VisibilityElapsedSeconds}s; ending this tick to avoid filing new work on top of an unconfirmed recovery. A later tick will drain it."
    }
}

$script:AutoLockPath = Join-Path $env:TEMP 'rge-ai-dispatch-auto.lock'
$script:AutoLockHeld = $false

function Release-AutoLock {
    if ($script:AutoLockHeld) {
        Remove-Item -LiteralPath $script:AutoLockPath -Force -ErrorAction SilentlyContinue
        $script:AutoLockHeld = $false
    }
}

function Acquire-AutoLock {
    # Atomically create the auto-driver lock (FileMode.CreateNew fails if it
    # already exists) so two ticks cannot both select and file the same task.
    # A stale lock whose owner process is gone is replaced; a live owner means
    # skip this tick.
    $ownerStart = [long]0
    $self = Get-Process -Id $PID -ErrorAction SilentlyContinue
    if ($self) { try { $ownerStart = [long]$self.StartTime.Ticks } catch { } }
    $content = "pid=$PID procstart=$ownerStart at=$((Get-Date).ToString('o'))"
    for ($attempt = 0; $attempt -lt 2; $attempt++) {
        try {
            $fs = [System.IO.File]::Open($script:AutoLockPath,
                [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write,
                [System.IO.FileShare]::None)
            try {
                $bytes = [System.Text.Encoding]::UTF8.GetBytes($content)
                $fs.Write($bytes, 0, $bytes.Length)
            } finally { $fs.Close() }
            $script:AutoLockHeld = $true
            return $true
        } catch [System.IO.IOException] {
            $raw = (Get-Content -Raw -LiteralPath $script:AutoLockPath -ErrorAction SilentlyContinue)
            $lpid = 0
            $lstart = [long]0
            if ($raw -match 'pid=(\d+)')       { $lpid = [int]$matches[1] }
            if ($raw -match 'procstart=(\d+)') { $lstart = [long]$matches[1] }
            $alive = $false
            if ($lpid -gt 0) {
                $lp = Get-Process -Id $lpid -ErrorAction SilentlyContinue
                if ($lp) {
                    try { $alive = ($lstart -eq 0) -or ($lp.StartTime.Ticks -eq $lstart) }
                    catch { $alive = $true }
                }
            }
            if ($alive) { return $false }
            Remove-Item -LiteralPath $script:AutoLockPath -Force -ErrorAction SilentlyContinue
        }
    }
    return $false
}

function New-AutoQueueArguments {
    # Build the exact child powershell.exe argument vector Auto uses to invoke
    # Invoke-AiDispatchQueue.ps1. Pure helper: no process launch, no gh, no
    # codex, no labels, and no network. Tests use it to dry-run the delegated
    # executor + publish posture without starting the autonomous loop.
    [CmdletBinding()]
    [OutputType([string[]])]
    param(
        [Parameter(Mandatory = $true)]
        [string]$QueueScript,

        [ValidateSet('branch', 'main', 'pr')]
        [string]$PublishMode = 'pr',

        [ValidateRange(0, 5)]
        [int]$MaxPlanRevisions = 1,

        [ValidateRange(0, 5)]
        [int]$MaxCorrectionRounds = 2,

        [ValidateSet('claude', 'codex')]
        [string]$Executor = 'claude',

        [bool]$TraceTiming = $false,

        [bool]$EnablePreflightAudit = $false
    )

    $args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $QueueScript,
        '-MaxPlanRevisions', $MaxPlanRevisions,
        '-MaxCorrectionRounds', $MaxCorrectionRounds,
        '-Executor', $Executor)
    switch ($PublishMode) {
        'branch' { $args += '-NoPublish' }
        'main'   { $args += @('-PublishMode', 'main') }
        'pr'     { $args += @('-PublishMode', 'pr') }
    }
    if ($TraceTiming) { $args += '-TraceTiming' }
    if ($EnablePreflightAudit) { $args += '-EnablePreflightAudit' }
    return ,$args
}

# --- Environment -----------------------------------------------------------

if ($env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -eq '1') {
    return
}

$script:RepoRoot = $PSScriptRoot
Set-Location -LiteralPath $script:RepoRoot

Require-Command git
Require-Command gh
Require-Command codex
Require-Command powershell.exe

$queueScript = Join-Path $script:RepoRoot 'Invoke-AiDispatchQueue.ps1'
if (-not (Test-Path -LiteralPath $queueScript)) {
    Fail "Dispatch queue script not found: $queueScript"
}

$briefPath = if ($TaskBrief) {
    if ([System.IO.Path]::IsPathRooted($TaskBrief)) { $TaskBrief }
    else { Join-Path $script:RepoRoot $TaskBrief }
} else {
    Join-Path $script:RepoRoot '.ai\dispatch.tasks.md'
}

$auth = Invoke-Tool -Exe 'gh' -CmdArgs @('auth', 'status')
if ($auth.Code -ne 0) {
    Fail "gh is not authenticated. Run 'gh auth login' first.`n$($auth.Text)"
}

$originUrl = (Invoke-Tool -Exe 'git' -CmdArgs @('remote', 'get-url', 'origin')).Text.Trim()
if ($originUrl -notmatch 'github\.com[:/](.+?)(?:\.git)?/?$') {
    Fail "Could not parse an owner/name slug from origin URL: $originUrl"
}
$repoSlug = $matches[1]

$queueLabel = 'ai-dispatch'
$autoLabel  = 'ai-auto'
$failLabel  = 'ai-dispatch-failed'

Write-Output "Autonomous dispatch tick - repo $repoSlug"
Write-TimingTrace "auto.tick: start (PID=$PID, repo=$repoSlug, mode=$PublishMode)"
Write-Output "Publish mode: $PublishMode   Task cap: $MaxAutonomousTasks"

# Serialize autonomous ticks: without this, two overlapping ticks could both
# see an empty queue and both file the same Codex-selected task.
if (-not $DryRun) {
    if (-not (Acquire-AutoLock)) {
        Write-Output "Another autonomous dispatch tick is already running; skipping this tick."
        Write-TimingTrace "auto.tick: end (exit=0, skipped=lock-held)"
        exit 0
    }
}

try {
# --- 1. Halt checks --------------------------------------------------------

Write-TimingTrace "auto.halt-checks: start"
$haltSentinel = Join-Path $script:RepoRoot '.ai\dispatch.auto-halt'
if (Test-Path -LiteralPath $haltSentinel) {
    Write-Output ''
    Write-Output "HALTED: a prior tick recorded a fault in $haltSentinel."
    $haltText = (Get-Content -Raw -LiteralPath $haltSentinel -ErrorAction SilentlyContinue)
    if ($haltText) { Write-Output "  $($haltText.Trim())" }
    Write-Output "Investigate, then delete that file to resume."
    Write-TimingTrace "auto.halt-checks: halted (sentinel=$haltSentinel)"
    Write-TimingTrace "auto.tick: end (exit=0, halted=true)"
    exit 0
}

# --- 1b. One-shot transient recovery ---------------------------------------
# Narrow Auto-layer repair hook: when the only thing blocking the loop is a
# single open autonomous issue whose terminal failure taxonomy is clearly
# transient (stall or timeout), requeue it ONCE. The 'ai-dispatch-recovered-
# transient' marker guarantees this is a one-shot per issue; the original
# taxonomy label is kept as audit evidence. Every other ineligible state --
# closed failures, multiple failed issues, mixed taxonomy, non-transient
# taxonomy, missing taxonomy, already-recovered -- falls through to the
# existing human-review halt below. Recovery never runs ahead of the local
# sentinel check above.

$recoverLabel    = 'ai-dispatch-recovered-transient'
$retryLabel      = 'ai-dispatch-retry'
$doneLabel       = 'ai-dispatch-done'
$transientLabels = @('ai-dispatch-failure-stall', 'ai-dispatch-failure-timeout')

# Set when a successful recovery mutation seeds $openQueue from the recovered
# issue. The queue-check below MUST honour this flag and skip the primary
# label re-fetch -- a stale empty result on the same issue must not let the
# rest of the tick route into new task selection.
$script:RecoveryDrainSeeded = $false

Write-TimingTrace "auto.recovery-check: start"
$openFailedAuto = Get-IssuesJson @(
    'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
    '--label', $failLabel, '--state', 'open', '--limit', '100',
    '--json', 'number,title,labels')
$decision = Get-RecoveryDecision -Issues $openFailedAuto `
    -FailLabel $failLabel -QueueLabel $queueLabel -DoneLabel $doneLabel `
    -RetryLabel $retryLabel -RecoverLabel $recoverLabel `
    -TransientLabels $transientLabels

if ($decision.Eligible) {
    $cand = $decision.Issue
    Write-Output ''
    Write-Output "Transient recovery candidate: open autonomous issue #$($cand.number) ('$($cand.title)') with '$($decision.TransientLabel)'."
    Write-Output ("  Remove labels: " + ($decision.LabelsToRemove -join ', '))
    Write-Output ("  Add labels:    " + ($decision.LabelsToAdd -join ', '))
    Write-Output ("  Keep label:    $($decision.TransientLabel) (audit evidence)")
    if ($DryRun) {
        Write-Output 'DryRun: no label mutation; queue not run for this recovery.'
        Write-TimingTrace "auto.recovery-check: dry-run eligible (issue=#$($cand.number), label=$($decision.TransientLabel))"
        Write-TimingTrace "auto.tick: end (exit=0, dry-run=true, recovery=eligible)"
        exit 0
    }
    # Ensure the recovery marker and retry labels exist before the edit. The
    # queue script also defines the retry label; recreating it with --force is
    # idempotent. The recover marker is owned by this Auto layer.
    Invoke-Tool -Exe 'gh' -CmdArgs @(
        'label', 'create', $recoverLabel, '--repo', $repoSlug,
        '--color', 'fbca04',
        '--description', 'AI dispatch one-shot transient recovery marker; do not remove',
        '--force') | Out-Null
    Invoke-Tool -Exe 'gh' -CmdArgs @(
        'label', 'create', $retryLabel, '--repo', $repoSlug,
        '--color', 'd4c5f9',
        '--description', 'AI dispatch re-queued for one retry',
        '--force') | Out-Null

    $editArgs = @('issue', 'edit', [string]$cand.number, '--repo', $repoSlug)
    foreach ($lbl in $decision.LabelsToRemove) { $editArgs += @('--remove-label', $lbl) }
    foreach ($lbl in $decision.LabelsToAdd)    { $editArgs += @('--add-label',    $lbl) }
    Write-TimingTrace "auto.recovery-mutate: start (issue=#$($cand.number))"
    $editResult = Invoke-Tool -Exe 'gh' -CmdArgs $editArgs
    Write-TimingTrace "auto.recovery-mutate: done (exit=$($editResult.Code))"
    if ($editResult.Code -ne 0) {
        Fail "Could not requeue recovered issue #$($cand.number) (exit $($editResult.Code)):`n$($editResult.Text)"
    }
    Write-Output "Issue #$($cand.number) requeued: '$failLabel' removed, '$recoverLabel' set, '$($decision.TransientLabel)' kept."

    # GitHub label index lag: queue label search may not see the relabeled
    # issue immediately. Poll until visibility confirms, then either seed
    # the queue from the recovery result (drain path) or end this tick.
    # The tick MUST NOT fall through to the cap check, Codex task selection,
    # gh issue create, or queue invocation when recovery succeeded -- a
    # stale empty label-index read on the same recovered issue must not let
    # the loop file new work in the same tick (Codex control round 0
    # finding for ISSUE-196).
    $visibilityElapsedSeconds = 0
    $visible = $false
    for ($poll = 1; $poll -le 12; $poll++) {
        Start-Sleep -Seconds 5
        $visibilityElapsedSeconds = $poll * 5
        $check = Get-IssuesJson @(
            'issue', 'list', '--repo', $repoSlug, '--label', $queueLabel,
            '--state', 'open', '--limit', '100', '--json', 'number')
        if (@($check | ForEach-Object { $_.number }) -contains [int]$cand.number) {
            $visible = $true
            break
        }
    }
    $postRecovery = Get-PostRecoveryQueueState `
        -RecoveredIssue $cand -VisibilityConfirmed $visible `
        -VisibilityElapsedSeconds $visibilityElapsedSeconds
    Write-Output $postRecovery.Reason
    Write-TimingTrace "auto.recovery-visibility: action=$($postRecovery.Action) elapsed=$($postRecovery.ElapsedSecs)s"
    if ($postRecovery.Action -eq 'EndTick') {
        Write-TimingTrace "auto.tick: end (exit=0, recovered=true, visibility=ambiguous)"
        exit 0
    }
    $openQueue = $postRecovery.SeededQueue
    $script:RecoveryDrainSeeded = $true
} elseif ($openFailedAuto.Count -gt 0) {
    Write-Output ''
    Write-Output "Transient recovery not eligible: $($decision.Reason)."
    Write-Output "Falling through to the human-review halt."
    Write-TimingTrace "auto.recovery-check: ineligible ($($decision.Reason))"
} else {
    Write-TimingTrace "auto.recovery-check: no open failed autonomous issues"
}

$failedAuto = Get-IssuesJson @(
    'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
    '--label', $failLabel, '--state', 'all', '--limit', '100',
    '--json', 'number,title')
if ($failedAuto.Count -gt 0) {
    $f = $failedAuto[0]
    Write-Output ''
    Write-Output "HALTED: autonomous task #$($f.number) ('$($f.title)') is marked '$failLabel'."
    Write-Output "Review it, then remove the '$failLabel' label to resume (closing the issue alone does not clear the halt)."
    Write-TimingTrace "auto.halt-checks: halted (issue=#$($f.number), label=$failLabel)"
    Write-TimingTrace "auto.tick: end (exit=0, halted=true)"
    exit 0
}
Write-TimingTrace "auto.halt-checks: done"

# --- 2. Is the queue already holding work? ---------------------------------
# Existing queued work is always drained. The task cap gates only the
# creation of NEW autonomous tasks, so an already-filed task is never
# stranded behind the cap.

if ($script:RecoveryDrainSeeded) {
    # Recovery confirmed visibility and seeded $openQueue from the recovery
    # result. A re-query here could return an intermittently stale empty
    # label-index read on the recovered issue and route the tick into new
    # task selection; skip it.
    Write-TimingTrace "auto.queue-check: skipped (recovery seeded queue, count=$($openQueue.Count))"
} else {
    Write-TimingTrace "auto.queue-check: primary start"
    $openQueue = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $queueLabel,
        '--state', 'open', '--limit', '100', '--json', 'number,title')
    Write-TimingTrace "auto.queue-check: primary done (count=$($openQueue.Count))"
}

$queueStateAmbiguous = $false
if ($openQueue.Count -eq 0) {
    # GitHub label search can occasionally report an empty queue even when the
    # queue runner can see an already-filed dispatch issue. Cross-check through
    # the REST issues endpoint immediately so a filed issue is not stranded
    # behind the autonomous task cap. If the cross-check itself fails, treat
    # queue state as ambiguous and skip this tick instead of selecting fresh
    # work or cap-halting on a possibly stale empty result.
    $restQueue = Get-OpenQueueIssuesRest -RepoSlug $repoSlug -QueueLabel $queueLabel
    if ($restQueue.Code -eq 0 -and @($restQueue.Items).Count -gt 0) {
        $count = @($restQueue.Items).Count
        Write-Output "Primary queue check returned empty, but REST issues check sees $count open '$queueLabel' issue(s); draining before cap check."
        $openQueue = @($restQueue.Items | ForEach-Object {
            [pscustomobject]@{
                number = $_.number
                title  = $_.title
            }
        })
    } elseif ($restQueue.Code -eq 0) {
        Write-Output "REST issues check confirms no open '$queueLabel' issues."
    } else {
        Write-Output "WARNING: REST issues queue cross-check failed (exit $($restQueue.Code)); queue state is ambiguous, so this tick will not select new work or cap-halt."
        $queueStateAmbiguous = $true
    }
}

if ($openQueue.Count -gt 0) {
    Write-Output "Queue already has $($openQueue.Count) pending '$queueLabel' issue(s); draining it, selecting nothing this tick."
} elseif ($queueStateAmbiguous) {
    Write-Output ''
    Write-Output "Queue state ambiguous after primary check and cross-check; skipping this autonomous tick without filing new work."
    Write-Output "A later tick will retry, or run Invoke-AiDispatchQueue.ps1 directly if a queued issue is visible."
    Write-TimingTrace "auto.tick: end (exit=0, skipped=queue-ambiguous)"
    exit 0
} else {
    # --- 3. Cap check (gates NEW task selection only) ----------------------

    Write-TimingTrace "auto.cap-check: start"
    $allAuto = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
        '--state', 'all', '--limit', '200', '--json', 'number')
    Write-TimingTrace "auto.cap-check: done (count=$($allAuto.Count), cap=$MaxAutonomousTasks)"
    if ($allAuto.Count -ge $MaxAutonomousTasks) {
        Write-Output ''
        Write-Output "HALTED for review: autonomous task cap reached ($($allAuto.Count) of $MaxAutonomousTasks). Queue is empty; nothing to drain."
        Write-Output "Re-run with a higher -MaxAutonomousTasks to continue."
        Write-TimingTrace "auto.tick: end (exit=0, halted=cap-reached)"
        exit 0
    }

    # --- 4. Select the next task with Codex --------------------------------

    if (-not (Test-Path -LiteralPath $briefPath)) {
        Write-Output ''
        Write-Output "No task brief at $briefPath - nothing to select. Create it to arm the loop."
        Write-TimingTrace "auto.tick: end (exit=0, skipped=no-brief)"
        exit 0
    }
    $brief = Get-Content -Raw -LiteralPath $briefPath
    if (-not $brief -or -not $brief.Trim()) {
        Write-Output "Task brief $briefPath is empty; nothing to select."
        Write-TimingTrace "auto.tick: end (exit=0, skipped=empty-brief)"
        exit 0
    }
    # Deterministic arming check: while the brief carries the UNARMED marker
    # the loop selects nothing -- no reliance on Codex interpreting prose.
    if ($brief -match '(?m)^\s*DISPATCH-TASKS-UNARMED\s*$') {
        Write-Output "Task brief $briefPath carries the DISPATCH-TASKS-UNARMED marker; the autonomous loop is not armed. Nothing selected."
        Write-TimingTrace "auto.tick: end (exit=0, skipped=brief-unarmed)"
        exit 0
    }

    $doneAuto = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
        '--state', 'all', '--limit', '200', '--json', 'number,title,state')
    $doneList = if ($doneAuto.Count -gt 0) {
        ($doneAuto | ForEach-Object { "- #$($_.number) [$($_.state)] $($_.title)" }) -join "`n"
    } else { '(none yet)' }

    $selectPrompt = @"
You are Planner / OpenAI Codex selecting the next task for an automated RGE
dispatch loop. Read only; do not edit any file.

TASK BRIEF (the authorized source of work):
---
$brief
---

AUTONOMOUS TASKS ALREADY FILED (do not repeat any of these):
$doneList

Choose exactly ONE next task to dispatch now. Prefer the brief's order, but
pick an earlier-in-dependency task first if it is a prerequisite ("sequence
necessity"). The task must be small, bounded, and independently shippable.

If the brief contains no real tasks yet (only instructions, placeholders, or
examples), or every real task is already filed/complete, respond with exactly
this single line and nothing else:
AUTO_SELECTION: none

Otherwise respond with exactly this block as the last thing in your reply:
<<<AUTO_TASK_BEGIN>>>
TITLE: <one concise imperative line, 70 chars or fewer>
BODY:
<2 to 8 lines: the goal, the in-scope files or areas, and the done-criteria.
This text becomes the dispatch goal that Codex plans and the selected executor executes.>
<<<AUTO_TASK_END>>>
"@

    $promptFile  = Join-Path $env:TEMP 'rge-ai-auto-select-prompt.txt'
    $codexLog    = Join-Path $env:TEMP 'rge-ai-auto-select.log'
    $codexAnswer = Join-Path $env:TEMP 'rge-ai-auto-select-answer.txt'
    Write-Utf8 $promptFile $selectPrompt
    Remove-Item -LiteralPath $codexAnswer -Force -ErrorAction SilentlyContinue

    Write-Output ''
    Write-Output 'Queue is empty; asking Codex to select the next task...'
    Write-TimingTrace "auto.select: codex start"
    # --output-last-message captures ONLY Codex's final message. Scanning the
    # full transcript instead would match the sentinel block echoed from this
    # very prompt and mistake the template placeholder for a real selection.
    $codexArgs = @('exec', '--cd', $script:RepoRoot, '--sandbox', 'read-only',
        '--output-last-message', $codexAnswer, '-')
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        Get-Content -Raw -LiteralPath $promptFile | & codex @codexArgs > $codexLog 2>&1
    } finally {
        $ErrorActionPreference = $prevEap
    }
    if ($LASTEXITCODE -ne 0) {
        Fail "codex exec (task selection) failed. See $codexLog"
    }
    Write-TimingTrace "auto.select: codex done (exit=$LASTEXITCODE)"
    $codexOut = (Get-Content -Raw -LiteralPath $codexAnswer -ErrorAction SilentlyContinue)
    if (-not $codexOut -or -not ([string]$codexOut).Trim()) {
        # Fallback only: no last-message file. The placeholder guard below
        # still protects against the echoed-prompt sentinel collision.
        $codexOut = (Get-Content -Raw -LiteralPath $codexLog -ErrorAction SilentlyContinue)
    }
    if ($null -eq $codexOut) { $codexOut = '' }

    $block = Get-BlockText -Text $codexOut -BeginMark '<<<AUTO_TASK_BEGIN>>>' -EndMark '<<<AUTO_TASK_END>>>'
    if (-not $block) {
        if ($codexOut -match '(?im)^\s*AUTO_SELECTION:\s*none\b') {
            Write-Output 'Codex reports no real task to select (brief empty/placeholder, or all tasks done).'
            Write-TimingTrace "auto.tick: end (exit=0, skipped=no-selection)"
            exit 0
        }
        Fail "Codex did not return a parseable task block. See $codexLog"
    }
    # Suffix-anchor: the task block must be the very end of Codex's reply, not
    # something quoted earlier in its reasoning.
    if (([string]$codexOut).TrimEnd() -notmatch '<<<AUTO_TASK_END>>>\s*$') {
        Fail "Codex's task block is not at the end of its reply (suspect quoted/echoed text). See $codexLog"
    }

    $titleMatch = [regex]::Match($block, '(?im)^\s*TITLE:\s*(.+?)\s*$')
    if (-not $titleMatch.Success) {
        Fail "Codex task block has no TITLE line. See $codexLog"
    }
    $taskTitle = $titleMatch.Groups[1].Value.Trim()
    $bodyMatch = [regex]::Match($block, '(?is)\bBODY:\s*(.+)$')
    $taskBody = if ($bodyMatch.Success) { $bodyMatch.Groups[1].Value.Trim() } else { $taskTitle }

    # Guard: reject a prompt-template placeholder echoed back instead of a real
    # selection (e.g. a value still wrapped in <angle brackets>).
    if (-not $taskTitle -or
        ($taskTitle.StartsWith('<') -and $taskTitle.EndsWith('>')) -or
        ($taskBody.StartsWith('<') -and $taskBody.EndsWith('>'))) {
        Fail "Codex returned a prompt placeholder, not a real task selection. See $codexLog`nThe task brief probably has no real tasks yet."
    }

    Write-Output ''
    Write-Output 'Codex selected:'
    Write-Output "  Title: $taskTitle"

    if ($DryRun) {
        Write-Output ''
        Write-Output '--- task body ---'
        Write-Output $taskBody
        Write-Output '--- end ---'
        Write-Output ''
        Write-Output 'DryRun: no issue created, queue not run.'
        Write-TimingTrace "auto.tick: end (exit=0, dry-run=true)"
        exit 0
    }

    # Ensure both labels exist (idempotent), then file the task issue.
    Invoke-Tool -Exe 'gh' -CmdArgs @(
        'label', 'create', $queueLabel, '--repo', $repoSlug,
        '--color', '0e8a16', '--description', 'Queued for the AI dispatch loop',
        '--force') | Out-Null
    Invoke-Tool -Exe 'gh' -CmdArgs @(
        'label', 'create', $autoLabel, '--repo', $repoSlug,
        '--color', '1d76db', '--description', 'Task selected by the autonomous dispatch driver',
        '--force') | Out-Null

    $briefName = Split-Path -Leaf $briefPath
    $issueBody = "$taskBody`r`n`r`n_Filed automatically by Invoke-AiDispatchAuto.ps1 - Codex-selected from $briefName._"
    $bodyFile = Join-Path $env:TEMP 'rge-ai-auto-issue-body.txt'
    Write-Utf8 $bodyFile $issueBody
    Write-TimingTrace "auto.issue-create: start"
    $created = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'create', '--repo', $repoSlug, '--title', $taskTitle,
        '--body-file', $bodyFile, '--label', $queueLabel, '--label', $autoLabel)
    Write-TimingTrace "auto.issue-create: done (exit=$($created.Code))"
    Remove-Item -LiteralPath $bodyFile -Force -ErrorAction SilentlyContinue
    if ($created.Code -ne 0) {
        Fail "Could not create the autonomous task issue (exit $($created.Code)):`n$($created.Text)"
    }
    Write-Output "Filed autonomous task issue: $($created.Text.Trim())"

    # GitHub's label index lags issue creation by a few seconds: gh issue
    # create returns immediately, but gh issue list --label may not see the
    # new issue yet. Wait until it is listable, or the queue (which selects by
    # label) would see an empty queue and no-op this whole tick.
    $newIssueNum = 0
    if ($created.Text -match '/(\d+)\s*$') { $newIssueNum = [int]$matches[1] }
    if ($newIssueNum -gt 0) {
        $visible = $false
        for ($poll = 1; $poll -le 12; $poll++) {
            Start-Sleep -Seconds 5
            $check = Get-IssuesJson @(
                'issue', 'list', '--repo', $repoSlug, '--label', $queueLabel,
                '--state', 'open', '--limit', '100', '--json', 'number')
            if (@($check | ForEach-Object { $_.number }) -contains $newIssueNum) {
                $visible = $true
                Write-Output "Issue #$newIssueNum is listable after $($poll * 5)s; running the queue."
                break
            }
        }
        if (-not $visible) {
            Write-Output "WARNING: issue #$newIssueNum not listable after 60s; the queue may no-op this tick (a later tick will pick it up)."
        }
    }
}

if ($DryRun) {
    Write-Output ''
    Write-Output 'DryRun: queue not run.'
    Write-TimingTrace "auto.tick: end (exit=0, dry-run=true)"
    exit 0
}

# --- 5. Drain: run the hardened queue on the pending issue -----------------

Write-Output ''
Write-Output "Running the dispatch queue ($PublishMode mode)..."
Write-TimingTrace "auto.tick: queue-invocation start"
Write-Output '================================================================'
$queueArgs = New-AutoQueueArguments -QueueScript $queueScript -PublishMode $PublishMode `
    -MaxPlanRevisions $MaxPlanRevisions -MaxCorrectionRounds $MaxCorrectionRounds `
    -Executor $Executor -TraceTiming ([bool]$TraceTiming) `
    -EnablePreflightAudit ([bool]$EnablePreflightAudit)

$prevEap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$global:LASTEXITCODE = 0
try {
    & powershell.exe @queueArgs
} finally {
    $ErrorActionPreference = $prevEap
}
$queueExit = $LASTEXITCODE
Write-Output '================================================================'
Write-TimingTrace "auto.tick: queue-invocation done (exit=$queueExit)"
Write-Output "Dispatch queue exited with code $queueExit."
if ($queueExit -ne 0) {
    # A non-zero queue exit means the tick could not be cleanly finalized
    # (e.g. a terminal failure that could not be labelled). Record a durable
    # halt so the next scheduled tick does not barrel on.
    Write-Utf8 $haltSentinel "Autonomous loop halted: dispatch queue tick exited $queueExit at $((Get-Date).ToString('o')). Investigate, then delete this file to resume."
    Write-Output "Wrote halt sentinel $haltSentinel; the autonomous loop is paused until you delete it."
}
switch ($PublishMode) {
    'branch' { Write-Output 'Branch mode: a passed task stays on its ai-dispatch/ISSUE-* branch for you to review and merge.' }
    'pr'     { Write-Output 'PR mode: a passed task pushes its branch to origin and opens a pull request targeting main; the source issue is not auto-closed.' }
    default  { Write-Output 'Main mode: a passed task was fast-forwarded onto origin/main.' }
}
Write-TimingTrace "auto.tick: end (exit=$queueExit)"
exit $queueExit
} finally {
    Release-AutoLock
}
