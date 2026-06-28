#Requires -Version 5.1
<#
.SYNOPSIS
    Regression coverage for the consecutive-failure breaker counter in
    Invoke-AiDispatchAuto.ps1 (bug-sweep K).

.DESCRIPTION
    Bug being pinned: Update-ConsecutiveFailureCounter silently reset a corrupt
    counter to 0 (fail-open) -- so on a FAILING tick a persistently corrupt counter
    masked accumulated failures and the armed breaker could never trip. Now a clean
    tick still self-heals to 0, but a FAILING tick THROWS (fail-closed); the call
    site writes a human-only consec-fail-corrupt halt, mirroring the seatbelt-corrupt
    path. Dot-sourced via the RGE_AI_DISPATCH_AUTO_SKIP_MAIN seam.
#>

BeforeAll {
    $env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN = '1'
    $script:AutoPath = Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchAuto.ps1'
    try { . $script:AutoPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Update-ConsecutiveFailureCounter (K: corruption fails closed on a failing tick)' {
    BeforeAll {
        $script:root = Join-Path $TestDrive 'consecfail'
        New-Item -ItemType Directory -Path (Join-Path $script:root '.ai') -Force | Out-Null
        $script:cf = Join-Path $script:root '.ai\dispatch.auto-consecutive-failures.json'
    }
    BeforeEach { Remove-Item -LiteralPath $script:cf -Force -ErrorAction SilentlyContinue }

    It 'increments a valid counter on a failed tick' {
        Set-Content -LiteralPath $script:cf -Value '{"count":2}' -NoNewline
        Update-ConsecutiveFailureCounter -RepoRoot $script:root -Failed $true | Should -Be 3
    }

    It 'resets to 0 on a clean tick' {
        Set-Content -LiteralPath $script:cf -Value '{"count":5}' -NoNewline
        Update-ConsecutiveFailureCounter -RepoRoot $script:root -Failed $false | Should -Be 0
    }

    It 'starts at 1 with no counter file (failed tick)' {
        Update-ConsecutiveFailureCounter -RepoRoot $script:root -Failed $true | Should -Be 1
    }

    It 'THROWS on a corrupt counter during a failing tick (fail-closed, not silent reset)' {
        Set-Content -LiteralPath $script:cf -Value 'not json {{{' -NoNewline
        { Update-ConsecutiveFailureCounter -RepoRoot $script:root -Failed $true } |
            Should -Throw -ExpectedMessage '*consec-fail-corrupt*'
    }

    It 'treats valid-JSON-without-count as corrupt on a failing tick' {
        Set-Content -LiteralPath $script:cf -Value '{"other":1}' -NoNewline
        { Update-ConsecutiveFailureCounter -RepoRoot $script:root -Failed $true } | Should -Throw
    }

    It 'self-heals a corrupt counter to 0 on a CLEAN tick (no throw)' {
        Set-Content -LiteralPath $script:cf -Value 'garbage-not-json' -NoNewline
        Update-ConsecutiveFailureCounter -RepoRoot $script:root -Failed $false | Should -Be 0
    }
}

Describe 'Auto consec-fail corruption wiring (source contract)' {
    BeforeAll { $script:AutoSrc = Get-Content -LiteralPath $script:AutoPath -Raw }

    It 'the failed-tick call site catches corruption and writes a human-only consec-fail-corrupt halt' {
        $script:AutoSrc | Should -Match 'CLASS: consec-fail-corrupt'
        $script:AutoSrc | Should -Match 'halted=consec-fail-corrupt'
    }
}
