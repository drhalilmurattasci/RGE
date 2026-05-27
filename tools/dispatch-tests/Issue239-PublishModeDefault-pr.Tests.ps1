#Requires -Version 5.1
<#
.SYNOPSIS
    ISSUE-239 default-publish-mode contract: assert that the queue's no-flag
    publish resolution pins to `pr`, and that Invoke-AiDispatchAuto.ps1 and
    Register-AiDispatchSchedule.ps1 default their -PublishMode parameter to
    `pr` without running a live dispatch, hitting GitHub, or registering a
    real Windows Scheduled Task.

.DESCRIPTION
    The mechanical default across the queue runner, the autonomous driver,
    and the autonomous scheduler is `pr` -- a no-flag invocation pushes the
    dispatch branch and opens a pull request targeting main for human review
    rather than fast-forwarding origin/main. This file pins that contract:

      * Resolve-DispatchPublishMode (queue, pure helper) is dot-sourced via
        the queue's test seam and called with no args; the result must be
        `pr`. Explicit -PublishMode main / -PublishMode branch / -NoPublish
        still resolve to their original modes so this dispatch only moves
        the default, not the modes themselves.

      * Invoke-AiDispatchAuto.ps1 and Register-AiDispatchSchedule.ps1 expose
        the same -PublishMode parameter contract; rather than execute them
        (which would require live gh auth, file real issues, and register a
        real Windows Scheduled Task), parse the scripts with the PowerShell
        AST and inspect the parameter's default value. The ValidateSet must
        still include all three modes so explicit `main`, `branch`, and `pr`
        remain available.

    No live dispatch, gh / git / codex / claude call, network call, real
    repository mutation, or Scheduled-Task registration runs from this file.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath    = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    $script:AutoScriptPath     = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    $script:ScheduleScriptPath = Join-Path $script:RepoRootForTest 'Register-AiDispatchSchedule.ps1'

    foreach ($p in @($script:QueueScriptPath, $script:AutoScriptPath, $script:ScheduleScriptPath)) {
        if (-not (Test-Path -LiteralPath $p)) {
            throw "ISSUE-239 default-publish-mode tests: required script not found at $p"
        }
    }

    # Pull Resolve-DispatchPublishMode into scope through the queue's existing
    # dot-source seam (the SKIP_MAIN env var short-circuits the queue's main
    # flow so dot-sourcing only registers the helpers and parameter block).
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    function script:Get-ParameterAst {
        # Parse $ScriptPath and return the ParameterAst for $ParameterName, or
        # $null when no such parameter exists. Throws on parser errors so a
        # malformed script does not silently pass these checks.
        param([string]$ScriptPath, [string]$ParameterName)
        $tokens = $null
        $errors = $null
        $ast = [System.Management.Automation.Language.Parser]::ParseFile(
            $ScriptPath, [ref]$tokens, [ref]$errors)
        if ($errors -and $errors.Count -gt 0) {
            $msgs = $errors | ForEach-Object { $_.Message }
            throw "Parser errors in $($ScriptPath): " + ($msgs -join '; ')
        }
        $matches = $ast.FindAll({
            param($n)
            $n -is [System.Management.Automation.Language.ParameterAst] -and
            $n.Name.VariablePath.UserPath -eq $ParameterName
        }, $true)
        if (-not $matches -or $matches.Count -eq 0) { return $null }
        return $matches[0]
    }

    function script:Get-ParameterDefaultValueString {
        # Return the textual default-value expression for a script parameter,
        # or '' if the parameter has no default expression. Used to pin the
        # PublishMode default to the literal 'pr' without invoking the script.
        param([string]$ScriptPath, [string]$ParameterName)
        $p = Get-ParameterAst -ScriptPath $ScriptPath -ParameterName $ParameterName
        if (-not $p) { return $null }
        if ($null -eq $p.DefaultValue) { return '' }
        if ($p.DefaultValue -is [System.Management.Automation.Language.StringConstantExpressionAst]) {
            return [string]$p.DefaultValue.Value
        }
        return [string]$p.DefaultValue.Extent.Text
    }

    function script:Get-ValidateSetValues {
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

Describe 'ISSUE-239: queue publish-mode default is pr' {

    It 'Resolve-DispatchPublishMode returns pr when no -PublishMode and no -NoPublish are given' {
        Resolve-DispatchPublishMode | Should -Be 'pr'
    }

    It 'Resolve-DispatchPublishMode returns pr when -PublishMode is the empty string' {
        Resolve-DispatchPublishMode -PublishMode '' | Should -Be 'pr'
    }

    It 'Resolve-DispatchPublishMode no-flag does not silently fall back to main' {
        Resolve-DispatchPublishMode | Should -Not -Be 'main'
    }

    It 'Resolve-DispatchPublishMode preserves explicit -PublishMode main' {
        Resolve-DispatchPublishMode -PublishMode 'main' | Should -Be 'main'
    }

    It 'Resolve-DispatchPublishMode preserves explicit -PublishMode branch' {
        Resolve-DispatchPublishMode -PublishMode 'branch' | Should -Be 'branch'
    }

    It 'Resolve-DispatchPublishMode preserves explicit -PublishMode pr' {
        Resolve-DispatchPublishMode -PublishMode 'pr' | Should -Be 'pr'
    }

    It 'Resolve-DispatchPublishMode preserves legacy -NoPublish as branch mode' {
        Resolve-DispatchPublishMode -NoPublish $true | Should -Be 'branch'
    }

    It 'Resolve-DispatchPublishMode keeps -NoPublish + -PublishMode main fail-fast' {
        { Resolve-DispatchPublishMode -PublishMode 'main' -NoPublish $true } |
            Should -Throw -ExpectedMessage '*NoPublish*main*'
    }

    It 'Resolve-DispatchPublishMode keeps -NoPublish + -PublishMode pr fail-fast' {
        { Resolve-DispatchPublishMode -PublishMode 'pr' -NoPublish $true } |
            Should -Throw -ExpectedMessage '*NoPublish*pr*'
    }
}

Describe 'ISSUE-239: Invoke-AiDispatchAuto.ps1 defaults -PublishMode to pr' {

    It 'declares -PublishMode default value pr' {
        $default = Get-ParameterDefaultValueString `
            -ScriptPath $script:AutoScriptPath -ParameterName 'PublishMode'
        $default | Should -Be 'pr'
    }

    It 'still accepts explicit branch, main, and pr via ValidateSet' {
        $values = Get-ValidateSetValues `
            -ScriptPath $script:AutoScriptPath -ParameterName 'PublishMode'
        $values | Should -Not -BeNullOrEmpty
        $values | Should -Contain 'branch'
        $values | Should -Contain 'main'
        $values | Should -Contain 'pr'
    }
}

Describe 'ISSUE-239: Register-AiDispatchSchedule.ps1 -Autonomous defaults -PublishMode to pr' {

    It 'declares -PublishMode default value pr' {
        $default = Get-ParameterDefaultValueString `
            -ScriptPath $script:ScheduleScriptPath -ParameterName 'PublishMode'
        $default | Should -Be 'pr'
    }

    It 'still accepts explicit branch, main, and pr via ValidateSet' {
        $values = Get-ValidateSetValues `
            -ScriptPath $script:ScheduleScriptPath -ParameterName 'PublishMode'
        $values | Should -Not -BeNullOrEmpty
        $values | Should -Contain 'branch'
        $values | Should -Contain 'main'
        $values | Should -Contain 'pr'
    }
}
