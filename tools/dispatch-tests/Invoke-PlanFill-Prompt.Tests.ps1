#Requires -Version 5.1
<#
.SYNOPSIS
    Pester regression coverage for the ISSUE-226 Invoke-PlanFill planner-prompt
    header/footer contract and the new-handoff.ps1 -Finalize -DryRun rejection
    of TASK packets missing top-level header fields.

.DESCRIPTION
    Two independent regressions, both confined to static text inspection and a
    synthetic temp packet under [System.IO.Path]::GetTempPath():

      1. Static inspection of the Invoke-PlanFill region of
         Invoke-AiDispatchLoop.ps1 asserts the production planner prompt names
         every required TASK header field (DISPATCH_ID, AUTHOR, TIMESTAMP,
         RELATED_FILES, STATUS) and every required machine-readable footer
         field (HANDOFF_STATUS, DISPATCH_ID, AUTHOR, NEXT_ROLE, EXIT_CODE),
         tells Codex to preserve the contract even when
         ai_handoffs/templates/TASK_PACKET.md cannot be read, and keeps the
         pre-existing backtick-token rule for `### MAY edit` and
         `### MAY add new files` so the queue scope guard parser remains
         satisfied.

      2. Synthetic-packet regression: writes a TASK packet that omits
         TIMESTAMP and STATUS, runs `new-handoff.ps1 -Finalize -DryRun`
         out-of-process, and asserts the script exits non-zero with stderr
         that names both missing fields.

    The tests never invoke codex, claude, gh, the queue runner, the
    scheduler, the network, or a real dispatch loop. They never edit
    new-handoff.ps1 or ai_handoffs/templates/TASK_PACKET.md.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:LoopScriptPath  = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchLoop.ps1'
    $script:NewHandoffPath  = Join-Path $script:RepoRootForTest 'new-handoff.ps1'
    $script:TaskTemplatePath = Join-Path $script:RepoRootForTest 'ai_handoffs\templates\TASK_PACKET.md'

    if (-not (Test-Path -LiteralPath $script:LoopScriptPath)) {
        throw "Invoke-AiDispatchLoop.ps1 not found at $script:LoopScriptPath"
    }
    if (-not (Test-Path -LiteralPath $script:NewHandoffPath)) {
        throw "new-handoff.ps1 not found at $script:NewHandoffPath"
    }
    if (-not (Test-Path -LiteralPath $script:TaskTemplatePath)) {
        throw "TASK_PACKET.md not found at $script:TaskTemplatePath"
    }

    $script:LoopScriptText = [System.IO.File]::ReadAllText($script:LoopScriptPath)
    $script:TaskTemplateText = [System.IO.File]::ReadAllText($script:TaskTemplatePath)

    # Bound the inspection to the Invoke-PlanFill function. Slicing keeps the
    # assertions from being satisfied by some unrelated string elsewhere in
    # the script (for example, the documentation strings that already
    # appeared in other prompt helpers).
    $startIdx = $script:LoopScriptText.IndexOf('function Invoke-PlanFill')
    if ($startIdx -lt 0) { throw 'Invoke-PlanFill function not found in loop script.' }
    $nextIdx = $script:LoopScriptText.IndexOf("`nfunction ", $startIdx + 1)
    if ($nextIdx -lt 0) { $nextIdx = $script:LoopScriptText.Length }
    $script:PlanFillRegion = $script:LoopScriptText.Substring($startIdx, $nextIdx - $startIdx)
}

Describe 'Invoke-PlanFill planner prompt (ISSUE-226 header/footer contract)' {

    It 'names every required TASK header field in the planner prompt' {
        foreach ($field in @('DISPATCH_ID','AUTHOR','TIMESTAMP','RELATED_FILES','STATUS')) {
            $script:PlanFillRegion | Should -Match ([regex]::Escape($field))
        }
        # Single-string assertion locks in the canonical order so a future
        # edit that drops one of the five fields fails loudly.
        $script:PlanFillRegion | Should -Match 'DISPATCH_ID, AUTHOR, TIMESTAMP, RELATED_FILES, STATUS'
    }

    It 'names every required machine-readable footer field in the planner prompt' {
        foreach ($field in @('HANDOFF_STATUS','DISPATCH_ID','AUTHOR','NEXT_ROLE','EXIT_CODE')) {
            $script:PlanFillRegion | Should -Match ([regex]::Escape($field))
        }
        $script:PlanFillRegion | Should -Match 'HANDOFF_STATUS, DISPATCH_ID, AUTHOR, NEXT_ROLE, EXIT_CODE'
    }

    It 'tells the planner to preserve header and footer even without the template file' {
        $script:PlanFillRegion | Should -Match 'preserve every top-level header field'
        $script:PlanFillRegion | Should -Match 'machine-readable completion footer'
        $script:PlanFillRegion | Should -Match 'ai_handoffs/templates/TASK_PACKET\.md'
    }

    It 'preserves the backtick-token rule for ### MAY edit and ### MAY add new files' {
        # In a PowerShell double-quoted here-string a doubled backtick (``)
        # collapses to a single literal backtick in the rendered prompt; the
        # planner-prompt source therefore contains ``### MAY edit`` /
        # ``### MAY add new files`` rather than `### MAY edit` /
        # `### MAY add new files`.
        $script:PlanFillRegion | Should -Match '``### MAY edit``'
        $script:PlanFillRegion | Should -Match '``### MAY add new files``'
        $script:PlanFillRegion | Should -Match 'wrapped in Markdown backticks'
        $script:PlanFillRegion | Should -Match 'queue\s+scope guard'
    }

    It 'instructs the planner how to fill the ADR-121 advisory envelope' {
        $script:PlanFillRegion | Should -Match '<!-- handoff:envelope v1 -->'
        $script:PlanFillRegion | Should -Match 'MAY_EDIT'
        $script:PlanFillRegion | Should -Match 'MUST_NOT_EDIT'
        $script:PlanFillRegion | Should -Match 'INCIDENTAL_OK'
        $script:PlanFillRegion | Should -Match 'raw\s+repo-relative\s+paths'
        $script:PlanFillRegion | Should -Match 'without\s+Markdown\s+backticks'
        $script:PlanFillRegion | Should -Match 'brace\s+expansion'
        $script:PlanFillRegion | Should -Match 'UNCHECKED'
    }
}

Describe 'TASK_PACKET template ADR-121 advisory envelope' {

    It 'carries a marker-delimited optional handoff envelope block' {
        $script:TaskTemplateText | Should -Match '<!-- handoff:envelope v1 -->'
        $script:TaskTemplateText | Should -Match '<!-- /handoff:envelope -->'
        $script:TaskTemplateText | Should -Match 'MAY_EDIT:'
        $script:TaskTemplateText | Should -Match 'MUST_NOT_EDIT:'
        $script:TaskTemplateText | Should -Match 'INCIDENTAL_OK: false'
    }

    It 'documents that the envelope is advisory and may remain unchecked' {
        $script:TaskTemplateText | Should -Match 'Optional ADR-121 helper block'
        $script:TaskTemplateText | Should -Match 'without Markdown backticks'
        $script:TaskTemplateText | Should -Match 'without brace expansion'
        $script:TaskTemplateText | Should -Match 'UNCHECKED'
    }
}

Describe 'new-handoff.ps1 -Finalize -DryRun (ISSUE-226 missing-header regression)' {

    BeforeAll {
        $script:TempPacketRoot = Join-Path ([System.IO.Path]::GetTempPath()) `
            ("rge-issue-226-finalize-" + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $script:TempPacketRoot -Force | Out-Null

        $script:SyntheticDispatchId = 'ISSUE-PESTER-FINALIZE'
        $packetName = $script:SyntheticDispatchId +
                      '_TASK_2026-01-01_00-00-00+0000.md'
        $script:SyntheticPacketPath = Join-Path $script:TempPacketRoot $packetName

        # Deliberately omit TIMESTAMP and STATUS from the top-level header so
        # the finalizer's "missing field" branch is exercised. The footer
        # carries DISPATCH_ID and AUTHOR so Get-PacketField's first-match
        # line-anchored regex still returns those values, isolating the
        # failure to the two header fields we want to assert on.
        $packetBody = @'
# Task Packet

DISPATCH_ID: ISSUE-PESTER-FINALIZE
AUTHOR: Planner / Pester Synthetic
RELATED_FILES:
- tools/dispatch-tests/Invoke-PlanFill-Prompt.Tests.ps1

## Goal

Synthetic pester packet that intentionally omits TIMESTAMP and STATUS in the
top-level header so the finalizer rejects it.

## Scope

### MAY edit
- `tools/dispatch-tests/Invoke-PlanFill-Prompt.Tests.ps1`

### MUST NOT edit
- everything else

### MAY add new files
- none

## Deliverables

- prove the finalizer rejects this packet

## Acceptance Criteria

- new-handoff.ps1 -Finalize -DryRun exits non-zero
- stderr names the missing fields

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: ISSUE-PESTER-FINALIZE
AUTHOR: Planner / Pester Synthetic
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
'@
        [System.IO.File]::WriteAllText(
            $script:SyntheticPacketPath,
            $packetBody,
            [System.Text.UTF8Encoding]::new($false))

        # Run new-handoff.ps1 out-of-process so we capture the real stderr
        # the Fail helper writes via [Console]::Error.WriteLine and the
        # real `exit 1` from the script. Dot-sourcing inside this session
        # would intercept the exit and corrupt the Pester runner.
        $exe = (Get-Command powershell.exe).Source
        $argString = (
            '-NoProfile -ExecutionPolicy Bypass -File "{0}" -Finalize -PacketPath "{1}" -DryRun' `
                -f $script:NewHandoffPath, $script:SyntheticPacketPath
        )

        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName               = $exe
        $psi.Arguments              = $argString
        $psi.UseShellExecute        = $false
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError  = $true
        $psi.CreateNoWindow         = $true
        $psi.WorkingDirectory       = $script:RepoRootForTest

        $proc = [System.Diagnostics.Process]::Start($psi)
        $script:FinalizeStdout = $proc.StandardOutput.ReadToEnd()
        $script:FinalizeStderr = $proc.StandardError.ReadToEnd()
        $proc.WaitForExit()
        $script:FinalizeExit   = $proc.ExitCode
    }

    AfterAll {
        if ($script:TempPacketRoot -and (Test-Path -LiteralPath $script:TempPacketRoot)) {
            Remove-Item -LiteralPath $script:TempPacketRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'exits non-zero when top-level header fields are missing' {
        $script:FinalizeExit | Should -Not -Be 0
    }

    It 'names every missing top-level TASK header field in the failure text' {
        $combined = ($script:FinalizeStderr + "`n" + $script:FinalizeStdout)
        $combined | Should -Match 'Refusing to finalize'
        $combined | Should -Match 'missing field:\s*timestamp'
        $combined | Should -Match 'missing field:\s*status'
    }

    It 'does not write a .meta.json sidecar in DryRun mode' {
        $sidecar = $script:SyntheticPacketPath -replace '\.md$', '.meta.json'
        Test-Path -LiteralPath $sidecar | Should -BeFalse
    }
}
