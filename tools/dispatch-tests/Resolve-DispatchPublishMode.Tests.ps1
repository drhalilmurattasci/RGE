#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-230 publish-mode normalization helper in
    Invoke-AiDispatchQueue.ps1 (Resolve-DispatchPublishMode).

.DESCRIPTION
    Dot-sources the production queue script through its testability seam so
    Resolve-DispatchPublishMode loads without running the dispatch flow, then
    exercises the helper across every supported -PublishMode / -NoPublish
    combination and confirms that conflicting inputs fail fast.

    The helper is pure and side-effect-free: it does not read or write files,
    call gh, git, codex, claude, the queue runner, the scheduler, or the
    network. The tests inherit that purity -- nothing here invokes any of
    those surfaces, no real GitHub issues are read or modified, no temporary
    repo is created, and no live publish path runs.
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
}

Describe 'Resolve-DispatchPublishMode (queue publish-mode normalization)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Resolve-DispatchPublishMode -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Default behavior (ISSUE-239: no-flag default is pr)' {
        It 'returns pr when no -PublishMode and no -NoPublish are given' {
            Resolve-DispatchPublishMode | Should -Be 'pr'
        }

        It 'returns branch when only -NoPublish is set' {
            Resolve-DispatchPublishMode -NoPublish $true | Should -Be 'branch'
        }

        It 'returns pr when -PublishMode is the empty string' {
            Resolve-DispatchPublishMode -PublishMode '' | Should -Be 'pr'
        }

        It 'does not fall back to main on a no-flag invocation' {
            Resolve-DispatchPublishMode | Should -Not -Be 'main'
        }
    }

    Context 'Explicit -PublishMode values' {
        It 'returns main for -PublishMode main' {
            Resolve-DispatchPublishMode -PublishMode 'main' | Should -Be 'main'
        }

        It 'returns branch for -PublishMode branch' {
            Resolve-DispatchPublishMode -PublishMode 'branch' | Should -Be 'branch'
        }

        It 'returns pr for -PublishMode pr' {
            Resolve-DispatchPublishMode -PublishMode 'pr' | Should -Be 'pr'
        }
    }

    Context 'Compatible -NoPublish + -PublishMode combinations' {
        It 'returns branch for -NoPublish + -PublishMode branch' {
            Resolve-DispatchPublishMode -PublishMode 'branch' -NoPublish $true | Should -Be 'branch'
        }
    }

    Context 'Conflicting combinations fail fast' {
        It 'throws when -NoPublish is combined with -PublishMode main' {
            { Resolve-DispatchPublishMode -PublishMode 'main' -NoPublish $true } |
                Should -Throw -ExpectedMessage '*NoPublish*main*'
        }

        It 'throws when -NoPublish is combined with -PublishMode pr' {
            { Resolve-DispatchPublishMode -PublishMode 'pr' -NoPublish $true } |
                Should -Throw -ExpectedMessage '*NoPublish*pr*'
        }

        It 'throws on an unrecognized publish mode value' {
            { Resolve-DispatchPublishMode -PublishMode 'totally-not-a-mode' } |
                Should -Throw -ExpectedMessage '*invalid*'
        }
    }

    Context 'Determinism and purity' {
        It 'returns identical results for repeated calls with the same inputs' {
            $first  = Resolve-DispatchPublishMode -PublishMode 'pr'
            $second = Resolve-DispatchPublishMode -PublishMode 'pr'
            $first | Should -BeExactly $second
        }

        It 'has the same return type as a plain string' {
            (Resolve-DispatchPublishMode -PublishMode 'pr').GetType().FullName |
                Should -Be 'System.String'
        }
    }
}
