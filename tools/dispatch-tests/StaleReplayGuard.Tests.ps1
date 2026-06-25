#Requires -Version 5.1
<#
.SYNOPSIS
    Regression coverage for the stale-replay guard's published-commit query in
    Invoke-AiDispatchQueue.ps1 -- specifically the migrated-history issue-number
    collision that wrongly skipped a fresh dispatch.

.DESCRIPTION
    Dot-sources the queue through its RGE_AI_DISPATCH_QUEUE_SKIP_MAIN seam to load
    the pure helper Get-StaleReplayPublishedShaArgs without running the dispatch flow.

    Bug being pinned: after the RustCADs/RGE -> drhalilmurattasci/RGE migration, the
    new repo restarted issue numbering, so a fresh "ISSUE-4" (task 171) collided with
    an ANCIENT imported commit "ai-dispatch ISSUE-4: ..." (2026-05-17). The unscoped
    grep matched it -> false "already published" -> the dispatch was skipped without
    running. The fix is a `--since=<issue.createdAt>` floor (a real publish is always
    newer than its issue).
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try { . $script:QueueScriptPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Get-StaleReplayPublishedShaArgs (migration issue-number collision floor)' {
    It 'adds a --since floor at the issue createdAt and keeps the issue-id grep' {
        $a = Get-StaleReplayPublishedShaArgs -IssueId 'ISSUE-4' -CreatedAt '2026-06-25T03:59:56Z'
        $a | Should -Contain '--grep=ai-dispatch ISSUE-4:'
        ($a -join ' ') | Should -Match '--since=2026-06-25T03:59:56Z'
    }

    It 'omits the floor when createdAt is empty (defensive fallback to the unscoped grep)' {
        ((Get-StaleReplayPublishedShaArgs -IssueId 'ISSUE-4' -CreatedAt '') -join ' ') | Should -Not -Match '--since'
        ((Get-StaleReplayPublishedShaArgs -IssueId 'ISSUE-4' -CreatedAt $null) -join ' ') | Should -Not -Match '--since'
    }

    It 'the floored query EXCLUDES a migrated old-repo "ai-dispatch ISSUE-4:" commit, the unfloored one matches it' {
        # Build a tiny repo whose history contains the colliding old commit (as the
        # migration imported it), with origin/main pointing at it so the helper's
        # `origin/main` revision resolves.
        $repo = Join-Path $TestDrive 'collide'
        New-Item -ItemType Directory -Path $repo -Force | Out-Null
        git -C $repo init -q | Out-Null
        git -C $repo config user.email 't@t.test' | Out-Null
        git -C $repo config user.name 'test' | Out-Null
        Set-Content -LiteralPath (Join-Path $repo 'a.txt') -Value 'x' -NoNewline
        git -C $repo add -A | Out-Null
        $env:GIT_AUTHOR_DATE = '2026-05-17T23:59:13'
        $env:GIT_COMMITTER_DATE = '2026-05-17T23:59:13'
        git -C $repo commit -q -m 'ai-dispatch ISSUE-4: Add verification gate pointer to AGENTS.md' | Out-Null
        Remove-Item Env:GIT_AUTHOR_DATE -ErrorAction SilentlyContinue
        Remove-Item Env:GIT_COMMITTER_DATE -ErrorAction SilentlyContinue
        git -C $repo update-ref refs/remotes/origin/main HEAD | Out-Null

        # Unfloored (the OLD behavior) wrongly matches the ancient commit:
        $unfloored = Get-StaleReplayPublishedShaArgs -IssueId 'ISSUE-4' -CreatedAt ''
        (git -C $repo @unfloored) | Should -Not -BeNullOrEmpty

        # Floored at a NEW issue's creation (after the old commit) -> no false match:
        $floored = Get-StaleReplayPublishedShaArgs -IssueId 'ISSUE-4' -CreatedAt '2026-06-25T03:59:56Z'
        (git -C $repo @floored) | Should -BeNullOrEmpty
    }
}
