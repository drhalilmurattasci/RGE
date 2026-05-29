#Requires -Version 5.1
<#
.SYNOPSIS
    ISSUE-259 invariants on Invoke-AiDispatchQueue.ps1: the loop's run dir
    (.ai/dispatch-<id>/) is copied OUT of the disposable worktree into the
    primary checkout's .ai/ BEFORE every `git worktree remove`, so
    Get-AiDispatchHealth.ps1 (which reads only the primary .ai/dispatch-*/)
    does not go blind after a successful run removes the worktree.

.DESCRIPTION
    Two layers of coverage:

    (A) Behavioural unit tests of the pure helper Copy-DispatchRunDirToPrimary
        against TestDrive directories (no git / gh / network / lock): the run
        dir is mirrored, re-copy refreshes it, missing source short-circuits,
        a self-copy (Worktree == Primary) short-circuits, a worktree-local
        dispatch-trace/*.jsonl is mirrored, and an unwritable destination
        warns-and-returns instead of throwing.

    (B) Source-level regression assertions on $script:QueueScriptText: the
        helper is defined, and EVERY `git worktree remove` destroy-site is
        immediately preceded by a Copy-DispatchRunDirToPrimary call, while the
        two `git worktree move` archive-sites (which preserve the run dir on
        disk) intentionally have none. A future edit that adds a new remove
        path without a copy-out fails this gate.

    The helper is loaded by dot-sourcing the queue script through the
    RGE_AI_DISPATCH_QUEUE_SKIP_MAIN seam (same as Issue231-WorktreeIsolation),
    so no gh/codex/claude/lock side effects run.
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

Describe 'ISSUE-259 Copy-DispatchRunDirToPrimary behaviour' {

    BeforeEach {
        # A unique sandbox per test: TestDrive is NOT reset between It blocks in
        # a Describe, so use a fresh GUID-named subdir to avoid cross-test leak.
        $sandbox        = Join-Path $TestDrive ([guid]::NewGuid().ToString('N'))
        $script:wt      = Join-Path $sandbox 'worktree'
        $script:primary = Join-Path $sandbox 'primary'
        $script:wtRun   = Join-Path $script:wt '.ai\dispatch-ISSUE-999'
        New-Item -ItemType Directory -Path $script:wtRun -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $script:primary '.ai') -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $script:wtRun 'codex.control.round0.json') -Value '{"verdict":"pass"}' -Encoding UTF8
        Set-Content -LiteralPath (Join-Path $script:wtRun 'execute.round0.txt')        -Value 'round0'           -Encoding UTF8
        Set-Content -LiteralPath (Join-Path $script:wtRun 'verification.round0.log')   -Value 'gate ok'          -Encoding UTF8
        # Mirror the real queue runtime: $script:RepoRoot is always set at every
        # Copy-DispatchRunDirToPrimary call site (Invoke-AiDispatchQueue.ps1
        # line ~1968). The helper's informational repo-relative path line routes
        # through Get-RepoRelativePathForQueue, which calls
        # [IO.Path]::GetFullPath on that root; an empty root would make the
        # (purely cosmetic) status line throw. Set it so the test exercises the
        # same path the runtime does.
        $script:RepoRoot = $script:primary
    }

    It 'mirrors the worktree run dir into the primary .ai/' {
        Copy-DispatchRunDirToPrimary -WorktreeRoot $script:wt -PrimaryRoot $script:primary -DispatchId 'ISSUE-999' 6>$null
        $dst = Join-Path $script:primary '.ai\dispatch-ISSUE-999\codex.control.round0.json'
        Test-Path -LiteralPath $dst | Should -BeTrue
        (Get-Content -LiteralPath $dst -Raw).Trim() | Should -Be '{"verdict":"pass"}'
        Test-Path -LiteralPath (Join-Path $script:primary '.ai\dispatch-ISSUE-999\execute.round0.txt')      | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $script:primary '.ai\dispatch-ISSUE-999\verification.round0.log') | Should -BeTrue
    }

    It 're-copy refreshes an existing primary mirror in place (idempotent)' {
        $dst = Join-Path $script:primary '.ai\dispatch-ISSUE-999\codex.control.round0.json'
        # Seed a stale primary mirror.
        New-Item -ItemType Directory -Path (Split-Path -Parent $dst) -Force | Out-Null
        Set-Content -LiteralPath $dst -Value '{"verdict":"STALE"}' -Encoding UTF8
        # Update the source, then re-copy.
        Set-Content -LiteralPath (Join-Path $script:wtRun 'codex.control.round0.json') -Value '{"verdict":"fresh"}' -Encoding UTF8
        Copy-DispatchRunDirToPrimary -WorktreeRoot $script:wt -PrimaryRoot $script:primary -DispatchId 'ISSUE-999' 6>$null
        (Get-Content -LiteralPath $dst -Raw).Trim() | Should -Be '{"verdict":"fresh"}'
    }

    It 'short-circuits without throwing when the worktree .ai/ is missing' {
        $emptyWt = Join-Path $TestDrive 'empty-worktree'
        New-Item -ItemType Directory -Path $emptyWt -Force | Out-Null
        { Copy-DispatchRunDirToPrimary -WorktreeRoot $emptyWt -PrimaryRoot $script:primary -DispatchId 'ISSUE-999' 6>$null } |
            Should -Not -Throw
        Test-Path -LiteralPath (Join-Path $script:primary '.ai\dispatch-ISSUE-999') | Should -BeFalse
    }

    It 'short-circuits when WorktreeRoot == PrimaryRoot (no self-copy)' {
        # Point both at the worktree; the run dir already lives there, and the
        # helper must not try to copy a directory onto itself.
        { Copy-DispatchRunDirToPrimary -WorktreeRoot $script:wt -PrimaryRoot $script:wt -DispatchId 'ISSUE-999' 6>$null } |
            Should -Not -Throw
        # The original source file is untouched.
        Test-Path -LiteralPath (Join-Path $script:wtRun 'codex.control.round0.json') | Should -BeTrue
    }

    It 'mirrors a worktree-local dispatch-trace/*.jsonl into the primary dispatch-trace/' {
        $srcTrace = Join-Path $script:wt '.ai\dispatch-trace'
        New-Item -ItemType Directory -Path $srcTrace -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $srcTrace 'Invoke-AiDispatchQueue-1234.jsonl') -Value '{"message":"queue.loop: start"}' -Encoding UTF8
        Copy-DispatchRunDirToPrimary -WorktreeRoot $script:wt -PrimaryRoot $script:primary -DispatchId 'ISSUE-999' 6>$null
        Test-Path -LiteralPath (Join-Path $script:primary '.ai\dispatch-trace\Invoke-AiDispatchQueue-1234.jsonl') | Should -BeTrue
    }

    It 'returns @() / does not throw when DispatchId is empty' {
        { Copy-DispatchRunDirToPrimary -WorktreeRoot $script:wt -PrimaryRoot $script:primary -DispatchId '' 6>$null } |
            Should -Not -Throw
    }

    It 'warns and returns (does not throw) when the run dir source cannot be read' {
        # Make the source run dir un-copyable by holding an exclusive lock on a
        # file inside it; Copy-Item -ErrorAction Stop then throws internally and
        # the helper's catch must swallow it (best-effort contract).
        $locked = Join-Path $script:wtRun 'codex.control.round0.json'
        $fs = [System.IO.File]::Open($locked, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::None)
        try {
            { Copy-DispatchRunDirToPrimary -WorktreeRoot $script:wt -PrimaryRoot $script:primary -DispatchId 'ISSUE-999' 6>$null } |
                Should -Not -Throw
        } finally {
            $fs.Close()
            $fs.Dispose()
        }
    }
}

Describe 'ISSUE-259 run-evidence copy-out source-level invariants' {

    It 'defines Copy-DispatchRunDirToPrimary' {
        (Get-Command -Name Copy-DispatchRunDirToPrimary -ErrorAction SilentlyContinue) |
            Should -Not -BeNullOrEmpty
        $script:QueueScriptText | Should -Match 'function\s+Copy-DispatchRunDirToPrimary'
    }

    It 'precedes EVERY `git worktree remove` with a Copy-DispatchRunDirToPrimary call' {
        $lines = $script:QueueScriptText -split "`r?`n"
        $removeLineNumbers = @()
        for ($i = 0; $i -lt $lines.Count; $i++) {
            if ($lines[$i] -match "'worktree'\s*,\s*'remove'") {
                $removeLineNumbers += $i
            }
        }
        # There must be at least the four known destroy-sites.
        $removeLineNumbers.Count | Should -BeGreaterOrEqual 4

        foreach ($idx in $removeLineNumbers) {
            # Look up to 3 lines above the remove for the copy-out call.
            $windowStart = [Math]::Max(0, $idx - 3)
            $window = ($lines[$windowStart..($idx - 1)] -join "`n")
            $window | Should -Match 'Copy-DispatchRunDirToPrimary' -Because (
                "the `git worktree remove` at source line $($idx + 1) must be immediately preceded by a run-evidence copy-out"
            )
        }
    }

    It 'does NOT add a copy-out before the two `git worktree move` archive-sites' {
        # The archive (.attempt/.interrupt) paths preserve the run dir on disk,
        # so they intentionally have no copy-out. Pin that the number of
        # copy-out calls equals the number of remove-sites, not remove+move.
        $copyOutCount = ([regex]::Matches($script:QueueScriptText, 'Copy-DispatchRunDirToPrimary\s+-WorktreeRoot')).Count
        $removeCount  = ([regex]::Matches($script:QueueScriptText, "'worktree'\s*,\s*'remove'")).Count
        $moveCount    = ([regex]::Matches($script:QueueScriptText, "'worktree'\s*,\s*'move'")).Count

        $moveCount       | Should -BeGreaterOrEqual 2
        $copyOutCount    | Should -Be $removeCount
    }
}
