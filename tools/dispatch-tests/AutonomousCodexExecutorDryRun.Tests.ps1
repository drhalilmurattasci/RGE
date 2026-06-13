#Requires -Version 5.1
<#
.SYNOPSIS
    Dry-run coverage for the Codex-as-executor autonomous plumbing.

.DESCRIPTION
    These tests prove the delegated Codex path is mechanically wired without
    running a live dispatch, invoking codex / claude / gh / git, publishing,
    or registering a Scheduled Task. They load only pure helper functions via
    each script's test seam or inspect parameter contracts with the
    PowerShell AST.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:LoopScriptPath     = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchLoop.ps1'
    $script:QueueScriptPath    = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    $script:AutoScriptPath     = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    $script:ScheduleScriptPath = Join-Path $script:RepoRootForTest 'Register-AiDispatchSchedule.ps1'

    foreach ($p in @(
            $script:LoopScriptPath,
            $script:QueueScriptPath,
            $script:AutoScriptPath,
            $script:ScheduleScriptPath)) {
        if (-not (Test-Path -LiteralPath $p)) {
            throw "Autonomous Codex executor dry-run tests: required script not found at $p"
        }
    }
    $script:QueueScriptText = [System.IO.File]::ReadAllText($script:QueueScriptPath)

    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    $env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN = '1'
    try {
        . $script:AutoScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    function script:Get-ParameterAst {
        param([string]$ScriptPath, [string]$ParameterName)
        $tokens = $null
        $errors = $null
        $targetName = $ParameterName
        $ast = [System.Management.Automation.Language.Parser]::ParseFile(
            $ScriptPath, [ref]$tokens, [ref]$errors)
        if ($errors -and $errors.Count -gt 0) {
            $msgs = $errors | ForEach-Object { $_.Message }
            throw "Parser errors in $($ScriptPath): " + ($msgs -join '; ')
        }
        $parameterMatches = $ast.FindAll({
            param($n)
            $n -is [System.Management.Automation.Language.ParameterAst] -and
            $n.Name.VariablePath.UserPath -eq $targetName
        }, $true)
        if (-not $parameterMatches -or $parameterMatches.Count -eq 0) { return $null }
        return $parameterMatches[0]
    }

    function script:Get-ParameterDefaultValueString {
        param([string]$ScriptPath, [string]$ParameterName)
        $p = Get-ParameterAst -ScriptPath $ScriptPath -ParameterName $ParameterName
        if (-not $p) { return $null }
        if ($null -eq $p.DefaultValue) { return '' }
        if ($p.DefaultValue -is [System.Management.Automation.Language.StringConstantExpressionAst]) {
            return [string]$p.DefaultValue.Value
        }
        return [string]$p.DefaultValue.Extent.Text
    }

    function script:Get-ValidateSetValue {
        param([string]$ScriptPath, [string]$ParameterName)
        $p = Get-ParameterAst -ScriptPath $ScriptPath -ParameterName $ParameterName
        if (-not $p) { return $null }
        foreach ($attr in $p.Attributes) {
            if ($attr -is [System.Management.Automation.Language.AttributeAst] -and
                $attr.TypeName.Name -eq 'ValidateSet') {
                return @($attr.PositionalArguments | ForEach-Object { $_.Value })
            }
        }
        return $null
    }
}

Describe 'Codex executor parameter contracts' {
    It 'defaults Executor to codex on every automation entry point' {
        foreach ($scriptPath in @(
                $script:LoopScriptPath,
                $script:QueueScriptPath,
                $script:AutoScriptPath,
                $script:ScheduleScriptPath)) {
            Get-ParameterDefaultValueString -ScriptPath $scriptPath -ParameterName 'Executor' |
                Should -Be 'codex'
        }
    }

    It 'accepts both claude and codex as Executor values everywhere' {
        foreach ($scriptPath in @(
                $script:LoopScriptPath,
                $script:QueueScriptPath,
                $script:AutoScriptPath,
                $script:ScheduleScriptPath)) {
            $values = Get-ValidateSetValue -ScriptPath $scriptPath -ParameterName 'Executor'
            $values | Should -Not -BeNullOrEmpty
            $values | Should -Contain 'claude'
            $values | Should -Contain 'codex'
        }
    }

    It 'Queue handoff claim TTL defaults to 12 hours' {
        Get-ParameterDefaultValueString -ScriptPath $script:QueueScriptPath -ParameterName 'HandoffClaimTtlSeconds' |
            Should -Be '43200'
    }
}

Describe 'Codex-as-human command dry-run' {
    It 'Auto would call Queue with main publish mode and Codex executor' {
        $queueArgs = New-AutoQueueArguments `
            -QueueScript 'Invoke-AiDispatchQueue.ps1' `
            -PublishMode 'main' `
            -MaxPlanRevisions 2 `
            -MaxCorrectionRounds 3 `
            -Executor 'codex' `
            -TraceTiming $true `
            -EnablePreflightAudit $true

        $queueArgs | Should -Contain '-File'
        $queueArgs | Should -Contain 'Invoke-AiDispatchQueue.ps1'
        $queueArgs | Should -Contain '-PublishMode'
        $queueArgs | Should -Contain 'main'
        $queueArgs | Should -Contain '-Executor'
        $queueArgs | Should -Contain 'codex'
        $queueArgs | Should -Contain '-TraceTiming'
        $queueArgs | Should -Contain '-EnablePreflightAudit'
        $queueArgs | Should -Not -Contain '-NoPublish'
    }

    It 'Queue would pass Codex executor through to the dispatch loop' {
        $loopArgs = New-DispatchLoopArguments `
            -LoopScript 'Invoke-AiDispatchLoop.ps1' `
            -DispatchId 'DRY-RUN-CODEX' `
            -GoalFile 'OLD\dry-run-goal.md' `
            -MaxPlanRevisions 2 `
            -MaxCorrectionRounds 3 `
            -Executor 'codex' `
            -EnablePreflightAudit $true

        $loopArgs | Should -Contain '-File'
        $loopArgs | Should -Contain 'Invoke-AiDispatchLoop.ps1'
        $loopArgs | Should -Contain '-DispatchId'
        $loopArgs | Should -Contain 'DRY-RUN-CODEX'
        $loopArgs | Should -Contain '-Executor'
        $loopArgs | Should -Contain 'codex'
        $loopArgs | Should -Contain '-EnablePreflightAudit'
    }

    It 'Queue progress text names Codex execute when the executor is codex' {
        $body = Format-DispatchProgressComment `
            -Stage 'loop-starting' `
            -IssueNumber 123 `
            -DispatchId 'DRY-RUN-CODEX' `
            -Branch 'ai-dispatch/ISSUE-123' `
            -LoopLogPath 'OLD\loop.log' `
            -Executor 'codex'

        $body | Should -Match 'Codex execute'
        $body | Should -Not -Match 'Claude execute'
    }
}

Describe 'Auto GitHub state snapshot for sandboxed audits' {
    It 'embeds queue evidence and tells the executor not to call gh' {
        $snapshot = Format-AutoGitHubStateSnapshot `
            -RepoSlug 'RustCADs/RGE' `
            -OpenQueueIssues @() `
            -OpenFailedAutoIssues @() `
            -FiledAutoIssues @(
                [pscustomobject]@{
                    number = 374
                    title  = 'Post-viewport-pan source audit'
                    state  = 'CLOSED'
                }
            ) `
            -GeneratedAt '2026-06-13T21:45:00.0000000+03:00'

        $snapshot | Should -Match 'Dispatcher GitHub state snapshot'
        $snapshot | Should -Match 'Open ai-dispatch issues before this issue was created:'
        $snapshot | Should -Match '\(none\)'
        $snapshot | Should -Match '#374 \[CLOSED\] Post-viewport-pan source audit'
        $snapshot | Should -Match 'Do not call gh or the network'
    }
}

Describe 'Queue ADR-121 handoff claim command dry-run' {
    It 'builds claim helper arguments with worktree events and primary live lock' {
        $claimArgs = New-HandoffClaimArguments `
            -ClaimScript 'Invoke-HandoffClaim.ps1' `
            -Action 'Claim' `
            -DispatchId 'ISSUE-CLAIM' `
            -Actor 'Invoke-AiDispatchQueue.ps1:1234' `
            -Harness 'Invoke-AiDispatchQueue.ps1' `
            -Branch 'ai-dispatch/ISSUE-CLAIM' `
            -Root 'A:\RCAD\dispatch-worktrees\ISSUE-CLAIM' `
            -LiveRoot 'A:\RCAD\RGE' `
            -TtlSeconds 43200

        $claimArgs | Should -Contain '-File'
        $claimArgs | Should -Contain 'Invoke-HandoffClaim.ps1'
        $claimArgs | Should -Contain '-Action'
        $claimArgs | Should -Contain 'Claim'
        $claimArgs | Should -Contain '-Root'
        $claimArgs | Should -Contain 'A:\RCAD\dispatch-worktrees\ISSUE-CLAIM'
        $claimArgs | Should -Contain '-LiveRoot'
        $claimArgs | Should -Contain 'A:\RCAD\RGE'
        $claimArgs | Should -Contain '-TtlSeconds'
        $claimArgs | Should -Contain 43200
        $claimArgs | Should -Contain '-JsonOnly'
    }

    It 'acquires the claim after worktree selection and before the loop starts' {
        $worktreeIdx = $script:QueueScriptText.IndexOf('$script:DispatchWorktreeRoot = $worktreePath')
        $claimIdx = $script:QueueScriptText.IndexOf('queue.claim: acquire start', $worktreeIdx)
        $loopIdx = $script:QueueScriptText.IndexOf('queue.loop: start', $claimIdx)

        $worktreeIdx | Should -BeGreaterThan -1
        $claimIdx | Should -BeGreaterThan $worktreeIdx
        $loopIdx | Should -BeGreaterThan $claimIdx
    }

    It 'releases the claim after the loop exits and before dispatch-log staging' {
        $loopDoneIdx = $script:QueueScriptText.IndexOf('queue.loop: done')
        $releaseIdx = $script:QueueScriptText.IndexOf('queue.claim: release start', $loopDoneIdx)
        $logIdx = $script:QueueScriptText.IndexOf('queue.commit: dispatch-log start', $releaseIdx)

        $loopDoneIdx | Should -BeGreaterThan -1
        $releaseIdx | Should -BeGreaterThan $loopDoneIdx
        $logIdx | Should -BeGreaterThan $releaseIdx
    }

    It 'threads the configured queue claim TTL into acquire and release calls' {
        $acquireIdx = $script:QueueScriptText.IndexOf('queue.claim: acquire start')
        $releaseIdx = $script:QueueScriptText.IndexOf('queue.claim: release start')
        $acquireIdx | Should -BeGreaterThan -1
        $releaseIdx | Should -BeGreaterThan -1

        $acquireCall = $script:QueueScriptText.Substring($acquireIdx, 500)
        $releaseCall = $script:QueueScriptText.Substring($releaseIdx, 500)
        $acquireCall | Should -Match '-TtlSeconds\s+\$HandoffClaimTtlSeconds'
        $releaseCall | Should -Match '-TtlSeconds\s+\$HandoffClaimTtlSeconds'
    }

    It 'exposes orphan claim cleanup for stale queue-owned claims' {
        (Get-Command -Name Release-OrphanHandoffClaim -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    It 'releases a queue-owned live claim without waiting for the TTL' {
        $oldRepoRoot = $script:RepoRoot
        $oldClaimScript = $script:claimScript
        $oldTtl = $HandoffClaimTtlSeconds
        try {
            $script:RepoRoot = $TestDrive
            $script:claimScript = Join-Path $script:RepoRootForTest 'Invoke-HandoffClaim.ps1'
            $script:HandoffClaimTtlSeconds = 43200

            $claimDir = Join-Path $TestDrive '.ai\handoff-claims\ISSUE-STALE'
            New-Item -ItemType Directory -Path $claimDir -Force | Out-Null
            $claim = [pscustomobject][ordered]@{
                dispatch_id = 'ISSUE-STALE'
                actor       = 'Invoke-AiDispatchQueue.ps1:1234'
                harness     = 'Invoke-AiDispatchQueue.ps1'
                branch      = 'ai-dispatch/ISSUE-STALE'
                timestamp   = '2026-06-13T12:00:00.0000000+03:00'
                ttl_seconds = 43200
                pid         = 1234
            }
            [System.IO.File]::WriteAllText(
                (Join-Path $claimDir 'claim.json'),
                ($claim | ConvertTo-Json -Depth 5),
                [System.Text.UTF8Encoding]::new($false))

            Release-OrphanHandoffClaim `
                -DispatchId 'ISSUE-STALE' `
                -Branch 'ai-dispatch/ISSUE-STALE' `
                -EventRoot $TestDrive `
                -Reason 'pester'

            Test-Path -LiteralPath (Join-Path $claimDir 'claim.json') | Should -BeFalse
            $releaseEvents = Get-ChildItem -LiteralPath (Join-Path $TestDrive 'ai_handoffs\claims') -Filter '*_release.json'
            $releaseEvents.Count | Should -Be 1
            $event = Get-Content -Raw -LiteralPath $releaseEvents[0].FullName | ConvertFrom-Json
            $event.dispatch_id | Should -Be 'ISSUE-STALE'
            $event.event | Should -Be 'release'
        } finally {
            $script:RepoRoot = $oldRepoRoot
            $script:claimScript = $oldClaimScript
            $script:HandoffClaimTtlSeconds = $oldTtl
        }
    }

    It 'clears queue-owned claims during orphan recovery before requeueing or finalizing' {
        $script:QueueScriptText | Should -Match "Release-OrphanHandoffClaim\s+(?:``\s*)?-DispatchId\s+\`$oid"
        $script:QueueScriptText | Should -Match "Release-OrphanHandoffClaim\s+(?:``\s*)?-DispatchId\s+\`$aheadId"
        $script:QueueScriptText | Should -Match "orphan recovery: interrupted run"
        $script:QueueScriptText | Should -Match "orphan recovery: already published"
        $script:QueueScriptText | Should -Match "orphan recovery: interrupted publish"
    }

    It 'pre-clears a queued issue claim before marking the issue running' {
        $cleanupIdx = $script:QueueScriptText.IndexOf("pre-claim queued issue cleanup")
        $labelSectionIdx = $script:QueueScriptText.IndexOf('# --- Mark running, build the goal')

        $cleanupIdx | Should -BeGreaterThan -1
        $labelSectionIdx | Should -BeGreaterThan $cleanupIdx
    }
}

Describe 'Loop contains Codex gate and execution routing' {
    BeforeAll {
        $script:LoopText = [System.IO.File]::ReadAllText($script:LoopScriptPath)
    }

    It 'defines a Codex plan gate alongside the Claude plan gate' {
        $script:LoopText | Should -Match 'function Invoke-CodexPlanGate'
        $script:LoopText | Should -Match 'function Invoke-ClaudePlanGate'
        $script:LoopText | Should -Match 'You are Codex acting as Executor preflight gate'
        $script:LoopText | Should -Match 'You are Claude acting as Executor preflight gate'
    }

    It 'requires Claude only when the Claude executor is selected' {
        $script:LoopText | Should -Match "if \(\`$Executor -eq 'claude'\) \{\s*Require-Command claude\s*\}"
        $script:LoopText | Should -Match "if \(\`$Executor -eq 'claude'\) \{\s*Test-ClaudeCliReady\s*\}"
    }

    It 'routes plan gate calls through Codex when the executor is codex' {
        $script:LoopText | Should -Match "if \(\`$Executor -eq 'codex'\) \{\s*Invoke-CodexPlanGate -TaskPacket"
        $script:LoopText | Should -Match 'Invoke-ClaudePlanGate -TaskPacket'
        $script:LoopText | Should -Match '\$gateFilePrefix = if \(\$Executor -eq ''codex''\)'
    }

    It 'defines Invoke-CodexExecute without replacing Invoke-ClaudeExecute' {
        $script:LoopText | Should -Match 'function Invoke-CodexExecute'
        $script:LoopText | Should -Match 'function Invoke-ClaudeExecute'
        $script:LoopText | Should -Match 'You are Executor / Codex'
        $script:LoopText | Should -Match 'You are Executor / Claude'
    }

    It 'branches on -Executor codex at the single execution call site' {
        $script:LoopText | Should -Match "if \(\`$Executor -eq 'codex'\)"
        $script:LoopText | Should -Match 'Invoke-CodexExecute -ActivePacket'
        $script:LoopText | Should -Match 'Invoke-ClaudeExecute -ActivePacket'
    }
}

Describe 'Queue requires Claude only for explicit Claude executor' {
    It 'keeps the queue preflight free of an unconditional Claude command requirement' {
        $script:QueueScriptText | Should -Match "if \(\`$Executor -eq 'claude'\) \{\s*Require-Command claude\s*\}"
        $script:QueueScriptText | Should -Not -Match "(?m)^Require-Command claude$"
    }
}
