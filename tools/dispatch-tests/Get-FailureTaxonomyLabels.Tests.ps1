#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for Get-FailureTaxonomyLabels in Invoke-AiDispatchQueue.ps1,
    focused on the plan-gate classification added so plan-gate exhaustion stops
    hiding in ai-dispatch-failure-unknown (which is non-recoverable).

.DESCRIPTION
    Dot-sources the production queue script through its RGE_AI_DISPATCH_QUEUE_SKIP_MAIN
    testability seam so the pure text classifier loads without running the dispatch
    flow or requiring gh / git / codex / claude. The helper reads no files and has
    no side effects; these tests inherit that purity.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    if (-not (Test-Path -LiteralPath $script:QueueScriptPath)) {
        throw "Invoke-AiDispatchQueue.ps1 not found at $script:QueueScriptPath"
    }
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }
    $script:QueueSource = Get-Content -LiteralPath $script:QueueScriptPath -Raw
}

Describe 'Get-FailureTaxonomyLabels classification + ordering' {
    It 'classifies <Expect> from loop text: <Why>' -ForEach @(
        @{ Loop = 'codex exec stalled: no log growth for 300s after first output. Killed process tree. See .ai/dispatch-x/codex.log'; Exec = ''; Pub = $false; Expect = 'ai-dispatch-failure-stall'; Why = 'watchdog stall' }
        @{ Loop = 'codex exec timed out after 1800s (terminal infrastructure failure). See .ai/dispatch-x/codex.log'; Exec = ''; Pub = $false; Expect = 'ai-dispatch-failure-timeout'; Why = 'codex hard timeout' }
        @{ Loop = 'claude timed out after 1800s (terminal infrastructure failure). See .ai/dispatch-x/claude.stderr.log'; Exec = ''; Pub = $false; Expect = 'ai-dispatch-failure-timeout'; Why = 'claude hard timeout' }
        @{ Loop = 'Verification timed out (over 900s) - terminal infrastructure failure, not a correctable task. See .ai/dispatch-x/verify.log'; Exec = ''; Pub = $false; Expect = 'ai-dispatch-failure-timeout'; Why = 'verification timeout' }
        @{ Loop = 'Codex did not approve the plan within MaxPlanRevisions=2. See codex.plan_gate.rev2.md'; Exec = ''; Pub = $false; Expect = 'ai-dispatch-failure-plan-gate'; Why = 'plan-gate exhaustion (was unknown)' }
        @{ Loop = 'Codex blocked the plan. See codex.plan_gate.rev0.md';         Exec = '';        Pub = $false; Expect = 'ai-dispatch-failure-plan-gate';    Why = 'plan-gate block' }
        @{ Loop = 'Verification gate failed (exit 1) and MaxCorrectionRounds=2 is exhausted. See .ai/dispatch-x/verify.log'; Exec = ''; Pub = $false; Expect = 'ai-dispatch-failure-verification'; Why = 'verify gate' }
        @{ Loop = 'Codex control blocked the dispatch. See .ai/dispatch-x/codex.control.round0.json'; Exec = ''; Pub = $false; Expect = 'ai-dispatch-failure-control'; Why = 'control block' }
        @{ Loop = 'Codex requested changes, but MaxCorrectionRounds=2 is exhausted.'; Exec = ''; Pub = $false; Expect = 'ai-dispatch-failure-control'; Why = 'control correction exhaustion' }
        @{ Loop = 'some unmatched internal error';                               Exec = '';        Pub = $false; Expect = 'ai-dispatch-failure-unknown';      Why = 'catch-all' }
    ) {
        (Get-FailureTaxonomyLabels -LoopText $Loop -ExecStatus $Exec -PublishHardFailed $Pub) |
            Should -Be @($Expect)
    }

    It 'does not classify quoted or mid-line incident text as canonical loop failures' {
        $quoted = @(
            'note: previous log said "codex exec stalled: no log growth for 300s"',
            'body quote: codex exec timed out after 1800s',
            'review text mentioned: Codex blocked the plan.',
            'summary: Verification gate failed (exit 1) and MaxCorrectionRounds=2 is exhausted.',
            'operator note: Codex requested changes, but MaxCorrectionRounds=2 is exhausted.'
        ) -join "`n"

        (Get-FailureTaxonomyLabels -LoopText $quoted -ExecStatus '' -PublishHardFailed $false) |
            Should -Be @('ai-dispatch-failure-unknown')
    }

    It 'publish-hard-failure wins over any loop text' {
        (Get-FailureTaxonomyLabels -LoopText 'Codex did not approve the plan within MaxPlanRevisions=2' -ExecStatus '' -PublishHardFailed $true) |
            Should -Be @('ai-dispatch-failure-publish')
    }

    It 'blocked execution wins over plan-gate wording' {
        (Get-FailureTaxonomyLabels -LoopText 'blocked the plan' -ExecStatus 'blocked' -PublishHardFailed $false) |
            Should -Be @('ai-dispatch-failure-blocked')
    }

    It 'a stall DURING plan-fill stays a stall, not plan-gate (ordering pin)' {
        # If a Codex plan-fill call stalls, the loop Fails with stall wording; that
        # must classify as stall (genuinely retriable as transient), NOT plan-gate.
        (Get-FailureTaxonomyLabels -LoopText 'codex exec stalled: no log growth for 300s after first output. Killed process tree. See codex.plan_gate.rev0.log' -ExecStatus '' -PublishHardFailed $false) |
            Should -Be @('ai-dispatch-failure-stall')
    }

    It 'registers the plan-gate label in the queue label spec (so gh add-label cannot fail)' {
        # The classifier returns the label; the label MUST also be in $labelSpec or
        # `gh label create` never runs it and the terminal `gh issue edit
        # --add-label` would fail. Two occurrences: the label spec + the classifier.
        ([regex]::Matches($script:QueueSource, 'ai-dispatch-failure-plan-gate').Count) |
            Should -BeGreaterOrEqual 2
    }
}
