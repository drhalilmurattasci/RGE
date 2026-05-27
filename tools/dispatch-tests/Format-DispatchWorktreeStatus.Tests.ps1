#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-231 worktree-status formatter in
    Invoke-AiDispatchQueue.ps1 (Format-DispatchWorktreeStatus).

.DESCRIPTION
    Dot-sources the production queue script through its testability seam so
    Format-DispatchWorktreeStatus loads without running the dispatch flow,
    then exercises the formatter for each disposition (preserved / removed /
    archived) and confirms it fails fast on missing inputs.

    The helper is pure and side-effect-free: it does not read or write files,
    call gh, git, codex, claude, the queue, or the network. The tests
    inherit that purity.
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

Describe 'Format-DispatchWorktreeStatus (queue worktree-status formatter)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Format-DispatchWorktreeStatus -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Preserved disposition' {
        It 'names the worktree path and gives an inspection hint' {
            $line = Format-DispatchWorktreeStatus -Disposition 'preserved' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $line | Should -Match 'preserved'
            $line | Should -Match 'A:\\RCAD\\dispatch-worktrees\\ISSUE-231'
            $line | Should -Match 'git -C'
        }

        It 'mentions manual remove instructions' {
            $line = Format-DispatchWorktreeStatus -Disposition 'preserved' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $line | Should -Match 'worktree remove'
        }
    }

    Context 'Removed disposition' {
        It 'states the worktree was removed after the publish action completed' {
            $line = Format-DispatchWorktreeStatus -Disposition 'removed' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $line | Should -Match 'removed'
            $line | Should -Match 'A:\\RCAD\\dispatch-worktrees\\ISSUE-231'
            $line | Should -Match 'publish action'
        }
    }

    Context 'Archived disposition' {
        It 'names both source and archive paths with an inspection hint' {
            $line = Format-DispatchWorktreeStatus `
                -Disposition 'archived' `
                -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231' `
                -ArchivePath  'A:\RCAD\dispatch-worktrees\ISSUE-231.attempt1'
            $line | Should -Match 'archived'
            $line | Should -Match 'A:\\RCAD\\dispatch-worktrees\\ISSUE-231'
            $line | Should -Match 'A:\\RCAD\\dispatch-worktrees\\ISSUE-231\.attempt1'
            $line | Should -Match 'git -C'
        }

        It 'fails fast if the archive path is missing' {
            { Format-DispatchWorktreeStatus -Disposition 'archived' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231' } |
                Should -Throw -ExpectedMessage '*ArchivePath*'
        }
    }

    Context 'Optional reason text' {
        It 'appends the reason in parentheses when supplied' {
            $line = Format-DispatchWorktreeStatus `
                -Disposition 'preserved' `
                -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231' `
                -Reason 'terminal failure; human inspection needed.'
            $line | Should -Match 'terminal failure; human inspection needed\.'
        }

        It 'omits the reason suffix when empty' {
            $line = Format-DispatchWorktreeStatus -Disposition 'preserved' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $line | Should -Not -Match '\($'
        }
    }

    Context 'Determinism and purity' {
        It 'returns identical strings for repeated calls with the same inputs' {
            $a = Format-DispatchWorktreeStatus -Disposition 'removed' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $b = Format-DispatchWorktreeStatus -Disposition 'removed' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $a | Should -BeExactly $b
        }

        It 'returns a System.String' {
            (Format-DispatchWorktreeStatus -Disposition 'removed' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231').GetType().FullName |
                Should -Be 'System.String'
        }
    }

    Context 'Invalid inputs fail fast' {
        It 'rejects an empty -WorktreePath' {
            { Format-DispatchWorktreeStatus -Disposition 'removed' -WorktreePath '' } |
                Should -Throw -ExpectedMessage '*WorktreePath*'
        }

        It 'rejects an unsupported -Disposition' {
            { Format-DispatchWorktreeStatus -Disposition 'totally-not-a-disposition' -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231' } |
                Should -Throw
        }
    }
}
