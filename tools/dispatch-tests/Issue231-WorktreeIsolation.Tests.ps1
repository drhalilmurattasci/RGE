#Requires -Version 5.1
<#
.SYNOPSIS
    ISSUE-231 invariants on Invoke-AiDispatchQueue.ps1: the queue runs each
    selected issue through the dispatch loop inside an isolated git worktree
    and no longer toggles the primary checkout to the per-issue branch as
    the normal run boundary.

.DESCRIPTION
    Static / source-level checks against the queue script:

    * The queue uses `git worktree add -b ...` (or equivalent) to set up
      the per-issue branch in a new worktree.
    * The queue no longer issues `git checkout -b <branch>` or
      `git checkout main` against the primary checkout as the normal
      run boundary for the selected issue. (References inside the orphan-
      recovery interrupt path or the dispatch-loop preflight, which are
      not the normal queue run boundary, are allowed.)
    * All three worktree-isolation helpers introduced by ISSUE-231 are
      exported by dot-sourcing the queue script.

    These assertions are intentionally source-level. They give the
    verification gate a fast, deterministic way to detect a regression
    that re-introduces primary-checkout-as-run-boundary without spinning
    up a real GitHub queue dispatch.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    if (-not (Test-Path -LiteralPath $script:QueueScriptPath)) {
        throw "Invoke-AiDispatchQueue.ps1 not found at $script:QueueScriptPath"
    }
    $script:QueueScriptText = Get-Content -Raw -LiteralPath $script:QueueScriptPath

    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }
}

Describe 'ISSUE-231 worktree-isolation invariants on Invoke-AiDispatchQueue.ps1' {

    Context 'Worktree-isolation helpers are exposed' {
        It 'exposes Resolve-DispatchWorktreePath' {
            (Get-Command -Name Resolve-DispatchWorktreePath -ErrorAction SilentlyContinue) |
                Should -Not -BeNullOrEmpty
        }
        It 'exposes Test-DispatchWorktreeCleanupDecision' {
            (Get-Command -Name Test-DispatchWorktreeCleanupDecision -ErrorAction SilentlyContinue) |
                Should -Not -BeNullOrEmpty
        }
        It 'exposes Format-DispatchWorktreeStatus' {
            (Get-Command -Name Format-DispatchWorktreeStatus -ErrorAction SilentlyContinue) |
                Should -Not -BeNullOrEmpty
        }
    }

    Context 'Queue creates the per-issue branch inside an isolated worktree' {
        It 'invokes `git worktree add -b $branch` against the resolved worktree path' {
            $script:QueueScriptText | Should -Match "worktree['""\s,]+add['""\s,]+-b['""\s,]+\`$branch['""\s,]+\`$worktreePath"
        }

        It 'sets $script:DispatchWorktreeRoot after the worktree add so repo-rel helpers route through it' {
            $script:QueueScriptText | Should -Match '\$script:DispatchWorktreeRoot\s*=\s*\$worktreePath'
        }
    }

    Context 'Queue no longer toggles the primary checkout to the dispatch branch' {
        It 'does not run `git checkout -b $branch` as the normal queue run boundary' {
            # `checkout -b $branch` was the pre-ISSUE-231 primary-checkout
            # branching call. Nothing in the queue's normal run path may
            # contain that exact array-literal shape any more.
            $script:QueueScriptText | Should -Not -Match "'checkout',\s*'-b',\s*\`$branch"
        }

        It 'does not run `git checkout main` as the normal queue run boundary' {
            # `checkout main` was the pre-ISSUE-231 return-to-main step.
            # No call in the queue's normal run path should re-introduce
            # the un-forced array-literal form. The orphan recovery's
            # `'checkout', '-f', 'main'` (with `-f`) is a separate primary-
            # checkout recovery path and is intentionally not the queue's
            # normal run boundary, so it is excluded by anchoring on the
            # exact array-literal shape `'checkout', 'main'`.
            $script:QueueScriptText | Should -Not -Match "'checkout',\s*'main'"
        }
    }

    Context 'Queue-owned git operations route through the isolated worktree' {
        It 'stages with `git -C $worktreePath add -A`' {
            $script:QueueScriptText | Should -Match "-C['""\s,]+\`$worktreePath['""\s,]+add['""\s,]+-A"
        }

        It 'commits with `git -C $worktreePath commit -F`' {
            $script:QueueScriptText | Should -Match "-C['""\s,]+\`$worktreePath['""\s,]+commit['""\s,]+-F"
        }

        It 'reads the dispatch run dir from inside the worktree' {
            $script:QueueScriptText | Should -Match '\$runDir\s*=\s*Join-Path\s+\$worktreePath\s+\(Join-Path\s+''\.ai''\s+"dispatch-\$id"\)'
        }
    }

    Context 'Cleanup discipline preserves the worktree on non-success paths' {
        It 'removes the worktree on main-mode success before deleting the branch' {
            $script:QueueScriptText | Should -Match 'worktree''\s*,\s*''remove''\s*,\s*\$worktreePath'
        }

        It 'archives the worktree alongside the branch on retry-eligible failure' {
            $script:QueueScriptText | Should -Match 'worktree''\s*,\s*''move''\s*,\s*\$worktreePath\s*,\s*\$archiveWorktree'
        }
    }
}
