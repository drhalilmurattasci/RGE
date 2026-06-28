#Requires -Version 5.1
<#
.SYNOPSIS
    Focused coverage for queue numeric parsing guards.

.DESCRIPTION
    Dot-sources Invoke-AiDispatchQueue.ps1 through its skip-main seam and proves
    malformed or overflowing numeric fields sort low / parse absent / classify as
    unknown instead of throwing. No gh, git, codex, claude, or network calls.
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

Describe 'Queue numeric parsing guards' {
    It 'sorts overflowing round artifact numbers low instead of throwing' {
        $runDir = Join-Path $TestDrive 'rounds'
        New-Item -ItemType Directory -Path $runDir -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $runDir 'codex.control.round999999999999999999999999.json') -Value '{}'
        Set-Content -LiteralPath (Join-Path $runDir 'codex.control.round2.json') -Value '{}'

        (Get-NewestRoundFile -RunDir $runDir -Filter 'codex.control.round*.json').Name |
            Should -Be 'codex.control.round2.json'
    }

    It 'ignores overflowing handoff claim actor pids instead of throwing' {
        $record = [pscustomobject]@{ actor = 'Invoke-AiDispatchQueue.ps1:999999999999999999999999' }
        Get-QueueHandoffClaimOwnerPid -Record $record | Should -Be 0
    }

    It 'treats overflowing lock pid and procstart values as absent' {
        $lock = Join-Path $TestDrive 'queue.lock'
        Set-Content -LiteralPath $lock -Value @(
            'pid=999999999999999999999999'
            'procstart=999999999999999999999999'
        )

        $info = Get-LockInfo -Path $lock -StaleLockMinutes 180
        $info.OwnerPid | Should -Be 0
        $info.OwnerStart | Should -Be 0
        $info.Alive | Should -BeFalse
    }

    It 'treats overflowing EXEC packet EXIT_CODE as unknown instead of executed' {
        $repo = Join-Path $TestDrive 'repo'
        $handoffDir = Join-Path $repo 'ai_handoffs'
        New-Item -ItemType Directory -Path $handoffDir -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $handoffDir 'ISSUE-99_EXEC_2026-06-28_12-00-00+0300.md') -Value @(
            'HANDOFF_STATUS: COMPLETE'
            'STATUS: COMPLETE'
            'EXIT_CODE: 999999999999999999999999'
        )

        $oldRepoRoot = $script:RepoRoot
        $oldDispatchWorktreeRoot = $script:DispatchWorktreeRoot
        try {
            $script:RepoRoot = $repo
            $script:DispatchWorktreeRoot = ''
            Get-ExecutionStatus -RunDir (Join-Path $TestDrive 'missing-run-dir') -DispatchId 'ISSUE-99' |
                Should -Be 'unknown'
        } finally {
            $script:RepoRoot = $oldRepoRoot
            $script:DispatchWorktreeRoot = $oldDispatchWorktreeRoot
        }
    }
}
