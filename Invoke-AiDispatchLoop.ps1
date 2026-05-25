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
      6. Run the verification gate (.ai/dispatch.verify.ps1). A non-zero
         exit fails the round before any control review runs.
      7. Ask Codex to perform a read-only control review of the diff,
         packets, and verification results.

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

    [string]$VerifyScript = '',

    [switch]$SkipVerification,

    [ValidateRange(60, 7200)]
    [int]$ModelTimeoutSec = 1800,

    [ValidateRange(120, 14400)]
    [int]$VerifyTimeoutSec = 3600,

    [ValidateRange(0, 3600)]
    [int]$CodexStallThresholdSec = 300,

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

function Invoke-WithTimeout {
    # Run a native command with a hard timeout. The command runs inside a
    # child powershell so that, on timeout, its whole process tree can be
    # killed (taskkill /T) -- a hung cargo/codex/claude cannot then sit until
    # the scheduled task kills the entire queue. Arguments are passed via a
    # clixml params file so the call operator quotes them correctly, including
    # a multi-line prompt argument. stdout goes to OutFile; stderr to ErrFile
    # if given, else merged into OutFile. Returns @{ Code; TimedOut; Stalled }.
    #
    # Optional Codex-only stall watchdog: when -StallThresholdSec is greater
    # than 0, the child is polled and OutFile is watched for progress. The
    # watchdog only arms once OutFile becomes non-empty, then treats output
    # activity as an increase in OutFile.Length. If the size does not advance
    # for StallThresholdSec, the same process tree is killed and the result
    # becomes @{ Code = 125; TimedOut = $true; Stalled = $true }. When
    # -StallThresholdSec is 0 or omitted, the legacy hard-timeout control flow
    # runs unchanged and the additive Stalled field remains $false.
    param(
        [string]$Exe,
        [string[]]$Arguments,
        [string]$OutFile,
        [int]$TimeoutSec,
        [string]$StdinFile = '',
        [string]$ErrFile = '',
        [ValidateRange(0, 3600)]
        [int]$StallThresholdSec = 0,
        [ValidateRange(1, 600)]
        [int]$PollIntervalSec = 5
    )
    $base = [System.IO.Path]::GetTempFileName()
    $paramsFile = "$base.params.xml"
    $launcherFile = "$base.launcher.ps1"
    @{
        Exe = $Exe; Arguments = $Arguments; OutFile = $OutFile
        StdinFile = $StdinFile; ErrFile = $ErrFile
    } | Export-Clixml -LiteralPath $paramsFile
    $launcher = @'
param([string]$ParamsFile)
$ErrorActionPreference = 'Continue'
$p = Import-Clixml -LiteralPath $ParamsFile
$a = @($p.Arguments)
$o = [string]$p.OutFile
$e = [string]$p.ErrFile
$s = [string]$p.StdinFile
if ($s) {
    if ($e) { Get-Content -Raw -LiteralPath $s | & $p.Exe @a > $o 2> $e }
    else    { Get-Content -Raw -LiteralPath $s | & $p.Exe @a > $o 2>&1 }
} else {
    if ($e) { & $p.Exe @a > $o 2> $e }
    else    { & $p.Exe @a > $o 2>&1 }
}
exit $LASTEXITCODE
'@
    [System.IO.File]::WriteAllText($launcherFile, $launcher, [System.Text.UTF8Encoding]::new($false))
    $timedOut = $false
    $stalled = $false
    $code = 0
    try {
        $proc = Start-Process -FilePath 'powershell.exe' -PassThru -NoNewWindow -ArgumentList @(
            '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $launcherFile, $paramsFile)
        # Touch .Handle so the Process object caches it; without this a
        # -PassThru process returns $null from .ExitCode after it exits.
        $null = $proc.Handle
        if ($StallThresholdSec -le 0) {
            if ($proc.WaitForExit($TimeoutSec * 1000)) {
                $proc.WaitForExit()
                $code = $proc.ExitCode
            } else {
                $timedOut = $true
                & taskkill /T /F /PID $proc.Id *> $null
                $code = 124
            }
        } else {
            # Stall watchdog path: poll OutFile for progress and kill on
            # whichever fires first -- the existing hard timeout or the new
            # stall threshold. The watchdog arms only after OutFile reaches
            # non-zero size, so quiet startup time does not count as a stall.
            $hardDeadline = [DateTime]::UtcNow.AddSeconds($TimeoutSec)
            $armed = $false
            $lastSize = [long]0
            $stallDeadline = [DateTime]::MaxValue
            while ($true) {
                if ($proc.HasExited) {
                    $proc.WaitForExit()
                    $code = $proc.ExitCode
                    break
                }
                $now = [DateTime]::UtcNow
                if ($now -ge $hardDeadline) {
                    $timedOut = $true
                    & taskkill /T /F /PID $proc.Id *> $null
                    $code = 124
                    break
                }
                $fi = $null
                if ($OutFile -and (Test-Path -LiteralPath $OutFile)) {
                    $fi = Get-Item -LiteralPath $OutFile -ErrorAction SilentlyContinue
                }
                if ($fi -and $fi.Length -gt 0) {
                    $curSize = [long]$fi.Length
                    if (-not $armed) {
                        $armed = $true
                        $lastSize = $curSize
                        $stallDeadline = $now.AddSeconds($StallThresholdSec)
                    } elseif ($curSize -gt $lastSize) {
                        $lastSize = $curSize
                        $stallDeadline = $now.AddSeconds($StallThresholdSec)
                    } elseif ($now -ge $stallDeadline) {
                        $stalled = $true
                        $timedOut = $true
                        & taskkill /T /F /PID $proc.Id *> $null
                        $code = 125
                        break
                    }
                }
                # Cap each sleep by the nearest pending deadline so the kill
                # is prompt and we do not overshoot the hard timeout by a
                # full poll interval. 50 ms is the lower bound so we never
                # spin in a tight polling loop.
                $nextDeadline = $hardDeadline
                if ($armed -and $stallDeadline -lt $nextDeadline) {
                    $nextDeadline = $stallDeadline
                }
                $msPoll = $PollIntervalSec * 1000
                $msUntil = ($nextDeadline - $now).TotalMilliseconds
                $msSleep = if ($msUntil -lt $msPoll) { $msUntil } else { $msPoll }
                if ($msSleep -lt 50) { $msSleep = 50 }
                Start-Sleep -Milliseconds ([int]$msSleep)
            }
        }
    } finally {
        Remove-Item -LiteralPath $base, $paramsFile, $launcherFile -Force -ErrorAction SilentlyContinue
    }
    return [pscustomobject]@{ Code = $code; TimedOut = $timedOut; Stalled = $stalled }
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

    $r = Invoke-WithTimeout -Exe 'codex' -Arguments $args -StdinFile $promptPath `
        -OutFile $LogPath -TimeoutSec $ModelTimeoutSec -StallThresholdSec $CodexStallThresholdSec
    if ($r.Stalled) {
        Fail "codex exec stalled: no log growth for ${CodexStallThresholdSec}s after first output. Killed process tree. See $LogPath"
    }
    if ($r.TimedOut) {
        Fail "codex exec timed out after ${ModelTimeoutSec}s (terminal infrastructure failure). See $LogPath"
    }
    if ($r.Code -ne 0) {
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

    $r = Invoke-WithTimeout -Exe 'claude' -Arguments ($args + $Prompt) `
        -OutFile $envelopePath -ErrFile $stderrPath -TimeoutSec $ModelTimeoutSec
    if ($r.TimedOut) {
        Fail "claude timed out after ${ModelTimeoutSec}s (terminal infrastructure failure). See $stderrPath"
    }
    if ($r.Code -ne 0) {
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

    # Tail-anchor: required markers must appear at the end of the response
    # (GATE_VERDICT as the final line; EXEC_STATUS/EXEC_PACKET as the final
    # two). Scanning only the last few non-empty lines stops a marker quoted
    # mid-prose from being read as the verdict.
    $tailText = (@($resultText -split "`r?`n" | Where-Object { $_.Trim() }) |
        Select-Object -Last 3) -join "`n"

    $extracted = @{}
    foreach ($name in @($Markers.Keys)) {
        $allowed = $Markers[$name]
        $value = Get-MarkerValue -Text $tailText -Name $name
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

    $r = Invoke-WithTimeout -Exe 'claude' -OutFile $probeOut -ErrFile $probeErr -TimeoutSec 180 `
        -Arguments @('-p', '--output-format', 'json', 'Return exactly: ready')
    if ($r.TimedOut) {
        Fail "claude readiness probe timed out after 180s. Check the Claude CLI / auth, then retry."
    }

    if (-not (Test-Path -LiteralPath $probeOut) -or (Get-Item -LiteralPath $probeOut).Length -eq 0) {
        if ($r.Code -ne 0) {
            Fail "claude readiness probe failed. See $probeErr"
        }
        Fail "claude readiness probe produced no JSON output. See $probeErr"
    }

    $probe = Read-JsonFile $probeOut
    $props = @($probe.PSObject.Properties.Name)
    if (($props -contains 'is_error') -and $probe.is_error) {
        Fail "claude is not ready: $($probe.result). Run Claude Code login/auth setup, then retry."
    }
    if ($r.Code -ne 0) {
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
    # When the executor claims success, its EXEC packet must be the canonical
    # ai_handoffs/<DispatchId>_EXEC_*.md -- not a stale or unrelated packet.
    if ($status -eq 'executed') {
        $execName = if ($packet) { Split-Path -Leaf $packet } else { '' }
        if (-not $packet -or ($execName -notlike "${DispatchId}_EXEC_*.md")) {
            Fail "claude reported EXEC_STATUS: executed but EXEC_PACKET '$packet' is not the canonical ai_handoffs/${DispatchId}_EXEC_*.md packet for this dispatch. See $out"
        }
    }
    return [pscustomobject]@{
        status      = $status
        exec_packet = $packet
        report      = $res.Text
    }
}

function Invoke-CodexControl {
    param(
        [System.IO.FileInfo]$TaskPacket,
        [System.IO.FileInfo]$ExecPacket,
        [int]$Round,
        [string]$VerificationLog = ''
    )

    $taskRel = Get-RepoRelativePath $TaskPacket.FullName
    $verifyNote = ''
    if ($VerificationLog -and (Test-Path -LiteralPath $VerificationLog)) {
        $verifyNote = "`n- $(Get-RepoRelativePath $VerificationLog) -- the orchestrator already ran the canonical CI verification gate (format, architecture lints, supply chain, workspace tests) and it passed; corroborate it rather than trusting EXECUTION_REPORT prose alone"
    }
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
- verification claims in the EXECUTION_REPORT$verifyNote
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

function Invoke-Verification {
    # Run the canonical verification gate (.ai/dispatch.verify.ps1 by default).
    # Exit 0 means the working tree passes the same checks CI enforces; a
    # non-zero exit fails the round before any control review or publish.
    param([int]$Round)

    $log = Join-Path $script:RunDir ("verification.round{0}.log" -f $Round)
    if ($SkipVerification) {
        Write-TextFile $log "Verification skipped: -SkipVerification was set."
        return [pscustomobject]@{ Passed = $true; Skipped = $true; ExitCode = 0; Log = $log }
    }

    $r = Invoke-WithTimeout -Exe 'powershell.exe' -OutFile $log -TimeoutSec $VerifyTimeoutSec `
        -Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $script:VerifyScriptPath)
    if ($r.TimedOut) {
        Add-Content -LiteralPath $log -Value "`n[orchestrator] verification timed out after ${VerifyTimeoutSec}s; process tree killed."
    }
    return [pscustomobject]@{
        Passed   = ((-not $r.TimedOut) -and ($r.Code -eq 0))
        Skipped  = $false
        TimedOut = $r.TimedOut
        ExitCode = $r.Code
        Log      = $log
    }
}

function Invoke-CorrectionPacket {
    param([object]$ControlResult, [object]$Verification, [int]$Round)
    $packet = Invoke-NewPacket -PacketType 'CORRECT' -Author 'Planner / OpenAI Codex'
    $packetRel = Get-RepoRelativePath $packet.FullName

    if ($ControlResult) {
        $reviewContext = "Codex control review result (JSON):`n`n" +
            ($ControlResult | ConvertTo-Json -Depth 16)
    } elseif ($Verification) {
        $verifyTail = ''
        if ($Verification.Log -and (Test-Path -LiteralPath $Verification.Log)) {
            $verifyTail = (Get-Content -LiteralPath $Verification.Log -Tail 120 -ErrorAction SilentlyContinue) -join "`n"
        }
        $reviewContext = @"
The post-execution verification gate FAILED (exit code $($Verification.ExitCode)).
The dispatch cannot pass until verification does. Verification runs the
canonical CI checks: format, architecture lints, supply chain, workspace
tests and doctests. Tail of the verification log:

$verifyTail
"@
    } else {
        $reviewContext = '(no review context was supplied)'
    }

    $prompt = @"
You are Planner / OpenAI Codex in the RGE repository.

Write a CORRECTION_PACKET only. Edit only this file:

$packetRel

$reviewContext

Rules:
- Enumerate only the fixes needed to make the dispatch pass review and the
  verification gate.
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

function Invoke-GitCapture {
    # Run git with PS 5.1 EAP isolation: native git stderr (e.g. benign
    # warnings about the user ignore file) under EAP=Stop would otherwise
    # terminate the script. stderr is dropped; the exit code still reflects
    # success. Returns [pscustomobject]@{ Code; Lines }.
    param([string[]]$GitArgs)
    $global:LASTEXITCODE = 0
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $out = & git @GitArgs 2>$null
    } finally {
        $ErrorActionPreference = $prevEap
    }
    return [pscustomobject]@{ Code = $LASTEXITCODE; Lines = @($out) }
}

function Get-WorktreeChangeSet {
    # Snapshot of `git status --porcelain` lines, for detecting what a
    # workspace-write Codex step changed.
    return @((Invoke-GitCapture @('status', '--porcelain=v1')).Lines |
        Where-Object { ([string]$_).Trim() })
}

function Assert-PlannerScopeClean {
    # A Codex plan/correction step runs under the workspace-write sandbox.
    # Compare the worktree before vs after and fail if the step touched any
    # path other than its own packet -- a prompt injection in untrusted issue
    # text could otherwise make the planner edit source files.
    param([string[]]$Before, [System.IO.FileInfo]$Packet, [string]$StepName)
    $allowed = @(
        (Get-RepoRelativePath $Packet.FullName),
        (Get-RepoRelativePath ($Packet.FullName -replace '\.md$', '.meta.json'))
    )
    $beforeSet = @{}
    foreach ($b in $Before) { $beforeSet[$b] = $true }
    $stray = @()
    foreach ($line in (Get-WorktreeChangeSet)) {
        if ($beforeSet[$line]) { continue }
        $t = [string]$line
        $p = if ($t.Length -gt 3) { $t.Substring(3) } else { $t }
        $p = ($p.Trim().Trim('"')) -replace '\\', '/'
        if ($p -match '->\s*(.+)$') { $p = ($matches[1].Trim().Trim('"')) }
        if ($allowed -notcontains $p) { $stray += $t }
    }
    if ($stray.Count -gt 0) {
        Fail ("$StepName changed files outside its packet (possible prompt-" +
            "injection scope escape):`n" + ($stray -join "`n"))
    }
}

Require-Command git
Require-Command codex
Require-Command claude

if ($ResumeApprovedTask -and $PlanOnly) {
    Fail "-PlanOnly cannot be combined with -ResumeApprovedTask; resume mode runs the execution loop on an already-approved TASK."
}

$repoRootGit = Invoke-GitCapture @('rev-parse', '--show-toplevel')
$script:RepoRoot = ($repoRootGit.Lines -join "`n").Trim()
if ($repoRootGit.Code -ne 0 -or -not $script:RepoRoot) {
    Fail "Not inside a git repository."
}
Set-Location -LiteralPath $script:RepoRoot

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

$script:VerifyScriptPath = if ($VerifyScript) {
    if ([System.IO.Path]::IsPathRooted($VerifyScript)) { $VerifyScript }
    else { Join-Path $script:RepoRoot $VerifyScript }
} else {
    Join-Path $script:AiDir 'dispatch.verify.ps1'
}
if (-not $SkipVerification -and -not (Test-Path -LiteralPath $script:VerifyScriptPath)) {
    Fail "Verification script not found: $script:VerifyScriptPath. Create it, pass -VerifyScript <path>, or run with -SkipVerification (an unverified dispatch can then publish - not recommended)."
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

$aheadGit = Invoke-GitCapture @('rev-list', '--left-right', '--count', 'origin/main...HEAD')
$aheadBehind = ($aheadGit.Lines -join "`n").Trim()
if ($aheadGit.Code -eq 0 -and $aheadBehind -ne "0`t0") {
    Fail "Branch is not synced with origin/main: $aheadBehind"
}

$statusGit = Invoke-GitCapture @('status', '--porcelain=v1')
if ($statusGit.Code -ne 0) {
    Fail "git status --porcelain failed (exit $($statusGit.Code))."
}
$statusLines = $statusGit.Lines
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
        $beforePlan = Get-WorktreeChangeSet
        Invoke-PlanFill -TaskPacket $taskPacket -RevisionNumber $i -PriorClaudeGatePath $gatePath
        Assert-PlannerScopeClean -Before $beforePlan -Packet $taskPacket -StepName "Plan-fill rev $i"
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
$verification = $null

for ($round = 0; $round -le $MaxCorrectionRounds; $round++) {
    $execResult = Invoke-ClaudeExecute -ActivePacket $activePacket -PacketKind $activeKind -Round $round
    Write-Output "Claude execution round ${round}: $($execResult.status)"

    if ($execResult.status -ne 'executed') {
        Fail "Claude execution round ${round} did not complete (EXEC_STATUS: $($execResult.status)). A blocked or failed execution is not eligible to verify or publish; resolve it before re-running."
    }

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

    # Hard verification gate: the working tree must pass the canonical CI
    # checks before Codex control runs. A non-zero exit cannot become a pass.
    $verification = Invoke-Verification -Round $round
    if ($verification.Skipped) {
        Write-Output "Verification round ${round}: SKIPPED (-SkipVerification)"
    } elseif ($verification.TimedOut) {
        Write-Output "Verification round ${round}: TIMED OUT (over ${VerifyTimeoutSec}s)"
    } elseif ($verification.Passed) {
        Write-Output "Verification round ${round}: pass (exit 0)"
    } else {
        Write-Output "Verification round ${round}: FAIL (exit $($verification.ExitCode)) - see $(Get-RepoRelativePath $verification.Log)"
    }

    if ($verification.TimedOut) {
        Fail "Verification timed out (over ${VerifyTimeoutSec}s) - terminal infrastructure failure, not a correctable task. See $(Get-RepoRelativePath $verification.Log)"
    }
    if (-not $verification.Passed) {
        if ($round -ge $MaxCorrectionRounds) {
            Fail "Verification gate failed (exit $($verification.ExitCode)) and MaxCorrectionRounds=$MaxCorrectionRounds is exhausted. See $(Get-RepoRelativePath $verification.Log)"
        }
        $beforeCorrect = Get-WorktreeChangeSet
        $activePacket = Invoke-CorrectionPacket -Verification $verification -Round $round
        Assert-PlannerScopeClean -Before $beforeCorrect -Packet $activePacket -StepName "Correction (verification) round $round"
        $activeKind = 'CORRECTION'
        Write-Output "CORRECTION finalized (verification failure): $(Get-RepoRelativePath $activePacket.FullName)"
        continue
    }

    # Hand Codex the verification log only when verification actually ran:
    # supplying it asserts the gate passed, which is false under
    # -SkipVerification (the log exists but records a skip).
    $verifyLogForControl = if ($verification.Skipped) { '' } else { $verification.Log }
    $finalControl = Invoke-CodexControl -TaskPacket $taskPacket -ExecPacket $lastExecPacket `
        -Round $round -VerificationLog $verifyLogForControl
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

    $beforeCorrect = Get-WorktreeChangeSet
    $activePacket = Invoke-CorrectionPacket -ControlResult $finalControl -Round $round
    Assert-PlannerScopeClean -Before $beforeCorrect -Packet $activePacket -StepName "Correction (control) round $round"
    $activeKind = 'CORRECTION'
    Write-Output "CORRECTION finalized: $(Get-RepoRelativePath $activePacket.FullName)"
}

Write-Output ""
Write-Output "Dispatch loop finished."
Write-Output "Task: $(Get-RepoRelativePath $taskPacket.FullName)"
if ($lastExecPacket) {
    Write-Output "Latest EXEC: $(Get-RepoRelativePath $lastExecPacket.FullName)"
}
if ($verification) {
    if ($verification.Skipped) {
        Write-Output "Verification: skipped (-SkipVerification)"
    } else {
        Write-Output "Verification: pass (exit 0)"
    }
}
if ($finalControl) {
    Write-Output "Codex control verdict: $($finalControl.verdict)"
    Write-Output "Commit readiness: $($finalControl.commit_readiness)"
}
Write-Output "No commit or push was performed."
