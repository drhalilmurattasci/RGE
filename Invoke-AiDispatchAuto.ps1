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
      2. Cap check   - continuity: the binding cap counts only OPEN 'ai-auto'
                       issues (-MaxAutonomousTasks = open-backlog ceiling), so
                       completed work never saturates a lifetime wall. A
                       periodic seatbelt (-SeatbeltInterval) still pauses for
                       human review every N new tasks.
      3. Select      - when no 'ai-dispatch' issue is pending, Codex reads the
                       task brief (.ai/dispatch.tasks.md), picks the next
                       task, and a GitHub issue is filed for it (labels
                       'ai-dispatch' + 'ai-auto'). Codex picks the WHAT; the
                       issue is an internal record, not a human gate.
      4. Run         - Invoke-AiDispatchQueue.ps1 runs the pending issue
                       through the full hardened path: Codex plan -> selected
                       executor gate -> selected executor -> verification gate
                       -> Codex control -> publish. The default executor is
                       Codex; `-Executor claude` is an explicit opt-in.

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
    Open-backlog ceiling: halt if this many OPEN 'ai-auto' issues exist at once
    (a stuck-publish guard, not a lifetime cap). Default 5. Completed dispatches
    close their issue, so this rarely binds.

.PARAMETER SeatbeltInterval
    Forced human-review checkpoint: pause the loop (write the halt sentinel and
    file a 'needs-human' review issue) every this many NEW 'ai-auto' tasks.
    Default 50. The window is tracked in .ai/dispatch.auto-seatbelt.json.

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

    [ValidateRange(1, 1000)]
    [int]$SeatbeltInterval = 50,

    [string]$TaskBrief = '',

    [ValidateRange(0, 5)]
    [int]$MaxPlanRevisions = 2,

    [ValidateRange(0, 5)]
    [int]$MaxCorrectionRounds = 2,

    [ValidateSet('claude', 'codex')]
    [string]$Executor = 'codex',

    [switch]$CodexExecutorExternalScratch,

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

function New-NeedsHumanIssue {
    # Idempotently surface a human-review boundary as a GitHub issue. Used by
    # both the periodic seatbelt and the NEEDS_HUMAN self-re-arm boundary. The
    # issue carries ONLY the 'needs-human' label -- never 'ai-dispatch' or
    # 'ai-auto', which would feed it back into the queue/selector. Dedup: if a
    # 'needs-human' issue is already open, file nothing (the loop is paused
    # either way, so one open review issue at a time is correct).
    param([string]$RepoSlug, [string]$Title, [string]$Body)
    $label = 'needs-human'
    $script:LastNeedsHumanFiled = $false
    # Dedup: skip if a 'needs-human' issue is already open. gh-failure-tolerant
    # on purpose -- callers (seatbelt fire, idle hook) rely on this NOT throwing,
    # so a transient list error must not crash the tick. Get-IssuesJson would
    # Fail/exit on a gh non-zero; use Invoke-Tool and, on error, warn and fall
    # through to attempt filing rather than aborting the tick mid-finalize.
    $listed = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'list', '--repo', $RepoSlug, '--label', $label,
        '--state', 'open', '--limit', '20', '--json', 'number,title')
    if ($listed.Code -eq 0 -and $listed.Text -and $listed.Text.Trim()) {
        try {
            $openIssues = @($listed.Text | ConvertFrom-Json)
            if ($openIssues.Count -gt 0) {
                # The label SEARCH index lags issue-state changes (the same lag the
                # queue works around with Get-OpenQueueIssuesRest). A recently-closed
                # needs-human issue can still appear here as "open", which would
                # wrongly SKIP filing and FALSELY set LastNeedsHumanFiled -- the exact
                # bug that left three real NEEDS_HUMAN pauses with no review issue.
                # Verify the issue is ACTUALLY open via the REST issue-view before
                # trusting the dedup; if it is stale, fall through and file.
                $num = [int]$openIssues[0].number
                $view = Invoke-Tool -Exe 'gh' -CmdArgs @(
                    'issue', 'view', "$num", '--repo', $RepoSlug, '--json', 'state')
                $reallyOpen = $false
                if ($view.Code -eq 0 -and $view.Text) {
                    try { $reallyOpen = (([string]($view.Text | ConvertFrom-Json).state).ToUpperInvariant() -eq 'OPEN') } catch { $reallyOpen = $false }
                }
                if ($reallyOpen) {
                    Write-Output "A '$label' review issue is genuinely open (#$num, REST-confirmed); not filing another."
                    $script:LastNeedsHumanFiled = $true
                    return
                }
                Write-Output "Label search showed #$num as open but REST issue-view says it is not (stale search index); proceeding to file a fresh '$label' issue."
            }
        } catch {
            Write-Output "WARNING: could not parse '$label' dedup list; proceeding to file."
        }
    } elseif ($listed.Code -ne 0) {
        Write-Output "WARNING: '$label' dedup list failed (exit $($listed.Code)); proceeding to file."
    }
    $bodyFile = Join-Path $env:TEMP 'rge-ai-needs-human-body.txt'
    Write-Utf8 $bodyFile $Body
    # Retry create up to 3x with backoff: a transient gh blip must NOT silently
    # lose the only human notification (a real NEEDS_HUMAN pause once filed no
    # issue because a single gh hiccup was swallowed). Re-ensure the label each
    # attempt (idempotent) so an earlier label-create blip can't wedge it.
    $created = $null
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        Invoke-Tool -Exe 'gh' -CmdArgs @(
            'label', 'create', $label, '--repo', $RepoSlug,
            '--color', 'b60205',
            '--description', 'AI dispatch paused for human review/decision',
            '--force') | Out-Null
        $created = Invoke-Tool -Exe 'gh' -CmdArgs @(
            'issue', 'create', '--repo', $RepoSlug, '--title', $Title,
            '--body-file', $bodyFile, '--label', $label)
        if ($created.Code -eq 0) { break }
        Write-Output "WARNING: '$label' issue create attempt $attempt/3 failed (exit $($created.Code)): $($created.Text.Trim())"
        if ($attempt -lt 3) { Start-Sleep -Seconds 5 }
    }
    Remove-Item -LiteralPath $bodyFile -Force -ErrorAction SilentlyContinue
    if ($created -and $created.Code -eq 0) {
        Write-Output "Filed '$label' review issue: $($created.Text.Trim())"
        $script:LastNeedsHumanFiled = $true
    } else {
        Write-Output "ERROR: could not file '$label' review issue after 3 attempts. The loop stays paused via the halt sentinel; review .ai/dispatch.tasks.md."
    }
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
    # Pure decision helper for bounded one-shot recovery. Given the list of OPEN
    # failed autonomous issues plus the label set this loop uses, return the
    # eligibility verdict and the exact intended label transition. No GitHub side
    # effects, so the same function is callable from a non-mutating verification
    # harness with hand-crafted inputs.
    #
    # TWO bounded, taxonomy-specific recovery tiers, each ONE-SHOT per issue via
    # its OWN marker (so a deterministic failure burns exactly one retry per tier
    # then halts for a human -- no unbounded same-class re-recovery):
    #   - TRANSIENT (infra)        stall / timeout
    #                              -> marker $RecoverLabel
    #   - FLAKY (stochastic gate)  verification / control / plan-gate
    #                              -> marker $FlakyRecoverLabel
    # Everything else is INELIGIBLE and falls through to the human-review halt:
    # blocked / publish-hard-failed / unknown taxonomy (these MUST NOT auto-recover),
    # multiple or mixed taxonomy, missing taxonomy, an issue already recovered for
    # its tier, or more than one open failed issue. Fail-closed by default.
    param(
        [object[]]$Issues,
        [string]$FailLabel,
        [string]$QueueLabel,
        [string]$DoneLabel,
        [string]$RetryLabel,
        [string]$RecoverLabel,
        [string]$FlakyRecoverLabel,
        [string[]]$TransientLabels,
        [string[]]$FlakyLabels
    )
    $decision = [pscustomobject]@{
        Eligible         = $false
        Reason           = ''
        Issue            = $null
        Tier             = $null
        RecoverableLabel = $null
        Marker           = $null
        LabelsToRemove   = @()
        LabelsToAdd      = @()
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
    $taxonomy = @($labels | Where-Object { $_ -like 'ai-dispatch-failure-*' })
    if ($taxonomy.Count -eq 0) {
        $decision.Reason = "issue #$($cand.number) has no failure taxonomy label"
        return $decision
    }
    if ($taxonomy.Count -gt 1) {
        $decision.Reason = "issue #$($cand.number) has multiple taxonomy labels: " + ($taxonomy -join ', ')
        return $decision
    }
    $theLabel = $taxonomy[0]
    $tier   = $null
    $marker = $null
    if ($TransientLabels -contains $theLabel) {
        $tier = 'transient'; $marker = $RecoverLabel
    } elseif ($FlakyLabels -contains $theLabel) {
        $tier = 'flaky'; $marker = $FlakyRecoverLabel
    } else {
        $decision.Reason = "issue #$($cand.number) has a non-recoverable taxonomy label: $theLabel (blocked/publish/unknown never auto-recover)"
        return $decision
    }
    if ($labels -contains $marker) {
        $decision.Reason = "issue #$($cand.number) already recovered for the $tier tier (carries '$marker')"
        return $decision
    }

    $remove = @($FailLabel)
    if ($labels -contains $DoneLabel) { $remove += $DoneLabel }
    $add = @($QueueLabel, $RetryLabel, $marker)

    $decision.Eligible         = $true
    $decision.Issue            = $cand
    $decision.Tier             = $tier
    $decision.RecoverableLabel = $theLabel
    $decision.Marker           = $marker
    $decision.LabelsToRemove   = $remove
    $decision.LabelsToAdd      = $add
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
        [int]$MaxPlanRevisions = 2,

        [ValidateRange(0, 5)]
        [int]$MaxCorrectionRounds = 2,

        [ValidateSet('claude', 'codex')]
        [string]$Executor = 'codex',

        [bool]$CodexExecutorExternalScratch = $false,

        [bool]$TraceTiming = $false,

        [bool]$EnablePreflightAudit = $false
    )

    $args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $QueueScript,
        '-MaxPlanRevisions', $MaxPlanRevisions,
        '-MaxCorrectionRounds', $MaxCorrectionRounds,
        '-Executor', $Executor)
    if ($CodexExecutorExternalScratch) { $args += '-CodexExecutorExternalScratch' }
    switch ($PublishMode) {
        'branch' { $args += '-NoPublish' }
        'main'   { $args += @('-PublishMode', 'main') }
        'pr'     { $args += @('-PublishMode', 'pr') }
    }
    if ($TraceTiming) { $args += '-TraceTiming' }
    if ($EnablePreflightAudit) { $args += '-EnablePreflightAudit' }
    return ,$args
}

function Format-AutoGitHubStateSnapshot {
    # Build the small GitHub-state appendix that Auto adds to each generated
    # issue body. Audit tasks run inside a sandboxed executor that may not be
    # able to call gh/network; this snapshot lets them satisfy queue/filed-task
    # checks from state gathered by the Auto layer before issue creation.
    [CmdletBinding()]
    [OutputType([string])]
    param(
        [Parameter(Mandatory = $true)]
        [string]$RepoSlug,

        [AllowNull()][AllowEmptyCollection()]
        [object[]]$OpenQueueIssues,

        [AllowNull()][AllowEmptyCollection()]
        [object[]]$OpenFailedAutoIssues,

        [AllowNull()][AllowEmptyCollection()]
        [object[]]$FiledAutoIssues,

        [string]$QueueLabel = 'ai-dispatch',

        [string]$AutoLabel = 'ai-auto',

        [string]$GeneratedAt = ''
    )

    if (-not $GeneratedAt) { $GeneratedAt = (Get-Date).ToString('o') }
    if ($null -eq $OpenQueueIssues)      { $OpenQueueIssues = @() }
    if ($null -eq $OpenFailedAutoIssues) { $OpenFailedAutoIssues = @() }
    if ($null -eq $FiledAutoIssues)      { $FiledAutoIssues = @() }

    function Format-IssueSummaryLines {
        param([AllowNull()][AllowEmptyCollection()][object[]]$Issues)
        $items = @($Issues)
        if ($items.Count -eq 0) { return '(none)' }
        return (($items | Sort-Object number | ForEach-Object {
            $num = if ($_.number) { "#$($_.number)" } else { '#?' }
            $state = if ($_.state) { " [$($_.state)]" } else { '' }
            $title = if ($_.title) { [string]$_.title } else { '(no title)' }
            "- $num$state $title"
        }) -join "`n")
    }

    $queueLines = Format-IssueSummaryLines -Issues $OpenQueueIssues
    $failedLines = Format-IssueSummaryLines -Issues $OpenFailedAutoIssues
    $filedLines = Format-IssueSummaryLines -Issues $FiledAutoIssues

    return @"
---

Dispatcher GitHub state snapshot

Generated by Invoke-AiDispatchAuto.ps1 at $GeneratedAt before this issue was created.

- Repo: $RepoSlug
- Open $QueueLabel issues before this issue was created:
$queueLines
- Open failed autonomous issues ($AutoLabel + ai-dispatch-failed):
$failedLines
- Autonomous issues already filed ($AutoLabel, all states):
$filedLines

Executor instruction: for audit/task-selection checks that ask to confirm GitHub queue state or already-filed autonomous tasks, use this dispatcher-provided snapshot as the GitHub evidence. Do not call gh or the network from inside the executor sandbox for that confirmation; use local source reads for repo/source evidence.
"@
}

# --- Environment -----------------------------------------------------------

if ($env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -eq '1') {
    return
}

$script:RepoRoot = $PSScriptRoot
Set-Location -LiteralPath $script:RepoRoot

if ($CodexExecutorExternalScratch -and $Executor -ne 'codex') {
    Fail "-CodexExecutorExternalScratch is only valid with -Executor codex; it does not apply to Claude execution."
}

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

# --- 1b. Bounded one-shot recovery (two taxonomy-specific tiers) -----------
# Narrow Auto-layer repair hook: when the only thing blocking the loop is a
# single open autonomous issue whose terminal failure taxonomy is recoverable,
# requeue it ONCE. Two tiers, each one-shot per issue via its OWN marker:
#   - TRANSIENT (infra): stall / timeout            -> 'ai-dispatch-recovered-transient'
#   - FLAKY (stochastic gate): verification /        -> 'ai-dispatch-recovered-flaky'
#     control / plan-gate
# The per-tier marker guarantees a deterministic defect burns exactly one retry
# per tier then halts (no unbounded same-class re-recovery); the original
# taxonomy label is kept as audit evidence. Every other ineligible state --
# blocked / publish-hard-failed / unknown taxonomy (which MUST NOT auto-recover),
# closed failures, multiple failed issues, mixed/missing taxonomy, already-
# recovered-for-its-tier -- falls through to the existing human-review halt below.
# Recovery never runs ahead of the local sentinel check above.

$recoverLabel      = 'ai-dispatch-recovered-transient'
$flakyRecoverLabel = 'ai-dispatch-recovered-flaky'
$retryLabel        = 'ai-dispatch-retry'
$doneLabel         = 'ai-dispatch-done'
# TRANSIENT (infra) classes auto-recover once via $recoverLabel.
$transientLabels   = @('ai-dispatch-failure-stall', 'ai-dispatch-failure-timeout')
# FLAKY (stochastic gate) classes auto-recover once via the SEPARATE
# $flakyRecoverLabel marker, so each is bounded to a single retry per issue
# (a deterministic gate defect burns one retry then halts). blocked / publish /
# unknown are deliberately NOT here and always fall through to the human halt.
$flakyLabels       = @('ai-dispatch-failure-verification', 'ai-dispatch-failure-control', 'ai-dispatch-failure-plan-gate')

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
    -FlakyRecoverLabel $flakyRecoverLabel `
    -TransientLabels $transientLabels -FlakyLabels $flakyLabels

if ($decision.Eligible) {
    $cand = $decision.Issue
    Write-Output ''
    Write-Output "Bounded $($decision.Tier) recovery candidate: open autonomous issue #$($cand.number) ('$($cand.title)') with '$($decision.RecoverableLabel)'."
    Write-Output ("  Remove labels: " + ($decision.LabelsToRemove -join ', '))
    Write-Output ("  Add labels:    " + ($decision.LabelsToAdd -join ', '))
    Write-Output ("  Keep label:    $($decision.RecoverableLabel) (audit evidence)")
    if ($DryRun) {
        Write-Output 'DryRun: no label mutation; queue not run for this recovery.'
        Write-TimingTrace "auto.recovery-check: dry-run eligible (issue=#$($cand.number), label=$($decision.RecoverableLabel), tier=$($decision.Tier))"
        Write-TimingTrace "auto.tick: end (exit=0, dry-run=true, recovery=eligible)"
        exit 0
    }
    # Ensure the matched tier's one-shot recovery marker and the retry label exist
    # before the edit. The queue script also defines the retry label; recreating it
    # with --force is idempotent. The recovery markers are owned by this Auto layer;
    # only the marker actually being applied ($decision.Marker) is ensured here.
    $markerDesc = if ($decision.Tier -eq 'flaky') {
        'AI dispatch one-shot FLAKY-gate (verification/control/plan-gate) recovery marker; do not remove'
    } else {
        'AI dispatch one-shot transient (stall/timeout) recovery marker; do not remove'
    }
    Invoke-Tool -Exe 'gh' -CmdArgs @(
        'label', 'create', $decision.Marker, '--repo', $repoSlug,
        '--color', 'fbca04',
        '--description', $markerDesc,
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
    Write-Output "Issue #$($cand.number) requeued ($($decision.Tier) tier): '$failLabel' removed, '$($decision.Marker)' set, '$($decision.RecoverableLabel)' kept (audit evidence)."

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
    # --- 3a. Open-backlog cap (continuity; gates NEW task selection) -------
    #
    # Continuity change for non-stop operation. The old binding cap counted
    # ALL-TIME 'ai-auto' issues ('--state all') and permanently halted once the
    # count reached -MaxAutonomousTasks -- a lifetime ceiling that saturates and
    # bricks the loop. The binding cap now counts only OPEN 'ai-auto' issues: a
    # backlog/backpressure limit, not a lifetime wall. Completed dispatches
    # close their issue, so the open count is ~1 under normal operation and this
    # never trips; it halts only if un-published autonomous issues pile up (a
    # stuck-publish runaway guard). -MaxAutonomousTasks is now the open-backlog
    # ceiling. Periodic human review is enforced separately by the seatbelt.
    Write-TimingTrace "auto.cap-check: start"
    $openAuto = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
        '--state', 'open', '--limit', '200', '--json', 'number')
    Write-TimingTrace "auto.cap-check: done (open=$($openAuto.Count), backlogCap=$MaxAutonomousTasks)"
    if ($openAuto.Count -ge $MaxAutonomousTasks) {
        Write-Output ''
        Write-Output "HALTED for review: open autonomous-issue backlog reached ($($openAuto.Count) of $MaxAutonomousTasks open '$autoLabel' issues). Publishing or review may be stuck."
        Write-Output "Clear the backlog (merge/close published work) or raise -MaxAutonomousTasks to continue."
        Write-TimingTrace "auto.tick: end (exit=0, halted=open-backlog)"
        exit 0
    }

    # --- 3b. Periodic seatbelt (forced human checkpoint every N tasks) -----
    #
    # Non-stop, but not unattended-forever: pause for human review every
    # $SeatbeltInterval NEW autonomous tasks. The window is a machine-local
    # MONOTONIC counter (filedSinceReview) in .ai/dispatch.auto-seatbelt.json
    # (gitignored via .ai/*.json), incremented once per successfully filed task
    # below. A local counter -- not a GitHub issue count -- is used on purpose:
    # the 'ai-auto' label is never removed, so any `gh issue list` row count
    # saturates at its --limit and would silently disable the seatbelt forever
    # in the non-stop regime this very change targets. The counter increments by
    # exactly one per filed task, so the pause lands after exactly N tasks.
    Write-TimingTrace "auto.seatbelt: start"
    $seatbeltFile = Join-Path $script:RepoRoot '.ai\dispatch.auto-seatbelt.json'
    $filedSinceReview = 0
    if (Test-Path -LiteralPath $seatbeltFile) {
        try {
            $sbObj = (Get-Content -Raw -LiteralPath $seatbeltFile) | ConvertFrom-Json
            # [int]$null yields 0 (no throw) in PS 5.1, so a partial / hand-edited
            # file would silently read as 0; guard the field explicitly so a
            # missing value routes to the corrupt-counter halt instead.
            if ($null -eq $sbObj.filedSinceReview) { throw 'filedSinceReview field missing' }
            $filedSinceReview = [int]$sbObj.filedSinceReview
        } catch {
            Write-Utf8 $haltSentinel "Autonomous loop halted: seatbelt counter $seatbeltFile is unreadable/corrupt. Repair or delete it, then delete this sentinel to resume."
            Write-Output "HALTED: seatbelt counter $seatbeltFile is corrupt; wrote halt sentinel. Repair/delete it and the sentinel to resume."
            Write-TimingTrace "auto.tick: end (exit=0, halted=seatbelt-corrupt)"
            exit 0
        }
    } else {
        # First run under the seatbelt regime: start the window at zero.
        $filedSinceReview = 0
        $sbInit = [pscustomobject]@{ filedSinceReview = 0; note = 'auto-initialized'; updated = (Get-Date).ToString('o') }
        Write-Utf8 $seatbeltFile ($sbInit | ConvertTo-Json -Compress)
        Write-Output "Seatbelt initialized (interval $SeatbeltInterval, counter 0)."
    }
    Write-TimingTrace "auto.seatbelt: done (filedSinceReview=$filedSinceReview, interval=$SeatbeltInterval)"
    if ($filedSinceReview -ge $SeatbeltInterval) {
        # Order matters (review M2): write the durable halt sentinel FIRST so the
        # pause holds even if issue filing fails, then file the review issue,
        # then reset the counter LAST so a resume (sentinel deleted) starts a
        # fresh interval. New-NeedsHumanIssue is gh-failure-tolerant (never throws).
        Write-Utf8 $haltSentinel ("Seatbelt: {0} autonomous tasks filed since last review. Review the recent batch, then delete this file to resume the next {1}." -f $filedSinceReview, $SeatbeltInterval)
        New-NeedsHumanIssue -RepoSlug $repoSlug `
            -Title "AI dispatch seatbelt: review last $filedSinceReview autonomous tasks" `
            -Body "The autonomous dispatch loop reached its periodic seatbelt: $filedSinceReview new autonomous tasks have been filed since the last human review.`r`n`r`nReview the recent batch (merged work, drift, direction), then delete the halt sentinel ``.ai/dispatch.auto-halt`` to resume the next $SeatbeltInterval-task interval. The counter has been reset, so resuming will not immediately re-pause."
        $sbReset = [pscustomobject]@{ filedSinceReview = 0; note = 'seatbelt paused for review'; updated = (Get-Date).ToString('o') }
        Write-Utf8 $seatbeltFile ($sbReset | ConvertTo-Json -Compress)
        Write-Output ''
        Write-Output "SEATBELT: $filedSinceReview new autonomous tasks since last review; pausing for human review."
        Write-Output "Wrote halt sentinel $haltSentinel and filed/confirmed a needs-human review issue. Delete the sentinel to resume."
        Write-TimingTrace "auto.tick: end (exit=0, halted=seatbelt)"
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

Choose exactly ONE best next task to dispatch now. Re-evaluate current repo,
issue, and task-brief state on every invocation. Treat the brief's order as the
primary priority signal, but choose an earlier-in-dependency task first if it is
a prerequisite ("sequence necessity") and skip any task already filed or marked
DONE / DONE-SUPERSEDED. The task must be small, bounded, and independently
shippable.

If the brief contains no real tasks yet (only instructions, placeholders, or
examples), or every real task is already filed/complete, respond with exactly
this single line and nothing else:
AUTO_SELECTION: none

Otherwise respond with exactly this block as the last thing in your reply:
<<<AUTO_TASK_BEGIN>>>
TITLE: <one concise imperative line, 70 chars or fewer>
BODY:
<2 to 8 lines: the goal, the in-scope files or areas, and the done-criteria.
This text becomes the dispatch goal that Codex plans and the selected executor executes.
If the chosen task's brief block contains a "Self-re-arm" instruction or an
append-the-next-task done-criterion, you MUST reproduce that instruction
verbatim in this BODY -- it keeps the loop armed and must NOT be summarized away.>
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
            # Non-stop self-re-arm: under the self-re-arm protocol the brief
            # only runs dry when an executor recorded a NEEDS_HUMAN boundary
            # (marker line 'NEEDS_HUMAN_RECORDED:') instead of appending the
            # next task. Surface that boundary to a human via a labeled issue
            # (idempotent) so a genuine policy/architecture stop is not silently
            # idle. A plain exhausted brief (no marker) just ends the tick.
            $briefForHuman = Get-Content -Raw -LiteralPath $briefPath -ErrorAction SilentlyContinue
            if ($briefForHuman -and ($briefForHuman -match '(?m)^\s*NEEDS_HUMAN_RECORDED:\s*(\d{4}-\d{2}-\d{2}.+)$')) {
                # Case 1: an executor deliberately recorded a NEEDS_HUMAN
                # boundary instead of appending the next task.
                $nhReason = $matches[1].Trim()
                $nhSnippet = if ($nhReason.Length -gt 60) { $nhReason.Substring(0, 60) } else { $nhReason }
                New-NeedsHumanIssue -RepoSlug $repoSlug `
                    -Title "AI dispatch NEEDS_HUMAN: $nhSnippet" `
                    -Body "The autonomous dispatch loop recorded a NEEDS_HUMAN boundary in ``.ai/dispatch.tasks.md`` and cannot self-re-arm without a human decision.`r`n`r`nRecorded reason:`r`n$nhReason`r`n`r`nResolve by deciding the next task (append it to the brief per the self-re-arm protocol) or adjusting scope, then remove the ``NEEDS_HUMAN_RECORDED:`` line and close this issue. Per the operator's policy this decision may be delegated to Codex."
            } else {
                # Case 2: the brief is dry with NO NEEDS_HUMAN marker. Under the
                # self-re-arm protocol every task either appends the next one or
                # records NEEDS_HUMAN, so a clean idle is an anomaly -- a broken
                # self-re-arm chain (an executor failed to append) or genuinely
                # exhausted work. Either way, surface it rather than stalling
                # silently. Idempotent: only one needs-human issue stays open.
                New-NeedsHumanIssue -RepoSlug $repoSlug `
                    -Title 'AI dispatch idle: brief dry, self-re-arm chain may have broken' `
                    -Body "The autonomous dispatch loop selected no task and the brief carries no ``NEEDS_HUMAN_RECORDED:`` marker. Under the self-re-arm protocol the brief should never run dry on its own, so this usually means the last dispatched task did not append its next task (a broken chain), or all planned work is genuinely complete.`r`n`r`nReview ``.ai/dispatch.tasks.md`` and either append the next task to re-arm the loop or unarm it deliberately, then close this issue."
            }
            # Pause the loop after surfacing the idle (both cases). Without this,
            # the open-only dedup re-files an identical issue every tick once the
            # human closes the prior one; the sentinel makes the next tick
            # short-circuit at the top-of-tick halt check until the brief is
            # re-armed and the human deletes the sentinel.
            $nhNote = if ($script:LastNeedsHumanFiled) { 'a needs-human review issue was filed' } else { 'a needs-human review issue could NOT be filed (gh error) -- this sentinel and .ai/dispatch.tasks.md are the record' }
            Write-Utf8 $haltSentinel "Autonomous loop idle: the brief produced no task and $nhNote. Re-arm the brief (or unarm it deliberately), then delete this file to resume."
            Write-Output "Wrote halt sentinel $haltSentinel; the loop is paused until the brief is re-armed and the sentinel deleted."
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

    $preCreateOpenQueue = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $queueLabel,
        '--state', 'open', '--limit', '100', '--json', 'number,title,state')
    if ($preCreateOpenQueue.Count -gt 0) {
        Write-Output "Queue changed while Codex was selecting; skipping issue creation so the next tick can drain existing work."
        Write-TimingTrace "auto.tick: end (exit=0, skipped=queue-filled-before-create)"
        exit 0
    }

    $preCreateFailedAuto = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
        '--label', $failLabel, '--state', 'open', '--limit', '100',
        '--json', 'number,title,state')
    $preCreateFiledAuto = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
        '--state', 'all', '--limit', '200',
        '--json', 'number,title,state')
    $githubSnapshot = Format-AutoGitHubStateSnapshot `
        -RepoSlug $repoSlug `
        -OpenQueueIssues $preCreateOpenQueue `
        -OpenFailedAutoIssues $preCreateFailedAuto `
        -FiledAutoIssues $preCreateFiledAuto `
        -QueueLabel $queueLabel `
        -AutoLabel $autoLabel

    $briefName = Split-Path -Leaf $briefPath
    $issueBody = "$taskBody`r`n`r`n$githubSnapshot`r`n`r`n_Filed automatically by Invoke-AiDispatchAuto.ps1 - Codex-selected from $briefName._"
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

    # Seatbelt: count this filed task toward the next human checkpoint. The
    # counter is the sole seatbelt input (see section 3b); it increments by
    # exactly one per filed task. Best-effort persist -- a miss here only delays
    # the next pause by one task and never crashes the tick.
    try {
        $sbCount = 0
        if (Test-Path -LiteralPath $seatbeltFile) {
            $sbCur = (Get-Content -Raw -LiteralPath $seatbeltFile) | ConvertFrom-Json
            if ($sbCur -and $null -ne $sbCur.filedSinceReview) { $sbCount = [int]$sbCur.filedSinceReview }
        }
        $sbCount++
        $sbUpd = [pscustomobject]@{ filedSinceReview = $sbCount; note = 'incremented on file'; updated = (Get-Date).ToString('o') }
        Write-Utf8 $seatbeltFile ($sbUpd | ConvertTo-Json -Compress)
        Write-TimingTrace "auto.seatbelt: counter incremented to $sbCount"
    } catch {
        Write-Output "WARNING: could not update seatbelt counter ($seatbeltFile): $($_.Exception.Message)"
    }

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
    -CodexExecutorExternalScratch ([bool]$CodexExecutorExternalScratch) `
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
