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

function Test-PacketForbidsSidecar {
    param([System.IO.FileInfo]$Packet)

    $text = Get-Content -Raw -LiteralPath $Packet.FullName
    return (
        $text -match '(?is)MUST NOT add new files.*sidecar\s+`?\.meta\.json`?' -or
        $text -match '(?is)Do not create,\s*finalize,\s*repair,\s*delete,\s*or\s*regenerate handoff sidecars' -or
        $text -match '(?is)MUST NOT.*sidecar\s+`?\.meta\.json`?'
    )
}

function Get-MarkerValue {
    # Extract a line-anchored 'NAME: value' marker from free-form model output.
    # Tolerates leading list/quote/emphasis decoration and surrounding backticks
    # or quotes; preserves interior characters (path separators, underscores).
    # Returns the value of the last matching line, or $null if none is present.
    param([string]$Text, [string]$Name)

    $value = $null
    $pattern = '^' + [regex]::Escape($Name) + '\s*:\s*(.+)$'
    foreach ($line in ($Text -split "`r?`n")) {
        $norm = ($line -replace '`', '').Trim()
        $norm = ($norm -replace '^[>\-\*\+\s]+', '').Trim()
        if ($norm -match $pattern) {
            $candidate = ($matches[1] -replace '^[\*"''\s]+', '' -replace '[\*"''\s.,;]+$', '')
            if ($candidate) { $value = $candidate }
        }
    }
    return $value
}

function Resolve-ExecStatusFromPacket {
    param([System.IO.FileInfo]$Packet)

    if (-not $Packet -or -not (Test-Path -LiteralPath $Packet.FullName)) {
        return $null
    }

    $text = Get-Content -Raw -LiteralPath $Packet.FullName
    $handoff = Get-MarkerValue -Text $text -Name 'HANDOFF_STATUS'
    $packetStatus = Get-MarkerValue -Text $text -Name 'STATUS'
    $exitRaw = Get-MarkerValue -Text $text -Name 'EXIT_CODE'

    $handoffNorm = if ($handoff) { $handoff.ToUpperInvariant() } else { '' }
    $packetStatusNorm = if ($packetStatus) { $packetStatus.ToUpperInvariant() } else { '' }
    $exitCode = $null
    if ($exitRaw -and $exitRaw -match '^-?\d+$') {
        $exitCode = [int]$exitRaw
    }

    if ($handoffNorm -eq 'COMPLETE' -and $exitCode -eq 0) {
        return 'executed'
    }
    if ($handoffNorm -in @('BLOCKED', 'NEEDS_HUMAN') -or $packetStatusNorm -in @('BLOCKED', 'NEEDS_HUMAN')) {
        return 'blocked'
    }
    if ($handoffNorm -eq 'FAILED' -or $packetStatusNorm -eq 'FAILED' -or ($null -ne $exitCode -and $exitCode -ne 0)) {
        return 'failed'
    }

    return $null
}

function Invoke-ClaudeMarker {
    # Run a Claude step and extract line-anchored markers from its prose output.
    # The verbatim response is saved to OutputPath as the record and as Codex
    # revision context. No JSON parsing or schema validation on the critical
    # path: each marker in $Markers maps a NAME to either an allowed-values
    # array (required, enum-checked) or $null (optional, free-form).
    param(
        [string]$Prompt,
        [hashtable]$Markers,
        [string]$OutputPath,
        [ValidateSet('acceptEdits', 'auto', 'bypassPermissions', 'default', 'dontAsk', 'plan')]
        [string]$PermissionMode
    )

    $base = $OutputPath -replace '\.[^.\\/]+$', ''
    $envelopePath = "$base.envelope.json"
    $stderrPath = "$base.stderr.txt"

    $args = @(
        '-p',
        '--mcp-config', $script:McpConfig,
        '--permission-mode', $PermissionMode,
        '--output-format', 'json'
    )
    if ($ClaudeModel) { $args += @('--model', $ClaudeModel) }

    # Same PS 5.1 stderr/EAP hazard as Invoke-CodexPrompt - isolate the npm claude shim.
    $global:LASTEXITCODE = 0
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & claude @args $Prompt > $envelopePath 2> $stderrPath
    } finally {
        $ErrorActionPreference = $prevEap
    }
    if ($LASTEXITCODE -ne 0) {
        Fail "claude failed. See $stderrPath"
    }
    if (-not (Test-Path -LiteralPath $envelopePath) -or (Get-Item -LiteralPath $envelopePath).Length -eq 0) {
        Fail "claude produced no output. See $stderrPath"
    }

    $envelope = Read-JsonFile $envelopePath
    $props = @($envelope.PSObject.Properties.Name)
    if (($props -contains 'is_error') -and $envelope.is_error) {
        Fail "claude reported an error: $($envelope.result). See $envelopePath"
    }
    if (-not ($props -contains 'result')) {
        Fail "claude did not return a result payload. See $envelopePath"
    }

    $resultText = [string]$envelope.result
    Write-TextFile $OutputPath $resultText

    $extracted = @{}
    foreach ($name in @($Markers.Keys)) {
        $allowed = $Markers[$name]
        $value = Get-MarkerValue -Text $resultText -Name $name
        if ($null -eq $value) {
            if ($allowed) {
                Fail "claude response is missing the required '${name}:' marker line. See $OutputPath"
            }
        } elseif ($allowed -and ($allowed -notcontains $value)) {
            Fail "claude '${name}:' marker value '$value' is not one of: $($allowed -join ', '). See $OutputPath"
        }
        $extracted[$name] = $value
    }

    return [pscustomobject]@{
        Text    = $resultText
        Markers = $extracted
    }
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
    $out = Join-Path $script:RunDir ("claude.plan_gate.rev{0}.md" -f $RevisionNumber)
    $prompt = @"
You are Claude acting as Executor preflight gate for RGE.

Review the TASK_PACKET:

$taskRel

You must not edit files. Read the packet, inspect only the repo context needed
to decide whether the plan is executable, bounded, and protocol-safe.

Write your review as free-form prose. Cover, in whatever structure you prefer:
- the verdict reasoning,
- any blocking reasons,
- recommended changes to the TASK packet,
- the commands you actually ran.

End your response with exactly one line, by itself, anchored at column 1:

GATE_VERDICT: approve

Substitute one of these values for 'approve':
- approve        the task is safe to execute as written.
- needs_changes  Codex should revise the TASK packet first.
- block          execution must not proceed without human arbitration.

That GATE_VERDICT line must be the final line of your response. Do not wrap it
in Markdown, quotes, or a code block.
"@
    $res = Invoke-ClaudeMarker -Prompt $prompt `
        -Markers @{ 'GATE_VERDICT' = @('approve', 'needs_changes', 'block') } `
        -OutputPath $out -PermissionMode 'plan'
    return [pscustomobject]@{ verdict = $res.Markers['GATE_VERDICT']; review = $res.Text }
}

function Invoke-ClaudeExecute {
    param([System.IO.FileInfo]$ActivePacket, [string]$PacketKind, [int]$Round)

    $packetRel = Get-RepoRelativePath $ActivePacket.FullName
    $out = Join-Path $script:RunDir ("claude.execute.round{0}.md" -f $Round)
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
- If the active packet forbids sidecar .meta.json creation, do not finalize
  the EXEC packet; note that deliberate skip in your summary.

Write a free-form prose summary of the execution: what changed, the
verification commands you ran and their results, the final git status, and
any notes for the reviewer.

End your response with exactly these two lines, by themselves, anchored at
column 1:

EXEC_STATUS: executed
EXEC_PACKET: ai_handoffs/<EXECUTION_REPORT file name>.md

Substitute one EXEC_STATUS value for 'executed':
- executed  the enumerated scope was carried out.
- blocked   a halt condition stopped execution.
- failed    execution was attempted but did not complete.

For EXEC_PACKET give the repo-relative path to the EXECUTION_REPORT you wrote,
or the single word none if no report was written. These two lines must be the
final lines of your response. Do not wrap them in Markdown, quotes, or a code
block.
"@
    $res = Invoke-ClaudeMarker -Prompt $prompt `
        -Markers @{ 'EXEC_STATUS' = $null; 'EXEC_PACKET' = $null } `
        -OutputPath $out -PermissionMode $ClaudePermissionMode
    $status = $res.Markers['EXEC_STATUS']
    if ($status -and (@('executed', 'blocked', 'failed') -notcontains $status)) {
        Fail "claude 'EXEC_STATUS:' marker value '$status' is not one of: executed, blocked, failed. See $out"
    }
    $packet = $res.Markers['EXEC_PACKET']
    if ($packet -and ($packet -match '^(none|<none>|n/?a|null|-)$')) { $packet = $null }
    if (-not $status) {
        $fallbackPacket = $null
        if ($packet) {
            $candidate = Join-Path $script:RepoRoot (($packet -replace '/', '\'))
            if (Test-Path -LiteralPath $candidate) {
                $fallbackPacket = Get-Item -LiteralPath $candidate
            }
        }
        if (-not $fallbackPacket) {
            $fallbackPacket = Get-LatestPacket -PacketType 'EXEC'
        }
        if ($fallbackPacket) {
            $status = Resolve-ExecStatusFromPacket -Packet $fallbackPacket
            if ($status -and -not $packet) {
                $packet = Get-RepoRelativePath $fallbackPacket.FullName
            }
        }
        if (-not $status) {
            Fail "claude response is missing the required 'EXEC_STATUS:' marker line and no canonical EXEC packet footer could be used as fallback. See $out"
        }
    }
    return [pscustomobject]@{
        status      = $status
        exec_packet = $packet
        report      = $res.Text
    }
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
- verdict=pass only if the work is ready for queue commit/publish.
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
        $gatePath = Join-Path $script:RunDir ("claude.plan_gate.rev{0}.md" -f $i)
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
