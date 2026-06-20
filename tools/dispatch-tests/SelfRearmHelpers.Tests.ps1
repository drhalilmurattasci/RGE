#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the pure self-rearm helpers in Invoke-AiDispatchAuto.ps1
    (Get-BriefRecommendationBlock, Get-BriefTaskHeadingCount,
    Test-SelfRearmPostConditions) used by default-OFF -AllowCodexSelfRearm.

.DESCRIPTION
    Dot-sources the Auto driver through its RGE_AI_DISPATCH_AUTO_SKIP_MAIN seam.
    Pure functions; no side effects (the codex-exec / git seam in
    Invoke-CodexSelfRearm is exercised only when the loop is armed).
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:AutoScriptPath  = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    $env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN = '1'
    try { . $script:AutoScriptPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Get-BriefRecommendationBlock' {
    It 'returns the text from the marker to end of brief' {
        $brief = @'
166. Some feature
   done.
NEEDS_HUMAN_RECORDED: 2026-06-20 - audit complete
AUTO_APPROVABLE: yes
AUTO_APPROVE_SURFACE: `a/b.rs`
'@
        $b = Get-BriefRecommendationBlock -BriefText $brief
        $b | Should -Match 'NEEDS_HUMAN_RECORDED:'
        $b | Should -Match 'AUTO_APPROVABLE: yes'
        $b | Should -Not -Match '166\. Some feature'
    }

    It 'returns empty string when there is no marker' {
        (Get-BriefRecommendationBlock -BriefText "166. Feature`n167. Audit") | Should -BeNullOrEmpty
    }

    It 'uses the LAST marker when several are present' {
        $brief = "NEEDS_HUMAN_RECORDED: 2026-01-01 - old`nmid`nNEEDS_HUMAN_RECORDED: 2026-06-20 - newest`ntail"
        $b = Get-BriefRecommendationBlock -BriefText $brief
        $b | Should -Match 'newest'
        $b | Should -Not -Match 'old'
    }
}

Describe 'Get-BriefTaskHeadingCount' {
    It 'counts numbered task headings' {
        (Get-BriefTaskHeadingCount -BriefText "1. a`n2. b`nnot a heading`n10. c") | Should -Be 3
    }
    It 'is zero for no headings' {
        (Get-BriefTaskHeadingCount -BriefText "no headings here") | Should -Be 0
    }
}

Describe 'Test-SelfRearmPostConditions' {
    It 'passes when exactly one task is appended and the marker is neutralized' {
        $before = "166. Feature`nNEEDS_HUMAN_RECORDED: 2026-06-20 - audit"
        $after  = "166. Feature`nRESOLVED (auto-approved) -- kept for provenance: NEEDS_HUMAN_RECORDED: 2026-06-20 - audit`n167. Next feature"
        $d = Test-SelfRearmPostConditions -BeforeText $before -AfterText $after
        $d.Ok | Should -BeTrue
    }

    It 'fails when no new task heading was added' {
        $before = "166. Feature`nNEEDS_HUMAN_RECORDED: 2026-06-20 - audit"
        $after  = "166. Feature`nRESOLVED -- kept for provenance: NEEDS_HUMAN_RECORDED: x"
        $d = Test-SelfRearmPostConditions -BeforeText $before -AfterText $after
        $d.Ok | Should -BeFalse
        $d.Reason | Should -Match 'exactly one new task heading'
    }

    It 'fails when more than one task heading was added' {
        $before = "166. Feature`nNEEDS_HUMAN_RECORDED: 2026-06-20 - audit"
        $after  = "166. Feature`nRESOLVED: x`n167. a`n168. b"
        (Test-SelfRearmPostConditions -BeforeText $before -AfterText $after).Ok | Should -BeFalse
    }

    It 'fails when a live NEEDS_HUMAN_RECORDED marker still remains (not neutralized)' {
        $before = "166. Feature`nNEEDS_HUMAN_RECORDED: 2026-06-20 - audit"
        $after  = "166. Feature`nNEEDS_HUMAN_RECORDED: 2026-06-20 - audit`n167. Next feature"
        $d = Test-SelfRearmPostConditions -BeforeText $before -AfterText $after
        $d.Ok | Should -BeFalse
        $d.Reason | Should -Match 'still remains'
    }
}

Describe 'Test-SeatbeltReviewContinue (fail-closed)' {
    It 'is true only for an exact continue line' {
        Test-SeatbeltReviewContinue -AnswerText "reasoning...`nSEATBELT_REVIEW: continue" | Should -BeTrue
    }
    It 'is false for hold: <Ans>' -ForEach @(
        @{ Ans = 'SEATBELT_REVIEW: hold' }
        @{ Ans = 'looks fine, continue' }
        @{ Ans = 'SEATBELT_REVIEW: continue-ish' }
        @{ Ans = '' }
        @{ Ans = 'garbage' }
    ) {
        Test-SeatbeltReviewContinue -AnswerText $Ans | Should -BeFalse
    }
}
