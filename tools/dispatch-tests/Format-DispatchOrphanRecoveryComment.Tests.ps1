#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-231 orphan-recovery comment formatter in
    Invoke-AiDispatchQueue.ps1 (Format-DispatchOrphanRecoveryComment).

.DESCRIPTION
    Pins the durable reporting contract for the GitHub issue comments
    Invoke-OrphanRecovery posts when it archives or preserves an isolated
    worktree:

      * the `.interrupt<N>` archive path returned by
        Save-OrphanDispatchWorktree appears verbatim in the emitted comment
        body whenever the recovery archived a worktree, and
      * the deterministic inspection / removal commands
        (`git -C "<path>" status --short --branch`,
        `git -C "<path>" log --oneline -5`,
        `git worktree remove "<path>"`)
        appear alongside it so a human can recover the preserved state
        without leaving the GitHub issue.

    The helper is pure and side-effect-free: it does not read or write
    files, call gh, git, codex, claude, the queue runner, the scheduler,
    or the network. The tests inherit that purity -- nothing here invokes
    any of those surfaces.

    These invariants are the correction bar the ISSUE-231 task packet
    spelled out: a focused test must explicitly pin that interrupted /
    orphan recovery reporting carries a `.interrupt<N>` archive path or
    preserved worktree path in the durable comment text, so a future
    refactor that drops the path from the comment fails closed here.
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

Describe 'Format-DispatchOrphanRecoveryComment (orphan-recovery comment formatter)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Format-DispatchOrphanRecoveryComment -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Interrupted stage with archived worktree' {
        It 'names the .interrupt<N> archive path verbatim' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt1'
            $body | Should -Match 'A:\\RCAD\\dispatch-worktrees\\ISSUE-231\.interrupt1'
            $body | Should -Match '\.interrupt1'
        }

        It 'includes the git -C status --short --branch inspection command' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt1'
            $body | Should -Match 'git -C "A:\\RCAD\\dispatch-worktrees\\ISSUE-231\.interrupt1" status --short --branch'
        }

        It 'includes the git -C log --oneline -5 history command' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt1'
            $body | Should -Match 'log --oneline -5'
        }

        It 'includes the git worktree remove cleanup command' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt1'
            $body | Should -Match 'git worktree remove "A:\\RCAD\\dispatch-worktrees\\ISSUE-231\.interrupt1"'
        }

        It 'still mentions the queue label so the human knows the issue was re-queued' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt1'
            $body | Should -Match 'reset it to `ai-dispatch`'
        }
    }

    Context 'Interrupted stage with preserved (not archived) worktree' {
        It 'names the preserved worktree path verbatim when ArchivePath is empty' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch' `
                -PreservedPath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $body | Should -Match 'preserved at `A:\\RCAD\\dispatch-worktrees\\ISSUE-231`'
            $body | Should -Match 'git -C "A:\\RCAD\\dispatch-worktrees\\ISSUE-231" status --short --branch'
            $body | Should -Match 'git worktree remove "A:\\RCAD\\dispatch-worktrees\\ISSUE-231"'
        }
    }

    Context 'Interrupted stage with no surviving worktree' {
        It 'falls back to the legacy text when neither ArchivePath nor PreservedPath is set' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch'
            $body | Should -Match 'interrupted before it finished'
            $body | Should -Match 'reset it to `ai-dispatch`'
            $body | Should -Not -Match 'isolated worktree'
            $body | Should -Not -Match '\.interrupt'
        }
    }

    Context 'Already-published stage carries the archive path when one was produced' {
        It 'names the .interrupt<N> archive path when the recovery archived a worktree' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'already-published' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -PublishedShortSha 'abcd1234' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt2'
            $body | Should -Match 'published this work \(abcd1234\)'
            $body | Should -Match 'A:\\RCAD\\dispatch-worktrees\\ISSUE-231\.interrupt2'
            $body | Should -Match 'git worktree remove "A:\\RCAD\\dispatch-worktrees\\ISSUE-231\.interrupt2"'
        }

        It 'omits the inspection block when no archive was produced' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'already-published' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -PublishedShortSha 'abcd1234'
            $body | Should -Match 'published this work \(abcd1234\)'
            $body | Should -Not -Match 'isolated worktree'
        }
    }

    Context 'Interrupted-publish stage carries the archive path when one was produced' {
        It 'names the .interrupt<N> archive path on a left-ahead-of-origin path' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted-publish' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt1'
            $body | Should -Match 'interrupted between the local merge and the push'
            $body | Should -Match 'ai-dispatch/ISSUE-231'
            $body | Should -Match 'A:\\RCAD\\dispatch-worktrees\\ISSUE-231\.interrupt1'
            $body | Should -Match 'git worktree remove "A:\\RCAD\\dispatch-worktrees\\ISSUE-231\.interrupt1"'
        }

        It 'falls back to the legacy ahead-of-origin text when no archive was produced' {
            $body = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted-publish' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231'
            $body | Should -Match 'interrupted between the local merge and the push'
            $body | Should -Match 'ai-dispatch/ISSUE-231'
            $body | Should -Not -Match 'isolated worktree'
        }
    }

    Context 'Invalid inputs fail fast' {
        It 'rejects an unsupported Stage' {
            { Format-DispatchOrphanRecoveryComment `
                -Stage 'not-a-stage' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' } |
                Should -Throw
        }

        It 'rejects a DispatchId with disallowed characters' {
            { Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'has spaces' `
                -Branch 'ai-dispatch/x' } |
                Should -Throw
        }
    }

    Context 'Determinism and purity' {
        It 'returns identical strings for repeated calls with the same inputs' {
            $a = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt1'
            $b = Format-DispatchOrphanRecoveryComment `
                -Stage 'interrupted' `
                -DispatchId 'ISSUE-231' `
                -Branch 'ai-dispatch/ISSUE-231' `
                -QueueLabel 'ai-dispatch' `
                -ArchivePath 'A:\RCAD\dispatch-worktrees\ISSUE-231.interrupt1'
            $a | Should -BeExactly $b
        }
    }
}

Describe 'ISSUE-231 orphan-recovery reporting routes through Format-DispatchOrphanRecoveryComment' {

    Context 'Invoke-OrphanRecovery consumes the formatter for every orphan stage' {
        It 'builds the genuinely-interrupted comment via the formatter' {
            # Source-level invariant: the orphan-recovery branch that resets
            # the issue back to the queue label must hand the archive path
            # produced by Save-OrphanDispatchWorktree to the formatter so the
            # comment carries the `.interrupt<N>` path. A regression that
            # re-introduces the literal pre-correction text (no path) would
            # leave this assertion red.
            $script:QueueScriptText | Should -Match "Format-DispatchOrphanRecoveryComment\s+(?:``\s*)?-Stage\s+'interrupted'"
        }

        It 'builds the already-published cleanup comment via the formatter' {
            $script:QueueScriptText | Should -Match "Format-DispatchOrphanRecoveryComment\s+(?:``\s*)?-Stage\s+'already-published'"
        }

        It 'builds the ahead-of-origin / interrupted-publish comment via the formatter' {
            $script:QueueScriptText | Should -Match "Format-DispatchOrphanRecoveryComment\s+(?:``\s*)?-Stage\s+'interrupted-publish'"
        }

        It 'reports the archived branch name on the interrupted-publish path' {
            $script:QueueScriptText | Should -Match '\$aheadCommentBranch\s*=\s*\$aheadBranch'
            $script:QueueScriptText | Should -Match '\$saveResult\.ArchiveBranch'
            $script:QueueScriptText | Should -Match '-Branch\s+\$aheadCommentBranch'
        }

        It 'feeds the Save-OrphanDispatchWorktree archive path into the formatter' {
            # The captured `$saveResult.ArchivePath` value (or `$archivePath`
            # variable populated from it) is the surviving deterministic
            # handle for a human. Either pattern is acceptable so long as
            # the formatter's -ArchivePath argument is plumbed from the
            # orphan-save return value rather than a hard-coded empty.
            $script:QueueScriptText | Should -Match '\$saveResult\s*=\s*Save-OrphanDispatchWorktree'
            $script:QueueScriptText | Should -Match '-ArchivePath\s+\$archivePath'
        }
    }
}
