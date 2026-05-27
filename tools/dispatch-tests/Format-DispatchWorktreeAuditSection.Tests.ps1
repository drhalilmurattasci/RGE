#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for the ISSUE-231 worktree audit-log section formatter
    (Format-DispatchWorktreeAuditSection) embedded by Write-DispatchLog when
    the dispatch ran inside an isolated worktree.

.DESCRIPTION
    Pins the durable on-branch contract: the committed dispatch audit log
    body must carry the worktree path AND the deterministic inspection /
    removal commands a human needs to recover, inspect, or remove the
    surviving state. Using the worktree root only for log location or
    git-scope flag is not sufficient (that was the ISSUE-231 correction
    finding).

    The helper is pure and side-effect-free: it does not read or write
    files, call gh, git, codex, claude, the queue runner, the scheduler,
    or the network. The tests inherit that purity.
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

Describe 'Format-DispatchWorktreeAuditSection (audit-log worktree section)' {

    It 'exposes the helper after dot-sourcing the queue script' {
        (Get-Command -Name Format-DispatchWorktreeAuditSection -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
    }

    Context 'Section content' {
        BeforeAll {
            $script:Section = Format-DispatchWorktreeAuditSection `
                -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
        }

        It 'starts with the Isolated Worktree header so it slots into the audit log' {
            $script:Section | Should -Match '##\s+Isolated Worktree'
        }

        It 'names the worktree path verbatim' {
            $script:Section | Should -Match 'Worktree path: `A:\\RCAD\\dispatch-worktrees\\ISSUE-231`'
        }

        It 'includes the git -C status --short --branch inspection command' {
            $script:Section | Should -Match 'git -C "A:\\RCAD\\dispatch-worktrees\\ISSUE-231" status --short --branch'
        }

        It 'includes the git -C log --oneline -5 history command' {
            $script:Section | Should -Match 'git -C "A:\\RCAD\\dispatch-worktrees\\ISSUE-231" log --oneline -5'
        }

        It 'includes the git worktree remove cleanup command' {
            $script:Section | Should -Match 'git worktree remove "A:\\RCAD\\dispatch-worktrees\\ISSUE-231"'
        }

        It 'references the .attempt<N> and .interrupt<N> archive conventions' {
            $script:Section | Should -Match '\.attempt'
            $script:Section | Should -Match '\.interrupt'
        }
    }

    Context 'Invalid inputs fail fast' {
        It 'rejects an empty -WorktreePath' {
            { Format-DispatchWorktreeAuditSection -WorktreePath '' } |
                Should -Throw -ExpectedMessage '*WorktreePath*'
        }

        It 'rejects a whitespace-only -WorktreePath' {
            { Format-DispatchWorktreeAuditSection -WorktreePath "   `t" } |
                Should -Throw -ExpectedMessage '*WorktreePath*'
        }
    }

    Context 'Determinism and purity' {
        It 'returns identical sections for repeated calls with the same input' {
            $a = Format-DispatchWorktreeAuditSection -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $b = Format-DispatchWorktreeAuditSection -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231'
            $a | Should -BeExactly $b
        }

        It 'returns a System.String' {
            (Format-DispatchWorktreeAuditSection -WorktreePath 'A:\RCAD\dispatch-worktrees\ISSUE-231').GetType().FullName |
                Should -Be 'System.String'
        }
    }
}

Describe 'Write-DispatchLog embeds Format-DispatchWorktreeAuditSection when run in a worktree' {

    Context 'Audit log body includes worktree path/status when -WorktreeRoot is supplied' {
        BeforeAll {
            # Build a stand-in for the GitHub issue object Write-DispatchLog
            # expects. The helper only reads `.number`, `.title`, and `.url`.
            $script:Issue = [pscustomobject]@{
                number = 9999
                title  = 'Audit-section integration test'
                url    = 'https://example.invalid/issues/9999'
            }
            $script:TmpRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("rge-dispatch-test-" + [Guid]::NewGuid().ToString('N'))
            $script:PrimaryRoot  = Join-Path $script:TmpRoot 'primary'
            $script:WorktreeRoot = Join-Path $script:TmpRoot 'dispatch-worktrees\ISSUE-AUDIT'
            New-Item -ItemType Directory -Path $script:PrimaryRoot -Force | Out-Null
            New-Item -ItemType Directory -Path $script:WorktreeRoot -Force | Out-Null
            # Initialise a minimal git repo inside the fake worktree so the
            # `git -C <worktree> status` and `git diff` calls Write-DispatchLog
            # makes do not crash. No network, no external services.
            & git init --quiet $script:WorktreeRoot
            & git -C $script:WorktreeRoot config user.email 'test@invalid'
            & git -C $script:WorktreeRoot config user.name  'Test'
            & git -C $script:WorktreeRoot commit --allow-empty --quiet -m 'init'

            $script:OrigRepoRoot = $script:RepoRoot
            $script:OrigQueueLabel = $QueueLabel
            $script:OrigRunLabel = $runLabel
            $script:RepoRoot   = $script:PrimaryRoot
            $script:QueueLabel = 'ai-dispatch'
            $script:runLabel   = 'ai-dispatch-running'

            $script:LogPath = Write-DispatchLog `
                -Id 'ISSUE-AUDIT' `
                -Issue $script:Issue `
                -Branch 'ai-dispatch/ISSUE-AUDIT' `
                -LoopLog (Join-Path $script:TmpRoot 'loop.log') `
                -LoopText 'fake loop text' `
                -LoopExit 0 `
                -Verdict 'pass' `
                -WorktreeRoot $script:WorktreeRoot
            $script:LogBody = Get-Content -Raw -LiteralPath $script:LogPath
        }

        AfterAll {
            $script:RepoRoot   = $script:OrigRepoRoot
            $script:QueueLabel = $script:OrigQueueLabel
            $script:runLabel   = $script:OrigRunLabel
            if (Test-Path -LiteralPath $script:TmpRoot) {
                Remove-Item -LiteralPath $script:TmpRoot -Recurse -Force -ErrorAction SilentlyContinue
            }
        }

        It 'writes an Isolated Worktree section into the audit log body' {
            $script:LogBody | Should -Match '##\s+Isolated Worktree'
        }

        It 'names the worktree path in the committed audit log content' {
            $expected = [regex]::Escape($script:WorktreeRoot)
            $script:LogBody | Should -Match "Worktree path: ``$expected``"
        }

        It 'includes the deterministic inspection command in the audit log body' {
            $expected = [regex]::Escape($script:WorktreeRoot)
            $script:LogBody | Should -Match "git -C ""$expected"" status --short --branch"
        }

        It 'includes the deterministic removal command in the audit log body' {
            $expected = [regex]::Escape($script:WorktreeRoot)
            $script:LogBody | Should -Match "git worktree remove ""$expected"""
        }
    }
}
