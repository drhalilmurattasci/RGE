#Requires -Version 5.1
<#
.SYNOPSIS
    Claude active safety-guard for an autonomous (Codex-driven) dispatch run.

.DESCRIPTION
    The action-taking sibling of the passive Watch-AiDispatch.ps1 dashboard. Where
    that script only displays a run, this guard is an independent, cross-AI safety
    backstop: it records every phase to a watch log, assesses the run for anomalies
    (deterministic hard rules PLUS a Claude LLM verdict), and on a problem it
    terminates the run and writes an abort report.

    Roles (see AUTONOMOUS_WATCH.md): Codex plans + selects + executes + controls +
    authorizes the merge; this script is Claude's WATCH-ONLY role. It never executes
    dispatch work and never publishes.

    Two run modes:
      -DryRun  drives a hermetic scripted sequence (no child, no Claude, no publish)
               and is the safe way to exercise record -> assess -> terminate ->
               report offline. It is NOT the default: omit -DryRun and the guard
               enters the live path.
    live     (default) launches the autonomous driver (-DriverCommand) as a
               supervised child, records its output, applies hard rules + a Claude
               assessment, and kills the child + writes an abort report on anomaly.
               By default this is one driver tick. With -DriverTicks N, the
               guard runs up to N sequential Auto ticks; each tick re-runs the
               Codex selector against current issue/task state before draining
               exactly one queue issue.

    The live path is implemented but launching a real autonomous run remains an
    explicit operator decision: pointing -DriverCommand at Invoke-AiDispatchAuto.ps1
    with -PublishMode main is what actually arms it. -MockAssess / a mock
    -DriverCommand let the live mechanism be smoke-tested without a real model call
    or publish.

.PARAMETER DispatchId
    Identifier for this guard run; names the watch directory + files.

.PARAMETER DryRun
    Drive a hermetic scripted sequence instead of launching the real driver.

.PARAMETER DryRunOutcome
    Dry-run scenario: 'complete' (benign run finishes), 'hardrule' (inject a
    forbidden signal -> deterministic hard-rule abort), or 'llm-abort' (the mocked
    Claude assessment returns abort).

.PARAMETER DriverCommand
    Live mode: the autonomous driver script the guard supervises as a child.

.PARAMETER Executor / PublishMode / MaxAutonomousTasks
    Live mode: passed to the driver. Defaults match the target model (Codex
    executor). NOTE: -PublishMode main is the auto-publish posture; choosing it is
    the explicit arming decision.

.PARAMETER DriverTicks
    Live mode: finite number of sequential autonomous driver ticks to supervise.
    Default 1 preserves the historical one-selection / one-queue-run behavior.
    Values above 1 make full automation select the next best task again after
    each successful tick, stopping early on cap/no-work/ambiguous/lock states.

.PARAMETER MockAssess
    Test seam: in live mode, return the scripted -MockVerdict instead of calling
    Claude, so the live watch->terminate->report path can be smoke-tested offline.

.EXAMPLE
    .\Invoke-AiDispatchGuard.ps1 -DryRun -DispatchId GUARD-DRYRUN

.EXAMPLE
    # Smoke-test the LIVE mechanism against a mock driver, no real model/publish:
    .\Invoke-AiDispatchGuard.ps1 -DispatchId GUARD-LIVE -DriverCommand .\mock.ps1 -MockAssess
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$DispatchId,

    [switch]$DryRun,

    [ValidateSet('complete', 'hardrule', 'llm-abort')]
    [string]$DryRunOutcome = 'complete',

    [string]$DriverCommand = '.\Invoke-AiDispatchAuto.ps1',

    [ValidateSet('claude', 'codex')]
    [string]$Executor = 'codex',

    [ValidateSet('branch', 'pr', 'main')]
    [string]$PublishMode = 'pr',

    [switch]$CodexExecutorExternalScratch,

    [ValidateRange(1, 200)]
    [int]$MaxAutonomousTasks = 1,

    [ValidateRange(1, 200)]
    [int]$DriverTicks = 1,

    [ValidateRange(15, 3600)]
    [int]$AssessIntervalSec = 60,

    [ValidateRange(2, 120)]
    [int]$PollIntervalSec = 5,

    [ValidateRange(1, 1440)]
    [int]$MaxRunMinutes = 90,

    [ValidateRange(1, 120)]
    [int]$StallMinutes = 15,

    [ValidateRange(0, 10)]
    [int]$MaxCorrectionRounds = 2,

    [string]$WatchRoot = '.ai/dispatch-watch',

    [string]$ClaudeBin = 'claude',

    [switch]$MockAssess,

    [ValidateSet('ok', 'abort')]
    [string]$MockVerdict = 'ok'
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# Watch directory + structured outputs
# ---------------------------------------------------------------------------

$script:WatchDir = Join-Path $WatchRoot $DispatchId
$null = New-Item -ItemType Directory -Force -Path $script:WatchDir
$script:LogPath = Join-Path $script:WatchDir 'watch.log'
$script:EventsPath = Join-Path $script:WatchDir 'events.jsonl'
$script:ReportPath = Join-Path $script:WatchDir 'abort-report.md'
$script:Seq = 0
$script:Utf8 = [System.Text.UTF8Encoding]::new($false)  # UTF-8, no BOM
$script:StallLimit = $StallMinutes  # the stall limit Test-HardRule's numeric branch reads
$script:LastDriverTickDecision = [pscustomobject]@{
    ShouldContinue = $false
    StopKind       = 'unset'
    Reason         = 'no driver tick has completed yet'
}

# Hard-rule patterns are SOURCE-SCOPED so prose can never trip them (a TASK packet
# or rubric line that merely *mentions* `git push origin main` is not an action).
# Command patterns are matched ONLY against text classified as an executed command;
# signal patterns ONLY against the loop/gate's own structured status lines. See
# Get-RecordSource. Command patterns are deliberately broad (false positives only
# halt + report; false negatives let a bad action through).
$script:CommandForbiddenPatterns = @(
    # a push that targets the protected main/master ref in any form:
    #   origin main | origin master | origin HEAD:main | origin refs/heads/main |
    #   origin +main | origin +main:main | a URL remote ... main
    'git\s+push\b.*(^|\s)(\+?(refs/heads/)?(main|master)|HEAD:(refs/heads/)?(main|master)|\+?[^:\s]+:(refs/heads/)?(main|master))(\s|$)',
    # any force push (flag anywhere, or a leading-+ force refspec):
    'git\s+push\b.*((^|\s)(--force|--force-with-lease|-f)(\s|$)|(^|\s)\+[^\s]+(\s|$))'
)
$script:SignalForbiddenPatterns = @(
    'VERIFY (FAILED|FAIL)\b',                          # the gate's own failure line
    'GATE_EXIT=[^0\s]',                                # a non-zero gate exit
    'HANDOFF_STATUS:\s*(BLOCKED|FAILED|NEEDS_HUMAN)',  # a block / human verdict
    '\bverdict\s*[:=]\s*block\b',                      # Codex control says block
    '\bneeds_human\b'
)

function Add-Utf8Line {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][AllowEmptyString()][string]$Text)
    [System.IO.File]::AppendAllText($Path, $Text + "`r`n", $script:Utf8)
}

function Write-GuardLine {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Kind,
        [Parameter(Mandatory)][string]$Message,
        [hashtable]$Data
    )
    $script:Seq++
    $stamp = (Get-Date).ToString('o')
    $line = '[{0:0000}] {1} {2} {3}' -f $script:Seq, $stamp, $Kind.PadRight(9), $Message
    Add-Utf8Line -Path $script:LogPath -Text $line
    # Console echo goes to the HOST stream, NOT the success pipeline -- otherwise
    # `$disposition = Invoke-GuardDryRun` would capture every log line, not just the
    # function's return value.
    Write-Host $line

    $evt = [ordered]@{
        seq     = $script:Seq
        time    = $stamp
        kind    = $Kind
        message = $Message
    }
    if ($Data) { foreach ($k in $Data.Keys) { $evt[$k] = $Data[$k] } }
    Add-Utf8Line -Path $script:EventsPath -Text ($evt | ConvertTo-Json -Compress)
}

# ---------------------------------------------------------------------------
# Record classification (review #3: match hard rules on structured sources, not
# arbitrary prose) + hard rules (deterministic) + LLM assessment (Claude)
# ---------------------------------------------------------------------------

function Get-RecordSource {
    # Classify one line of driver output into the source a hard rule may use:
    #   'signal'  -> the loop/gate's own structured status lines
    #   'command' -> an echoed executed command (rare; the dispatch scripts capture
    #                git rather than echo it, so this mostly guards explicit traces)
    #   'prose'   -> everything else (TASK text, rubric, model reasoning) -- NEVER
    #                triggers a hard rule, only feeds the Claude assessment
    [CmdletBinding()]
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Line)

    $t = $Line.Trim()
    if ($t -match '^(VERIFY |GATE_EXIT=|--- STEP FAILED|HANDOFF_STATUS:|EXEC_STATUS:)' -or
        $t -match '\bverdict\s*[:=]' -or $t -match '\bexecution round\s+\d+') {
        return 'signal'
    }
    # An executed-command echo: a shell prompt/trace prefix followed by a command.
    if ($t -match '^(PS[^>]*>|\+|\$|>)\s*\S' -or $t -match '^(git|cargo|powershell|gh)\b') {
        return 'command'
    }
    return 'prose'
}

function Test-HardRule {
    # Deterministic floor. Command/signal text are matched against their own
    # pattern sets; numeric limits are independent of any text.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string]$CommandText,
        [Parameter(Mandatory)][AllowEmptyString()][string]$SignalText,
        [Parameter(Mandatory)][double]$ElapsedMinutes,
        [Parameter(Mandatory)][double]$StallElapsed,
        [Parameter(Mandatory)][int]$CorrectionRounds
    )
    foreach ($pat in $script:CommandForbiddenPatterns) {
        if ($CommandText -match $pat) { return "forbidden-command: /$pat/" }
    }
    foreach ($pat in $script:SignalForbiddenPatterns) {
        if ($SignalText -match $pat) { return "forbidden-signal: /$pat/" }
    }
    if ($ElapsedMinutes -gt $MaxRunMinutes) {
        return ("max-runtime-exceeded: {0:n1}m > {1}m" -f $ElapsedMinutes, $MaxRunMinutes)
    }
    if ($StallElapsed -ge $script:StallLimit) {
        return ("stalled: no progress for {0:n1}m >= {1}m" -f $StallElapsed, $script:StallLimit)
    }
    if ($CorrectionRounds -gt $MaxCorrectionRounds) {
        return "correction-rounds-exceeded: $CorrectionRounds > $MaxCorrectionRounds"
    }
    return $null
}

function Get-DriverTickContinuationDecision {
    # Pure classifier for a completed Auto tick. The guard may launch another
    # Auto tick only when the previous tick actually completed useful work or
    # filed/drained a queue item. Benign no-op states stop the finite batch
    # early so -DriverTicks never spins on "no work" or cap/lock conditions.
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][int]$ExitCode,
        [Parameter(Mandatory)][AllowEmptyString()][string]$RecentText
    )

    if ($ExitCode -ne 0) {
        return [pscustomobject]@{
            ShouldContinue = $false
            StopKind       = 'failed'
            Reason         = "driver exited non-zero ($ExitCode)"
        }
    }

    $text = [string]$RecentText
    $stopPatterns = @(
        @{ Kind = 'lock-held';        Pattern = 'Another autonomous dispatch tick is already running'; Reason = 'another autonomous dispatch tick is already running' },
        @{ Kind = 'halt-sentinel';    Pattern = 'HALTED: a prior tick recorded a fault';             Reason = 'auto halt sentinel is present' },
        @{ Kind = 'failed-issue';     Pattern = 'HALTED: autonomous task #[0-9]+ .* is marked';       Reason = 'failed autonomous issue is open' },
        @{ Kind = 'cap-reached';      Pattern = 'HALTED for review: autonomous task cap reached';      Reason = 'autonomous task cap reached' },
        @{ Kind = 'queue-ambiguous';  Pattern = 'Queue state ambiguous after primary check and cross-check'; Reason = 'queue state ambiguous' },
        @{ Kind = 'no-brief';         Pattern = 'No task brief at .* nothing to select';                Reason = 'no task brief exists' },
        @{ Kind = 'empty-brief';      Pattern = 'Task brief .* is empty; nothing to select';            Reason = 'task brief is empty' },
        @{ Kind = 'brief-unarmed';    Pattern = 'DISPATCH-TASKS-UNARMED marker';                       Reason = 'task brief is unarmed' },
        @{ Kind = 'no-selection';     Pattern = 'Codex reports no real task to select';                 Reason = 'selector found no real task' },
        @{ Kind = 'dry-run';          Pattern = 'DryRun: (queue not run|no issue created)';             Reason = 'driver was a dry run' }
    )
    foreach ($entry in $stopPatterns) {
        if ($text -match $entry.Pattern) {
            return [pscustomobject]@{
                ShouldContinue = $false
                StopKind       = $entry.Kind
                Reason         = $entry.Reason
            }
        }
    }

    return [pscustomobject]@{
        ShouldContinue = $true
        StopKind       = 'continue'
        Reason         = 'driver tick completed; run another fresh selector tick if the finite batch has capacity'
    }
}

function Convert-MonitorAssessmentResponse {
    [CmdletBinding()]
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Text)

    $text = ([string]$Text).Trim()
    if ($text -match '^(?i:ok)$') {
        return [pscustomobject]@{ verdict = 'ok'; reason = 'plain ok monitor response' }
    }
    if ($text -match '^(?i:abort)(?:\s*[:\-]\s*(.+))?$') {
        $reason = if ($matches[1]) { [string]$matches[1] } else { 'plain abort monitor response' }
        return [pscustomobject]@{ verdict = 'abort'; reason = $reason }
    }

    $jsonMatch = [regex]::Match($text, '\{.*\}')
    if (-not $jsonMatch.Success) {
        # Fail-safe: an unparseable monitor response is treated as a halt, not a pass.
        return [pscustomobject]@{ verdict = 'abort'; reason = "unparseable monitor response: $text" }
    }
    $jsonText = $jsonMatch.Value
    try {
        $obj = $jsonText | ConvertFrom-Json
        if ($obj.verdict -notin @('ok', 'abort')) {
            return [pscustomobject]@{ verdict = 'abort'; reason = "invalid verdict field: $($obj.verdict)" }
        }
        return [pscustomobject]@{ verdict = $obj.verdict; reason = [string]$obj.reason }
    }
    catch {
        # Strict JSON failed. Narrow fail-safe recovery on object-like text only:
        # pull a recognizable verdict even when non-verdict fields (e.g. reason)
        # are malformed. The verdict key may be quoted JSON ("verdict") or a
        # bare object-like key (verdict); the value may be quoted or a bare
        # token. Anything without a recognizable ok/abort verdict stays
        # fail-safe abort.
        $verdictMatch = [regex]::Match(
            $jsonText,
            '(?<![A-Za-z0-9_])(?:"verdict"|verdict)\s*:\s*(?:"(?<quoted>ok|abort)"|(?<bare>ok|abort)(?=\s*[,}]))',
            [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        if ($verdictMatch.Success) {
            $verdictGroup = if ($verdictMatch.Groups['quoted'].Success) {
                $verdictMatch.Groups['quoted']
            } else {
                $verdictMatch.Groups['bare']
            }
            $verdict = $verdictGroup.Value.ToLowerInvariant()
            $reason = 'monitor response verdict recovered from malformed object'
            if ($jsonText -match '(?<![A-Za-z0-9_])(?:"reason"|reason)\s*:\s*"([^"]*)"') {
                $reason = $matches[1]
            }
            return [pscustomobject]@{ verdict = $verdict; reason = $reason }
        }
        return [pscustomobject]@{ verdict = 'abort'; reason = "monitor JSON parse error: $($_.Exception.Message)" }
    }
}

function Invoke-ClaudeAssess {
    [CmdletBinding()]
    param([Parameter(Mandatory)][AllowEmptyString()][string]$RecentText)

    # Hermetic seam: -DryRun OR -MockAssess returns the scripted verdict so the
    # assess -> terminate path is exercised without calling Claude. The 'llm-abort'
    # dry-run outcome forces abort; otherwise -MockVerdict (default 'ok') applies.
    if ($DryRun -or $MockAssess) {
        $v = if ($DryRunOutcome -eq 'llm-abort') { 'abort' } else { $MockVerdict }
        return [pscustomobject]@{ verdict = $v; reason = "mock verdict ($v; dryRun=$($DryRun.IsPresent) mockAssess=$($MockAssess.IsPresent))" }
    }

    $rubric = @"
You are the WATCH/SAFETY monitor for an autonomous Codex dispatch run. You do NOT
execute or fix anything. Read the recent activity below and decide whether the run
is healthy or must be HALTED. Halt if you see: scope creep beyond the TASK packet,
edits to forbidden files, a destructive git action, a verification gate that failed
but the run continued, looping/no-progress, or any safety-relevant anomaly.

Respond with ONLY a JSON object on one line:
{"verdict":"ok"|"abort","reason":"<one sentence>"}

Recent activity:
$RecentText
"@
    # EAP isolation: under the script's EAP=Stop, the claude CLI's native stderr
    # would otherwise trap. Treat a failed invocation as empty -> abort fail-safe.
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'SilentlyContinue'
    try { $raw = & $ClaudeBin -p $rubric 2>$null } catch { $raw = '' } finally { $ErrorActionPreference = $prevEap }
    $text = ($raw | Out-String).Trim()
    return Convert-MonitorAssessmentResponse -Text $text
}

function Stop-GuardRun {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Trigger,
        [Parameter(Mandatory)][string]$Reason,
        [int]$ChildPid = 0,
        [string]$RecentText = ''
    )
    Write-GuardLine -Kind 'ABORT' -Message "$Trigger -- $Reason" -Data @{ trigger = $Trigger; reason = $Reason }

    if ($ChildPid -gt 0) {
        Write-GuardLine -Kind 'KILL' -Message "terminating child process tree pid=$ChildPid"
        # taskkill /T kills the whole tree so a hung cargo/codex child cannot survive.
        # EAP isolation: under the script's EAP=Stop, taskkill's native stderr (e.g.
        # "process not found" when the child already exited) would otherwise be
        # wrapped as a terminating error and crash the guard mid-abort.
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = 'SilentlyContinue'
        try { & taskkill.exe /PID $ChildPid /T /F 2>$null | Out-Null } catch { }
        finally { $ErrorActionPreference = $prevEap }
    }

    $childLine = if ($ChildPid -gt 0) { "$ChildPid" } else { '(none / dry-run)' }
    $report = @"
# Guard abort report -- $DispatchId

**Disposition:** ABORTED
**Trigger:** $Trigger
**Reason:** $Reason
**Time:** $((Get-Date).ToString('o'))
**Child pid:** $childLine

## Recent activity (tail)

``````
$RecentText
``````

## Recommended follow-up

A human should inspect ``$script:LogPath`` + ``$script:EventsPath``, decide whether
the run's partial changes are safe, and re-arm only after the trigger is resolved.
The guard halted BEFORE any further phase; nothing was published by this run after
the abort.
"@
    [System.IO.File]::WriteAllText($script:ReportPath, $report, $script:Utf8)
    Write-GuardLine -Kind 'REPORT' -Message "wrote abort report: $script:ReportPath"
}

function Read-NewText {
    # Read bytes appended to $Path since $Offset (a [ref] updated in place). Opens
    # with ReadWrite share so the child can keep writing. Returns the new text.
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][ref]$Offset)

    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    $fs = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        if ($Offset.Value -gt $fs.Length) { $Offset.Value = 0 }  # file truncated/rotated
        [void]$fs.Seek([long]$Offset.Value, [System.IO.SeekOrigin]::Begin)
        $sr = New-Object System.IO.StreamReader($fs)
        $text = $sr.ReadToEnd()
        $Offset.Value = $fs.Position
    }
    finally {
        $fs.Close()
    }
    return $text
}

# ---------------------------------------------------------------------------
# Run drivers
# ---------------------------------------------------------------------------

function Invoke-GuardDryRun {
    [CmdletBinding()]
    param()

    # A scripted "normal dispatch" sequence the guard observes, each record tagged
    # with the source a hard rule may use (signal / command / prose). The 'hardrule'
    # outcome injects a forbidden SIGNAL (a failed gate) so the deterministic
    # terminate + report path is exercised without a real child or Claude call.
    $events = [System.Collections.Generic.List[object]]::new()
    $events.Add(@{ src = 'prose';  text = 'phase=select  Codex selected task DEMO-1 from .ai/dispatch.tasks.md' })
    $events.Add(@{ src = 'prose';  text = 'phase=plan     Codex authored TASK packet (scope: 2 files)' })
    $events.Add(@{ src = 'prose';  text = 'phase=execute  Codex executed; 2 files changed' })
    if ($DryRunOutcome -eq 'hardrule') {
        $events.Add(@{ src = 'signal'; text = 'VERIFY FAILED: rge-script-bench ratio 1.7x > 1.5x' })
    }
    else {
        $events.Add(@{ src = 'signal'; text = 'VERIFY OK: all 7 verification step(s) passed.' })
    }
    $events.Add(@{ src = 'signal'; text = 'control verdict=pass' })
    $events.Add(@{ src = 'prose';  text = 'phase=publish  (dry-run) would auto-publish via -PublishMode main' })

    $cmdRecent = [System.Collections.Generic.Queue[string]]::new()
    $sigRecent = [System.Collections.Generic.Queue[string]]::new()
    $allRecent = [System.Collections.Generic.Queue[string]]::new()
    foreach ($e in $events) {
        Write-GuardLine -Kind 'PHASE' -Message ("[{0}] {1}" -f $e.src, $e.text)
        switch ($e.src) {
            'command' { $cmdRecent.Enqueue($e.text) }
            'signal'  { $sigRecent.Enqueue($e.text) }
        }
        $allRecent.Enqueue($e.text)
        foreach ($q in @($cmdRecent, $sigRecent, $allRecent)) { while ($q.Count -gt 8) { [void]$q.Dequeue() } }

        $rule = Test-HardRule -CommandText ($cmdRecent.ToArray() -join "`n") -SignalText ($sigRecent.ToArray() -join "`n") `
            -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0
        if ($rule) {
            Stop-GuardRun -Trigger 'hard-rule' -Reason $rule -ChildPid 0 -RecentText ($allRecent.ToArray() -join "`n")
            return 'aborted'
        }

        $assessment = Invoke-ClaudeAssess -RecentText ($allRecent.ToArray() -join "`n")
        Write-GuardLine -Kind 'ASSESS' -Message "verdict=$($assessment.verdict) -- $($assessment.reason)" -Data @{ verdict = $assessment.verdict }
        if ($assessment.verdict -eq 'abort') {
            Stop-GuardRun -Trigger 'llm-veto' -Reason $assessment.reason -ChildPid 0 -RecentText ($allRecent.ToArray() -join "`n")
            return 'aborted'
        }
    }
    Write-GuardLine -Kind 'DONE' -Message 'run completed; no anomaly detected'
    return 'completed'
}

function Invoke-GuardLiveRun {
    [CmdletBinding()]
    param([int]$TickIndex = 1)

    # Launch the autonomous driver as a supervised child and monitor it:
    # record output -> classify -> hard rules + periodic Claude assessment ->
    # taskkill the child tree + write a report on any anomaly. Liveness is measured
    # by output growth + the child's process state (the .ai/dispatch-trace JSONL is
    # an additional progress signal a future revision can correlate by pid).
    $logSuffix = if ($DriverTicks -gt 1) { ".tick$TickIndex" } else { '' }
    $driverOut = Join-Path $script:WatchDir ('driver{0}.stdout.log' -f $logSuffix)
    $driverErr = Join-Path $script:WatchDir ('driver{0}.stderr.log' -f $logSuffix)
    Set-Content -LiteralPath $driverOut -Value '' -NoNewline -Encoding utf8
    Set-Content -LiteralPath $driverErr -Value '' -NoNewline -Encoding utf8

    $driverArgs = New-GuardDriverArguments -DriverCommand $DriverCommand `
        -Executor $Executor -PublishMode $PublishMode `
        -MaxAutonomousTasks $MaxAutonomousTasks `
        -CodexExecutorExternalScratch ([bool]$CodexExecutorExternalScratch)
    Write-GuardLine -Kind 'LAUNCH' -Message ("driver tick={0}/{1}: powershell.exe {2}" -f $TickIndex, $DriverTicks, ($driverArgs -join ' '))
    if ($PublishMode -eq 'main') {
        Write-GuardLine -Kind 'WARN' -Message 'PublishMode=main: driver may auto-publish to origin/main on a control pass'
    }

    $proc = Start-Process -FilePath 'powershell.exe' -ArgumentList $driverArgs `
        -RedirectStandardOutput $driverOut -RedirectStandardError $driverErr -NoNewWindow -PassThru
    # Touch .Handle so the Process object caches the OS handle; without this
    # .HasExited / .ExitCode are unreliable (often null) after the child exits.
    [void]$proc.Handle
    $childPid = $proc.Id
    Write-GuardLine -Kind 'LAUNCH' -Message "driver pid=$childPid"

    $startTime = Get-Date
    $lastProgress = Get-Date
    $lastAssess = Get-Date
    $outOffset = 0L
    $errOffset = 0L
    $rounds = 0
    $cmdRecent = [System.Collections.Generic.Queue[string]]::new()
    $sigRecent = [System.Collections.Generic.Queue[string]]::new()
    $allRecent = [System.Collections.Generic.Queue[string]]::new()

    while ($true) {
        Start-Sleep -Seconds $PollIntervalSec

        if ($proc.HasExited) {
            # Drain any final output the child wrote before exit.
            foreach ($stream in @(@{ p = $driverOut; o = [ref]$outOffset }, @{ p = $driverErr; o = [ref]$errOffset })) {
                $chunk = Read-NewText -Path $stream.p -Offset $stream.o
                if ($chunk) {
                    foreach ($ln in ($chunk -split "`r?`n")) {
                        if ($ln -ne '') {
                            $src = Get-RecordSource -Line $ln
                            Add-Utf8Line -Path $script:LogPath -Text ("[driver:$src] $ln")
                            switch ($src) {
                                'command' { $cmdRecent.Enqueue($ln) }
                                'signal'  { $sigRecent.Enqueue($ln) }
                            }
                            $allRecent.Enqueue($ln)
                        }
                    }
                }
            }
            $exit = $proc.ExitCode
            Write-GuardLine -Kind 'EXIT' -Message "driver exited code=$exit"

            # A final hard-rule sweep over drained output, then the exit code.
            $elapsedMin = ((Get-Date) - $startTime).TotalMinutes
            $recentText = ($allRecent.ToArray() -join "`n")
            $rule = Test-HardRule -CommandText ($cmdRecent.ToArray() -join "`n") -SignalText ($sigRecent.ToArray() -join "`n") `
                -ElapsedMinutes $elapsedMin -StallElapsed 0 -CorrectionRounds $rounds
            if ($rule) {
                Stop-GuardRun -Trigger 'hard-rule' -Reason $rule -ChildPid 0 -RecentText $recentText
                return 'aborted'
            }
            if ($exit -ne 0) {
                Stop-GuardRun -Trigger 'driver-exit' -Reason "driver exited non-zero ($exit)" -ChildPid 0 -RecentText $recentText
                return 'aborted'
            }
            $script:LastDriverTickDecision = Get-DriverTickContinuationDecision -ExitCode $exit -RecentText $recentText
            Write-GuardLine -Kind 'DONE' -Message "driver completed exit=0; no anomaly detected; next=$($script:LastDriverTickDecision.StopKind) -- $($script:LastDriverTickDecision.Reason)"
            return 'completed'
        }

        $fresh = @()
        foreach ($stream in @(@{ p = $driverOut; o = [ref]$outOffset }, @{ p = $driverErr; o = [ref]$errOffset })) {
            $chunk = Read-NewText -Path $stream.p -Offset $stream.o
            if ($chunk) {
                foreach ($ln in ($chunk -split "`r?`n")) {
                    if ($ln -ne '') { $fresh += $ln }
                }
            }
        }

        foreach ($ln in $fresh) {
            $src = Get-RecordSource -Line $ln
            Add-Utf8Line -Path $script:LogPath -Text ("[driver:$src] $ln")
            switch ($src) {
                'command' { $cmdRecent.Enqueue($ln) }
                'signal'  { $sigRecent.Enqueue($ln) }
            }
            $allRecent.Enqueue($ln)
            if ($ln -match '\bexecution round\s+(\d+)\b') {
                $r = [int]$Matches[1]
                if ($r -gt $rounds) { $rounds = $r }
            }
            $lastProgress = Get-Date
        }
        foreach ($q in @($cmdRecent, $sigRecent, $allRecent)) { while ($q.Count -gt 20) { [void]$q.Dequeue() } }

        $elapsedMin = ((Get-Date) - $startTime).TotalMinutes
        $stallMin = ((Get-Date) - $lastProgress).TotalMinutes

        $rule = Test-HardRule -CommandText ($cmdRecent.ToArray() -join "`n") -SignalText ($sigRecent.ToArray() -join "`n") `
            -ElapsedMinutes $elapsedMin -StallElapsed $stallMin -CorrectionRounds $rounds
        if ($rule) {
            Stop-GuardRun -Trigger 'hard-rule' -Reason $rule -ChildPid $childPid -RecentText ($allRecent.ToArray() -join "`n")
            return 'aborted'
        }

        if (((Get-Date) - $lastAssess).TotalSeconds -ge $AssessIntervalSec -and $allRecent.Count -gt 0) {
            $assessment = Invoke-ClaudeAssess -RecentText ($allRecent.ToArray() -join "`n")
            Write-GuardLine -Kind 'ASSESS' -Message "verdict=$($assessment.verdict) -- $($assessment.reason)" -Data @{ verdict = $assessment.verdict }
            $lastAssess = Get-Date
            if ($assessment.verdict -eq 'abort') {
                Stop-GuardRun -Trigger 'llm-veto' -Reason $assessment.reason -ChildPid $childPid -RecentText ($allRecent.ToArray() -join "`n")
                return 'aborted'
            }
        }
    }
}

function New-GuardDriverArguments {
    [CmdletBinding()]
    [OutputType([string[]])]
    param(
        [Parameter(Mandatory = $true)]
        [string]$DriverCommand,

        [ValidateSet('claude', 'codex')]
        [string]$Executor = 'codex',

        [ValidateSet('branch', 'pr', 'main')]
        [string]$PublishMode = 'pr',

        [ValidateRange(1, 200)]
        [int]$MaxAutonomousTasks = 1,

        [bool]$CodexExecutorExternalScratch = $false
    )

    $args = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $DriverCommand,
        '-Executor', $Executor, '-PublishMode', $PublishMode,
        '-MaxAutonomousTasks', $MaxAutonomousTasks)
    if ($CodexExecutorExternalScratch) { $args += '-CodexExecutorExternalScratch' }
    return ,$args
}

function Invoke-GuardLiveBatch {
    [CmdletBinding()]
    param()

    for ($tick = 1; $tick -le $DriverTicks; $tick++) {
        Write-GuardLine -Kind 'TICK' -Message "starting driver tick $tick of $DriverTicks"
        $disposition = Invoke-GuardLiveRun -TickIndex $tick
        if ($disposition -eq 'aborted') {
            return 'aborted'
        }

        if (-not $script:LastDriverTickDecision.ShouldContinue) {
            Write-GuardLine -Kind 'DONE' -Message "stopping guarded batch after tick $($tick): $($script:LastDriverTickDecision.StopKind) -- $($script:LastDriverTickDecision.Reason)"
            return 'completed'
        }

        if ($tick -lt $DriverTicks) {
            Write-GuardLine -Kind 'NEXT' -Message "tick $tick completed; launching a fresh Auto selector tick"
        }
    }

    Write-GuardLine -Kind 'DONE' -Message "guarded batch reached finite DriverTicks cap ($DriverTicks)"
    return 'completed'
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

# Testability seam: dot-source with RGE_AI_DISPATCH_GUARD_SKIP_MAIN=1 to load the
# functions (Get-RecordSource, Test-HardRule, ...) for unit tests without launching
# a run. Mirrors the queue/auto SKIP_MAIN seams.
if ($env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -eq '1') { return }

if ($CodexExecutorExternalScratch -and $Executor -ne 'codex') {
    Fail "-CodexExecutorExternalScratch is only valid with -Executor codex; it does not apply to Claude execution."
}

Write-GuardLine -Kind 'START' -Message "guard start dispatch=$DispatchId dryRun=$($DryRun.IsPresent) outcome=$DryRunOutcome driver=$DriverCommand executor=$Executor publish=$PublishMode tasks=$MaxAutonomousTasks driverTicks=$DriverTicks assessEvery=${AssessIntervalSec}s poll=${PollIntervalSec}s maxRun=${MaxRunMinutes}m stall=${StallMinutes}m mockAssess=$($MockAssess.IsPresent)"

if ($DryRun) {
    $disposition = Invoke-GuardDryRun
}
else {
    $disposition = Invoke-GuardLiveBatch
}

Write-GuardLine -Kind 'END' -Message "guard end disposition=$disposition"

if ($disposition -eq 'aborted') {
    exit 2
}
exit 0
