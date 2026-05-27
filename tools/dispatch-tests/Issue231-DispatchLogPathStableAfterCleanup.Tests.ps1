#Requires -Version 5.1
<#
.SYNOPSIS
    ISSUE-231 correction round: the queue's final user-facing references to
    the dispatch log (result comment, PR body, close comment, and the audit
    log's own commit message) must keep reporting the committed repo-relative
    path -- e.g. `ai_dispatch_logs/log_*.md` -- even after isolated-worktree
    cleanup clears `$script:DispatchWorktreeRoot`.

.DESCRIPTION
    The pre-correction queue called `Get-RepoRelativePathForQueue
    $dispatchLogPath` directly at the result-comment, PR-body, and close-
    comment sites. That formatter prefers `$script:DispatchWorktreeRoot`
    when set and falls back to `$script:RepoRoot` otherwise. After a
    successful `main` / `pr` publish (or the terminal cleanup-decision
    remove) the queue clears `$script:DispatchWorktreeRoot`; the dispatch
    log lives at an absolute path inside the now-removed isolated worktree
    (a sibling of the primary repo root, NOT under it), so the fallback
    branch returned an absolute, removed-worktree path string instead of
    the stable `ai_dispatch_logs/log_*.md` repo-relative path.

    This file pins two complementary invariants:

      1. Runtime: dot-sourced `Get-RepoRelativePathForQueue` reproduces
         the defect when `$script:DispatchWorktreeRoot` is cleared while
         the input path still references the (removed) worktree -- proving
         that any call site that defers formatting until after cleanup
         emits an unstable path.
      2. Source-level: the queue script precomputes the repo-relative
         dispatch-log path once (`$dispatchLogRel = Get-RepoRelativePathForQueue
         $dispatchLogPath`) immediately after `Write-DispatchLog` returns
         and BEFORE any `$script:DispatchWorktreeRoot = $null` assignment,
         and the result comment / PR body / close comment all consume
         `$dispatchLogRel` rather than re-calling the formatter on the
         absolute path.

    Together these fail against the defect and pass only when the queue
    pins the stable repo-relative path before cleanup and reuses it.
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

Describe 'ISSUE-231 stable dispatch-log path across worktree cleanup' {

    Context 'Get-RepoRelativePathForQueue defect under cleared worktree root' {
        BeforeEach {
            # Pin a deterministic primary-repo / worktree pairing so the
            # formatter's "is the path under the active root" check is
            # exercised both before and after cleanup. The worktree is the
            # documented sibling-of-repo location for ISSUE-231.
            $script:RepoRoot             = 'A:\RCAD\RGE'
            $script:DispatchWorktreeRoot = 'A:\RCAD\dispatch-worktrees\ISSUE-231'
        }

        AfterEach {
            $script:DispatchWorktreeRoot = $null
        }

        It 'returns the stable repo-relative path while the worktree root is set' {
            $absLog = 'A:\RCAD\dispatch-worktrees\ISSUE-231\ai_dispatch_logs\log_x.md'
            (Get-RepoRelativePathForQueue $absLog) |
                Should -Be 'ai_dispatch_logs/log_x.md'
        }

        It 'falls back to an absolute removed-worktree path once the worktree root is cleared' {
            # This is the regression that the queue must NOT rely on at the
            # result-comment / PR-body / close-comment sites: after the
            # worktree-remove step clears $script:DispatchWorktreeRoot, the
            # dispatch-log path lives outside the primary repo root and the
            # formatter no longer matches the prefix, so it returns the
            # absolute forward-slash form of a path that no longer exists.
            $absLog = 'A:\RCAD\dispatch-worktrees\ISSUE-231\ai_dispatch_logs\log_x.md'
            $script:DispatchWorktreeRoot = $null
            $after = Get-RepoRelativePathForQueue $absLog
            $after | Should -Be 'A:/RCAD/dispatch-worktrees/ISSUE-231/ai_dispatch_logs/log_x.md'
            $after | Should -Not -Be 'ai_dispatch_logs/log_x.md'
        }
    }

    Context 'Queue precomputes and reuses a stable dispatch-log repo-relative path' {
        It 'assigns $dispatchLogRel immediately after Write-DispatchLog returns' {
            # The single source of truth: $dispatchLogRel is captured from
            # Get-RepoRelativePathForQueue $dispatchLogPath right after the
            # log is written, while $script:DispatchWorktreeRoot is still
            # set to the isolated worktree. All later comment/PR/close
            # sites must consume this variable.
            $script:QueueScriptText | Should -Match '\$dispatchLogRel\s*=\s*Get-RepoRelativePathForQueue\s+\$dispatchLogPath'
        }

        It 'precomputes $dispatchLogRel BEFORE any $script:DispatchWorktreeRoot = $null assignment' {
            $assignIdx = $script:QueueScriptText.IndexOf('$dispatchLogRel = Get-RepoRelativePathForQueue $dispatchLogPath')
            $assignIdx | Should -BeGreaterThan -1
            $clearIdx  = $script:QueueScriptText.IndexOf('$script:DispatchWorktreeRoot = $null')
            $clearIdx  | Should -BeGreaterThan -1
            $assignIdx | Should -BeLessThan $clearIdx
        }

        It 'does not call Get-RepoRelativePathForQueue on $dispatchLogPath at any other site' {
            # Once $dispatchLogRel exists, every other reference to the
            # dispatch log's repo-relative path -- result comment, PR body,
            # close comment, and the audit-log commit message -- must
            # consume $dispatchLogRel. A second formatter call on
            # $dispatchLogPath would re-introduce the post-cleanup defect.
            $matches = [regex]::Matches($script:QueueScriptText,
                'Get-RepoRelativePathForQueue\s+\$dispatchLogPath')
            $matches.Count | Should -Be 1
        }

        It 'uses $dispatchLogRel in the result-comment "Detailed log:" bullet' {
            $script:QueueScriptText | Should -Match '- Detailed log: ``\$dispatchLogRel``'
        }

        It 'uses $dispatchLogRel in the PR body Format-DispatchPrBody call' {
            $script:QueueScriptText | Should -Match '-DispatchLogPath\s+\$dispatchLogRel'
        }

        It 'uses $dispatchLogRel in both branches of the main-mode close comment' {
            $script:QueueScriptText | Should -Match 'Auto-published to origin/main as \$publishedSha\. Detailed log: \$dispatchLogRel'
            $script:QueueScriptText | Should -Match 'Dispatch completed with no committable changes\. Detailed log: \$dispatchLogRel'
        }

        It 'uses $dispatchLogRel in the audit-log commit message body' {
            $script:QueueScriptText | Should -Match 'Detailed log: \$dispatchLogRel'
        }
    }
}
