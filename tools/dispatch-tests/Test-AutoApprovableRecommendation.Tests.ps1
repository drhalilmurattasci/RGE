#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for Test-AutoApprovableRecommendation in
    Invoke-AiDispatchAuto.ps1 -- the fail-closed eligibility check for default-OFF
    Codex self-re-arm.

.DESCRIPTION
    Dot-sources the Auto driver through its RGE_AI_DISPATCH_AUTO_SKIP_MAIN seam so
    the pure helper loads without running a tick. No side effects. All
    recommendation fixtures use single-quoted here-strings so the backtick-quoted
    surface tokens are literal (backtick is the escape char in double quotes).
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:AutoScriptPath  = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    $env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN = '1'
    try { . $script:AutoScriptPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -ErrorAction SilentlyContinue }

    $script:Ceiling = @('crates/editor-ui/src/menus/command.rs', 'crates/editor-shell/src/render_path.rs', '.ai/dispatch.tasks.md')
}

Describe 'Test-AutoApprovableRecommendation' {
    It 'approves a qualifying recommendation (opt-in + subset surface + no stop phrase)' {
        $rec = @'
Recommendation for human approval
AUTO_APPROVABLE: yes
AUTO_APPROVE_SURFACE: `crates/editor-ui/src/menus/command.rs` `.ai/dispatch.tasks.md`
Proposed next feature: small follow-up within the audited surface.
'@
        $d = Test-AutoApprovableRecommendation -RecommendationText $rec -CeilingSurfaceTokens $Ceiling
        $d.Approvable | Should -BeTrue
        $d.ProposedSurface | Should -Contain 'crates/editor-ui/src/menus/command.rs'
    }

    It 'defers when there is no AUTO_APPROVABLE opt-in (the default for every existing recommendation)' {
        $rec = @'
Recommendation for human approval
Proposed next feature: something.
'@
        (Test-AutoApprovableRecommendation -RecommendationText $rec -CeilingSurfaceTokens $Ceiling).Approvable | Should -BeFalse
    }

    It 'defers on AUTO_APPROVABLE: no' {
        $rec = @'
AUTO_APPROVABLE: no
AUTO_APPROVE_SURFACE: `crates/editor-ui/src/menus/command.rs`
'@
        (Test-AutoApprovableRecommendation -RecommendationText $rec -CeilingSurfaceTokens $Ceiling).Approvable | Should -BeFalse
    }

    It 'defers when the proposed surface exceeds the audit ceiling (scope widening)' {
        $rec = @'
AUTO_APPROVABLE: yes
AUTO_APPROVE_SURFACE: `crates/cad-core/src/lib.rs`
'@
        $d = Test-AutoApprovableRecommendation -RecommendationText $rec -CeilingSurfaceTokens $Ceiling
        $d.Approvable | Should -BeFalse
        $d.Reason | Should -Match 'exceeds the audit ceiling'
    }

    It 'defers when a human-decision stop phrase is present despite the opt-in: <Phrase>' -ForEach @(
        @{ Phrase = 'This needs a separate human-approved packet.' }
        @{ Phrase = 'Halt and request a decision first.' }
        @{ Phrase = 'This is a human architecture decision.' }
        @{ Phrase = 'Do not auto-approve this one.' }
    ) {
        $base = @'
AUTO_APPROVABLE: yes
AUTO_APPROVE_SURFACE: `crates/editor-ui/src/menus/command.rs`
'@
        $rec = $base + "`n" + $Phrase
        (Test-AutoApprovableRecommendation -RecommendationText $rec -CeilingSurfaceTokens $Ceiling).Approvable | Should -BeFalse
    }

    It 'defers when AUTO_APPROVE_SURFACE is missing' {
        $rec = @'
AUTO_APPROVABLE: yes
Proposed next feature: something.
'@
        (Test-AutoApprovableRecommendation -RecommendationText $rec -CeilingSurfaceTokens $Ceiling).Approvable | Should -BeFalse
    }

    It 'defers when AUTO_APPROVE_SURFACE declares no backtick tokens' {
        $rec = @'
AUTO_APPROVABLE: yes
AUTO_APPROVE_SURFACE: some prose without tokens
'@
        (Test-AutoApprovableRecommendation -RecommendationText $rec -CeilingSurfaceTokens $Ceiling).Approvable | Should -BeFalse
    }

    It 'defers when no ceiling surface is provided (refuses unbounded scope)' {
        $rec = @'
AUTO_APPROVABLE: yes
AUTO_APPROVE_SURFACE: `crates/editor-ui/src/menus/command.rs`
'@
        $d = Test-AutoApprovableRecommendation -RecommendationText $rec -CeilingSurfaceTokens @()
        $d.Approvable | Should -BeFalse
        $d.Reason | Should -Match 'no ceiling surface'
    }
}
