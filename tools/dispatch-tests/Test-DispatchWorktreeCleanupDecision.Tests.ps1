#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-231 worktree cleanup decision in
    Invoke-AiDispatchQueue.ps1 (Test-DispatchWorktreeCleanupDecision).

.DESCRIPTION
    Dot-sources the production queue script through its testability seam so
    Test-DispatchWorktreeCleanupDecision loads without running the dispatch
    flow, then exercises every terminal-state combination the queue can
    feed it and confirms the layered preserve/archive/remove decision.

    The helper is pure and side-effect-free: it does not read or write files,
    call gh, git, codex, claude, the queue runner, the scheduler, or the
    network. The tests inherit that purity.
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

Describe 'Test-DispatchWorktreeCleanupDecision (queue worktree cleanup decision)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Test-DispatchWorktreeCleanupDecision -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Terminal success' {
        It 'removes the worktree when nothing went wrong' {
            $d = Test-DispatchWorktreeCleanupDecision `
                -RunFailed $false -RunBlocked $false -WillRetry $false -PublishHardFailed $false
            $d.Action | Should -Be 'remove'
            $d.Reason | Should -Match 'terminal success'
        }
    }

    Context 'Publish-pipeline failure wins over everything else' {
        It 'preserves the worktree when PublishHardFailed=true even if other flags also fire' {
            $d = Test-DispatchWorktreeCleanupDecision `
                -RunFailed $true -RunBlocked $true -WillRetry $true -PublishHardFailed $true
            $d.Action | Should -Be 'preserve'
            $d.Reason | Should -Match 'publish pipeline failed'
        }
    }

    Context 'Blocked execution beats retry and generic failure' {
        It 'preserves the worktree on EXEC_STATUS: blocked' {
            $d = Test-DispatchWorktreeCleanupDecision `
                -RunFailed $true -RunBlocked $true -WillRetry $true -PublishHardFailed $false
            $d.Action | Should -Be 'preserve'
            $d.Reason | Should -Match 'EXEC_STATUS: blocked'
        }
    }

    Context 'Retry-eligible failure archives' {
        It 'archives the worktree on a retry-eligible accidental failure' {
            $d = Test-DispatchWorktreeCleanupDecision `
                -RunFailed $true -RunBlocked $false -WillRetry $true -PublishHardFailed $false
            $d.Action | Should -Be 'archive'
            $d.Reason | Should -Match 'retry-eligible'
        }
    }

    Context 'Terminal failure (non-retry, non-blocked, non-publish-hard) preserves' {
        It 'preserves the worktree when no specific reason fires but the run still failed' {
            $d = Test-DispatchWorktreeCleanupDecision `
                -RunFailed $true -RunBlocked $false -WillRetry $false -PublishHardFailed $false
            $d.Action | Should -Be 'preserve'
            $d.Reason | Should -Match 'terminal failure'
        }
    }

    Context 'Decision ordering' {
        It 'puts PublishHardFailed before RunBlocked' {
            $a = Test-DispatchWorktreeCleanupDecision -RunFailed $true -RunBlocked $true -WillRetry $false -PublishHardFailed $true
            $a.Reason | Should -Match 'publish pipeline failed'
        }

        It 'puts RunBlocked before WillRetry' {
            $a = Test-DispatchWorktreeCleanupDecision -RunFailed $true -RunBlocked $true -WillRetry $true -PublishHardFailed $false
            $a.Reason | Should -Match 'EXEC_STATUS: blocked'
        }

        It 'puts WillRetry before generic RunFailed' {
            $a = Test-DispatchWorktreeCleanupDecision -RunFailed $true -RunBlocked $false -WillRetry $true -PublishHardFailed $false
            $a.Action | Should -Be 'archive'
        }
    }

    Context 'Determinism and purity' {
        It 'returns identical decisions for repeated calls with the same inputs' {
            $a = Test-DispatchWorktreeCleanupDecision -RunFailed $false -RunBlocked $false -WillRetry $false -PublishHardFailed $false
            $b = Test-DispatchWorktreeCleanupDecision -RunFailed $false -RunBlocked $false -WillRetry $false -PublishHardFailed $false
            $a.Action | Should -BeExactly $b.Action
            $a.Reason | Should -BeExactly $b.Reason
        }

        It 'returns a pscustomobject with both Action and Reason fields' {
            $d = Test-DispatchWorktreeCleanupDecision -RunFailed $false -RunBlocked $false -WillRetry $false -PublishHardFailed $false
            $d.PSObject.Properties.Name | Should -Contain 'Action'
            $d.PSObject.Properties.Name | Should -Contain 'Reason'
        }
    }
}
