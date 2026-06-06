#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0' }

BeforeAll {
    $script:TestsRoot = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:ValidatorPath = Join-Path $script:RepoRootForTest 'Test-HandoffPacket.ps1'
    if (-not (Test-Path -LiteralPath $script:ValidatorPath)) {
        throw "Test-HandoffPacket.ps1 not found at $script:ValidatorPath"
    }

    $env:RGE_HANDOFF_VALIDATOR_SKIP_MAIN = '1'
    try {
        . $script:ValidatorPath
    } finally {
        Remove-Item Env:RGE_HANDOFF_VALIDATOR_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    function Write-Utf8NoBomFile {
        param(
            [Parameter(Mandatory)][string]$Path,
            [Parameter(Mandatory)][AllowEmptyString()][string]$Text
        )
        [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
    }

    function Get-CommitTouchedFiles {
        param([Parameter(Mandatory)][string]$Commit)
        $files = & git -C $script:RepoRootForTest diff-tree --no-commit-id --name-only -r $Commit
        if ($LASTEXITCODE -ne 0) { throw "git diff-tree failed for $Commit" }
        return @($files | Where-Object { $_ })
    }

    function New-HandoffEnvelopeText {
        param(
            [Parameter(Mandatory)][string[]]$MayEdit,
            [string[]]$MustNotEdit = @(),
            [bool]$IncidentalOk = $false
        )
        $lines = @('<!-- handoff:envelope v1 -->', 'MAY_EDIT:')
        foreach ($item in $MayEdit) { $lines += "  - $item" }
        $lines += 'MUST_NOT_EDIT:'
        foreach ($item in $MustNotEdit) { $lines += "  - $item" }
        $lines += "INCIDENTAL_OK: $($IncidentalOk.ToString().ToLowerInvariant())"
        $lines += '<!-- /handoff:envelope -->'
        return ($lines -join "`n")
    }

    function Write-HistoricalTaskPacket {
        param(
            [Parameter(Mandatory)][string]$Directory,
            [Parameter(Mandatory)][string]$DispatchId,
            [Parameter(Mandatory)][AllowEmptyString()][string]$Envelope,
            [string]$Extra = ''
        )
        $text = @"
# TASK PACKET: $DispatchId

DISPATCH_ID: $DispatchId
AUTHOR: Planner / Codex
TIMESTAMP: 2026-06-06T13:00:00+03:00
STATUS: OPEN
RELATED_FILES:
- Test-HandoffPacket.ps1

## Objective

Historical smoke fixture for ADR-121 scope validation.

$Envelope

$Extra

---
HANDOFF_STATUS: COMPLETE
DISPATCH_ID: $DispatchId
AUTHOR: Planner / Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0
---
"@
        $path = Join-Path $Directory "$($DispatchId)_TASK_2026-06-06_13-00-00+0300.md"
        Write-Utf8NoBomFile -Path $path -Text $text
        return $path
    }

    $script:HistoricalSamples = @(
        @{
            Name = 'ADR-121 validator slice'
            Commit = '5730f5c'
            MayEdit = @(
                'Test-HandoffPacket.ps1',
                'tools/dispatch-tests/**',
                'docs/adr/**',
                'Status.md',
                'HANDOFF.md',
                'change.md'
            )
        },
        @{
            Name = 'guard flight-ready merge'
            Commit = 'a0258db'
            MayEdit = @(
                'AUTONOMOUS_WATCH.md',
                'Invoke-AiDispatchGuard.ps1',
                'tools/dispatch-tests/**'
            )
        },
        @{
            Name = 'Codex executor dry-run plumbing'
            Commit = '1bdee7e'
            MayEdit = @(
                'AI_DISPATCH_AUTOMATION.md',
                'AUTONOMOUS_WATCH.md',
                'Invoke-AiDispatch*.ps1',
                'Register-AiDispatchSchedule.ps1',
                'tools/dispatch-tests/**'
            )
        },
        @{
            Name = 'guard and ADR state reconciliation'
            Commit = 'dbd2bda'
            MayEdit = @(
                'Status.md',
                'HANDOFF.md',
                'change.md'
            )
        },
        @{
            Name = 'ADR-121 proposal'
            Commit = '36365e5'
            MayEdit = @(
                'docs/adr/**'
            )
        }
    )
}

Describe 'ADR-121 historical scope smoke' {
    It 'passes hand-authored envelopes for <Name> (<Commit>)' -ForEach $script:HistoricalSamples {
        param(
            [string]$Name,
            [string]$Commit,
            [string[]]$MayEdit
        )

        $envelope = New-HandoffEnvelopeText -MayEdit $MayEdit -MustNotEdit @('crates/**')
        $task = Write-HistoricalTaskPacket -Directory $TestDrive -DispatchId "SMOKE-$Commit" -Envelope $envelope
        $touched = Get-CommitTouchedFiles -Commit $Commit

        $result = Test-HandoffScope -TaskPath $task -TouchedFiles $touched

        $result.verdict | Should -Be 'PASS'
        $result.violations | Should -BeNullOrEmpty
    }

    It 'fails a known historical envelope when an out-of-envelope file is injected' {
        $sample = $script:HistoricalSamples[0]
        $envelope = New-HandoffEnvelopeText -MayEdit $sample.MayEdit -MustNotEdit @('crates/**')
        $task = Write-HistoricalTaskPacket -Directory $TestDrive -DispatchId 'SMOKE-INJECTED-FAIL' -Envelope $envelope
        $touched = @(Get-CommitTouchedFiles -Commit $sample.Commit) + @('crates/cad-core/src/lib.rs')

        $result = Test-HandoffScope -TaskPath $task -TouchedFiles $touched

        $result.verdict | Should -Be 'FAIL'
        $result.violations | Should -Contain 'crates/cad-core/src/lib.rs'
    }

    It 'downgrades the same injected smoke violation only when Planner authored the override' {
        $sample = $script:HistoricalSamples[0]
        $envelope = New-HandoffEnvelopeText -MayEdit $sample.MayEdit -MustNotEdit @('crates/**')
        $task = Write-HistoricalTaskPacket -Directory $TestDrive -DispatchId 'SMOKE-PLANNER-OVERRIDE' `
            -Envelope $envelope -Extra 'SCOPE_OVERRIDE: crates/cad-core/src/lib.rs for smoke-only injected violation'
        $touched = @(Get-CommitTouchedFiles -Commit $sample.Commit) + @('crates/cad-core/src/lib.rs')

        $result = Test-HandoffScope -TaskPath $task -TouchedFiles $touched

        $result.verdict | Should -Be 'WARN'
        $result.overridden | Should -BeTrue
    }

    It 'keeps legacy historical packets UNCHECKED instead of failing closed' {
        $task = Write-HistoricalTaskPacket -Directory $TestDrive -DispatchId 'SMOKE-LEGACY-UNCHECKED' -Envelope ''
        $touched = Get-CommitTouchedFiles -Commit '36365e5'

        $result = Test-HandoffScope -TaskPath $task -TouchedFiles $touched

        $result.verdict | Should -Be 'UNCHECKED'
        $result.unchecked | Should -BeTrue
    }
}
