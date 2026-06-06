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
    It 'defaults Executor to claude on every automation entry point' {
        foreach ($scriptPath in @(
                $script:LoopScriptPath,
                $script:QueueScriptPath,
                $script:AutoScriptPath,
                $script:ScheduleScriptPath)) {
            Get-ParameterDefaultValueString -ScriptPath $scriptPath -ParameterName 'Executor' |
                Should -Be 'claude'
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
}

Describe 'Loop contains an opt-in Codex execution branch' {
    BeforeAll {
        $script:LoopText = [System.IO.File]::ReadAllText($script:LoopScriptPath)
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
