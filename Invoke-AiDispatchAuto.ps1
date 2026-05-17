#Requires -Version 5.1
<#
.SYNOPSIS
    Autonomous AI dispatch driver: Codex selects the next task, the hardened
    dispatch queue runs it. One task per invocation -- schedule it for a
    continuous, self-restarting loop.

.DESCRIPTION
    This is the "Codex decides what to do" layer on top of
    Invoke-AiDispatchQueue.ps1. Each tick:

      1. Halt check  - if any prior autonomous task carries 'ai-dispatch-
                       failed', stop and do nothing until a human clears it.
      2. Cap check   - stop once -MaxAutonomousTasks 'ai-auto' issues exist,
                       so a human reviews each batch before more run.
      3. Select      - when no 'ai-dispatch' issue is pending, Codex reads the
                       task brief (.ai/dispatch.tasks.md), picks the next
                       task, and a GitHub issue is filed for it (labels
                       'ai-dispatch' + 'ai-auto'). Codex picks the WHAT; the
                       issue is an internal record, not a human gate.
      4. Run         - Invoke-AiDispatchQueue.ps1 runs the pending issue
                       through the full hardened path: Codex plan -> Claude
                       gate -> Claude execute -> verification gate -> Codex
                       control -> publish.

    -PublishMode chooses what happens to a passed task:
      branch (default) - work stays on its ai-dispatch/ISSUE-* branch and the
                         issue stays open; a human reviews and merges it.
      main             - the queue fast-forwards origin/main automatically.

    The loop is INERT until .ai/dispatch.tasks.md is populated with real
    tasks; an empty or instructions-only brief selects nothing.

.PARAMETER PublishMode
    'branch' (default, human-gated publish) or 'main' (auto-publish).

.PARAMETER MaxAutonomousTasks
    Halt for human review once this many 'ai-auto' issues exist. Default 5.
    Raise it (or re-register the schedule with a higher value) to continue.

.PARAMETER TaskBrief
    Path to the task-selection brief. Default .ai/dispatch.tasks.md.

.PARAMETER DryRun
    Report the halt/cap state and the task Codex would select; create no
    issue and run no dispatch.

.EXAMPLE
    .\Invoke-AiDispatchAuto.ps1 -DryRun
    .\Invoke-AiDispatchAuto.ps1                      # branch mode (default)
    .\Invoke-AiDispatchAuto.ps1 -PublishMode main    # auto-publish mode

.NOTES
    Requires git, gh (authenticated), codex, powershell.exe, and
    Invoke-AiDispatchQueue.ps1 in the repo root.
#>
[CmdletBinding()]
param(
    [ValidateSet('branch', 'main')]
    [string]$PublishMode = 'branch',

    [ValidateRange(1, 100)]
    [int]$MaxAutonomousTasks = 5,

    [string]$TaskBrief = '',

    [switch]$DryRun
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

function Write-Utf8 {
    param([string]$Path, [string]$Text)
    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Invoke-Tool {
    # Run a native command with PS 5.1 EAP isolation (native stderr under
    # EAP=Stop becomes a terminating error). Returns @{ Code; Text }.
    param([string]$Exe, [string[]]$CmdArgs)
    $tmp = [System.IO.Path]::GetTempFileName()
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & $Exe @CmdArgs > $tmp 2>&1
    } finally {
        $ErrorActionPreference = $prev
    }
    $code = $LASTEXITCODE
    $text = (Get-Content -Raw -LiteralPath $tmp -ErrorAction SilentlyContinue)
    if ($null -eq $text) { $text = '' }
    Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
    return [pscustomobject]@{ Code = $code; Text = $text }
}

function Get-IssuesJson {
    # gh issue list --json ... -> array. PS 5.1 ConvertFrom-Json yields a
    # single non-enumerated object for a JSON array, so wrap before returning.
    param([string[]]$GhArgs)
    $r = Invoke-Tool -Exe 'gh' -CmdArgs $GhArgs
    if ($r.Code -ne 0) {
        Fail "gh issue list failed (exit $($r.Code)):`n$($r.Text)"
    }
    $items = @()
    if ($r.Text -and $r.Text.Trim()) {
        try { $parsed = $r.Text | ConvertFrom-Json }
        catch { Fail "Could not parse gh issue JSON: $($_.Exception.Message)" }
        if ($null -ne $parsed) { $items = @($parsed) }
    }
    return $items
}

function Get-BlockText {
    # Extract the text between two sentinel lines from free-form model output.
    # Last occurrence wins, so a sentinel echoed in reasoning cannot mask the
    # real answer block.
    param([string]$Text, [string]$BeginMark, [string]$EndMark)
    $pattern = [regex]::Escape($BeginMark) + '(.*?)' + [regex]::Escape($EndMark)
    $blocks = [regex]::Matches([string]$Text, $pattern,
        [System.Text.RegularExpressions.RegexOptions]::Singleline)
    if ($blocks.Count -gt 0) {
        return $blocks[$blocks.Count - 1].Groups[1].Value.Trim()
    }
    return ''
}

# --- Environment -----------------------------------------------------------

$script:RepoRoot = $PSScriptRoot
Set-Location -LiteralPath $script:RepoRoot

Require-Command git
Require-Command gh
Require-Command codex
Require-Command powershell.exe

$queueScript = Join-Path $script:RepoRoot 'Invoke-AiDispatchQueue.ps1'
if (-not (Test-Path -LiteralPath $queueScript)) {
    Fail "Dispatch queue script not found: $queueScript"
}

$briefPath = if ($TaskBrief) {
    if ([System.IO.Path]::IsPathRooted($TaskBrief)) { $TaskBrief }
    else { Join-Path $script:RepoRoot $TaskBrief }
} else {
    Join-Path $script:RepoRoot '.ai\dispatch.tasks.md'
}

$auth = Invoke-Tool -Exe 'gh' -CmdArgs @('auth', 'status')
if ($auth.Code -ne 0) {
    Fail "gh is not authenticated. Run 'gh auth login' first.`n$($auth.Text)"
}

$originUrl = (Invoke-Tool -Exe 'git' -CmdArgs @('remote', 'get-url', 'origin')).Text.Trim()
if ($originUrl -notmatch 'github\.com[:/](.+?)(?:\.git)?/?$') {
    Fail "Could not parse an owner/name slug from origin URL: $originUrl"
}
$repoSlug = $matches[1]

$queueLabel = 'ai-dispatch'
$autoLabel  = 'ai-auto'
$failLabel  = 'ai-dispatch-failed'

Write-Output "Autonomous dispatch tick - repo $repoSlug"
Write-Output "Publish mode: $PublishMode   Task cap: $MaxAutonomousTasks"

# --- 1. Halt check: did a prior autonomous task fail? ----------------------

$failedAuto = Get-IssuesJson @(
    'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
    '--label', $failLabel, '--state', 'all', '--limit', '100',
    '--json', 'number,title')
if ($failedAuto.Count -gt 0) {
    $f = $failedAuto[0]
    Write-Output ''
    Write-Output "HALTED: autonomous task #$($f.number) ('$($f.title)') is marked '$failLabel'."
    Write-Output "Review it, then remove the '$failLabel' label (or close the issue) to resume."
    exit 0
}

# --- 2. Cap check ----------------------------------------------------------

$allAuto = Get-IssuesJson @(
    'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
    '--state', 'all', '--limit', '200', '--json', 'number')
if ($allAuto.Count -ge $MaxAutonomousTasks) {
    Write-Output ''
    Write-Output "HALTED: autonomous task cap reached ($($allAuto.Count) of $MaxAutonomousTasks)."
    Write-Output "Review the batch, then re-run with a higher -MaxAutonomousTasks to continue."
    exit 0
}

# --- 3. Is the queue already holding work? ---------------------------------

$openQueue = Get-IssuesJson @(
    'issue', 'list', '--repo', $repoSlug, '--label', $queueLabel,
    '--state', 'open', '--limit', '100', '--json', 'number,title')

if ($openQueue.Count -gt 0) {
    Write-Output "Queue already has $($openQueue.Count) pending '$queueLabel' issue(s); selecting nothing this tick."
} else {
    # --- 4. Select the next task with Codex --------------------------------

    if (-not (Test-Path -LiteralPath $briefPath)) {
        Write-Output ''
        Write-Output "No task brief at $briefPath - nothing to select. Create it to arm the loop."
        exit 0
    }
    $brief = Get-Content -Raw -LiteralPath $briefPath
    if (-not $brief -or -not $brief.Trim()) {
        Write-Output "Task brief $briefPath is empty; nothing to select."
        exit 0
    }

    $doneAuto = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
        '--state', 'all', '--limit', '200', '--json', 'number,title,state')
    $doneList = if ($doneAuto.Count -gt 0) {
        ($doneAuto | ForEach-Object { "- #$($_.number) [$($_.state)] $($_.title)" }) -join "`n"
    } else { '(none yet)' }

    $selectPrompt = @"
You are Planner / OpenAI Codex selecting the next task for an automated RGE
dispatch loop. Read only; do not edit any file.

TASK BRIEF (the authorized source of work):
---
$brief
---

AUTONOMOUS TASKS ALREADY FILED (do not repeat any of these):
$doneList

Choose exactly ONE next task to dispatch now. Prefer the brief's order, but
pick an earlier-in-dependency task first if it is a prerequisite ("sequence
necessity"). The task must be small, bounded, and independently shippable.

If the brief contains no real tasks yet (only instructions, placeholders, or
examples), or every real task is already filed/complete, respond with exactly
this single line and nothing else:
AUTO_SELECTION: none

Otherwise respond with exactly this block as the last thing in your reply:
<<<AUTO_TASK_BEGIN>>>
TITLE: <one concise imperative line, 70 chars or fewer>
BODY:
<2 to 8 lines: the goal, the in-scope files or areas, and the done-criteria.
This text becomes the dispatch goal that Codex plans and Claude executes.>
<<<AUTO_TASK_END>>>
"@

    $promptFile  = Join-Path $env:TEMP 'rge-ai-auto-select-prompt.txt'
    $codexLog    = Join-Path $env:TEMP 'rge-ai-auto-select.log'
    $codexAnswer = Join-Path $env:TEMP 'rge-ai-auto-select-answer.txt'
    Write-Utf8 $promptFile $selectPrompt
    Remove-Item -LiteralPath $codexAnswer -Force -ErrorAction SilentlyContinue

    Write-Output ''
    Write-Output 'Queue is empty; asking Codex to select the next task...'
    # --output-last-message captures ONLY Codex's final message. Scanning the
    # full transcript instead would match the sentinel block echoed from this
    # very prompt and mistake the template placeholder for a real selection.
    $codexArgs = @('exec', '--cd', $script:RepoRoot, '--sandbox', 'read-only',
        '--output-last-message', $codexAnswer, '-')
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        Get-Content -Raw -LiteralPath $promptFile | & codex @codexArgs > $codexLog 2>&1
    } finally {
        $ErrorActionPreference = $prevEap
    }
    if ($LASTEXITCODE -ne 0) {
        Fail "codex exec (task selection) failed. See $codexLog"
    }
    $codexOut = (Get-Content -Raw -LiteralPath $codexAnswer -ErrorAction SilentlyContinue)
    if (-not $codexOut -or -not ([string]$codexOut).Trim()) {
        # Fallback only: no last-message file. The placeholder guard below
        # still protects against the echoed-prompt sentinel collision.
        $codexOut = (Get-Content -Raw -LiteralPath $codexLog -ErrorAction SilentlyContinue)
    }
    if ($null -eq $codexOut) { $codexOut = '' }

    $block = Get-BlockText -Text $codexOut -BeginMark '<<<AUTO_TASK_BEGIN>>>' -EndMark '<<<AUTO_TASK_END>>>'
    if (-not $block) {
        if ($codexOut -match '(?im)^\s*AUTO_SELECTION:\s*none\b') {
            Write-Output 'Codex reports no real task to select (brief empty/placeholder, or all tasks done).'
            exit 0
        }
        Fail "Codex did not return a parseable task block. See $codexLog"
    }

    $titleMatch = [regex]::Match($block, '(?im)^\s*TITLE:\s*(.+?)\s*$')
    if (-not $titleMatch.Success) {
        Fail "Codex task block has no TITLE line. See $codexLog"
    }
    $taskTitle = $titleMatch.Groups[1].Value.Trim()
    $bodyMatch = [regex]::Match($block, '(?is)\bBODY:\s*(.+)$')
    $taskBody = if ($bodyMatch.Success) { $bodyMatch.Groups[1].Value.Trim() } else { $taskTitle }

    # Guard: reject a prompt-template placeholder echoed back instead of a real
    # selection (e.g. a value still wrapped in <angle brackets>).
    if (-not $taskTitle -or
        ($taskTitle.StartsWith('<') -and $taskTitle.EndsWith('>')) -or
        ($taskBody.StartsWith('<') -and $taskBody.EndsWith('>'))) {
        Fail "Codex returned a prompt placeholder, not a real task selection. See $codexLog`nThe task brief probably has no real tasks yet."
    }

    Write-Output ''
    Write-Output 'Codex selected:'
    Write-Output "  Title: $taskTitle"

    if ($DryRun) {
        Write-Output ''
        Write-Output '--- task body ---'
        Write-Output $taskBody
        Write-Output '--- end ---'
        Write-Output ''
        Write-Output 'DryRun: no issue created, queue not run.'
        exit 0
    }

    # Ensure both labels exist (idempotent), then file the task issue.
    Invoke-Tool -Exe 'gh' -CmdArgs @(
        'label', 'create', $queueLabel, '--repo', $repoSlug,
        '--color', '0e8a16', '--description', 'Queued for the AI dispatch loop',
        '--force') | Out-Null
    Invoke-Tool -Exe 'gh' -CmdArgs @(
        'label', 'create', $autoLabel, '--repo', $repoSlug,
        '--color', '1d76db', '--description', 'Task selected by the autonomous dispatch driver',
        '--force') | Out-Null

    $briefName = Split-Path -Leaf $briefPath
    $issueBody = "$taskBody`r`n`r`n_Filed automatically by Invoke-AiDispatchAuto.ps1 - Codex-selected from $briefName._"
    $bodyFile = Join-Path $env:TEMP 'rge-ai-auto-issue-body.txt'
    Write-Utf8 $bodyFile $issueBody
    $created = Invoke-Tool -Exe 'gh' -CmdArgs @(
        'issue', 'create', '--repo', $repoSlug, '--title', $taskTitle,
        '--body-file', $bodyFile, '--label', $queueLabel, '--label', $autoLabel)
    Remove-Item -LiteralPath $bodyFile -Force -ErrorAction SilentlyContinue
    if ($created.Code -ne 0) {
        Fail "Could not create the autonomous task issue (exit $($created.Code)):`n$($created.Text)"
    }
    Write-Output "Filed autonomous task issue: $($created.Text.Trim())"
}

if ($DryRun) {
    Write-Output ''
    Write-Output 'DryRun: queue not run.'
    exit 0
}

# --- 5. Drain: run the hardened queue on the pending issue -----------------

Write-Output ''
Write-Output "Running the dispatch queue ($PublishMode mode)..."
Write-Output '================================================================'
$queueArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $queueScript)
if ($PublishMode -eq 'branch') { $queueArgs += '-NoPublish' }

$prevEap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$global:LASTEXITCODE = 0
try {
    & powershell.exe @queueArgs
} finally {
    $ErrorActionPreference = $prevEap
}
$queueExit = $LASTEXITCODE
Write-Output '================================================================'
Write-Output "Dispatch queue exited with code $queueExit."
if ($PublishMode -eq 'branch') {
    Write-Output 'Branch mode: a passed task stays on its ai-dispatch/ISSUE-* branch for you to review and merge.'
} else {
    Write-Output 'Main mode: a passed task was fast-forwarded onto origin/main.'
}
exit $queueExit
