#Requires -Version 5.1
<#
.SYNOPSIS
    Regression coverage for the oversized-brief liveness guard in
    Invoke-AiDispatchAuto.ps1.

.DESCRIPTION
    Bug being pinned (bug-sweep finding E): the task-selection prompt embeds the
    WHOLE brief, and `codex exec` hard-fails (exit 1) past its ~1 MiB input
    ceiling. The old code let that reach the bare-`exit 1` Fail with NO halt
    sentinel and NO needs-human issue, so a brief that grew past the limit bricked
    EVERY tick silently (a realized multi-hour stall; the brief only grows via
    self-re-arm). The fix is a pre-send byte guard (Test-SelectionPromptOversize)
    that halts gracefully (sentinel + needs-human + exit 0), mirroring the
    seatbelt pause.

    Dot-sources Auto through its RGE_AI_DISPATCH_AUTO_SKIP_MAIN seam to load the
    pure helper without running the tick.
#>

BeforeAll {
    $script:TestsRoot  = Split-Path -Parent $PSCommandPath
    $script:RepoRoot   = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:AutoScript = Join-Path $script:RepoRoot 'Invoke-AiDispatchAuto.ps1'
    $env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN = '1'
    try { . $script:AutoScript }
    finally { Remove-Item Env:RGE_AI_DISPATCH_AUTO_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Test-SelectionPromptOversize (codex input-ceiling guard)' {
    It 'is false for a normal-sized prompt' {
        Test-SelectionPromptOversize -Prompt ('x' * 1000) | Should -BeFalse
    }

    It 'is false just under the ceiling and true at/over it' {
        Test-SelectionPromptOversize -Prompt ('x' * 999999)  -CeilingBytes 1000000 | Should -BeFalse
        Test-SelectionPromptOversize -Prompt ('x' * 1000000) -CeilingBytes 1000000 | Should -BeTrue
        Test-SelectionPromptOversize -Prompt ('x' * 1000001) -CeilingBytes 1000000 | Should -BeTrue
    }

    It 'counts UTF-8 BYTES, not chars (multi-byte aware)' {
        # 'e-acute' is 2 UTF-8 bytes, so 500k chars = 1,000,000 bytes -> oversize
        # even though the CHARACTER count is only half the ceiling.
        $s = ([string][char]0x00E9) * 500000
        Test-SelectionPromptOversize -Prompt $s -CeilingBytes 1000000 | Should -BeTrue
    }

    It 'defaults to a ceiling that pauses with margin under the 1,048,576-byte (1 MiB) hard limit' {
        Test-SelectionPromptOversize -Prompt ('x' * 1048576) | Should -BeTrue   # at the 1 MiB hard limit -> oversize
        Test-SelectionPromptOversize -Prompt ('x' * 900000)  | Should -BeFalse  # comfortably under
    }

    It 'tolerates an empty prompt' {
        Test-SelectionPromptOversize -Prompt '' | Should -BeFalse
    }
}

Describe 'Auto oversized-brief guard wiring (source contract)' {
    BeforeAll { $script:AutoSrc = Get-Content -LiteralPath $script:AutoScript -Raw }

    It 'guards the selection prompt with the helper before the codex call' {
        $script:AutoSrc | Should -Match 'Test-SelectionPromptOversize -Prompt \$selectPrompt'
    }

    It 'the oversize path halts gracefully (sentinel + needs-human + exit 0), not the bare Fail' {
        $script:AutoSrc | Should -Match 'CLASS: brief-oversize'
        $script:AutoSrc | Should -Match 'halted=brief-oversize'
    }
}
