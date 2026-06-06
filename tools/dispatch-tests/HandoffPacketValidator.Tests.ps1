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

    function New-TaskPacketText {
        param(
            [string]$DispatchId = 'ISSUE-PESTER-121',
            [string]$Envelope = '',
            [string]$Extra = ''
        )
        return @"
# TASK PACKET: $DispatchId

DISPATCH_ID: $DispatchId
AUTHOR: Planner / Codex
TIMESTAMP: 2026-06-06T12:00:00+03:00
STATUS: OPEN
RELATED_FILES:
- Test-HandoffPacket.ps1

## Objective

Validate the advisory handoff packet tooling.

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
    }

    function Write-Packet {
        param(
            [Parameter(Mandatory)][string]$Directory,
            [Parameter(Mandatory)][string]$Name,
            [Parameter(Mandatory)][string]$Text
        )
        if (-not (Test-Path -LiteralPath $Directory)) {
            New-Item -ItemType Directory -Path $Directory -Force | Out-Null
        }
        $path = Join-Path $Directory $Name
        Write-Utf8NoBomFile -Path $path -Text $Text
        return $path
    }
}

Describe 'Test-HandoffPacketFile' {
    It 'passes a canonical task packet with required header, related files, and EOF footer' {
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText)

        $result = Test-HandoffPacketFile -Path $packet

        $result.verdict | Should -Be 'PASS'
        $result.errors | Should -BeNullOrEmpty
        $result.packet_type | Should -Be 'TASK'
    }

    It 'fails when prose appears after the footer' {
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text ((New-TaskPacketText) + "`nnot part of the footer`n")

        $result = Test-HandoffPacketFile -Path $packet

        $result.verdict | Should -Be 'FAIL'
        ($result.errors -join "`n") | Should -Match 'footer missing final horizontal rule at EOF'
    }

    It 'fails when the sidecar disagrees with packet fields' {
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText)
        $sidecar = @{
            dispatch_id    = 'OTHER-DISPATCH'
            packet_type    = 'TASK'
            author         = 'Planner / Codex'
            timestamp      = '2026-06-06T12:00:00+03:00'
            status         = 'OPEN'
            handoff_status = 'COMPLETE'
            next_role      = 'EXECUTOR_AI'
            exit_code      = '0'
        } | ConvertTo-Json
        Write-Utf8NoBomFile -Path ($packet -replace '\.md$', '.meta.json') -Text $sidecar

        $result = Test-HandoffPacketFile -Path $packet

        $result.verdict | Should -Be 'FAIL'
        ($result.errors -join "`n") | Should -Match 'sidecar dispatch_id mismatch'
    }

    It 'passes a closed closeout with required evidence sections and a commit hash' {
        $dispatchId = 'ISSUE-PESTER-121'
        $closeout = @"
# FINAL CLOSEOUT: $dispatchId

DISPATCH_ID: $dispatchId
AUTHOR: Human Arbiter / Codex
TIMESTAMP: 2026-06-06T12:30:00+03:00
STATUS: CLOSED
RELATED_FILES:
- Test-HandoffPacket.ps1

## Final Commit(s)

- a0258db feat(dispatch): add advisory validator

## Verification Gates

- PASS: focused validator tests

## Test Count Delta

- +12 focused tests

## Remaining Risks Carried Forward

- Advisory-only; not wired into live verification.

## Suggested Follow-On Tasks

- Smoke against representative historical packets.

---
HANDOFF_STATUS: COMPLETE
DISPATCH_ID: $dispatchId
AUTHOR: Human Arbiter / Codex
NEXT_ROLE: NONE
EXIT_CODE: 0
---
"@
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_CLOSEOUT_2026-06-06_12-30-00+0300.md' `
            -Text $closeout

        $result = Test-HandoffPacketFile -Path $packet

        $result.verdict | Should -Be 'PASS'
        $result.errors | Should -BeNullOrEmpty
    }

    It 'fails a closeout that omits remaining risks' {
        $dispatchId = 'ISSUE-PESTER-121'
        $closeout = @"
# FINAL CLOSEOUT: $dispatchId

DISPATCH_ID: $dispatchId
AUTHOR: Human Arbiter / Codex
TIMESTAMP: 2026-06-06T12:30:00+03:00
STATUS: CLOSED
RELATED_FILES:
- Test-HandoffPacket.ps1

## Final Commit(s)

- a0258db feat(dispatch): add advisory validator

## Verification Gates

- PASS: focused validator tests

## Test Count Delta

- +12 focused tests

## Suggested Follow-On Tasks

- Smoke against representative historical packets.

---
HANDOFF_STATUS: COMPLETE
DISPATCH_ID: $dispatchId
AUTHOR: Human Arbiter / Codex
NEXT_ROLE: NONE
EXIT_CODE: 0
---
"@
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_CLOSEOUT_2026-06-06_12-30-00+0300.md' `
            -Text $closeout

        $result = Test-HandoffPacketFile -Path $packet

        $result.verdict | Should -Be 'FAIL'
        ($result.errors -join "`n") | Should -Match 'Remaining Risks'
    }
}

Describe 'Test-HandoffScope' {
    It 'returns UNCHECKED for legacy task packets without an envelope' {
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText)

        $result = Test-HandoffScope -TaskPath $packet -TouchedFiles @('src/lib.rs')

        $result.verdict | Should -Be 'UNCHECKED'
        $result.unchecked | Should -BeTrue
    }

    It 'passes allowed paths plus protocol and incidental exemptions' {
        $envelope = @'
<!-- handoff:envelope v1 -->
MAY_EDIT:
  - src/**
MUST_NOT_EDIT:
  - src/generated/**
INCIDENTAL_OK: true
<!-- /handoff:envelope -->
'@
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText -Envelope $envelope)

        $result = Test-HandoffScope -TaskPath $packet -TouchedFiles @(
            'src/lib.rs',
            'ai_handoffs/ISSUE-PESTER-121_EXEC_2026-06-06_12-10-00+0300.md',
            'Cargo.lock',
            'packet.meta.json',
            'nested/packet.meta.json'
        )

        $result.verdict | Should -Be 'PASS'
        $result.violations | Should -BeNullOrEmpty
    }

    It 'fails out-of-envelope files and MUST_NOT paths' {
        $envelope = @'
<!-- handoff:envelope v1 -->
MAY_EDIT:
  - src/**
MUST_NOT_EDIT:
  - src/generated/**
INCIDENTAL_OK: true
<!-- /handoff:envelope -->
'@
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText -Envelope $envelope)

        $result = Test-HandoffScope -TaskPath $packet -TouchedFiles @(
            'src/generated/out.rs',
            'docs/out-of-scope.md'
        )

        $result.verdict | Should -Be 'FAIL'
        $result.violations | Should -Contain 'src/generated/out.rs'
        $result.violations | Should -Contain 'docs/out-of-scope.md'
    }

    It 'downgrades violations to WARN only for Planner-owned overrides' {
        $envelope = @'
<!-- handoff:envelope v1 -->
MAY_EDIT:
  - src/**
MUST_NOT_EDIT:
INCIDENTAL_OK: false
<!-- /handoff:envelope -->
'@
        $task = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText -Envelope $envelope -Extra 'SCOPE_OVERRIDE: docs/out-of-scope.md because docs were Planner-approved')

        $result = Test-HandoffScope -TaskPath $task -TouchedFiles @('docs/out-of-scope.md')

        $result.verdict | Should -Be 'WARN'
        $result.overridden | Should -BeTrue
    }

    It 'does not accept Executor-authored scope overrides' {
        $envelope = @'
<!-- handoff:envelope v1 -->
MAY_EDIT:
  - src/**
MUST_NOT_EDIT:
INCIDENTAL_OK: false
<!-- /handoff:envelope -->
'@
        $task = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText -Envelope $envelope)
        $exec = @'
DISPATCH_ID: ISSUE-PESTER-121
AUTHOR: Executor / Claude
TIMESTAMP: 2026-06-06T12:10:00+03:00
STATUS: AWAITING_REVIEW
SCOPE_OVERRIDE: docs/out-of-scope.md
'@
        $execPath = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_EXEC_2026-06-06_12-10-00+0300.md' `
            -Text $exec

        $result = Test-HandoffScope -TaskPath $task -TouchedFiles @('docs/out-of-scope.md') -OverridePacket @($execPath)

        $result.verdict | Should -Be 'FAIL'
        $result.overridden | Should -BeFalse
    }

    It 'supports denylist mode when MAY_EDIT is empty and MUST_NOT_EDIT is present' {
        $envelope = @'
<!-- handoff:envelope v1 -->
MAY_EDIT:
MUST_NOT_EDIT:
  - secrets/**
INCIDENTAL_OK: false
<!-- /handoff:envelope -->
'@
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText -Envelope $envelope)

        $result = Test-HandoffScope -TaskPath $packet -TouchedFiles @('src/lib.rs', 'secrets/key.txt')

        $result.verdict | Should -Be 'FAIL'
        $result.violations | Should -Contain 'secrets/key.txt'
        $result.violations | Should -Not -Contain 'src/lib.rs'
    }

    It 'fails envelopes that use unsupported brace expansion' {
        $envelope = @'
<!-- handoff:envelope v1 -->
MAY_EDIT:
  - crates/{a,b}/**
MUST_NOT_EDIT:
INCIDENTAL_OK: false
<!-- /handoff:envelope -->
'@
        $packet = Write-Packet -Directory $TestDrive -Name 'ISSUE-PESTER-121_TASK_2026-06-06_12-00-00+0300.md' `
            -Text (New-TaskPacketText -Envelope $envelope)

        $result = Test-HandoffScope -TaskPath $packet -TouchedFiles @('crates/a/src/lib.rs')

        $result.verdict | Should -Be 'FAIL'
        ($result.errors -join "`n") | Should -Match 'brace expansion is not supported'
    }
}
