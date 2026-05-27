#Requires -Version 5.1
<#
.SYNOPSIS
    Pester regression test for Invoke-AiDispatchQueue.ps1's Write-DispatchLog.

.DESCRIPTION
    Builds a temporary git repo + synthetic .ai/dispatch-<id>/ run dir, dot-
    sources the production queue script through its testability seam, calls
    the production Write-DispatchLog with synthetic inputs, then asserts:

      1. The generated audit log contains zero unexpanded PowerShell variable
         tokens (regex `\$[A-Za-z_][A-Za-z0-9_]*`) across the entire markdown.
      2. The synthetic dispatch id, issue number/title/url, branch, loop log
         path, loop output text, loop exit code, control verdict, queue
         labels / process trace, git status, diff name-status, diff stat,
         generated run files, Claude marker lines, and Codex control JSON
         all appear in the output -- so the no-token assertion above is not
         vacuously satisfied by an empty log.

    The test never touches the real repo's ai_dispatch_logs/ tree -- the
    production writer is steered at a temp filesystem root via
    $script:RepoRoot.
#>

BeforeAll {
    $script:TestsRoot = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTests = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTests 'Invoke-AiDispatchQueue.ps1'
    if (-not (Test-Path -LiteralPath $script:QueueScriptPath)) {
        throw "Invoke-AiDispatchQueue.ps1 not found at $script:QueueScriptPath"
    }

    # Dot-source the production queue script through the testability seam so
    # its function definitions (Write-DispatchLog, Git-Step, ...) land in
    # this Pester session without running the dispatch flow.
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }
}

Describe 'Write-DispatchLog (production audit-log writer)' {

    BeforeAll {
        # --- Temporary on-disk git repo for the synthetic dispatch -----------
        $script:TempRepoRoot = Join-Path ([System.IO.Path]::GetTempPath()) `
            ("rge-write-dispatch-log-" + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $script:TempRepoRoot -Force | Out-Null

        Push-Location $script:TempRepoRoot
        try {
            & git init -q
            if ($LASTEXITCODE -ne 0) { throw "git init failed in $script:TempRepoRoot" }
            & git config user.email 'pester@example.invalid'
            & git config user.name  'Pester Test'
            & git config commit.gpgsign false

            $trackedFile = Join-Path $script:TempRepoRoot 'tracked-fixture.txt'
            Set-Content -LiteralPath $trackedFile -Value 'initial fixture content' -Encoding utf8
            & git add 'tracked-fixture.txt' | Out-Null
            & git commit -q -m 'pester fixture: initial commit' | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'git commit failed for pester fixture' }

            # Tracked modification + untracked add so git status / diff are non-empty.
            Set-Content -LiteralPath $trackedFile -Value 'modified fixture content' -Encoding utf8
            $untrackedFile = Join-Path $script:TempRepoRoot 'untracked-fixture.txt'
            Set-Content -LiteralPath $untrackedFile -Value 'untracked fixture content' -Encoding utf8
        } finally {
            Pop-Location
        }

        # --- Synthetic .ai/dispatch-<id> run-dir with marker + control JSON --
        $script:DispatchId      = 'ISSUE-PESTER-99999'
        $script:DispatchRunDir  = Join-Path $script:TempRepoRoot (Join-Path '.ai' "dispatch-$($script:DispatchId)")
        New-Item -ItemType Directory -Path $script:DispatchRunDir -Force | Out-Null

        $executeMd = Join-Path $script:DispatchRunDir 'claude.execute.round0.md'
        $executeBody = @'
# Synthetic Claude execute round (pester fixture)

This file exists so Write-DispatchLog has a marker-bearing file to summarize.

EXEC_PACKET: ai_handoffs/ISSUE-PESTER-99999_EXEC_2026-01-01_00-00-00+0000.md
EXEC_STATUS: executed
GATE_VERDICT: pass
'@
        [System.IO.File]::WriteAllText($executeMd, $executeBody, [System.Text.UTF8Encoding]::new($false))

        $controlJson = Join-Path $script:DispatchRunDir 'codex.control.round0.json'
        $controlBody = @'
{
  "verdict": "pass",
  "summary": "Synthetic Codex control summary for pester regression test.",
  "required_fixes": []
}
'@
        [System.IO.File]::WriteAllText($controlJson, $controlBody, [System.Text.UTF8Encoding]::new($false))

        # --- Loop log fixture ----------------------------------------------
        $script:LoopLogPath = Join-Path $script:TempRepoRoot 'pester-loop.log'
        $script:LoopTextBody = "loop fixture line one`nloop fixture line two`nloop fixture line three"
        [System.IO.File]::WriteAllText($script:LoopLogPath, $script:LoopTextBody, [System.Text.UTF8Encoding]::new($false))

        # --- Synthetic issue payload ---------------------------------------
        $script:SyntheticIssue = [pscustomobject]@{
            number = 99999
            title  = 'Pester synthetic issue title for dispatch log writer test'
            url    = 'https://example.invalid/RCAD/RGE/issues/99999'
        }
        $script:SyntheticBranch = "ai-dispatch/$($script:DispatchId)"

        # --- Steer the production writer at the temp repo ------------------
        # The top-level body that normally initializes these was skipped via
        # the testability seam, so set them by hand for this single call.
        $script:RepoRoot = $script:TempRepoRoot
        Set-Variable -Name 'QueueLabel' -Value 'pester-queue-label' -Scope 'Script'
        Set-Variable -Name 'runLabel'   -Value 'pester-queue-label-running' -Scope 'Script'

        # Git-Step uses the current working directory.
        Push-Location $script:TempRepoRoot
        try {
            $script:GeneratedLogPath = Write-DispatchLog `
                -Id $script:DispatchId `
                -Issue $script:SyntheticIssue `
                -Branch $script:SyntheticBranch `
                -LoopLog $script:LoopLogPath `
                -LoopText $script:LoopTextBody `
                -LoopExit 0 `
                -Verdict 'pass'
        } finally {
            Pop-Location
        }

        $script:GeneratedLogContent = [System.IO.File]::ReadAllText($script:GeneratedLogPath)
    }

    AfterAll {
        if ($script:TempRepoRoot -and (Test-Path -LiteralPath $script:TempRepoRoot)) {
            # Best-effort cleanup; never fail the test on a stuck file handle.
            Remove-Item -LiteralPath $script:TempRepoRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'writes the log under the temp repo''s ai_dispatch_logs/ directory' {
        $script:GeneratedLogPath | Should -Not -BeNullOrEmpty
        $script:GeneratedLogPath | Should -Match 'ai_dispatch_logs[\\/]log_.+\.md$'
        $script:GeneratedLogPath.StartsWith($script:TempRepoRoot, [StringComparison]::OrdinalIgnoreCase) `
            | Should -BeTrue
    }

    It 'expands every PowerShell variable token in the generated markdown' {
        $tokenRegex = [regex]'\$[A-Za-z_][A-Za-z0-9_]*'
        $tokenMatches = $tokenRegex.Matches($script:GeneratedLogContent)
        if ($tokenMatches.Count -gt 0) {
            $tokens = ($tokenMatches | ForEach-Object { $_.Value } | Sort-Object -Unique) -join ', '
            throw "Unexpanded PowerShell variable token(s) leaked into the generated audit log: $tokens"
        }
        $tokenMatches.Count | Should -Be 0
    }

    It 'includes the synthetic dispatch id, issue, and branch' {
        $script:GeneratedLogContent | Should -Match ([regex]::Escape($script:DispatchId))
        $script:GeneratedLogContent | Should -Match '#99999'
        $script:GeneratedLogContent | Should -Match 'Pester synthetic issue title for dispatch log writer test'
        $script:GeneratedLogContent | Should -Match 'https://example\.invalid/RCAD/RGE/issues/99999'
        $script:GeneratedLogContent | Should -Match ([regex]::Escape($script:SyntheticBranch))
    }

    It 'includes the loop log path, output text, and exit code' {
        $script:GeneratedLogContent | Should -Match ([regex]::Escape($script:LoopLogPath))
        $script:GeneratedLogContent | Should -Match 'loop fixture line one'
        $script:GeneratedLogContent | Should -Match 'loop fixture line three'
        # Header lines wrap values in single backticks because the writer's
        # double-quoted here-string collapses `` `` `` to one literal backtick.
        $script:GeneratedLogContent | Should -Match 'Loop exit code: `0`'
    }

    It 'records the Codex control verdict in the header' {
        $script:GeneratedLogContent | Should -Match 'Codex control verdict: `pass`'
    }

    It 'embeds the synthetic queue label and running label in the process trace' {
        $script:GeneratedLogContent | Should -Match '## Process Trace'
        $script:GeneratedLogContent | Should -Match 'pester-queue-label'
        $script:GeneratedLogContent | Should -Match 'pester-queue-label-running'
    }

    It 'embeds the git status summary' {
        $script:GeneratedLogContent | Should -Match '## Files Changed / Added / Deleted'
        $script:GeneratedLogContent | Should -Match 'tracked-fixture\.txt'
        $script:GeneratedLogContent | Should -Match 'untracked-fixture\.txt'
    }

    It 'embeds the git diff --name-status output' {
        $script:GeneratedLogContent | Should -Match '`git diff --name-status`'
        $script:GeneratedLogContent | Should -Match '(?m)^[A-Z]\s+tracked-fixture\.txt'
    }

    It 'embeds the git diff --stat output' {
        $script:GeneratedLogContent | Should -Match '`git diff --stat`'
    }

    It 'lists the generated .ai/dispatch-<id>/ run files' {
        $script:GeneratedLogContent | Should -Match '## Generated Run Files'
        $script:GeneratedLogContent | Should -Match 'claude\.execute\.round0\.md'
        $script:GeneratedLogContent | Should -Match 'codex\.control\.round0\.json'
    }

    It 'summarizes the Claude marker lines' {
        $script:GeneratedLogContent | Should -Match '## Claude Marker Summary'
        $script:GeneratedLogContent | Should -Match 'EXEC_STATUS: executed'
        $script:GeneratedLogContent | Should -Match 'GATE_VERDICT: pass'
        $script:GeneratedLogContent | Should -Match 'EXEC_PACKET: ai_handoffs/ISSUE-PESTER-99999_EXEC_'
    }

    It 'embeds the Codex control JSON body' {
        $script:GeneratedLogContent | Should -Match '## Codex Control JSON'
        $script:GeneratedLogContent | Should -Match '"verdict": "pass"'
        $script:GeneratedLogContent | Should -Match 'Synthetic Codex control summary for pester regression test\.'
    }

    It 'includes the truncated loop output section' {
        $script:GeneratedLogContent | Should -Match '## Loop Output'
        $script:GeneratedLogContent | Should -Match 'loop fixture line two'
    }
}
