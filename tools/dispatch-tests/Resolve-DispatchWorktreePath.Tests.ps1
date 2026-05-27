#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-231 worktree-path resolver in
    Invoke-AiDispatchQueue.ps1 (Resolve-DispatchWorktreePath).

.DESCRIPTION
    Dot-sources the production queue script through its testability seam so
    Resolve-DispatchWorktreePath loads without running the dispatch flow, then
    exercises the helper across representative repo-root shapes (Windows
    drive paths, trailing slashes, nested paths) and confirms it fails fast
    on invalid inputs.

    The helper is pure and side-effect-free: it does not read or write files,
    call gh, git, codex, claude, or the queue, and it never spawns external
    processes. The tests inherit that purity -- nothing here invokes any of
    those surfaces, no real worktree is created, and no live publish path
    runs.
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

Describe 'Resolve-DispatchWorktreePath (queue worktree-path resolver)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Resolve-DispatchWorktreePath -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Sibling-of-repo convention' {
        It 'places the dispatch worktree under <parent>/dispatch-worktrees/<DispatchId>' {
            $result = Resolve-DispatchWorktreePath -RepoRoot 'A:\RCAD\RGE' -DispatchId 'ISSUE-231'
            $result | Should -Be 'A:\RCAD\dispatch-worktrees\ISSUE-231'
        }

        It 'tolerates a trailing backslash on -RepoRoot' {
            $result = Resolve-DispatchWorktreePath -RepoRoot 'A:\RCAD\RGE\' -DispatchId 'ISSUE-231'
            $result | Should -Be 'A:\RCAD\dispatch-worktrees\ISSUE-231'
        }

        It 'tolerates a trailing forward slash on -RepoRoot' {
            $result = Resolve-DispatchWorktreePath -RepoRoot 'A:/RCAD/RGE/' -DispatchId 'ISSUE-231'
            # GetDirectoryName accepts forward slashes on Windows; the result
            # uses the OS path separator from Join-Path.
            $result.TrimEnd('\','/') | Should -Match 'dispatch-worktrees[/\\]ISSUE-231$'
        }

        It 'puts a different dispatch under its own subdirectory of the same parent' {
            $a = Resolve-DispatchWorktreePath -RepoRoot 'A:\RCAD\RGE' -DispatchId 'ISSUE-230'
            $b = Resolve-DispatchWorktreePath -RepoRoot 'A:\RCAD\RGE' -DispatchId 'ISSUE-231'
            $a | Should -Be 'A:\RCAD\dispatch-worktrees\ISSUE-230'
            $b | Should -Be 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            (Split-Path $a -Parent) | Should -Be (Split-Path $b -Parent)
        }
    }

    Context 'Determinism and purity' {
        It 'returns identical results for repeated calls with the same inputs' {
            $first  = Resolve-DispatchWorktreePath -RepoRoot 'A:\RCAD\RGE' -DispatchId 'ISSUE-231'
            $second = Resolve-DispatchWorktreePath -RepoRoot 'A:\RCAD\RGE' -DispatchId 'ISSUE-231'
            $first | Should -BeExactly $second
        }

        It 'returns a System.String' {
            (Resolve-DispatchWorktreePath -RepoRoot 'A:\RCAD\RGE' -DispatchId 'ISSUE-231').GetType().FullName |
                Should -Be 'System.String'
        }
    }

    Context 'Invalid inputs fail fast' {
        It 'throws when -RepoRoot is the empty string' {
            { Resolve-DispatchWorktreePath -RepoRoot '' -DispatchId 'ISSUE-231' } |
                Should -Throw -ExpectedMessage '*RepoRoot*'
        }

        It 'throws when -RepoRoot is whitespace-only' {
            { Resolve-DispatchWorktreePath -RepoRoot "   `t" -DispatchId 'ISSUE-231' } |
                Should -Throw -ExpectedMessage '*RepoRoot*'
        }

        It 'throws when -RepoRoot has no parent directory' {
            { Resolve-DispatchWorktreePath -RepoRoot 'C:\' -DispatchId 'ISSUE-231' } |
                Should -Throw -ExpectedMessage '*parent*'
        }

        It 'rejects DispatchId values with disallowed characters' {
            { Resolve-DispatchWorktreePath -RepoRoot 'A:\RCAD\RGE' -DispatchId 'has spaces' } |
                Should -Throw
        }
    }
}
