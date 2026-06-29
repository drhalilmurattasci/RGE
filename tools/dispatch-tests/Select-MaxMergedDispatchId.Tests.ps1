#Requires -Version 5.1
<#
.SYNOPSIS
    Regression coverage for the merged-dispatch-id subject scan that drives the
    stale-readout banner in Get-AiDispatchHealth.ps1.

.DESCRIPTION
    Dot-sources Get-AiDispatchHealth.ps1 through its RGE_AI_DISPATCH_HEALTH_SKIP_MAIN
    seam to load the pure helper without running the readout:
      * Select-MaxMergedDispatchId -Subjects  -> highest <n> across commit subjects
        that BEGIN with "ai-dispatch ISSUE-<n>:"

    The banner previously scanned `git log --pretty=%s` with an UNANCHORED match
    'ai-dispatch ISSUE-(\d+)', so a commit that merely MENTIONED the marker
    mid-subject (a revert / follow-up such as "Revert ... ai-dispatch ISSUE-9999:")
    could inflate the merged id. This mirrors the subject-only fix already applied
    to Invoke-AiDispatchQueue.ps1's published-commit guards
    (Select-StaleReplayPublishedSha): match only subjects that START with the
    marker, with the trailing colon disambiguating ISSUE-4 from ISSUE-40.

    The migration issue-number collision (ancient imported ISSUE-1:..ISSUE-7: of
    2026-05-17 are genuine subject-line publishes and still prefix-match) is NOT
    floored out here on purpose: $maxMergedId is a global MAXIMUM dominated by the
    current highest issue, so an ancient low id can never change the result. That
    "harmless" property is pinned below so a future change cannot silently rely on
    a floor that does not exist.
#>

BeforeAll {
    $script:TestsRoot        = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest  = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:HealthScriptPath = Join-Path $script:RepoRootForTest 'Get-AiDispatchHealth.ps1'
    $env:RGE_AI_DISPATCH_HEALTH_SKIP_MAIN = '1'
    try { . $script:HealthScriptPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_HEALTH_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Select-MaxMergedDispatchId (subject-start prefix scan)' {
    It 'returns the highest <n> across genuine subject-line publishes' {
        $subs = @(
            'ai-dispatch ISSUE-12: a publish'
            'ai-dispatch ISSUE-431: a newer publish'
            'ai-dispatch ISSUE-7: an older publish'
        )
        Select-MaxMergedDispatchId -Subjects $subs | Should -Be 431
    }

    It 'ignores a commit that only MENTIONS the marker mid-subject (the unanchored bug)' {
        # The old unanchored 'ai-dispatch ISSUE-(\d+)' would read 9999 here.
        $subs = @(
            'ai-dispatch ISSUE-5: real publish'
            'Revert merge of ai-dispatch ISSUE-9999: bad change'
        )
        Select-MaxMergedDispatchId -Subjects $subs | Should -Be 5
    }

    It 'disambiguates ISSUE-4 from ISSUE-40 (greedy capture + trailing colon)' {
        Select-MaxMergedDispatchId -Subjects @('ai-dispatch ISSUE-40: forty') | Should -Be 40
        Select-MaxMergedDispatchId -Subjects @(
            'ai-dispatch ISSUE-4: four'
            'ai-dispatch ISSUE-40: forty'
        ) | Should -Be 40
    }

    It 'requires the trailing colon: a marker with no colon is not a publish' {
        Select-MaxMergedDispatchId -Subjects @('ai-dispatch ISSUE-8 work in progress') | Should -Be -1
    }

    It 'accepts a single string (PowerShell coerces it to a one-element array)' {
        Select-MaxMergedDispatchId -Subjects 'ai-dispatch ISSUE-21: one line' | Should -Be 21
    }

    It 'tolerates blank / null / non-matching elements mixed in' {
        $subs = @('', $null, 'Merge branch ''x''', 'ai-dispatch ISSUE-3: ok')
        Select-MaxMergedDispatchId -Subjects $subs | Should -Be 3
    }

    It 'returns -1 for an empty array and for $null (banner then never fires)' {
        Select-MaxMergedDispatchId -Subjects @()   | Should -Be -1
        Select-MaxMergedDispatchId -Subjects $null | Should -Be -1
    }

    It 'migration collision is harmless: ancient low ids never beat the current top' {
        # Ancient imported subjects DO still prefix-match (they are real old publishes),
        # but the global maximum is dominated by the current highest issue, so no
        # time-floor is needed for correctness -- pin that property.
        $subs = @(
            'ai-dispatch ISSUE-1: ancient migrated 2026-05-17'
            'ai-dispatch ISSUE-7: ancient migrated 2026-05-17'
            'ai-dispatch ISSUE-433: current top'
        )
        Select-MaxMergedDispatchId -Subjects $subs | Should -Be 433
    }
}

Describe 'Source wiring contract (banner routes through the anchored helper)' {
    BeforeAll {
        $script:HealthSource = Get-Content -LiteralPath $script:HealthScriptPath -Raw
    }

    It 'the banner routes through Select-MaxMergedDispatchId (definition + call site)' {
        ([regex]::Matches($script:HealthSource, 'Select-MaxMergedDispatchId')).Count |
            Should -BeGreaterOrEqual 2
    }

    It 'the subject scan is anchored to subject-start with a trailing colon' {
        $script:HealthSource.Contains("'^ai-dispatch ISSUE-(\d+):'") | Should -BeTrue
    }

    It 'the old unanchored substring match is gone' {
        # The anchored literal '^ai-dispatch ISSUE-(\d+):' does NOT contain this
        # bare form, so a True here means the pre-fix regex literal is still present.
        $script:HealthSource.Contains("'ai-dispatch ISSUE-(\d+)'") | Should -BeFalse
    }

    It 'exposes the RGE_AI_DISPATCH_HEALTH_SKIP_MAIN test seam' {
        $script:HealthSource.Contains('RGE_AI_DISPATCH_HEALTH_SKIP_MAIN') | Should -BeTrue
    }
}
