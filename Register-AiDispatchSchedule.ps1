#Requires -Version 5.1
<#
.SYNOPSIS
    Register, inspect, or remove a Windows Scheduled Task that runs the RGE AI
    dispatch automation on a recurring interval -- the unattended trigger.

.DESCRIPTION
    By default the task runs Invoke-AiDispatchQueue.ps1, which processes one
    human-labelled `ai-dispatch` GitHub issue per tick. With -Autonomous it
    runs Invoke-AiDispatchAuto.ps1 instead: Codex selects the next task from
    .ai/dispatch.tasks.md and the queue runs it -- no human-labelled issue
    needed. Either way the queue's single-run lock keeps overlapping ticks
    from colliding and its orphan-recovery janitor cleans up interrupted ones.

    The task runs as the current user with an Interactive logon: it fires
    while you are logged on and needs no stored password. A run missed because
    the machine was asleep is caught up on the next wake (-StartWhenAvailable).

.PARAMETER IntervalMinutes
    Minutes between ticks. Default 30. A full dispatch can take longer than one
    interval; that is fine -- the queue lock makes a new tick skip while the
    previous one is still running.

.PARAMETER Autonomous
    Schedule the autonomous driver (Invoke-AiDispatchAuto.ps1) instead of the
    plain issue queue. Codex picks the work; see -PublishMode and
    -MaxAutonomousTasks.

.PARAMETER PublishMode
    Autonomous only. 'pr' (default) pushes the dispatch branch and opens a
    pull request targeting main without merging or pushing origin/main and
    without closing the source issue; 'branch' leaves passed work on a branch
    for a human to merge; 'main' auto-publishes to origin/main (explicit
    opt-in for delegated-human auto-publish batches).

.PARAMETER MaxAutonomousTasks
    Autonomous only. Halt for human review after this many tasks. Default 5.

.PARAMETER MaxRunHours
    Scheduled-task execution time limit, in hours. Default 3. Raise it if a
    full verification run (model calls + cargo test --workspace) needs longer.

.PARAMETER MaxCorrectionRounds
    Per-task correction-round budget passed through to the dispatch loop.
    Default 2. Raise it (3-5) for real coding tasks -- they hit Codex
    control's needs_changes far more often than documentation tasks do.

.PARAMETER Executor
    Executor passed through Auto -> Queue -> Loop. Default `codex` keeps the
    automation on the Codex-as-executor path. `claude` is an explicit opt-in.

.PARAMETER MaxPlanRevisions
    Per-task plan-revision budget passed through to the dispatch loop.
    Default 1.

.PARAMETER TaskName
    Scheduled Task name. Default 'RGE-AiDispatch'.

.PARAMETER Unregister
    Remove the task instead of creating it.

.PARAMETER Status
    Report the task state, last run, and next run; change nothing.

.EXAMPLE
    .\Register-AiDispatchSchedule.ps1
    # Issue-queue mode: run the queue every 30 minutes.

.EXAMPLE
    .\Register-AiDispatchSchedule.ps1 -Autonomous
    # Autonomous mode (pr default): Codex picks tasks; passed work is pushed
    # and opened as a pull request targeting main for human review.

.EXAMPLE
    .\Register-AiDispatchSchedule.ps1 -IntervalMinutes 15
    .\Register-AiDispatchSchedule.ps1 -Status
    .\Register-AiDispatchSchedule.ps1 -Unregister

.NOTES
    No elevation is needed: the task is registered for the current user only.
    Interactive logon is used deliberately -- an S4U (run-when-logged-off) task
    may be unable to read the `gh` auth token from Windows Credential Manager.
#>
[CmdletBinding(DefaultParameterSetName = 'Register')]
param(
    [Parameter(ParameterSetName = 'Register')]
    [ValidateRange(10, 1440)]
    [int]$IntervalMinutes = 30,

    [Parameter(ParameterSetName = 'Register')]
    [switch]$Autonomous,

    [Parameter(ParameterSetName = 'Register')]
    [ValidateSet('branch', 'main', 'pr')]
    [string]$PublishMode = 'pr',

    [Parameter(ParameterSetName = 'Register')]
    [ValidateRange(1, 200)]
    [int]$MaxAutonomousTasks = 5,

    [Parameter(ParameterSetName = 'Register')]
    [ValidateRange(1, 12)]
    [int]$MaxRunHours = 3,

    [Parameter(ParameterSetName = 'Register')]
    [ValidateRange(0, 5)]
    [int]$MaxPlanRevisions = 1,

    [Parameter(ParameterSetName = 'Register')]
    [ValidateRange(0, 5)]
    [int]$MaxCorrectionRounds = 2,

    [Parameter(ParameterSetName = 'Register')]
    [ValidateSet('claude', 'codex')]
    [string]$Executor = 'codex',

    [Parameter(ParameterSetName = 'Register')]
    [switch]$CodexExecutorExternalScratch,

    [ValidatePattern('^[A-Za-z0-9 ._-]+$')]
    [string]$TaskName = 'RGE-AiDispatch',

    [Parameter(Mandatory, ParameterSetName = 'Unregister')]
    [switch]$Unregister,

    [Parameter(Mandatory, ParameterSetName = 'Status')]
    [switch]$Status
)

$ErrorActionPreference = 'Stop'

function Fail {
    param([string]$Message)
    [Console]::Error.WriteLine($Message)
    exit 1
}

$RepoRoot = $PSScriptRoot
$queueScript = Join-Path $RepoRoot 'Invoke-AiDispatchQueue.ps1'

# --- Status ----------------------------------------------------------------
if ($Status) {
    $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if (-not $task) {
        Write-Output "Scheduled task '$TaskName' is not registered."
        exit 0
    }
    $info = Get-ScheduledTaskInfo -TaskName $TaskName
    $act = @($task.Actions)[0]
    Write-Output "Task:      $TaskName"
    Write-Output "State:     $($task.State)"
    Write-Output "Last run:  $($info.LastRunTime)  (result 0x$('{0:X}' -f $info.LastTaskResult))"
    Write-Output "Next run:  $($info.NextRunTime)"
    Write-Output "Runs:      $($act.Execute) $($act.Arguments)"
    exit 0
}

# --- Unregister ------------------------------------------------------------
if ($Unregister) {
    $task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    if (-not $task) {
        Write-Output "Scheduled task '$TaskName' is not registered; nothing to remove."
        exit 0
    }
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    Write-Output "Scheduled task '$TaskName' removed."
    exit 0
}

# --- Register --------------------------------------------------------------
$autoScript = Join-Path $RepoRoot 'Invoke-AiDispatchAuto.ps1'
$externalScratchArg = if ($CodexExecutorExternalScratch) { ' -CodexExecutorExternalScratch' } else { '' }
if ($CodexExecutorExternalScratch -and $Executor -ne 'codex') {
    Fail "-CodexExecutorExternalScratch is only valid with -Executor codex; it does not apply to Claude execution."
}
if ($Autonomous) {
    if (-not (Test-Path -LiteralPath $autoScript)) {
        Fail "Autonomous driver not found next to this script: $autoScript"
    }
    $targetScript = $autoScript
    $scriptArgs = (' -PublishMode {0} -MaxAutonomousTasks {1} -MaxPlanRevisions {2} -MaxCorrectionRounds {3} -Executor {4}{5}' -f $PublishMode, $MaxAutonomousTasks, $MaxPlanRevisions, $MaxCorrectionRounds, $Executor, $externalScratchArg)
    $modeLine = "autonomous driver - Codex selects tasks (publish=$PublishMode, cap=$MaxAutonomousTasks, executor=$Executor)"
} else {
    if (-not (Test-Path -LiteralPath $queueScript)) {
        Fail "Queue script not found next to this script: $queueScript"
    }
    $targetScript = $queueScript
    $scriptArgs = (' -MaxPlanRevisions {0} -MaxCorrectionRounds {1} -Executor {2}{3}' -f $MaxPlanRevisions, $MaxCorrectionRounds, $Executor, $externalScratchArg)
    $modeLine = "issue queue - runs human-labelled ai-dispatch issues (executor=$Executor)"
}

$action = New-ScheduledTaskAction -Execute 'powershell.exe' `
    -Argument ('-NoProfile -ExecutionPolicy Bypass -File "{0}"{1}' -f $targetScript, $scriptArgs) `
    -WorkingDirectory $RepoRoot

# Repeat every $IntervalMinutes, effectively indefinitely. Building the
# Repetition from a nested trigger supplies a long RepetitionDuration without
# tripping the TimeSpan.MaxValue bug in older Schedule cmdlet builds.
$startAt = (Get-Date).AddMinutes(2)
$trigger = New-ScheduledTaskTrigger -Once -At $startAt
$trigger.Repetition = (New-ScheduledTaskTrigger -Once -At $startAt `
        -RepetitionInterval (New-TimeSpan -Minutes $IntervalMinutes) `
        -RepetitionDuration (New-TimeSpan -Days 3650)).Repetition

$settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -MultipleInstances IgnoreNew `
    -ExecutionTimeLimit (New-TimeSpan -Hours $MaxRunHours) `
    -DontStopOnIdleEnd `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries

$currentUser = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
$principal = New-ScheduledTaskPrincipal -UserId $currentUser `
    -LogonType Interactive -RunLevel Limited

try {
    Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger `
        -Settings $settings -Principal $principal -Force `
        -Description "RGE AI dispatch automation - $modeLine, every $IntervalMinutes min." | Out-Null
} catch {
    Fail "Could not register scheduled task '$TaskName': $($_.Exception.Message)"
}

Write-Output "Scheduled task '$TaskName' registered."
Write-Output "  Mode:    $modeLine"
Write-Output "  Runs:    powershell.exe -File $targetScript$scriptArgs"
Write-Output "  Every:   $IntervalMinutes minute(s), starting $($startAt.ToString('yyyy-MM-dd HH:mm'))"
Write-Output "  As user: $currentUser (Interactive; runs while logged on)"
Write-Output ''
Write-Output "Inspect: .\Register-AiDispatchSchedule.ps1 -Status"
Write-Output "Remove:  .\Register-AiDispatchSchedule.ps1 -Unregister"
Write-Output "Run now: Start-ScheduledTask -TaskName '$TaskName'"
