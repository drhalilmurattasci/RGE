#Requires -Version 5.1
<#
.SYNOPSIS
    Run a Codex-plans, Claude-executes, Codex-controls dispatch loop.

.DESCRIPTION
    This is a thin orchestration layer over the canonical ai_handoffs/
    packet protocol. It automates model routing, but it does not commit or
    push. Human authorization remains required for any git publish step.

    Flow:
      1. Scaffold TASK packet.
      2. Ask Codex to fill the TASK packet from the supplied goal.
      3. Ask Claude to review the TASK as an executor gate.
      4. If Claude approves, finalize the TASK sidecar.
      5. Ask Claude to execute and write/finalize an EXECUTION_REPORT.
      6. Ask Codex to perform a read-only control review of the diff,
         packets, and verification claims.

    If Codex control returns needs_changes and MaxCorrectionRounds is greater
    than zero, the script asks Codex to write a CORRECTION_PACKET and routes
    that packet back to Claude for another execution round.

    With -ResumeApprovedTask, steps 1-4 are skipped: the loop locates the
    already-approved, finalized TASK packet for the given DispatchId and runs
    only the execution and control phase (steps 5-6).

.EXAMPLE
    .\Invoke-AiDispatchLoop.ps1 `
      -DispatchId POSTV0-HANDOFF-ARTIFACT-TRIAGE-001 `
      -Goal "Audit untracked handoff artifacts and recommend cleanup. No edits."

.EXAMPLE
    # Resume mode: skip planning, execute an already-approved + finalized TASK
    # packet (one that has a .meta.json sidecar) without scaffolding a new one.
    .\Invoke-AiDispatchLoop.ps1 `
      -DispatchId POSTV0-HANDOFF-ARTIFACT-TRIAGE-004 `
      -ResumeApprovedTask

.NOTES
    Requires local `codex`, `claude`, `git`, `.mcp.json`, `new-handoff.ps1`,
    and the ai_handoffs packet templates.
#>
[CmdletBinding(DefaultParameterSetName = 'GoalText')]
param(
    [Parameter(Mandatory)]
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$DispatchId,

    [Parameter(Mandatory, ParameterSetName = 'GoalText')]
    [string]$Goal,

    [Parameter(Mandatory, ParameterSetName = 'GoalFile')]
    [string]$GoalFile,

    [ValidateRange(0, 5)]
    [int]$MaxPlanRevisions = 1,

    [ValidateRange(0, 5)]
    [int]$MaxCorrectionRounds = 1,

    [ValidateSet('acceptEdits', 'auto', 'bypassPermissions', 'default', 'dontAsk', 'plan')]
    [string]$ClaudePermissionMode = 'acceptEdits',

    [string]$CodexModel = '',

    [string]$ClaudeModel = '',

    [switch]$AllowDirtyTracked,

    [switch]$PlanOnly,

    [Parameter(Mandatory, ParameterSetName = 'ResumeTask')]
    [switch]$ResumeApprovedTask
)

$ErrorActionPreference = 'Stop'

function Fail {
    param([string]$Message)
    [Console]::Error.WriteLine($Message)
    exit 1
}

function Require-Command {
    param([string]$Name)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Fail "Required command not found on PATH: $Name"
    }
}

function Write-TextFile {
    param([string]$Path, [string]$Text)
    $parent = Split-Path -Parent $Path
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Read-JsonFile {
    param([string]$Path)
    try {
        return (Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json)
    } catch {
        Fail "Could not parse JSON at $Path. Error: $($_.Exception.Message)"
    }
}

function Get-RepoRelativePath {
    param([string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    $root = [System.IO.Path]::GetFullPath($script:RepoRoot).TrimEnd('\', '/')
    if ($full.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
        return (($full.Substring($root.Length)).TrimStart('\', '/') -replace '\\', '/')
    }
    return ($full -replace '\\', '/')
}

function Get-LatestPacket {
    param([string]$PacketType)
    $filter = "${DispatchId}_${PacketType}_*.md"
    return Get-ChildItem -LiteralPath $script:HandoffDir -Filter $filter -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc, Name |
        Select-Object -Last 1
}

function Invoke-NewPacket {
    param([string]$PacketType, [string]$Author)
    $global:LASTEXITCODE = 0
    $output = & $script:NewHandoff -DispatchId $DispatchId -PacketType $PacketType -Author $Author
    if ($LASTEXITCODE -ne 0) {
        Fail "new-handoff.ps1 failed while creating $PacketType packet."
    }
    $packetPath = ($output | Select-Object -First 1)
    if (-not $packetPath -or -not (Test-Path -LiteralPath $packetPath)) {
        Fail "Could not determine created $PacketType packet path."
    }
    return (Get-Item -LiteralPath $packetPath)
}

function Test-PacketFinalizeDryRun {
    param([System.IO.FileInfo]$Packet, [string]$LogPath)
    $global:LASTEXITCODE = 0
    & $script:NewHandoff -Finalize -PacketPath $Packet.FullName -DryRun > $LogPath 2>&1
    if ($LASTEXITCODE -ne 0) {
        Fail "Packet did not pass finalize dry-run validation: $(Get-RepoRelativePath $Packet.FullName). See $LogPath"
    }
}

function Finalize-Packet {
    param([System.IO.FileInfo]$Packet)
    $sidecarPath = $Packet.FullName -replace '\.md$', '.meta.json'
    if (Test-Path -LiteralPath $sidecarPath) {
        return (Get-Item -LiteralPath $sidecarPath)
    }
    $global:LASTEXITCODE = 0
    $output = & $script:NewHandoff -Finalize -PacketPath $Packet.FullName
    if ($LASTEXITCODE -ne 0) {
        Fail "Finalizing packet failed: $(Get-RepoRelativePath $Packet.FullName)"
    }
    $created = ($output | Select-Object -First 1)
    if (-not $created -or -not (Test-Path -LiteralPath $created)) {
        Fail "Could not determine sidecar path after finalizing $(Get-RepoRelativePath $Packet.FullName)"
    }
    return (Get-Item -LiteralPath $created)
}

function Invoke-CodexPrompt {
    param(
        [string]$Prompt,
        [ValidateSet('read-only', 'workspace-write', 'danger-full-access')]
        [string]$Sandbox,
        [string]$LogPath,
        [string]$OutputSchema = '',
        [string]$OutputPath = ''
    )

    $promptPath = Join-Path $script:RunDir 'codex.prompt.md'
    Write-TextFile $promptPath $Prompt

    $args = @('exec', '--cd', $script:RepoRoot, '--sandbox', $Sandbox)
    if ($CodexModel) { $args += @('--model', $CodexModel) }
    if ($OutputSchema) {
        $args += @('--output-schema', $OutputSchema, '--output-last-message', $OutputPath)
    }
    $args += '-'

    # PS 5.1 turns a native command's stderr into a terminating error under EAP=Stop; the npm codex shim banners to stderr, so isolate it with Continue.
    $global:LASTEXITCODE = 0
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        Get-Content -Raw -LiteralPath $promptPath | & codex @args > $LogPath 2>&1
    } finally {
        $ErrorActionPreference = $prevEap
    }
    if ($LASTEXITCODE -ne 0) {
        Fail "codex exec failed. See $LogPath"
    }
}

function Get-JsonValueType {
    param($Value)
    if ($null -eq $Value) { return 'null' }
    if ($Value -is [string]) { return 'string' }
    if ($Value -is [bool]) { return 'boolean' }
    if ($Value -is [byte] -or $Value -is [int16] -or $Value -is [int] -or $Value -is [long]) { return 'integer' }
    if ($Value -is [float] -or $Value -is [double] -or $Value -is [decimal]) { return 'number' }
    if ($Value -is [System.Array]) { return 'array' }
    if ($Value -is [pscustomobject] -or $Value -is [hashtable]) { return 'object' }
    return $Value.GetType().Name
}

function Test-JsonSchemaSubset {
    param(
        $Value,
        $Schema,
        [string]$Path = '$'
    )

    $schemaProps = @($Schema.PSObject.Properties.Name)
    if ($schemaProps -contains 'type') {
        $allowedTypes = @($Schema.type)
        $actualType = Get-JsonValueType $Value
        $matchesType = $false
        foreach ($allowedType in $allowedTypes) {
            if ($actualType -eq $allowedType -or ($allowedType -eq 'number' -and $actualType -eq 'integer')) {
                $matchesType = $true
                break
            }
        }
        if (-not $matchesType) {
            Fail "Claude JSON result does not match schema at ${Path}: expected $($allowedTypes -join '/'), got $actualType."
        }
    }

    if (($schemaProps -contains 'enum') -and $null -ne $Value) {
        $enumValues = @($Schema.enum)
        if ($enumValues -notcontains $Value) {
            Fail "Claude JSON result does not match schema at ${Path}: '$Value' is not one of $($enumValues -join ', ')."
        }
    }

    $actualType = Get-JsonValueType $Value
    if ($actualType -eq 'object') {
        $valueProps = @($Value.PSObject.Properties.Name)
        if ($schemaProps -contains 'required') {
            foreach ($requiredName in @($Schema.required)) {
                if ($valueProps -notcontains $requiredName) {
                    # Tolerate a missing required array (Claude omits empty []); a missing required scalar still fails.
                    $missingSchema = $null
                    if ($schemaProps -contains 'properties') { $missingSchema = $Schema.properties.$requiredName }
                    if ($missingSchema -and (@($missingSchema.type) -contains 'array')) { continue }
                    Fail "Claude JSON result does not match schema at ${Path}: missing required property '$requiredName'."
                }
            }
        }
        if (($schemaProps -contains 'additionalProperties') -and $Schema.additionalProperties -eq $false -and ($schemaProps -contains 'properties')) {
            $allowedProps = @($Schema.properties.PSObject.Properties.Name)
            foreach ($valueName in $valueProps) {
                if ($allowedProps -notcontains $valueName) {
                    Fail "Claude JSON result does not match schema at ${Path}: unexpected property '$valueName'."
                }
            }
        }
        if ($schemaProps -contains 'properties') {
            foreach ($propertySchema in @($Schema.properties.PSObject.Properties)) {
                if ($valueProps -contains $propertySchema.Name) {
                    Test-JsonSchemaSubset -Value $Value.($propertySchema.Name) -Schema $propertySchema.Value -Path "${Path}.$($propertySchema.Name)"
                }
            }
        }
    } elseif ($actualType -eq 'array' -and ($schemaProps -contains 'items')) {
        $index = 0
        foreach ($item in @($Value)) {
            Test-JsonSchemaSubset -Value $item -Schema $Schema.items -Path "${Path}[$index]"
            $index++
        }
    }
}

function Convert-ClaudeResultJson {
    param(
        [string]$ResultText,
        [string]$SchemaPath
    )

    $payload = $ResultText.Trim()
    if ($payload -match '(?s)^```(?:json)?\s*(.*?)\s*```$') {
        $payload = $matches[1].Trim()
    }

    try {
        $result = $payload | ConvertFrom-Json
    } catch {
        Fail "Claude result was not parseable JSON. Error: $($_.Exception.Message)"
    }

    $schema = Read-JsonFile $SchemaPath
    Test-JsonSchemaSubset -Value $result -Schema $schema
    return $result
}

function Test-PacketForbidsSidecar {
    param([System.IO.FileInfo]$Packet)

    $text = Get-Content -Raw -LiteralPath $Packet.FullName
    return (
        $text -match '(?is)MUST NOT add new files.*sidecar\s+`?\.meta\.json`?' -or
        $text -match '(?is)Do not create,\s*finalize,\s*repair,\s*delete,\s*or\s*regenerate handoff sidecars' -or
        $text -match '(?is)MUST NOT.*sidecar\s+`?\.meta\.json`?'
    )
}

function Invoke-ClaudeJson {
    param(
        [string]$Prompt,
        [string]$SchemaPath,
        [string]$OutputPath,
        [ValidateSet('acceptEdits', 'auto', 'bypassPermissions', 'default', 'dontAsk', 'plan')]
        [string]$PermissionMode
    )

    $schemaContent = Get-Content -Raw -LiteralPath $SchemaPath
    $envelopePath = $OutputPath -replace '\.json$', '.envelope.json'
    $stderrPath = $OutputPath -replace '\.json$', '.stderr.txt'
    $wrappedPrompt = @"
CRITICAL OUTPUT CONTRACT:
- Your final terminal response must be exactly one JSON object.
- Do not return prose, Markdown, a summary, or a table outside that JSON object.
- If you need to perform repo work first, do the work, then make the final
  response only the JSON object.
- The JSON object must match the schema below.

$Prompt

Return exactly one JSON object matching this schema. Do not wrap it in Markdown.
Do not include explanatory text outside the JSON object.

Schema:
$schemaContent
"@

    $args = @(
        '-p',
        '--mcp-config', $script:McpConfig,
        '--permission-mode', $PermissionMode,
        '--output-format', 'json'
    )
    if ($ClaudeModel) { $args += @('--model', $ClaudeModel) }

    # Same PS 5.1 stderr/EAP hazard as Invoke-CodexPrompt — isolate the npm claude shim.
    $global:LASTEXITCODE = 0
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & claude @args $wrappedPrompt > $envelopePath 2> $stderrPath
    } finally {
        $ErrorActionPreference = $prevEap
    }
    if ($LASTEXITCODE -ne 0) {
        Fail "claude failed. See $stderrPath"
    }
    if (-not (Test-Path -LiteralPath $envelopePath) -or (Get-Item -LiteralPath $envelopePath).Length -eq 0) {
        Fail "claude produced no JSON output. See $stderrPath"
    }

    $envelope = Read-JsonFile $envelopePath
    $props = @($envelope.PSObject.Properties.Name)
    if (($props -contains 'is_error') -and $envelope.is_error) {
        Fail "claude reported an error: $($envelope.result). See $envelopePath"
    }
    if (-not ($props -contains 'result')) {
        Fail "claude did not return a result payload. See $envelopePath"
    }
    $result = Convert-ClaudeResultJson -ResultText $envelope.result -SchemaPath $SchemaPath

    Write-TextFile $OutputPath ($result | ConvertTo-Json -Depth 16)
    return $result
}

function Test-ClaudeCliReady {
    $probeOut = Join-Path $script:RunDir 'claude.ready.envelope.json'
    $probeErr = Join-Path $script:RunDir 'claude.ready.stderr.txt'

    $global:LASTEXITCODE = 0
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & claude -p --output-format json 'Return exactly: ready' > $probeOut 2> $probeErr
    } finally {
        $ErrorActionPreference = $prevEap
    }

    if (-not (Test-Path -LiteralPath $probeOut) -or (Get-Item -LiteralPath $probeOut).Length -eq 0) {
        if ($LASTEXITCODE -ne 0) {
            Fail "claude readiness probe failed. See $probeErr"
        }
        Fail "claude readiness probe produced no JSON output. See $probeErr"
    }

    $probe = Read-JsonFile $probeOut
    $props = @($probe.PSObject.Properties.Name)
    if (($props -contains 'is_error') -and $probe.is_error) {
        Fail "claude is not ready: $($probe.result). Run Claude Code login/auth setup, then retry."
    }
    if ($LASTEXITCODE -ne 0) {
        Fail "claude readiness probe failed. See $probeErr"
    }
}

function Invoke-PlanFill {
    param(
        [System.IO.FileInfo]$TaskPacket,
        [int]$RevisionNumber,
        [string]$PriorClaudeGatePath
    )

    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $gateContext = 'No prior Claude gate.'
    if ($PriorClaudeGatePath -and (Test-Path -LiteralPath $PriorClaudeGatePath)) {
        $gateContext = Get-Content -Raw -LiteralPath $PriorClaudeGatePath
    }

    $prompt = @"
You are Planner / OpenAI Codex in the RGE repository.

Fill or revise this TASK_PACKET only:

$taskRel

User goal:

$script:GoalText

Revision number: $RevisionNumber

Prior Claude gate result, if any:

$gateContext

Rules:
- Edit only the TASK_PACKET above.
- Do not edit source, docs, schemas, scripts, .gitignore, or any other packet.
- Replace every placeholder.
- Make scope precise: MAY edit, MUST NOT edit, deliverables, gates, halt conditions.
- If the task is audit-only, make that explicit and set MAY edit to none.
- Footer must be:
  HANDOFF_STATUS: COMPLETE
  NEXT_ROLE: EXECUTOR_AI
  EXIT_CODE: 0
- The packet must pass new-handoff.ps1 -Finalize -DryRun.
"@

    $log = Join-Path $script:RunDir ("codex.plan.rev{0}.log" -f $RevisionNumber)
    Invoke-CodexPrompt -Prompt $prompt -Sandbox 'workspace-write' -LogPath $log
    Test-PacketFinalizeDryRun -Packet $TaskPacket -LogPath (Join-Path $script:RunDir ("task.finalize-dryrun.rev{0}.log" -f $RevisionNumber))
}

function Invoke-ClaudePlanGate {
    param([System.IO.FileInfo]$TaskPacket, [int]$RevisionNumber)
    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $schema = Join-Path $script:AiDir 'claude_plan_gate.schema.json'
    $out = Join-Path $script:RunDir ("claude.plan_gate.rev{0}.json" -f $RevisionNumber)
    $prompt = @"
You are Claude acting as Executor preflight gate for RGE.

Review the TASK_PACKET:

$taskRel

You must not edit files. Read the packet, inspect only the repo context needed
to decide whether the plan is executable, bounded, and protocol-safe.

Return structured JSON only. Use:
- verdict=approve if the task is safe to execute as written.
- verdict=needs_changes if Codex should revise the TASK packet first.
- verdict=block if execution should not proceed without human arbitration.

Include commands_run with any commands you actually ran.
"@
    return Invoke-ClaudeJson -Prompt $prompt -SchemaPath $schema -OutputPath $out -PermissionMode 'plan'
}

function Invoke-ClaudeExecute {
    param([System.IO.FileInfo]$ActivePacket, [string]$PacketKind, [int]$Round)

    $packetRel = Get-RepoRelativePath $ActivePacket.FullName
    $schema = Join-Path $script:AiDir 'claude_execution_result.schema.json'
    $out = Join-Path $script:RunDir ("claude.execute.round{0}.json" -f $Round)
    $prompt = @"
You are Executor / Claude in the RGE repository.

Read and execute this $PacketKind packet:

$packetRel

Protocol rules:
- Execute only the enumerated scope.
- Do not commit.
- Do not push.
- If a halt condition triggers, stop and write an EXECUTION_REPORT with
  STATUS: BLOCKED or NEEDS_HUMAN as appropriate.
- If execution proceeds, write an EXECUTION_REPORT using:
  .\new-handoff.ps1 -DispatchId $DispatchId -PacketType EXEC -Author "Executor / Claude"
- Fill the EXEC packet completely.
- If the active packet allows sidecar creation, run:
  .\new-handoff.ps1 -Finalize -PacketPath <exec packet path>
- If the active packet forbids sidecar `.meta.json` creation, do not finalize
  the EXEC packet; mention that deliberate skip in the returned JSON notes.
- Return structured JSON only, including exec_packet as the repo-relative
  path to the EXECUTION_REPORT if one was written.
"@
    return Invoke-ClaudeJson -Prompt $prompt -SchemaPath $schema -OutputPath $out -PermissionMode $ClaudePermissionMode
}

function Invoke-CodexControl {
    param([System.IO.FileInfo]$TaskPacket, [System.IO.FileInfo]$ExecPacket, [int]$Round)

    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $execRel = if ($ExecPacket) { Get-RepoRelativePath $ExecPacket.FullName } else { '<none>' }
    $schema = Join-Path $script:AiDir 'codex_control.schema.json'
    $out = Join-Path $script:RunDir ("codex.control.round{0}.json" -f $Round)
    $log = Join-Path $script:RunDir ("codex.control.round{0}.log" -f $Round)

    $prompt = @"
You are Codex Controller / Reviewer for an automated RGE dispatch loop.

Review without editing anything.

Task packet:
$taskRel

Latest execution report:
$execRel

Also inspect:
- git status --short --branch
- git diff
- relevant changed files
- verification claims in the EXECUTION_REPORT
- ai_handoffs/AI_HANDOFF_PROTOCOL.md if protocol interpretation matters

Return schema-compliant JSON only. Use:
- verdict=pass only if the work is ready for human commit authorization.
- verdict=needs_changes if Codex should write a CORRECTION_PACKET and route it
  back to Claude.
- verdict=block if human arbitration is required.

Do not edit files. Do not stage. Do not commit. Do not push.
"@
    Invoke-CodexPrompt -Prompt $prompt -Sandbox 'read-only' -LogPath $log -OutputSchema $schema -OutputPath $out
    return (Read-JsonFile $out)
}

function Invoke-CorrectionPacket {
    param([object]$ControlResult, [int]$Round)
    $packet = Invoke-NewPacket -PacketType 'CORRECT' -Author 'Planner / OpenAI Codex'
    $packetRel = Get-RepoRelativePath $packet.FullName
    $controlJson = ($ControlResult | ConvertTo-Json -Depth 16)
    $prompt = @"
You are Planner / OpenAI Codex in the RGE repository.

Write a CORRECTION_PACKET only. Edit only this file:

$packetRel

Codex control review result:

$controlJson

Rules:
- Enumerate only the fixes approved by the control review.
- Do not expand scope.
- Do not edit any source, docs, schemas, scripts, or other packets.
- Fill every placeholder.
- Footer must be:
  HANDOFF_STATUS: COMPLETE
  NEXT_ROLE: EXECUTOR_AI
  EXIT_CODE: 0
"@
    $log = Join-Path $script:RunDir ("codex.correct.round{0}.log" -f $Round)
    Invoke-CodexPrompt -Prompt $prompt -Sandbox 'workspace-write' -LogPath $log
    Test-PacketFinalizeDryRun -Packet $packet -LogPath (Join-Path $script:RunDir ("correct.finalize-dryrun.round{0}.log" -f $Round))
    Finalize-Packet -Packet $packet | Out-Null
    return $packet
}

Require-Command git
Require-Command codex
Require-Command claude

if ($ResumeApprovedTask -and $PlanOnly) {
    Fail "-PlanOnly cannot be combined with -ResumeApprovedTask; resume mode runs the execution loop on an already-approved TASK."
}

$script:RepoRoot = (& git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0 -or -not $script:RepoRoot) {
    Fail "Not inside a git repository."
}
Set-Location $script:RepoRoot

$script:AiDir = Join-Path $script:RepoRoot '.ai'
$script:HandoffDir = Join-Path $script:RepoRoot 'ai_handoffs'
$script:NewHandoff = Join-Path $script:RepoRoot 'new-handoff.ps1'
$script:McpConfig = Join-Path $script:RepoRoot '.mcp.json'
$script:RunDir = Join-Path $script:AiDir ("dispatch-{0}" -f $DispatchId)

foreach ($path in @(
    $script:NewHandoff,
    $script:McpConfig,
    (Join-Path $script:AiDir 'claude_plan_gate.schema.json'),
    (Join-Path $script:AiDir 'claude_execution_result.schema.json'),
    (Join-Path $script:AiDir 'codex_control.schema.json'),
    (Join-Path $script:HandoffDir 'AI_HANDOFF_PROTOCOL.md')
)) {
    if (-not (Test-Path -LiteralPath $path)) {
        Fail "Required file missing: $path"
    }
}

if ($ResumeApprovedTask) {
    $script:GoalText = ''
} elseif ($GoalFile) {
    if (-not (Test-Path -LiteralPath $GoalFile)) {
        Fail "Goal file not found: $GoalFile"
    }
    $script:GoalText = Get-Content -Raw -LiteralPath $GoalFile
} else {
    $script:GoalText = $Goal
}

New-Item -ItemType Directory -Path $script:RunDir -Force | Out-Null

$aheadBehind = (& git rev-list --left-right --count origin/main...HEAD 2>$null).Trim()
if ($LASTEXITCODE -eq 0 -and $aheadBehind -ne "0`t0") {
    Fail "Branch is not synced with origin/main: $aheadBehind"
}

$statusLines = & git status --porcelain=v1
$trackedDirty = @($statusLines | Where-Object { $_ -notmatch '^\?\? ' })
if ($trackedDirty.Count -gt 0 -and -not $AllowDirtyTracked) {
    Fail "Tracked files are already dirty. Re-run with -AllowDirtyTracked only if this is intentional."
}

Test-ClaudeCliReady

Write-Output "AI dispatch loop: $DispatchId"
Write-Output "Repo: $script:RepoRoot"
Write-Output "Run dir: $(Get-RepoRelativePath $script:RunDir)"

if ($ResumeApprovedTask) {
    $taskPacket = Get-LatestPacket -PacketType 'TASK'
    if (-not $taskPacket) {
        Fail "No TASK packet found for dispatch '$DispatchId' in $(Get-RepoRelativePath $script:HandoffDir)."
    }
    $taskSidecar = $taskPacket.FullName -replace '\.md$', '.meta.json'
    if (-not (Test-Path -LiteralPath $taskSidecar)) {
        Fail "TASK packet has no .meta.json sidecar, so it was never approved and finalized: $(Get-RepoRelativePath $taskPacket.FullName). Run a planning dispatch for this DispatchId first."
    }
    Write-Output "Resuming approved TASK: $(Get-RepoRelativePath $taskPacket.FullName)"
} else {
    $taskPacket = Invoke-NewPacket -PacketType 'TASK' -Author 'Planner / OpenAI Codex'
    Write-Output "TASK scaffolded: $(Get-RepoRelativePath $taskPacket.FullName)"

    $gate = $null
    $gatePath = ''
    $approved = $false
    for ($i = 0; $i -le $MaxPlanRevisions; $i++) {
        Invoke-PlanFill -TaskPacket $taskPacket -RevisionNumber $i -PriorClaudeGatePath $gatePath
        $gate = Invoke-ClaudePlanGate -TaskPacket $taskPacket -RevisionNumber $i
        $gatePath = Join-Path $script:RunDir ("claude.plan_gate.rev{0}.json" -f $i)
        Write-Output "Claude plan gate rev ${i}: $($gate.verdict)"
        if ($gate.verdict -eq 'approve') {
            $approved = $true
            break
        }
        if ($gate.verdict -eq 'block') {
            Fail "Claude blocked the plan. See $(Get-RepoRelativePath $gatePath)"
        }
    }

    if (-not $approved) {
        Fail "Claude did not approve the plan within MaxPlanRevisions=$MaxPlanRevisions. See $(Get-RepoRelativePath $gatePath)"
    }

    Finalize-Packet -Packet $taskPacket | Out-Null
    Write-Output "TASK finalized."

    if ($PlanOnly) {
        Write-Output "PlanOnly requested. Stopping after approved TASK."
        exit 0
    }
}

$activePacket = $taskPacket
$activeKind = 'TASK'
$lastExecPacket = $null
$finalControl = $null

for ($round = 0; $round -le $MaxCorrectionRounds; $round++) {
    $execResult = Invoke-ClaudeExecute -ActivePacket $activePacket -PacketKind $activeKind -Round $round
    Write-Output "Claude execution round ${round}: $($execResult.status)"

    $lastExecPacket = $null
    if ($execResult.exec_packet) {
        $candidate = Join-Path $script:RepoRoot (($execResult.exec_packet -replace '/', '\'))
        if (Test-Path -LiteralPath $candidate) {
            $lastExecPacket = Get-Item -LiteralPath $candidate
        }
    }
    if (-not $lastExecPacket) {
        $lastExecPacket = Get-LatestPacket -PacketType 'EXEC'
    }
    if ($lastExecPacket) {
        $sidecar = $lastExecPacket.FullName -replace '\.md$', '.meta.json'
        if (Test-PacketForbidsSidecar -Packet $activePacket) {
            Write-Output "EXEC sidecar finalization skipped; active packet forbids sidecar creation."
        } elseif (-not (Test-Path -LiteralPath $sidecar)) {
            Finalize-Packet -Packet $lastExecPacket | Out-Null
        }
    }

    $finalControl = Invoke-CodexControl -TaskPacket $taskPacket -ExecPacket $lastExecPacket -Round $round
    Write-Output "Codex control round ${round}: $($finalControl.verdict)"

    if ($finalControl.verdict -eq 'pass') {
        break
    }
    if ($finalControl.verdict -eq 'block') {
        Fail "Codex control blocked the dispatch. See $(Get-RepoRelativePath (Join-Path $script:RunDir ("codex.control.round{0}.json" -f $round)))"
    }
    if ($round -ge $MaxCorrectionRounds) {
        Fail "Codex requested changes, but MaxCorrectionRounds=$MaxCorrectionRounds is exhausted."
    }

    $activePacket = Invoke-CorrectionPacket -ControlResult $finalControl -Round $round
    $activeKind = 'CORRECTION'
    Write-Output "CORRECTION finalized: $(Get-RepoRelativePath $activePacket.FullName)"
}

Write-Output ""
Write-Output "Dispatch loop finished."
Write-Output "Task: $(Get-RepoRelativePath $taskPacket.FullName)"
if ($lastExecPacket) {
    Write-Output "Latest EXEC: $(Get-RepoRelativePath $lastExecPacket.FullName)"
}
if ($finalControl) {
    Write-Output "Codex control verdict: $($finalControl.verdict)"
    Write-Output "Commit readiness: $($finalControl.commit_readiness)"
}
Write-Output "No commit or push was performed."
