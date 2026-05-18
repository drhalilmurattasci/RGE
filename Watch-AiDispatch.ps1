#Requires -Version 5.1
<#
.SYNOPSIS
    Watch an AI dispatch run without participating in it.

.DESCRIPTION
    Read-only terminal watcher for Invoke-AiDispatchLoop.ps1 runs. It summarizes
    packets under ai_handoffs/ plus scratch JSON/log files under .ai/dispatch-<ID>.
    It does not call codex, claude, git write commands, new-handoff.ps1, or any
    project build/test command.

.EXAMPLE
    .\Watch-AiDispatch.ps1 -DispatchId POSTV0-SCRIPT-BENCH-RSS-SOAK-BASELINE-001

.EXAMPLE
    .\Watch-AiDispatch.ps1 -Latest -Once
#>
[CmdletBinding(DefaultParameterSetName = 'ById')]
param(
    [Parameter(Mandatory, ParameterSetName = 'ById')]
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string]$DispatchId,

    [Parameter(Mandatory, ParameterSetName = 'Latest')]
    [switch]$Latest,

    [ValidateRange(2, 3600)]
    [int]$IntervalSeconds = 15,

    [switch]$Once,

    [switch]$NoClear,

    [switch]$Tail,

    [ValidateRange(1, 200)]
    [int]$TailLines = 40
)

$ErrorActionPreference = 'Stop'

$script:RepoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$script:AiDir = Join-Path $script:RepoRoot '.ai'
$script:HandoffDir = Join-Path $script:RepoRoot 'ai_handoffs'

function Get-RepoRelativePath {
    param([string]$Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    $root = [System.IO.Path]::GetFullPath($script:RepoRoot).TrimEnd('\', '/')
    if ($full.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
        return (($full.Substring($root.Length)).TrimStart('\', '/') -replace '\\', '/')
    }
    return ($full -replace '\\', '/')
}

function Read-JsonOrNull {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return $null }
    try {
        return (Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json)
    } catch {
        return $null
    }
}

function Get-MarkerValue {
    param([string]$Text, [string]$Name)

    $value = $null
    $pattern = '^' + [regex]::Escape($Name) + '\s*:\s*(.+)$'
    foreach ($line in ($Text -split "`r?`n")) {
        $norm = ($line -replace '`', '').Trim()
        $norm = ($norm -replace '^[>\-\*\+\s]+', '').Trim()
        if ($norm -match $pattern) {
            $candidate = ($matches[1] -replace '^[\*"''\s]+', '' -replace '[\*"''\s]+$', '')
            if ($candidate) { $value = $candidate }
        }
    }
    return $value
}

function Get-PacketInfo {
    param([System.IO.FileInfo]$File)

    $type = 'UNKNOWN'
    if ($File.Name -match '_(TASK|EXEC|REVIEW|CORRECT|CLOSEOUT|STATE)_') {
        $type = $matches[1]
    }

    $values = @{}
    try {
        Get-Content -LiteralPath $File.FullName | ForEach-Object {
            if ($_ -match '^(DISPATCH_ID|STATUS|HANDOFF_STATUS|NEXT_ROLE|EXIT_CODE):\s*(.*)$') {
                $values[$matches[1]] = $matches[2]
            }
        }
    } catch {
        $values['READ_ERROR'] = $_.Exception.Message
    }

    [PSCustomObject]@{
        Type = $type
        Name = $File.Name
        Updated = $File.LastWriteTime
        Status = $values['STATUS']
        Handoff = $values['HANDOFF_STATUS']
        NextRole = $values['NEXT_ROLE']
        ExitCode = $values['EXIT_CODE']
        Path = Get-RepoRelativePath $File.FullName
    }
}

function Get-LatestDispatchId {
    $task = Get-ChildItem -LiteralPath $script:HandoffDir -Filter '*_TASK_*.md' -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc, Name |
        Select-Object -Last 1

    if (-not $task) {
        throw "No TASK packets found under $(Get-RepoRelativePath $script:HandoffDir)."
    }

    if ($task.Name -notmatch '^(.+)_TASK_\d{4}-\d{2}-\d{2}_.+\.md$') {
        throw "Could not infer DispatchId from latest TASK packet: $($task.Name)"
    }

    return $matches[1]
}

function Get-GitLine {
    $oldLocation = Get-Location
    $oldEap = $ErrorActionPreference
    try {
        Set-Location -LiteralPath $script:RepoRoot
        $ErrorActionPreference = 'Continue'
        $branch = (& git status --short --branch --untracked-files=no 2>$null | Select-Object -First 1)
        $sync = (& git rev-list --left-right --count origin/main...HEAD 2>$null)
        if ($LASTEXITCODE -eq 0 -and $sync) {
            return "$branch | origin/main...HEAD $sync"
        }
        return "$branch"
    } catch {
        return "git status unavailable: $($_.Exception.Message)"
    } finally {
        $ErrorActionPreference = $oldEap
        Set-Location -LiteralPath $oldLocation
    }
}

function Format-OptionalValue {
    param($Value)
    if ($null -eq $Value -or $Value -eq '') { return '-' }
    return [string]$Value
}

function Show-JsonSummary {
    param(
        [string]$Label,
        [System.IO.FileInfo[]]$Files,
        [string[]]$Fields
    )

    if (-not $Files -or $Files.Count -eq 0) {
        Write-Host "${Label}: -"
        return
    }

    foreach ($file in ($Files | Sort-Object Name)) {
        $json = Read-JsonOrNull $file.FullName
        if ($null -eq $json) {
            Write-Host "${Label}: $($file.Name) (unreadable JSON)"
            continue
        }

        $parts = @()
        foreach ($field in $Fields) {
            $value = $json.$field
            if ($null -ne $value -and $value -ne '') {
                $parts += "$field=$value"
            }
        }
        if ($parts.Count -eq 0) {
            $parts += 'no summary fields'
        }
        Write-Host "${Label}: $($file.Name) $($parts -join '; ')"
    }
}

function Show-PlanGateSummary {
    param([System.IO.FileInfo[]]$Files)

    if (-not $Files -or $Files.Count -eq 0) {
        Write-Host "Plan gate: -"
        return
    }

    foreach ($file in ($Files | Sort-Object Name)) {
        if ($file.Extension -eq '.md') {
            $text = Get-Content -Raw -LiteralPath $file.FullName
            $verdict = Get-MarkerValue -Text $text -Name 'GATE_VERDICT'
            Write-Host "Plan gate: $($file.Name) verdict=$(Format-OptionalValue $verdict)"
        } else {
            $json = Read-JsonOrNull $file.FullName
            $verdict = if ($json) { $json.verdict } else { $null }
            Write-Host "Plan gate: $($file.Name) verdict=$(Format-OptionalValue $verdict)"
        }
    }
}

function Show-ExecutionSummary {
    param([System.IO.FileInfo[]]$Files)

    if (-not $Files -or $Files.Count -eq 0) {
        Write-Host "Execution: -"
        return
    }

    foreach ($file in ($Files | Sort-Object Name)) {
        if ($file.Extension -eq '.md') {
            $text = Get-Content -Raw -LiteralPath $file.FullName
            $status = Get-MarkerValue -Text $text -Name 'EXEC_STATUS'
            $packet = Get-MarkerValue -Text $text -Name 'EXEC_PACKET'
            Write-Host "Execution: $($file.Name) status=$(Format-OptionalValue $status); exec_packet=$(Format-OptionalValue $packet)"
        } else {
            $json = Read-JsonOrNull $file.FullName
            $status = if ($json) { $json.status } else { $null }
            $packet = if ($json) { $json.exec_packet } else { $null }
            Write-Host "Execution: $($file.Name) status=$(Format-OptionalValue $status); exec_packet=$(Format-OptionalValue $packet)"
        }
    }
}

function Get-PhaseMaxIndex {
    param([string[]]$Names, [string]$Pattern)
    $found = @($Names | ForEach-Object { if ($_ -match $Pattern) { [int]$matches[1] } })
    if ($found.Count -eq 0) { return -1 }
    ($found | Measure-Object -Maximum).Maximum
}

function Show-Progress {
    param([System.IO.FileInfo[]]$RunFiles)

    if (-not $RunFiles -or $RunFiles.Count -eq 0) {
        Write-Host "Progress: [----------]   0%  ->  not started"
        return
    }

    $names = @($RunFiles | ForEach-Object { $_.Name })
    $planMax   = Get-PhaseMaxIndex -Names $names -Pattern '^codex\.plan\.rev(\d+)\.log$'
    $gateMax   = Get-PhaseMaxIndex -Names $names -Pattern '^claude\.plan_gate\.rev(\d+)\.md$'
    $execMax   = Get-PhaseMaxIndex -Names $names -Pattern '^claude\.execute\.round(\d+)\.md$'
    $verifyMax = Get-PhaseMaxIndex -Names $names -Pattern '^verification\.round(\d+)\.log$'
    $ctrlMax   = Get-PhaseMaxIndex -Names $names -Pattern '^codex\.control\.round(\d+)\.json$'

    # Current correction round = the highest execution round seen (0 = first
    # attempt). verify/control must be at >= that round to count as done, so a
    # correction loop that re-runs them correctly reads as "not done yet".
    $round = if ($execMax -ge 0) { $execMax } else { 0 }

    $phases = @(
        @{ Name = 'plan';    Done = ($planMax   -ge 0);      Pct = 28 },
        @{ Name = 'gate';    Done = ($gateMax   -ge 0);      Pct = 42 },
        @{ Name = 'execute'; Done = ($execMax   -ge 0);      Pct = 62 },
        @{ Name = 'verify';  Done = ($verifyMax -ge $round); Pct = 82 },
        @{ Name = 'control'; Done = ($ctrlMax   -ge $round); Pct = 96 }
    )

    $pct = 0
    $current = $null
    foreach ($p in $phases) {
        if ($p.Done -and ($null -eq $current)) {
            $pct = $p.Pct
        } elseif ($null -eq $current) {
            $current = $p.Name
        }
    }
    if ($null -eq $current) { $pct = 100; $current = 'done' }

    $fill = [int][Math]::Round($pct / 10.0)
    $bar = ('#' * $fill) + ('-' * (10 - $fill))
    $roundNote = if ($round -ge 1) { "  [correction round $round]" } else { '' }
    Write-Host ("Progress: [{0}] {1,3}%  ->  {2}{3}" -f $bar, $pct, $current, $roundNote)
}

function Show-Dispatch {
    param([string]$Id)

    $now = Get-Date
    $runDir = Join-Path $script:AiDir ("dispatch-{0}" -f $Id)
    $packets = @(Get-ChildItem -LiteralPath $script:HandoffDir -Filter "$Id`_*.md" -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc, Name |
        ForEach-Object { Get-PacketInfo $_ })

    Write-Host "AI dispatch watcher"
    Write-Host "Time:     $($now.ToString('yyyy-MM-dd HH:mm:ss zzz'))"
    Write-Host "Repo:     $script:RepoRoot"
    Write-Host "Dispatch: $Id"
    Write-Host "Git:      $(Get-GitLine)"
    Write-Host ""

    if ($packets.Count -eq 0) {
        Write-Host "Packets:  none yet"
    } else {
        Write-Host "Packets:"
        $packets |
            Select-Object Type,Updated,Status,Handoff,NextRole,ExitCode,Name |
            Format-Table -AutoSize |
            Out-String -Width 240 |
            Write-Host
    }

    Write-Host "Run dir:  $(Get-RepoRelativePath $runDir)"
    if (Test-Path -LiteralPath $runDir) {
        $runFiles = @(Get-ChildItem -LiteralPath $runDir -File -Recurse -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTimeUtc, FullName)

        if ($runFiles.Count -gt 0) {
            $latestFile = $runFiles | Select-Object -Last 1
            $age = New-TimeSpan -Start $latestFile.LastWriteTime -End $now
            Write-Host ("Latest:   {0} ({1:n0}s ago)" -f (Get-RepoRelativePath $latestFile.FullName), $age.TotalSeconds)
        } else {
            Write-Host "Latest:   no run files yet"
        }

        Show-Progress -RunFiles $runFiles

        $planFiles = @($runFiles | Where-Object { $_.Name -match '^claude\.plan_gate\.rev\d+\.(json|md)$' })
        $execFiles = @($runFiles | Where-Object { $_.Name -match '^claude\.execute\.round\d+\.(json|md)$' })
        $controlFiles = @($runFiles | Where-Object { $_.Name -match '^codex\.control\.round\d+\.json$' })

        Show-PlanGateSummary -Files $planFiles
        Show-ExecutionSummary -Files $execFiles
        Show-JsonSummary -Label 'Control' -Files $controlFiles -Fields @('verdict', 'commit_readiness', 'summary')

        if ($Tail -and $runFiles.Count -gt 0) {
            $latestText = $runFiles |
                Where-Object { $_.Extension -in @('.log', '.txt', '.md', '.json') } |
                Select-Object -Last 1
            if ($latestText) {
                Write-Host ""
                Write-Host "Tail: $(Get-RepoRelativePath $latestText.FullName)"
                Get-Content -LiteralPath $latestText.FullName -Tail $TailLines -ErrorAction SilentlyContinue
            }
        }
    } else {
        Write-Host "Latest:   run dir not created yet"
        Write-Host "Plan gate: -"
        Write-Host "Execution: -"
        Write-Host "Control: -"
    }
}

if ($Latest) {
    $DispatchId = Get-LatestDispatchId
}

do {
    if (-not $NoClear -and -not $Once) {
        Clear-Host
    }
    Show-Dispatch -Id $DispatchId
    if ($Once) { break }
    Start-Sleep -Seconds $IntervalSeconds
} while ($true)
