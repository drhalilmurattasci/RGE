#Requires -Version 5.1
<#
.SYNOPSIS
    ISSUE-237 regression coverage for Invoke-AiDispatchQueue.ps1's queue
    scope guard around backtick-quoted leading-dot repo path tokens, plus
    the post-dispatch-log / pre-publish-decision failure-visibility wrap.

.DESCRIPTION
    The ISSUE-234 dispatch stalled at the queue scope guard because the
    classifier in Test-LooksLikePathToken rejected the bare `.gitattributes`
    token: it had no `/`, no `*`, and no `.<ext>` suffix to clear the
    accept gates. The TASK packet explicitly listed `.gitattributes`
    under `### MAY edit`, so the rejection was a deterministic mis-
    classification of a valid leading-dot repo path token, not a real
    out-of-scope edit.

    This file pins three behaviours:

      1. Test-LooksLikePathToken accepts practical leading-dot repo path
         tokens (`.gitattributes`, `.gitignore`, `.env`, `.envrc`,
         `.cargo/config.toml`) AND rejects ordinary prose tokens
         (`important`, `the`, bare `.`, `..`, whitespace-bearing strings).

      2. Get-TaskPositiveAllowedTokens extracts `.gitattributes` when a
         TASK packet lists it under `### MAY edit`.

      3. Invoke-QueueScopeGuard passes against a synthetic temp-git
         worktree where the only changed path is `.gitattributes` and
         the active TASK packet explicitly allows `.gitattributes`. This
         reproduces the #234 scope-guard fail-closed regression and
         pins the fix.

    Also pins the failure-visibility wrap added by ISSUE-237: the
    production Fail helper mirrors its message to stdout (so a Fail
    exit inside the post-dispatch-log / pre-publish-decision window
    surfaces in captured queue output), and the queue script wraps
    that window in a try/catch that emits non-Fail terminating errors
    to stdout before re-throwing.

    The test never touches the real repo root `.gitattributes`: a temp
    git repo + synthetic TASK packet are used as the fixture, in line
    with the TASK packet's MUST NOT edit list.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    if (-not (Test-Path -LiteralPath $script:QueueScriptPath)) {
        throw "Invoke-AiDispatchQueue.ps1 not found at $script:QueueScriptPath"
    }
    $script:QueueScriptText = Get-Content -Raw -LiteralPath $script:QueueScriptPath

    # Dot-source the production queue script through the testability seam
    # so its helpers (Test-LooksLikePathToken, Get-TaskPositiveAllowedTokens,
    # Invoke-QueueScopeGuard, ...) land in this Pester session without
    # running the dispatch flow.
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try {
        . $script:QueueScriptPath
    } finally {
        Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    # Replace the production Fail (which calls exit 1) with a throw-based
    # wrapper that lives in the same dot-source scope. PowerShell resolves
    # `Fail` from inside Invoke-QueueScopeGuard etc. via dynamic scoping,
    # so this redefinition is what scope-guard tests can catch with
    # Should -Throw without nuking the Pester session via exit. The
    # production Fail is preserved on the on-disk source -- the queue's
    # source-level tests below assert against $script:QueueScriptText,
    # not against the dot-sourced runtime function.
    function Fail { param([string]$Message) throw $Message }
}

Describe 'ISSUE-237 Test-LooksLikePathToken classifier' {

    Context 'Accepts strict leading-dot repo path tokens' {
        It 'accepts <Token>' -TestCases @(
            @{ Token = '.gitattributes'     },
            @{ Token = '.gitignore'         },
            @{ Token = '.env'               },
            @{ Token = '.envrc'             },
            @{ Token = '.editorconfig'      },
            @{ Token = '.cargo/config.toml' },
            @{ Token = '.github/workflows/x.yml' }
        ) {
            param([string]$Token)
            (Test-LooksLikePathToken -Token $Token) | Should -BeTrue
        }
    }

    Context 'Rejects prose tokens, bare dots, and whitespace-bearing strings' {
        It 'rejects <Token>' -TestCases @(
            @{ Token = ''             },
            @{ Token = ' '            },
            @{ Token = 'important'    },
            @{ Token = 'the'          },
            @{ Token = 'git add'      },
            @{ Token = '.'            },
            @{ Token = '..'           },
            @{ Token = '...'          },
            @{ Token = '. gitattributes' },
            @{ Token = 'Write-DispatchLog' }
        ) {
            param([string]$Token)
            (Test-LooksLikePathToken -Token $Token) | Should -BeFalse
        }
    }

    Context 'Continues to accept the pre-existing path shapes' {
        It 'accepts <Token>' -TestCases @(
            @{ Token = 'src/lib.rs'                 },
            @{ Token = 'crates/foo/**/*.rs'         },
            @{ Token = 'tools/dispatch-tests/**'    },
            @{ Token = 'README.md'                  },
            @{ Token = 'foo/bar/baz.json'           }
        ) {
            param([string]$Token)
            (Test-LooksLikePathToken -Token $Token) | Should -BeTrue
        }
    }
}

Describe 'ISSUE-237 Get-TaskPositiveAllowedTokens extracts leading-dot tokens' {

    BeforeAll {
        $script:SyntheticTaskDir = Join-Path ([System.IO.Path]::GetTempPath()) `
            ("rge-issue237-task-" + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $script:SyntheticTaskDir -Force | Out-Null

        $script:SyntheticTaskPath = Join-Path $script:SyntheticTaskDir 'ISSUE-PESTER-237_TASK.md'
        $taskBody = @'
# Task Packet

DISPATCH_ID: ISSUE-PESTER-237

## Scope

### MAY edit

- `.gitattributes`
- `crates/example/**`
- `tools/dispatch-tests/**`

### MAY add new files

- `.envrc`
- `tools/dispatch-tests/**`

### MUST NOT edit

- `important` prose tokens are rejected by the classifier and must not
  enter the allowlist even though they appear inside this packet.
- `the` quick brown fox.
'@
        [System.IO.File]::WriteAllText($script:SyntheticTaskPath, $taskBody,
            [System.Text.UTF8Encoding]::new($false))
    }

    AfterAll {
        if ($script:SyntheticTaskDir -and (Test-Path -LiteralPath $script:SyntheticTaskDir)) {
            Remove-Item -LiteralPath $script:SyntheticTaskDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'includes `.gitattributes` from the MAY edit section' {
        $tokens = Get-TaskPositiveAllowedTokens -TaskPath $script:SyntheticTaskPath
        $tokens | Should -Contain '.gitattributes'
    }

    It 'includes `.envrc` from the MAY add new files section' {
        $tokens = Get-TaskPositiveAllowedTokens -TaskPath $script:SyntheticTaskPath
        $tokens | Should -Contain '.envrc'
    }

    It 'includes the pre-existing slash-bearing tokens' {
        $tokens = Get-TaskPositiveAllowedTokens -TaskPath $script:SyntheticTaskPath
        $tokens | Should -Contain 'crates/example/**'
        $tokens | Should -Contain 'tools/dispatch-tests/**'
    }

    It 'does not promote prose tokens from MUST NOT edit into the allowlist' {
        $tokens = Get-TaskPositiveAllowedTokens -TaskPath $script:SyntheticTaskPath
        # The classifier rejects bare prose like `important` and `the`, and
        # Get-TaskPositiveAllowedTokens only scans MAY sections in the
        # first place. Both safeguards together must keep the allowlist
        # free of prose tokens.
        $tokens | Should -Not -Contain 'important'
        $tokens | Should -Not -Contain 'the'
    }
}

Describe 'ISSUE-237 Invoke-QueueScopeGuard accepts a modified .gitattributes' {

    BeforeAll {
        # --- Synthetic temp git repo for the scope-guard fixture ----------
        $script:TempRepoRoot = Join-Path ([System.IO.Path]::GetTempPath()) `
            ("rge-issue237-scope-guard-" + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $script:TempRepoRoot -Force | Out-Null

        Push-Location $script:TempRepoRoot
        try {
            & git init -q
            if ($LASTEXITCODE -ne 0) { throw "git init failed in $script:TempRepoRoot" }
            & git config user.email 'pester@example.invalid'
            & git config user.name  'Pester Test'
            & git config commit.gpgsign false

            # Seed an initial commit with a tracked .gitattributes so the
            # synthetic modification below produces an M status line, not A.
            $gaPath = Join-Path $script:TempRepoRoot '.gitattributes'
            Set-Content -LiteralPath $gaPath -Value "* text=auto`n" -Encoding ascii
            & git add '.gitattributes' | Out-Null
            & git commit -q -m 'pester fixture: seed .gitattributes' | Out-Null
            if ($LASTEXITCODE -ne 0) { throw 'git commit failed for pester fixture seed' }

            # Modify .gitattributes so `git status --short --untracked-files=all`
            # reports exactly one changed path: ` M .gitattributes`.
            Set-Content -LiteralPath $gaPath -Value "* text=auto`n*.txt text=lf`n" -Encoding ascii
        } finally {
            Pop-Location
        }

        # --- Synthetic active TASK packet under ai_handoffs/ ----------------
        $script:DispatchIdForFixture = 'ISSUE-PESTER-237'
        $script:HandoffDir = Join-Path $script:TempRepoRoot 'ai_handoffs'
        New-Item -ItemType Directory -Path $script:HandoffDir -Force | Out-Null
        $stamp = (Get-Date).ToString('yyyy-MM-dd_HH-mm-sszzz').Replace(':', '')
        $taskPath = Join-Path $script:HandoffDir ("$($script:DispatchIdForFixture)_TASK_$stamp.md")
        $taskBody = @"
# Task Packet

DISPATCH_ID: $($script:DispatchIdForFixture)

## Scope

### MAY edit

- ``.gitattributes``

### MAY add new files

- ``tools/dispatch-tests/**``
"@
        [System.IO.File]::WriteAllText($taskPath, $taskBody,
            [System.Text.UTF8Encoding]::new($false))
        $script:SyntheticTaskPacketPath = $taskPath

        # --- Synthetic dispatch log path (used by Test-ExactQueueLogPath) ----
        # The scope guard accepts the EXACT log path Write-DispatchLog just
        # returned; nothing in this fixture writes one, so a placeholder
        # that does not match the porcelain status line is sufficient.
        $script:SyntheticLogDir = Join-Path $script:TempRepoRoot 'ai_dispatch_logs'
        New-Item -ItemType Directory -Path $script:SyntheticLogDir -Force | Out-Null
        $script:SyntheticLogPath = Join-Path $script:SyntheticLogDir 'log_pester-fixture.md'
        [System.IO.File]::WriteAllText($script:SyntheticLogPath, '# pester fixture',
            [System.Text.UTF8Encoding]::new($false))
    }

    AfterAll {
        if ($script:TempRepoRoot -and (Test-Path -LiteralPath $script:TempRepoRoot)) {
            Remove-Item -LiteralPath $script:TempRepoRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'passes when .gitattributes is modified and the TASK packet allows it' {
        # Steer the production scope guard at the temp repo. The guard
        # reads Get-ActiveTaskPacketPathForQueueGuard relative to
        # $script:DispatchWorktreeRoot, and Get-QueueStatusEntries shells
        # out to `git -C $script:DispatchWorktreeRoot status ...`.
        $script:RepoRoot             = $script:TempRepoRoot
        $script:DispatchWorktreeRoot = $script:TempRepoRoot
        try {
            { Invoke-QueueScopeGuard `
                -DispatchId $script:DispatchIdForFixture `
                -DispatchLogPath $script:SyntheticLogPath } |
                Should -Not -Throw
        } finally {
            $script:DispatchWorktreeRoot = $null
        }
    }

    It 'passes when an ADR-121 claim event for this dispatch is untracked' {
        $claimDir = Join-Path $script:HandoffDir 'claims'
        New-Item -ItemType Directory -Path $claimDir -Force | Out-Null
        $claimPath = Join-Path $claimDir `
            "$($script:DispatchIdForFixture)_2026-06-06_12-34-56-1234567+0300_claim.json"
        [System.IO.File]::WriteAllText(
            $claimPath,
            '{"dispatch_id":"ISSUE-PESTER-237","event":"claim"}',
            [System.Text.UTF8Encoding]::new($false))

        $script:RepoRoot             = $script:TempRepoRoot
        $script:DispatchWorktreeRoot = $script:TempRepoRoot
        try {
            { Invoke-QueueScopeGuard `
                -DispatchId $script:DispatchIdForFixture `
                -DispatchLogPath $script:SyntheticLogPath } |
                Should -Not -Throw
        } finally {
            $script:DispatchWorktreeRoot = $null
            Remove-Item -LiteralPath $claimPath -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects non-claim nested handoff paths' {
        $nestedDir = Join-Path $script:HandoffDir `
            "$($script:DispatchIdForFixture)_EXEC_2026-06-06_12-34-56+0300"
        New-Item -ItemType Directory -Path $nestedDir -Force | Out-Null
        $nestedPath = Join-Path $nestedDir 'nested.md'
        Set-Content -LiteralPath $nestedPath -Value 'nested handoff artifact' -Encoding ascii

        $script:RepoRoot             = $script:TempRepoRoot
        $script:DispatchWorktreeRoot = $script:TempRepoRoot
        try {
            { Invoke-QueueScopeGuard `
                -DispatchId $script:DispatchIdForFixture `
                -DispatchLogPath $script:SyntheticLogPath } |
                Should -Throw -ExpectedMessage '*nested.md*'
        } finally {
            $script:DispatchWorktreeRoot = $null
            Remove-Item -LiteralPath $nestedDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects an out-of-scope path even with the .gitattributes accept fix in place' {
        # Defense check: the leading-dot accept must not broaden the guard
        # into accepting arbitrary surface. Drop a clearly out-of-scope
        # file into the temp repo and confirm the guard fails closed.
        $strayPath = Join-Path $script:TempRepoRoot 'stray-out-of-scope.txt'
        Set-Content -LiteralPath $strayPath -Value 'stray' -Encoding ascii

        $script:RepoRoot             = $script:TempRepoRoot
        $script:DispatchWorktreeRoot = $script:TempRepoRoot
        try {
            { Invoke-QueueScopeGuard `
                -DispatchId $script:DispatchIdForFixture `
                -DispatchLogPath $script:SyntheticLogPath } |
                Should -Throw -ExpectedMessage '*stray-out-of-scope.txt*'
        } finally {
            $script:DispatchWorktreeRoot = $null
            Remove-Item -LiteralPath $strayPath -Force -ErrorAction SilentlyContinue
        }
    }
}

Describe 'ISSUE-237 post-dispatch-log failure-visibility wrap' {

    Context 'Fail helper mirrors to stdout' {
        It 'emits the Fail message to Write-Output as well as stderr' {
            # Re-read the script text so the assertion holds against the
            # checked-in source, not against the dot-sourced runtime
            # function (which is what subprocesses actually execute).
            $script:QueueScriptText | Should -Match 'function Fail \{[\s\S]*?Write-Output\s+\$Message[\s\S]*?exit 1'
        }
    }

    Context 'Queue wraps the post-log / pre-publish-decision window' {
        It 'opens a try block after the dispatch-log Write-Output line' {
            # The wrap starts at the scope-guard timing trace; the catch
            # ends just before Send-DispatchProgressComment fires the
            # publish-decision progress comment. Both anchors must be
            # present in the source.
            $script:QueueScriptText | Should -Match '(?ms)try \{\s*\r?\n\s*Write-TimingTrace\s+"queue\.guard: scope-check start"'
        }

        It 'catches and re-throws after Write-Output mirrors the exception message' {
            $script:QueueScriptText | Should -Match '(?ms)\}\s*catch\s*\{[\s\S]*?Write-Output[\s\S]*?queue step failed between dispatch-log write and[\s\S]*?publish-decision progress comment[\s\S]*?throw\s*\r?\n\s*\}'
        }

        It 'places the catch BEFORE the publish-decision Send-DispatchProgressComment call' {
            $catchIdx = $script:QueueScriptText.IndexOf('queue step failed between dispatch-log write and')
            $catchIdx | Should -BeGreaterThan -1
            # Find the publish-decision Send call by anchoring on the
            # exact Stage argument so the result-comment Send (Stage
            # 'loop-finished' / etc.) does not match.
            $sendIdx = $script:QueueScriptText.IndexOf("-Stage 'publish-decision' ``", $catchIdx)
            $sendIdx | Should -BeGreaterThan -1
            $catchIdx | Should -BeLessThan $sendIdx
        }
    }
}
