#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for Get-HaltClearEligibility in Invoke-AiDispatchAuto.ps1 --
    the fail-closed policy for default-OFF -AllowCodexClearHalt.

.DESCRIPTION
    Dot-sources the Auto driver through its RGE_AI_DISPATCH_AUTO_SKIP_MAIN seam.
    Pure function; no side effects.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:AutoScriptPath  = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    $env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN = '1'
    try { . $script:AutoScriptPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Get-HaltClearEligibility' {
    It 'allows clearing self-resolved class <Class>' -ForEach @(
        @{ Class = 'seatbelt' }
        @{ Class = 'recovery' }
    ) {
        (Get-HaltClearEligibility -HaltClass $Class).Clearable | Should -BeTrue
    }

    It 'holds human-only class <Class>' -ForEach @(
        @{ Class = 'queue-exit' }
        @{ Class = 'seatbelt-corrupt' }
        @{ Class = 'consec-fail' }
        @{ Class = 'idle' }
        @{ Class = 'needs-human' }
        @{ Class = 'fault' }
        @{ Class = 'manual' }
    ) {
        (Get-HaltClearEligibility -HaltClass $Class).Clearable | Should -BeFalse
    }

    It 'fail-closed on an unknown class' {
        (Get-HaltClearEligibility -HaltClass 'something-new').Clearable | Should -BeFalse
    }

    It 'fail-closed on a blank class' {
        $d = Get-HaltClearEligibility -HaltClass ''
        $d.Clearable | Should -BeFalse
        $d.Reason | Should -Match 'no halt class'
    }

    It 'is case-insensitive and trims whitespace' {
        (Get-HaltClearEligibility -HaltClass '  SEATBELT ').Clearable | Should -BeTrue
        (Get-HaltClearEligibility -HaltClass 'Recovery').Clearable | Should -BeTrue
    }
}

Describe 'Get-HaltSentinelClass' {
    It 'extracts the class from a CLASS: line' {
        (Get-HaltSentinelClass -SentinelText "CLASS: seatbelt`r`nSeatbelt: 50 tasks...") | Should -Be 'seatbelt'
    }
    It 'lowercases the class' {
        (Get-HaltSentinelClass -SentinelText "CLASS: SeatBelt`nmsg") | Should -Be 'seatbelt'
    }
    It 'returns empty for an untagged sentinel (fail-closed to human-only)' {
        (Get-HaltSentinelClass -SentinelText 'Autonomous loop idle: ...') | Should -BeNullOrEmpty
    }
    It 'an untagged sentinel is therefore not auto-clearable' {
        $cls = Get-HaltSentinelClass -SentinelText 'some queue-exit fault text'
        (Get-HaltClearEligibility -HaltClass $cls).Clearable | Should -BeFalse
    }
    It 'the tagged seatbelt sentinel resolves to a clearable class end-to-end' {
        $cls = Get-HaltSentinelClass -SentinelText "CLASS: seatbelt`r`nSeatbelt: 50 autonomous tasks filed since last review."
        (Get-HaltClearEligibility -HaltClass $cls).Clearable | Should -BeTrue
    }
}

Describe 'Test-HaltClearAnswer (fail-closed)' {
    It 'is true only for an exact clear line' {
        Test-HaltClearAnswer -AnswerText "reasoning`nHALT_CLEAR: clear" | Should -BeTrue
    }
    It 'is false for hold / prose / empty: <Ans>' -ForEach @(
        @{ Ans = 'HALT_CLEAR: hold' }
        @{ Ans = 'clear' }
        @{ Ans = 'HALT_CLEAR: clear-ish' }
        @{ Ans = '' }
    ) {
        Test-HaltClearAnswer -AnswerText $Ans | Should -BeFalse
    }
}

Describe 'Test-HaltClearGuard (re-validate before deleting the sentinel)' {
    It 'is safe to clear only when the sentinel still exists with the SAME adjudicated class' {
        $d = Test-HaltClearGuard -StillExists $true -AdjudicatedClass 'seatbelt' -CurrentClass 'seatbelt'
        $d.SafeToClear | Should -BeTrue
    }
    It 'refuses when the sentinel vanished during the codex call' {
        $d = Test-HaltClearGuard -StillExists $false -AdjudicatedClass 'seatbelt' -CurrentClass ''
        $d.SafeToClear | Should -BeFalse
        $d.Reason | Should -Match 'no longer present'
    }
    It 'refuses when the class changed mid-call (e.g. a human-only consec-fail halt was written)' {
        $d = Test-HaltClearGuard -StillExists $true -AdjudicatedClass 'seatbelt' -CurrentClass 'consec-fail'
        $d.SafeToClear | Should -BeFalse
        $d.Reason | Should -Match 'class changed'
    }
}
