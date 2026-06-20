#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0' }

# Unit coverage for Invoke-AiDispatchGuard.ps1 safety logic: source classification
# (review #3) and the broadened protected-ref / force-push + signal hard rules
# (review #1 / #2). Dot-sources the guard via its RGE_AI_DISPATCH_GUARD_SKIP_MAIN
# seam so Main never launches a run -- no child process, no claude, no git, no gh.

BeforeAll {
    $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN = '1'
    $guard = Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchGuard.ps1'
    . $guard -DispatchId 'PESTER' -WatchRoot $TestDrive
    $ErrorActionPreference = 'Continue'
}

AfterAll {
    Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue
}

Describe 'New-GuardDriverArguments forwards autonomy + surface-split flags to the driver' {
    It 'forwards every armed flag so the guarded path can reach surface-split (not raw -PublishMode main)' {
        $a = New-GuardDriverArguments -DriverCommand '.\Invoke-AiDispatchAuto.ps1' `
            -Executor 'codex' -PublishMode 'main' -MaxAutonomousTasks 5 `
            -AllowCodexSelfRearm $true `
            -AutoRearmCeilingSurface @('crates/editor-ui/tests', 'ai_handoffs') `
            -DelegateSeatbeltReview $true `
            -AllowCodexClearHalt $true `
            -MaxConsecutiveFailures 3 `
            -SurfaceSplitPublish $true `
            -MaxDiffFiles 40 -MaxDiffLines 1500
        $joined = $a -join ' '
        $a | Should -Contain '-AllowCodexSelfRearm'
        $a | Should -Contain '-DelegateSeatbeltReview'
        $a | Should -Contain '-AllowCodexClearHalt'
        $a | Should -Contain '-SurfaceSplitPublish'
        $joined | Should -Match '-AutoRearmCeilingSurface crates/editor-ui/tests,ai_handoffs'
        $joined | Should -Match '-MaxConsecutiveFailures 3'
        $joined | Should -Match '-MaxDiffFiles 40'
        $joined | Should -Match '-MaxDiffLines 1500'
    }

    It 'omits every flag at its default so the off-path driver invocation is byte-for-byte unchanged' {
        $a = New-GuardDriverArguments -DriverCommand '.\Invoke-AiDispatchAuto.ps1' `
            -Executor 'codex' -PublishMode 'pr' -MaxAutonomousTasks 1
        $a | Should -Not -Contain '-AllowCodexSelfRearm'
        $a | Should -Not -Contain '-AutoRearmCeilingSurface'
        $a | Should -Not -Contain '-DelegateSeatbeltReview'
        $a | Should -Not -Contain '-AllowCodexClearHalt'
        $a | Should -Not -Contain '-MaxConsecutiveFailures'
        $a | Should -Not -Contain '-SurfaceSplitPublish'
        $a | Should -Not -Contain '-MaxDiffFiles'
        $a | Should -Not -Contain '-MaxDiffLines'
        # The historical four-arg driver invocation is preserved verbatim.
        ($a -join ' ') | Should -Match '-Executor codex -PublishMode pr -MaxAutonomousTasks 1'
    }
}

Describe 'Get-RecordSource' {
    It 'classifies loop/gate status lines as signal: <Line>' -ForEach @(
        @{ Line = 'VERIFY OK: all 7 verification step(s) passed.' }
        @{ Line = 'VERIFY FAILED: ratio too high' }
        @{ Line = 'GATE_EXIT=2' }
        @{ Line = 'Codex execution round 1: executed' }
        @{ Line = 'control verdict=pass' }
        @{ Line = 'HANDOFF_STATUS: BLOCKED' }
    ) {
        Get-RecordSource -Line $Line | Should -Be 'signal'
    }

    It 'classifies an echoed command as command: <Line>' -ForEach @(
        @{ Line = '+ git push origin main' }
        @{ Line = 'git push origin HEAD:main' }
        @{ Line = 'PS A:\rge> git status' }
        @{ Line = 'cargo test --workspace' }
    ) {
        Get-RecordSource -Line $Line | Should -Be 'command'
    }

    It 'classifies free prose as prose -- a mention is not an action (review #3): <Line>' -ForEach @(
        @{ Line = 'The TASK says: do not run git push origin main under any circumstances.' }
        @{ Line = 'Codex selected task DEMO-1 from the queue' }
        @{ Line = 'Reasoning about whether to publish to main later' }
    ) {
        Get-RecordSource -Line $Line | Should -Be 'prose'
    }
}

Describe 'Test-HardRule command patterns (review #1/#2: broadened protected-ref + force)' {
    It 'trips on protected-ref push form: <Cmd>' -ForEach @(
        @{ Cmd = 'git push origin main' }
        @{ Cmd = 'git push origin master' }
        @{ Cmd = 'git push origin HEAD:main' }
        @{ Cmd = 'git push origin refs/heads/main' }
        @{ Cmd = 'git push --set-upstream origin main' }
        @{ Cmd = 'git push origin +main:main' }
        @{ Cmd = 'git push --force origin main' }
        @{ Cmd = 'git push -f origin master' }
        @{ Cmd = 'git push --force origin feature/widget' }
        @{ Cmd = 'git push origin +feature/widget' }
    ) {
        Test-HardRule -CommandText $Cmd -SignalText '' -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 |
            Should -Match 'forbidden-command'
    }

    It 'does NOT trip on a non-protected push: <Cmd>' -ForEach @(
        @{ Cmd = 'git push origin feature/widget' }
        @{ Cmd = 'git push origin feature/main-fix' }
        @{ Cmd = 'git push origin ai-dispatch/ISSUE-42' }
    ) {
        Test-HardRule -CommandText $Cmd -SignalText '' -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 |
            Should -BeNullOrEmpty
    }

    It 'does NOT trip when the dangerous push appears only in non-command text (review #3)' {
        Test-HardRule -CommandText '' -SignalText 'note: do not git push origin main' `
            -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 | Should -BeNullOrEmpty
    }
}

Describe 'Test-HardRule signal patterns' {
    It 'trips on failure signal: <Sig>' -ForEach @(
        @{ Sig = 'VERIFY FAILED: gate' }
        @{ Sig = 'GATE_EXIT=101' }
        @{ Sig = 'HANDOFF_STATUS: NEEDS_HUMAN' }
        @{ Sig = 'control verdict=block' }
    ) {
        Test-HardRule -CommandText '' -SignalText $Sig -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 |
            Should -Match 'forbidden-signal'
    }

    It 'does NOT trip on a passing gate / pass verdict: <Sig>' -ForEach @(
        @{ Sig = 'VERIFY OK: all 7 verification step(s) passed.' }
        @{ Sig = 'GATE_EXIT=0' }
        @{ Sig = 'control verdict=pass' }
    ) {
        Test-HardRule -CommandText '' -SignalText $Sig -ElapsedMinutes 0 -StallElapsed 0 -CorrectionRounds 0 |
            Should -BeNullOrEmpty
    }
}

Describe 'Test-HardRule stall limit' {
    It 'trips when the no-progress duration reaches the stall limit' {
        Test-HardRule -CommandText '' -SignalText '' -ElapsedMinutes 1 -StallElapsed 999 -CorrectionRounds 0 |
            Should -Match 'stalled'
    }
}

Describe 'Get-DriverTickContinuationDecision' {
    It 'continues after a successful useful Auto tick' {
        $decision = Get-DriverTickContinuationDecision -ExitCode 0 -RecentText @'
Queue is empty; asking Codex to select the next task...
Codex selected:
Dispatch queue exited with code 0.
Main mode: a passed task was fast-forwarded onto origin/main.
'@

        $decision.ShouldContinue | Should -BeTrue
        $decision.StopKind | Should -Be 'continue'
    }

    It 'stops after selector no-work and cap states' -ForEach @(
        @{ Text = 'Codex reports no real task to select (brief empty/placeholder, or all tasks done).'; Kind = 'no-selection' }
        @{ Text = "HALTED for review: open autonomous-issue backlog reached (5 of 5 open 'ai-auto' issues). Publishing or review may be stuck."; Kind = 'cap-reached' }
        @{ Text = 'SEATBELT: 50 new autonomous tasks since last review; pausing for human review.'; Kind = 'seatbelt-pause' }
        @{ Text = 'HALTED: seatbelt counter .ai/dispatch.auto-seatbelt.json is corrupt; wrote halt sentinel. Repair/delete it and the sentinel to resume.'; Kind = 'seatbelt-corrupt' }
        @{ Text = 'Queue state ambiguous after primary check and cross-check; skipping this autonomous tick without filing new work.'; Kind = 'queue-ambiguous' }
        @{ Text = 'HALTED: a prior tick recorded a fault in A:\rcad\rge\.ai\dispatch.auto-halt.'; Kind = 'halt-sentinel' }
        @{ Text = "HALTED: autonomous task #123 ('Demo') is marked 'ai-dispatch-failed'."; Kind = 'failed-issue' }
    ) {
        $decision = Get-DriverTickContinuationDecision -ExitCode 0 -RecentText $Text
        $decision.ShouldContinue | Should -BeFalse
        $decision.StopKind | Should -Be $Kind
    }
}

Describe 'Guard stop-patterns are pinned to the driver''s actual emitted strings (Gap-5 drift guard)' {
    BeforeAll {
        $autoScript = Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchAuto.ps1'
        $script:AutoSource = Get-Content -LiteralPath $autoScript -Raw
    }

    # Each row pins BOTH directions so a wording change on EITHER side fails the test:
    #  - GuardInput is a representative line as the driver emits it; the guard must
    #    classify it to the expected StopKind          (guard  <- representative)
    #  - SourceAnchor is a STATIC substring of the driver's Write-Output literal; it
    #    must still be present in Invoke-AiDispatchAuto.ps1 (representative <- driver)
    # Together they pin guard <-> driver. This is the regression that the cap-reached
    # drift (guard said 'autonomous task cap reached', driver emits 'open
    # autonomous-issue backlog reached') and the missing seatbelt patterns survived
    # because no test asserted the guard regexes against the driver's real output.
    It 'pins <Kind> against the driver source' -ForEach @(
        @{ Kind = 'lock-held';        GuardInput = 'Another autonomous dispatch tick is already running; skipping this tick.'; SourceAnchor = 'Another autonomous dispatch tick is already running' }
        @{ Kind = 'halt-sentinel';    GuardInput = 'HALTED: a prior tick recorded a fault in A:\rge\.ai\dispatch.auto-halt.'; SourceAnchor = 'HALTED: a prior tick recorded a fault in ' }
        @{ Kind = 'failed-issue';     GuardInput = "HALTED: autonomous task #7 ('Demo') is marked 'ai-dispatch-failed'."; SourceAnchor = 'HALTED: autonomous task #' }
        @{ Kind = 'cap-reached';      GuardInput = "HALTED for review: open autonomous-issue backlog reached (5 of 5 open 'ai-auto' issues). Publishing or review may be stuck."; SourceAnchor = 'HALTED for review: open autonomous-issue backlog reached (' }
        @{ Kind = 'seatbelt-pause';   GuardInput = 'SEATBELT: 50 new autonomous tasks since last review; pausing for human review.'; SourceAnchor = 'new autonomous tasks since last review; pausing for human review.' }
        @{ Kind = 'seatbelt-corrupt'; GuardInput = 'HALTED: seatbelt counter .ai/dispatch.auto-seatbelt.json is corrupt; wrote halt sentinel. Repair/delete it and the sentinel to resume.'; SourceAnchor = 'is corrupt; wrote halt sentinel.' }
        @{ Kind = 'queue-ambiguous';  GuardInput = 'Queue state ambiguous after primary check and cross-check; skipping this autonomous tick without filing new work.'; SourceAnchor = 'Queue state ambiguous after primary check and cross-check' }
        @{ Kind = 'no-brief';         GuardInput = 'No task brief at A:\rge\.ai\dispatch.tasks.md - nothing to select. Create it to arm the loop.'; SourceAnchor = 'nothing to select. Create it to arm the loop.' }
        @{ Kind = 'empty-brief';      GuardInput = 'Task brief A:\rge\.ai\dispatch.tasks.md is empty; nothing to select.'; SourceAnchor = 'is empty; nothing to select.' }
        @{ Kind = 'brief-unarmed';    GuardInput = 'Task brief X carries the DISPATCH-TASKS-UNARMED marker; the autonomous loop is not armed. Nothing selected.'; SourceAnchor = 'DISPATCH-TASKS-UNARMED marker' }
        @{ Kind = 'no-selection';     GuardInput = 'Codex reports no real task to select (brief empty/placeholder, or all tasks done).'; SourceAnchor = 'Codex reports no real task to select' }
        @{ Kind = 'dry-run';          GuardInput = 'DryRun: no issue created, queue not run.'; SourceAnchor = 'queue not run' }
    ) {
        $decision = Get-DriverTickContinuationDecision -ExitCode 0 -RecentText $GuardInput
        $decision.StopKind | Should -Be $Kind
        $decision.ShouldContinue | Should -BeFalse
        $script:AutoSource | Should -Match ([regex]::Escape($SourceAnchor))
    }
}

Describe 'Convert-MonitorAssessmentResponse' {
    It 'accepts exact plain ok from the monitor' {
        $assessment = Convert-MonitorAssessmentResponse -Text 'ok'

        $assessment.verdict | Should -Be 'ok'
        $assessment.reason | Should -Match 'plain ok'
    }

    It 'accepts exact plain abort from the monitor' {
        $assessment = Convert-MonitorAssessmentResponse -Text 'abort'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'plain abort'
    }

    It 'parses the requested strict JSON ok response' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":"ok","reason":"healthy"}'

        $assessment.verdict | Should -Be 'ok'
        $assessment.reason | Should -Be 'healthy'
    }

    It 'parses the requested strict JSON abort response' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":"abort","reason":"scope creep beyond TASK"}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Be 'scope creep beyond TASK'
    }

    It 'fails closed when strict JSON carries an unsupported verdict value' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":"warn","reason":"unsure"}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'invalid verdict field'
    }

    It 'accepts a JSON-like unquoted ok verdict from the monitor' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":ok,"reason":"healthy"}'

        $assessment.verdict | Should -Be 'ok'
        $assessment.reason | Should -Be 'healthy'
    }

    It 'accepts a JSON-like unquoted abort verdict from the monitor' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":abort,"reason":"scope creep"}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Be 'scope creep'
    }

    It 'accepts an object-like bare ok verdict key from the monitor' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{verdict:ok,reason:"healthy"}'

        $assessment.verdict | Should -Be 'ok'
        $assessment.reason | Should -Be 'healthy'
    }

    It 'accepts an object-like bare abort verdict key from the monitor' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{verdict:"abort",reason:"scope creep"}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Be 'scope creep'
    }

    It 'recovers a quoted ok verdict when only the reason field is malformed' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":"ok","reason":}'

        $assessment.verdict | Should -Be 'ok'
        $assessment.reason | Should -Match 'recovered from malformed object'
    }

    It 'stays authoritative on abort when the reason field is malformed' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":"abort","reason":}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'recovered from malformed object'
    }

    It 'fails closed on an object-like response with no recognizable verdict' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"status":"fine","reason":}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'parse error'
    }

    It 'fails closed when a malformed quoted verdict has an ok prefix' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":"ok-bad","reason":}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'parse error'
    }

    It 'fails closed when a malformed bare verdict has an ok prefix' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{"verdict":ok-bad,"reason":"x"}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'parse error'
    }

    It 'does not treat a longer bare key ending in verdict as a verdict key' {
        $assessment = Convert-MonitorAssessmentResponse -Text '{myverdict:ok,reason:"x"}'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'parse error'
    }

    It 'fails closed on malformed non-ok prose' {
        $assessment = Convert-MonitorAssessmentResponse -Text 'healthy enough'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'unparseable'
    }

    It 'does not treat arbitrary prose containing ok as an ok verdict' {
        $assessment = Convert-MonitorAssessmentResponse -Text 'looks ok to me, proceeding'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'unparseable'
    }
}

Describe 'Invoke-GuardLiveRun final-drain safety sweep' {
    It 'aborts when a forbidden command is emitted immediately before child exit' {
        $mockDriver = Join-Path $TestDrive 'instant-danger.ps1'
        Set-Content -LiteralPath $mockDriver -Encoding utf8 -Value @'
param(
    [string]$Executor,
    [string]$PublishMode,
    [int]$MaxAutonomousTasks
)
Write-Output "+ git push origin main"
exit 0
'@

        $guard = Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchGuard.ps1'
        $watchRoot = Join-Path $TestDrive 'watch'
        $oldSkipMain = $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN
        Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue
        try {
            $proc = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
                '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $guard,
                '-DispatchId', 'FINAL-DRAIN',
                '-DriverCommand', $mockDriver,
                '-MockAssess',
                '-AssessIntervalSec', '120',
                '-PollIntervalSec', '2',
                '-WatchRoot', $watchRoot
            ) -Wait -PassThru -NoNewWindow
        }
        finally {
            if ($oldSkipMain) { $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN = $oldSkipMain }
            else { Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue }
        }

        $proc.ExitCode | Should -Be 2
        Get-Content -LiteralPath (Join-Path $watchRoot 'FINAL-DRAIN\abort-report.md') -Raw |
            Should -Match 'forbidden-command'
    }
}

Describe 'Invoke-GuardLiveBatch multi-tick selector loop' {
    It 'launches a fresh second Auto tick and stops on no-selection' {
        $mockDriver = Join-Path $TestDrive 'multi-tick-driver.ps1'
        Set-Content -LiteralPath $mockDriver -Encoding utf8 -Value @'
param(
    [string]$Executor,
    [string]$PublishMode,
    [int]$MaxAutonomousTasks
)
$counterPath = Join-Path $PSScriptRoot 'multi-tick-count.txt'
$count = 0
if (Test-Path -LiteralPath $counterPath) {
    $raw = Get-Content -Raw -LiteralPath $counterPath
    if ($raw -match '^\d+$') { $count = [int]$raw }
}
$count++
Set-Content -LiteralPath $counterPath -Value ([string]$count) -NoNewline -Encoding utf8
if ($count -eq 1) {
    Write-Output 'Queue is empty; asking Codex to select the next task...'
    Write-Output 'Codex selected:'
    Write-Output 'Dispatch queue exited with code 0.'
    exit 0
}
Write-Output 'Codex reports no real task to select (brief empty/placeholder, or all tasks done).'
exit 0
'@

        $guard = Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchGuard.ps1'
        $watchRoot = Join-Path $TestDrive 'watch-multi'
        $oldSkipMain = $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN
        Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue
        try {
            $proc = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
                '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $guard,
                '-DispatchId', 'MULTI-TICK',
                '-DriverCommand', $mockDriver,
                '-MockAssess',
                '-DriverTicks', '5',
                '-AssessIntervalSec', '120',
                '-PollIntervalSec', '2',
                '-WatchRoot', $watchRoot
            ) -Wait -PassThru -NoNewWindow
        }
        finally {
            if ($oldSkipMain) { $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN = $oldSkipMain }
            else { Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue }
        }

        $proc.ExitCode | Should -Be 0
        Get-Content -LiteralPath (Join-Path $TestDrive 'multi-tick-count.txt') -Raw |
            Should -Be '2'
        $watchLog = Get-Content -LiteralPath (Join-Path $watchRoot 'MULTI-TICK\watch.log') -Raw
        $watchLog | Should -Match 'launching a fresh Auto selector tick'
        $watchLog | Should -Match 'stopping guarded batch after tick 2: no-selection'
        Test-Path -LiteralPath (Join-Path $watchRoot 'MULTI-TICK\driver.tick1.stdout.log') |
            Should -BeTrue
        Test-Path -LiteralPath (Join-Path $watchRoot 'MULTI-TICK\driver.tick2.stdout.log') |
            Should -BeTrue
    }
}

Describe 'Path anchoring (TICK114B): relative -WatchRoot with mismatched .NET current directory' {
    It 'resolves a relative -WatchRoot against $PWD even when the .NET cwd points elsewhere' {
        # Recreates the TICK114B failure: PowerShell $PWD and the process-level
        # .NET current directory disagree, and -WatchRoot is relative. Before the
        # anchoring fix the guard died on its first watch-log write (the dir was
        # created under $PWD, the [System.IO.File] write resolved the stale .NET
        # cwd). After the fix the run must complete and the watch log must land
        # under $PWD's resolution of the relative root.
        $work = Join-Path $TestDrive 'anchor-work'
        $other = Join-Path $TestDrive 'anchor-other'
        $null = New-Item -ItemType Directory -Force -Path $work
        $null = New-Item -ItemType Directory -Force -Path $other

        $guard = (Resolve-Path (Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchGuard.ps1')).Path
        $childCmd = "Set-Location -LiteralPath '$work'; " +
            "[System.Environment]::CurrentDirectory = '$other'; " +
            "& '$guard' -DryRun -DispatchId ANCHOR -WatchRoot 'rel\watch'"

        $oldSkipMain = $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN
        Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue
        try {
            $proc = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
                '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $childCmd
            ) -Wait -PassThru -NoNewWindow
        }
        finally {
            if ($oldSkipMain) { $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN = $oldSkipMain }
            else { Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue }
        }

        $proc.ExitCode | Should -Be 0
        $expectedLog = Join-Path $work 'rel\watch\ANCHOR\watch.log'
        Test-Path -LiteralPath $expectedLog | Should -BeTrue
        Get-Content -LiteralPath $expectedLog -Raw | Should -Match 'guard start dispatch=ANCHOR'
        # And nothing may have resolved against the mismatched .NET cwd.
        Test-Path -LiteralPath (Join-Path $other 'rel\watch\ANCHOR\watch.log') | Should -BeFalse
    }
}

Describe 'Invoke-ClaudeAssess prompt construction + delivery seam' {
    # The model call is isolated behind Invoke-MonitorModel (stdin delivery; argv
    # delivery mangled the rubric's embedded quotes under PS 5.1 and the activity
    # tail never reached the model). These tests mock the seam, so they are
    # CI-safe (no claude binary) and prove the CONSTRUCTION: the activity text
    # must be inside the prompt the seam receives, and the prompt must be
    # persisted for diagnosability.
    It 'includes the recent activity and the format spec in the prompt handed to the model' {
        Mock Invoke-MonitorModel {
            param([string]$Prompt)
            $script:CapturedPrompt = $Prompt
            return '{"verdict":"ok","reason":"mocked"}'
        }

        $canary = 'CANARY-ACTIVITY-7f3a9 the executor deleted crates'
        $assessment = Invoke-ClaudeAssess -RecentText $canary

        $assessment.verdict | Should -Be 'ok'
        $assessment.reason | Should -Be 'mocked'
        $script:CapturedPrompt | Should -Match 'CANARY-ACTIVITY-7f3a9'
        $script:CapturedPrompt | Should -Match 'Respond with ONLY a JSON object'

        $promptFiles = Get-ChildItem -LiteralPath $script:WatchDir -Filter 'assess-*.prompt.txt'
        @($promptFiles).Count | Should -BeGreaterThan 0
        $persisted = Get-Content -LiteralPath ($promptFiles | Sort-Object Name | Select-Object -Last 1).FullName -Raw
        $persisted | Should -Match 'CANARY-ACTIVITY-7f3a9'
    }

    It 'propagates an abort verdict from the model through the seam' {
        Mock Invoke-MonitorModel {
            param([string]$Prompt)
            return '{"verdict":"abort","reason":"destructive git action observed"}'
        }

        $assessment = Invoke-ClaudeAssess -RecentText 'rm -rf executed'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Be 'destructive git action observed'
    }

    It 'fails closed when the seam returns an empty response (failed invocation)' {
        Mock Invoke-MonitorModel {
            param([string]$Prompt)
            return ''
        }

        $assessment = Invoke-ClaudeAssess -RecentText 'some activity'

        $assessment.verdict | Should -Be 'abort'
        $assessment.reason | Should -Match 'unparseable'
    }
}

Describe 'Test-GuardStopRequested (operator kill switch)' {
    It 'returns true when the stop sentinel file exists' {
        $f = Join-Path $TestDrive 'guard-stop-present'
        Set-Content -LiteralPath $f -Value 'stop' -Encoding utf8
        Test-GuardStopRequested -Path $f | Should -BeTrue
    }
    It 'returns false when the stop sentinel is absent' {
        Test-GuardStopRequested -Path (Join-Path $TestDrive 'nope-missing-sentinel') | Should -BeFalse
    }
    It 'returns false for an empty path' {
        Test-GuardStopRequested -Path '' | Should -BeFalse
    }
}

Describe 'Operator kill switch aborts a guarded batch' {
    It 'aborts (exit 2) and writes a report when the stop sentinel is present' {
        $mockDriver = Join-Path $TestDrive 'killswitch-driver.ps1'
        Set-Content -LiteralPath $mockDriver -Encoding utf8 -Value @'
param([string]$Executor, [string]$PublishMode, [int]$MaxAutonomousTasks)
Write-Output "driver should not run when the kill switch is pre-set"
exit 0
'@
        $watchRoot = Join-Path $TestDrive 'watch-killswitch'
        $stop = Join-Path $TestDrive 'guard-stop.sentinel'
        Set-Content -LiteralPath $stop -Value 'operator requested stop' -Encoding utf8

        $guard = Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchGuard.ps1'
        $oldSkipMain = $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN
        Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue
        try {
            $proc = Start-Process -FilePath 'powershell.exe' -ArgumentList @(
                '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $guard,
                '-DispatchId', 'KILLSWITCH',
                '-DriverCommand', $mockDriver,
                '-MockAssess',
                '-DriverTicks', '3',
                '-PollIntervalSec', '2',
                '-StopSentinel', $stop,
                '-WatchRoot', $watchRoot
            ) -Wait -PassThru -NoNewWindow
        }
        finally {
            if ($oldSkipMain) { $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN = $oldSkipMain }
            else { Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue }
        }

        $proc.ExitCode | Should -Be 2
        Get-Content -LiteralPath (Join-Path $watchRoot 'KILLSWITCH\abort-report.md') -Raw |
            Should -Match 'stop sentinel'
    }
}

Describe 'Test-PublishConfirmation (real-signal + out-of-band SHA)' {
    It 'confirms a main publish when origin/main advanced and both real signals were seen' {
        $d = Test-PublishConfirmation -PreSha 'aaaa' -PostSha 'bbbb' -SawVerifyOk $true -SawControlPass $true -PublishMode 'main'
        $d.Published | Should -BeTrue
        $d.Confirmed | Should -BeTrue
        $d.Anomaly | Should -BeFalse
    }

    It 'flags an anomaly when origin/main advanced under main but <Missing> was not observed' -ForEach @(
        @{ Vok = $false; Cok = $true;  Missing = 'VERIFY OK' }
        @{ Vok = $true;  Cok = $false; Missing = 'Codex control passed' }
        @{ Vok = $false; Cok = $false; Missing = 'both signals' }
    ) {
        $d = Test-PublishConfirmation -PreSha 'aaaa' -PostSha 'bbbb' -SawVerifyOk $Vok -SawControlPass $Cok -PublishMode 'main'
        $d.Anomaly | Should -BeTrue
        $d.Confirmed | Should -BeFalse
    }

    It 'flags an anomaly when origin/main advances under a non-main publish posture' {
        $d = Test-PublishConfirmation -PreSha 'aaaa' -PostSha 'bbbb' -SawVerifyOk $true -SawControlPass $true -PublishMode 'pr'
        $d.Anomaly | Should -BeTrue
        $d.Reason | Should -Match "must NOT push to main"
    }

    It 'is a no-op (confirmed, not published) when origin/main did not advance: <Mode>' -ForEach @(
        @{ Mode = 'main' }
        @{ Mode = 'pr' }
    ) {
        $d = Test-PublishConfirmation -PreSha 'aaaa' -PostSha 'aaaa' -SawVerifyOk $false -SawControlPass $false -PublishMode $Mode
        $d.Published | Should -BeFalse
        $d.Anomaly | Should -BeFalse
        $d.Confirmed | Should -BeTrue
    }

    It 'treats empty/unknown SHAs as no publish (fail-safe, no false anomaly)' {
        (Test-PublishConfirmation -PreSha '' -PostSha '' -SawVerifyOk $false -SawControlPass $false -PublishMode 'main').Anomaly | Should -BeFalse
    }
}
