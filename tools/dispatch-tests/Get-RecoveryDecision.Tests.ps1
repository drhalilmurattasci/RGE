#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for Get-RecoveryDecision in Invoke-AiDispatchAuto.ps1 -- the
    bounded, taxonomy-specific one-shot auto-recovery decision.

.DESCRIPTION
    Dot-sources the production Auto driver through its RGE_AI_DISPATCH_AUTO_SKIP_MAIN
    testability seam so the pure decision helper loads without running a tick or
    touching gh / git / the network. The helper has no side effects; these tests
    inherit that purity (no real issues are read or mutated).

    Pins the two recovery tiers and their bounds:
      - TRANSIENT (stall/timeout)  -> one-shot via ai-dispatch-recovered-transient
      - FLAKY (verification/control/plan-gate) -> one-shot via ai-dispatch-recovered-flaky
      - blocked / publish / unknown -> NEVER auto-recover
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:AutoScriptPath  = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    if (-not (Test-Path -LiteralPath $script:AutoScriptPath)) {
        throw "Invoke-AiDispatchAuto.ps1 not found at $script:AutoScriptPath"
    }
    $env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN = '1'
    try {
        . $script:AutoScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    $script:Std = @{
        FailLabel         = 'ai-dispatch-failed'
        QueueLabel        = 'ai-dispatch'
        DoneLabel         = 'ai-dispatch-done'
        RetryLabel        = 'ai-dispatch-retry'
        RecoverLabel      = 'ai-dispatch-recovered-transient'
        FlakyRecoverLabel = 'ai-dispatch-recovered-flaky'
        TransientLabels   = @('ai-dispatch-failure-stall', 'ai-dispatch-failure-timeout')
        FlakyLabels       = @('ai-dispatch-failure-verification', 'ai-dispatch-failure-control', 'ai-dispatch-failure-plan-gate')
    }

    function script:NewIssue([int]$Number, [string[]]$Labels) {
        [pscustomobject]@{ number = $Number; title = "issue $Number"; labels = $Labels }
    }
}

Describe 'Get-RecoveryDecision eligibility' {
    It 'is ineligible with no open failed issues' {
        (Get-RecoveryDecision -Issues @() @Std).Eligible | Should -BeFalse
    }

    It 'is ineligible with more than one open failed issue' {
        $issues = @(
            (NewIssue 1 @('ai-dispatch-failed', 'ai-dispatch-failure-stall')),
            (NewIssue 2 @('ai-dispatch-failed', 'ai-dispatch-failure-timeout'))
        )
        $d = Get-RecoveryDecision -Issues $issues @Std
        $d.Eligible | Should -BeFalse
        $d.Reason | Should -Match 'requires exactly one'
    }

    It 'recovers <Label> as the <Tier> tier with marker <Marker>' -ForEach @(
        @{ Label = 'ai-dispatch-failure-stall';        Tier = 'transient'; Marker = 'ai-dispatch-recovered-transient' }
        @{ Label = 'ai-dispatch-failure-timeout';      Tier = 'transient'; Marker = 'ai-dispatch-recovered-transient' }
        @{ Label = 'ai-dispatch-failure-verification'; Tier = 'flaky';     Marker = 'ai-dispatch-recovered-flaky' }
        @{ Label = 'ai-dispatch-failure-control';      Tier = 'flaky';     Marker = 'ai-dispatch-recovered-flaky' }
        @{ Label = 'ai-dispatch-failure-plan-gate';    Tier = 'flaky';     Marker = 'ai-dispatch-recovered-flaky' }
    ) {
        $d = Get-RecoveryDecision -Issues @((NewIssue 7 @('ai-dispatch-failed', $Label))) @Std
        $d.Eligible | Should -BeTrue
        $d.Tier | Should -Be $Tier
        $d.RecoverableLabel | Should -Be $Label
        $d.Marker | Should -Be $Marker
        $d.LabelsToRemove | Should -Contain 'ai-dispatch-failed'
        $d.LabelsToAdd | Should -Contain 'ai-dispatch'
        $d.LabelsToAdd | Should -Contain 'ai-dispatch-retry'
        $d.LabelsToAdd | Should -Contain $Marker
    }

    It 'NEVER auto-recovers non-recoverable class <Label>' -ForEach @(
        @{ Label = 'ai-dispatch-failure-blocked' }
        @{ Label = 'ai-dispatch-failure-publish' }
        @{ Label = 'ai-dispatch-failure-unknown' }
    ) {
        $d = Get-RecoveryDecision -Issues @((NewIssue 8 @('ai-dispatch-failed', $Label))) @Std
        $d.Eligible | Should -BeFalse
        $d.Reason | Should -Match 'non-recoverable'
    }

    It 'is one-shot per tier: a transient issue already carrying the transient marker is ineligible' {
        $d = Get-RecoveryDecision -Issues @((NewIssue 9 @('ai-dispatch-failed', 'ai-dispatch-failure-stall', 'ai-dispatch-recovered-transient'))) @Std
        $d.Eligible | Should -BeFalse
        $d.Reason | Should -Match 'already recovered for the transient tier'
    }

    It 'is one-shot per tier: a flaky issue already carrying the flaky marker is ineligible' {
        $d = Get-RecoveryDecision -Issues @((NewIssue 10 @('ai-dispatch-failed', 'ai-dispatch-failure-plan-gate', 'ai-dispatch-recovered-flaky'))) @Std
        $d.Eligible | Should -BeFalse
        $d.Reason | Should -Match 'already recovered for the flaky tier'
    }

    It 'bound is per-tier: a flaky failure on an issue that previously had a TRANSIENT recovery still recovers once (max two per issue)' {
        # Documents the bound: at most one recovery per tier => at most two per issue.
        $d = Get-RecoveryDecision -Issues @((NewIssue 11 @('ai-dispatch-failed', 'ai-dispatch-failure-control', 'ai-dispatch-recovered-transient'))) @Std
        $d.Eligible | Should -BeTrue
        $d.Tier | Should -Be 'flaky'
    }

    It 'is ineligible with multiple taxonomy labels (mixed)' {
        $d = Get-RecoveryDecision -Issues @((NewIssue 12 @('ai-dispatch-failed', 'ai-dispatch-failure-stall', 'ai-dispatch-failure-verification'))) @Std
        $d.Eligible | Should -BeFalse
        $d.Reason | Should -Match 'multiple taxonomy labels'
    }

    It 'is ineligible with no taxonomy label' {
        $d = Get-RecoveryDecision -Issues @((NewIssue 13 @('ai-dispatch-failed'))) @Std
        $d.Eligible | Should -BeFalse
        $d.Reason | Should -Match 'no failure taxonomy label'
    }

    It 'removes a stale done label when present' {
        $d = Get-RecoveryDecision -Issues @((NewIssue 14 @('ai-dispatch-failed', 'ai-dispatch-done', 'ai-dispatch-failure-timeout'))) @Std
        $d.Eligible | Should -BeTrue
        $d.LabelsToRemove | Should -Contain 'ai-dispatch-done'
    }
}
