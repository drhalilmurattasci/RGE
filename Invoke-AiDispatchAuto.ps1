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

$script:AutoLockPath = Join-Path $env:TEMP 'rge-ai-dispatch-auto.lock'
$script:AutoLockHeld = $false

function Release-AutoLock {
    if ($script:AutoLockHeld) {
        Remove-Item -LiteralPath $script:AutoLockPath -Force -ErrorAction SilentlyContinue
        $script:AutoLockHeld = $false
    }
}

function Acquire-AutoLock {
    # Atomically create the auto-driver lock (FileMode.CreateNew fails if it
    # already exists) so two ticks cannot both select and file the same task.
    # A stale lock whose owner process is gone is replaced; a live owner means
    # skip this tick.
    $ownerStart = [long]0
    $self = Get-Process -Id $PID -ErrorAction SilentlyContinue
    if ($self) { try { $ownerStart = [long]$self.StartTime.Ticks } catch { } }
    $content = "pid=$PID procstart=$ownerStart at=$((Get-Date).ToString('o'))"
    for ($attempt = 0; $attempt -lt 2; $attempt++) {
        try {
            $fs = [System.IO.File]::Open($script:AutoLockPath,
                [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write,
                [System.IO.FileShare]::None)
            try {
                $bytes = [System.Text.Encoding]::UTF8.GetBytes($content)
                $fs.Write($bytes, 0, $bytes.Length)
            } finally { $fs.Close() }
            $script:AutoLockHeld = $true
            return $true
        } catch [System.IO.IOException] {
            $raw = (Get-Content -Raw -LiteralPath $script:AutoLockPath -ErrorAction SilentlyContinue)
            $lpid = 0
            $lstart = [long]0
            if ($raw -match 'pid=(\d+)')       { $lpid = [int]$matches[1] }
            if ($raw -match 'procstart=(\d+)') { $lstart = [long]$matches[1] }
            $alive = $false
            if ($lpid -gt 0) {
                $lp = Get-Process -Id $lpid -ErrorAction SilentlyContinue
                if ($lp) {
                    try { $alive = ($lstart -eq 0) -or ($lp.StartTime.Ticks -eq $lstart) }
                    catch { $alive = $true }
                }
            }
            if ($alive) { return $false }
            Remove-Item -LiteralPath $script:AutoLockPath -Force -ErrorAction SilentlyContinue
        }
    }
    return $false
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

# Serialize autonomous ticks: without this, two overlapping ticks could both
# see an empty queue and both file the same Codex-selected task.
if (-not $DryRun) {
    if (-not (Acquire-AutoLock)) {
        Write-Output "Another autonomous dispatch tick is already running; skipping this tick."
        exit 0
    }
}

try {
# --- 1. Halt checks --------------------------------------------------------

$haltSentinel = Join-Path $script:RepoRoot '.ai\dispatch.auto-halt'
if (Test-Path -LiteralPath $haltSentinel) {
    Write-Output ''
    Write-Output "HALTED: a prior tick recorded a fault in $haltSentinel."
    $haltText = (Get-Content -Raw -LiteralPath $haltSentinel -ErrorAction SilentlyContinue)
    if ($haltText) { Write-Output "  $($haltText.Trim())" }
    Write-Output "Investigate, then delete that file to resume."
    exit 0
}

$failedAuto = Get-IssuesJson @(
    'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
    '--label', $failLabel, '--state', 'all', '--limit', '100',
    '--json', 'number,title')
if ($failedAuto.Count -gt 0) {
    $f = $failedAuto[0]
    Write-Output ''
    Write-Output "HALTED: autonomous task #$($f.number) ('$($f.title)') is marked '$failLabel'."
    Write-Output "Review it, then remove the '$failLabel' label to resume (closing the issue alone does not clear the halt)."
    exit 0
}

# --- 2. Is the queue already holding work? ---------------------------------
# Existing queued work is always drained. The task cap gates only the
# creation of NEW autonomous tasks, so an already-filed task is never
# stranded behind the cap.

$openQueue = Get-IssuesJson @(
    'issue', 'list', '--repo', $repoSlug, '--label', $queueLabel,
    '--state', 'open', '--limit', '100', '--json', 'number,title')

if ($openQueue.Count -gt 0) {
    Write-Output "Queue already has $($openQueue.Count) pending '$queueLabel' issue(s); draining it, selecting nothing this tick."
} else {
    # --- 3. Cap check (gates NEW task selection only) ----------------------

    $allAuto = Get-IssuesJson @(
        'issue', 'list', '--repo', $repoSlug, '--label', $autoLabel,
        '--state', 'all', '--limit', '200', '--json', 'number')
    if ($allAuto.Count -ge $MaxAutonomousTasks) {
        Write-Output ''
        Write-Output "HALTED for review: autonomous task cap reached ($($allAuto.Count) of $MaxAutonomousTasks). Queue is empty; nothing to drain."
        Write-Output "Re-run with a higher -MaxAutonomousTasks to continue."
        exit 0
    }

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
    # Deterministic arming check: while the brief carries the UNARMED marker
    # the loop selects nothing -- no reliance on Codex interpreting prose.
    if ($brief -match '(?m)^\s*DISPATCH-TASKS-UNARMED\s*$') {
        Write-Output "Task brief $briefPath carries the DISPATCH-TASKS-UNARMED marker; the autonomous loop is not armed. Nothing selected."
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
    # Suffix-anchor: the task block must be the very end of Codex's reply, not
    # something quoted earlier in its reasoning.
    if (([string]$codexOut).TrimEnd() -notmatch '<<<AUTO_TASK_END>>>\s*$') {
        Fail "Codex's task block is not at the end of its reply (suspect quoted/echoed text). See $codexLog"
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
if ($queueExit -ne 0) {
    # A non-zero queue exit means the tick could not be cleanly finalized
    # (e.g. a terminal failure that could not be labelled). Record a durable
    # halt so the next scheduled tick does not barrel on.
    Write-Utf8 $haltSentinel "Autonomous loop halted: dispatch queue tick exited $queueExit at $((Get-Date).ToString('o')). Investigate, then delete this file to resume."
    Write-Output "Wrote halt sentinel $haltSentinel; the autonomous loop is paused until you delete it."
}
if ($PublishMode -eq 'branch') {
    Write-Output 'Branch mode: a passed task stays on its ai-dispatch/ISSUE-* branch for you to review and merge.'
} else {
    Write-Output 'Main mode: a passed task was fast-forwarded onto origin/main.'
}
exit $queueExit
} finally {
    Release-AutoLock
}
