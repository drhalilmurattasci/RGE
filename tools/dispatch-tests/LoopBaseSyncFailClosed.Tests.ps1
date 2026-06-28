#Requires -Version 5.1
<#
.SYNOPSIS
    Regression coverage for the origin/main sync gate in Invoke-AiDispatchLoop.ps1.

.DESCRIPTION
    Bug being pinned (bug-sweep finding C): the pre-dispatch sync gate compared the
    ahead/behind counts only when `$aheadGit.Code -eq 0`, so a non-zero `git
    rev-list` exit silently SKIPPED the gate and the loop proceeded on an
    unverifiable base (fail-open). The fix fails CLOSED on a git error -- a
    non-zero exit `Fail`s before the comparison -- mirroring the adjacent `git
    status --porcelain` check.

    Loop has no dot-source seam, so this is a source-contract pin (the inline gate
    is exercised behaviorally by the canonical verify gate + full dispatch suite).
#>

BeforeAll {
    $script:RepoRoot  = Split-Path -Parent (Split-Path -Parent (Split-Path -Parent $PSCommandPath))
    $script:LoopPath  = Join-Path $script:RepoRoot 'Invoke-AiDispatchLoop.ps1'
    $script:LoopSrc   = Get-Content -LiteralPath $script:LoopPath -Raw
}

Describe 'Loop origin/main sync gate fails closed on a git error' {
    It 'Fails when the rev-list sync check exits non-zero (before comparing counts)' {
        $script:LoopSrc | Should -Match 'if \(\$aheadGit\.Code -ne 0\) \{'
    }

    It 'no longer gates the sync comparison behind `Code -eq 0` (the fail-open form)' {
        $script:LoopSrc | Should -Not -Match '\$aheadGit\.Code -eq 0 -and'
    }

    It 'keeps the adjacent git status check fail-closed too (mirrored pattern)' {
        $script:LoopSrc | Should -Match 'if \(\$statusGit\.Code -ne 0\) \{'
    }
}
