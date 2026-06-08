#Requires -Version 5.1
<#
.SYNOPSIS
    Focused dry-run coverage for ISSUE-340 Codex external-scratch routing.

.DESCRIPTION
    Proves the operator switch is absent by default, opt-in only, Codex-only,
    and limited to the Codex execution sandbox path. The tests inspect parser
    contracts and pure argument builders; they never touch B:\sdk, run Codex,
    call GitHub, register a scheduled task, or publish.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:LoopScriptPath     = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchLoop.ps1'
    $script:QueueScriptPath    = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    $script:AutoScriptPath     = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    $script:GuardScriptPath    = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchGuard.ps1'
    $script:ScheduleScriptPath = Join-Path $script:RepoRootForTest 'Register-AiDispatchSchedule.ps1'

    foreach ($p in @(
            $script:LoopScriptPath,
            $script:QueueScriptPath,
            $script:AutoScriptPath,
            $script:GuardScriptPath,
            $script:ScheduleScriptPath)) {
        if (-not (Test-Path -LiteralPath $p)) {
            throw "ISSUE-340 tests: required script not found at $p"
        }
    }

    $script:LoopText     = [System.IO.File]::ReadAllText($script:LoopScriptPath)
    $script:QueueText    = [System.IO.File]::ReadAllText($script:QueueScriptPath)
    $script:AutoText     = [System.IO.File]::ReadAllText($script:AutoScriptPath)
    $script:GuardText    = [System.IO.File]::ReadAllText($script:GuardScriptPath)
    $script:ScheduleText = [System.IO.File]::ReadAllText($script:ScheduleScriptPath)

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

    $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN = '1'
    try {
        . $script:GuardScriptPath -DispatchId 'ISSUE-340-PESTER' -WatchRoot $TestDrive
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue
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
}

Describe 'Codex external scratch parameter contract' {
    It 'exposes the switch on every automation entry point' {
        foreach ($scriptPath in @(
                $script:LoopScriptPath,
                $script:QueueScriptPath,
                $script:AutoScriptPath,
                $script:GuardScriptPath,
                $script:ScheduleScriptPath)) {
            $paramAst = Get-ParameterAst -ScriptPath $scriptPath -ParameterName 'CodexExecutorExternalScratch'
            $paramAst | Should -Not -BeNullOrEmpty
            $paramAst.StaticType.FullName | Should -Be 'System.Management.Automation.SwitchParameter'
        }
    }

    It 'fails fast when paired with the Claude executor' {
        $expected = '-CodexExecutorExternalScratch is only valid with -Executor codex'
        foreach ($text in @(
                $script:LoopText,
                $script:QueueText,
                $script:AutoText,
                $script:GuardText,
                $script:ScheduleText)) {
            $text | Should -Match ([regex]::Escape($expected))
            $text | Should -Match '\$CodexExecutorExternalScratch\s+-and\s+\$Executor\s+-ne\s+''codex'''
        }
    }
}

Describe 'Loop Codex execution sandbox routing' {
    It 'keeps workspace-write as the default Codex executor sandbox' {
        $script:LoopText | Should -Match 'function Get-CodexExecutorSandbox'
        $script:LoopText | Should -Match "return 'workspace-write'"
        $script:LoopText | Should -Match 'Invoke-CodexPrompt -Prompt \$prompt -Sandbox \(Get-CodexExecutorSandbox\) -LogPath \$out'
    }

    It 'uses danger-full-access only for the opt-in Codex executor sandbox' {
        $script:LoopText | Should -Match 'if \(\$CodexExecutorExternalScratch\) \{ return ''danger-full-access'' \}'
        $script:LoopText | Should -Not -Match 'Invoke-CodexPrompt -Prompt \$prompt -Sandbox ''danger-full-access'''
    }

    It 'leaves planner, preflight, correction, and control sandbox calls unchanged' {
        $script:LoopText | Should -Match 'Invoke-CodexPrompt -Prompt \$prompt -Sandbox ''workspace-write'' -LogPath \$log'
        $script:LoopText | Should -Match 'Invoke-CodexPrompt -Prompt \$prompt -Sandbox ''read-only'' -LogPath \$out'
        $script:LoopText | Should -Match 'Invoke-CodexPrompt -Prompt \$prompt -Sandbox ''read-only'' -LogPath \$log -OutputSchema'
        $script:LoopText | Should -Not -Match 'Invoke-CodexPrompt -Prompt \$prompt -Sandbox \(Get-CodexExecutorSandbox\) -LogPath \$log'
    }
}

Describe 'Queue and Auto pass-through argument builders' {
    It 'omits the external-scratch switch by default in Queue to Loop args' {
        $args = New-DispatchLoopArguments `
            -LoopScript 'Invoke-AiDispatchLoop.ps1' `
            -DispatchId 'ISSUE-340-DRY' `
            -GoalFile 'OLD\goal.md' `
            -Executor 'codex'

        $args | Should -Contain '-Executor'
        $args | Should -Contain 'codex'
        $args | Should -Not -Contain '-CodexExecutorExternalScratch'
    }

    It 'adds the external-scratch switch when Queue invokes the Loop opt-in' {
        $args = New-DispatchLoopArguments `
            -LoopScript 'Invoke-AiDispatchLoop.ps1' `
            -DispatchId 'ISSUE-340-DRY' `
            -GoalFile 'OLD\goal.md' `
            -Executor 'codex' `
            -CodexExecutorExternalScratch $true

        $args | Should -Contain '-CodexExecutorExternalScratch'
    }

    It 'omits the external-scratch switch by default in Auto to Queue args' {
        $args = New-AutoQueueArguments `
            -QueueScript 'Invoke-AiDispatchQueue.ps1' `
            -PublishMode 'pr' `
            -Executor 'codex'

        $args | Should -Contain '-PublishMode'
        $args | Should -Contain 'pr'
        $args | Should -Not -Contain '-CodexExecutorExternalScratch'
        $args | Should -Not -Contain '-NoPublish'
    }

    It 'adds the external-scratch switch when Auto invokes Queue opt-in without changing PR mode' {
        $args = New-AutoQueueArguments `
            -QueueScript 'Invoke-AiDispatchQueue.ps1' `
            -PublishMode 'pr' `
            -Executor 'codex' `
            -CodexExecutorExternalScratch $true

        $args | Should -Contain '-PublishMode'
        $args | Should -Contain 'pr'
        $args | Should -Contain '-CodexExecutorExternalScratch'
        $args | Should -Not -Contain '-NoPublish'
    }
}

Describe 'Guard and Scheduler pass-through construction' {
    It 'omits the external-scratch switch by default in Guard to Auto args' {
        $args = New-GuardDriverArguments `
            -DriverCommand '.\Invoke-AiDispatchAuto.ps1' `
            -Executor 'codex' `
            -PublishMode 'pr' `
            -MaxAutonomousTasks 1

        $args | Should -Contain '-PublishMode'
        $args | Should -Contain 'pr'
        $args | Should -Not -Contain '-CodexExecutorExternalScratch'
    }

    It 'adds the external-scratch switch when Guard invokes Auto opt-in' {
        $args = New-GuardDriverArguments `
            -DriverCommand '.\Invoke-AiDispatchAuto.ps1' `
            -Executor 'codex' `
            -PublishMode 'pr' `
            -MaxAutonomousTasks 1 `
            -CodexExecutorExternalScratch $true

        $args | Should -Contain '-CodexExecutorExternalScratch'
    }

    It 'threads the scheduler switch only as an optional script argument' {
        $script:ScheduleText | Should -Match '\$externalScratchArg = if \(\$CodexExecutorExternalScratch\)'
        $script:ScheduleText | Should -Match '\{ '' -CodexExecutorExternalScratch'' \} else \{ '''' \}'
        $script:ScheduleText | Should -Match '\$Executor, \$externalScratchArg'
    }
}
