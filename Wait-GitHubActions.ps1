#Requires -Version 5.1
<#
.SYNOPSIS
    Wait for GitHub Actions runs for one commit without open-ended polling.

.DESCRIPTION
    Read-only helper for the publish/review lane. It polls `gh run list`,
    filters to a single commit SHA, keeps only the latest run per workflow
    name, and exits when all expected workflows are completed.

    Exit codes:
      0  all expected workflows completed with success/skipped/neutral
      1  at least one expected workflow completed with a failing conclusion
      2  timeout before all expected workflows reached a terminal state

    The timeout is checked before each poll and before each sleep. Sleep is
    capped to the remaining time, so the command cannot drift minutes past
    the requested deadline because of a long poll interval.

.PARAMETER Repo
    GitHub repository in owner/name form. Defaults to `gh repo view`.

.PARAMETER Commit
    Commit SHA to wait on. Defaults to `git rev-parse HEAD`.

.PARAMETER Branch
    Branch passed to `gh run list --branch`. Defaults to the current branch.

.PARAMETER WorkflowName
    Workflow names to wait for. Defaults to the `name:` values in
    `.github/workflows/*.yml`. Pass extra names such as `Push on main` if
    repository-level workflows not tracked in this repo should be included.

.PARAMETER TimeoutMinutes
    Maximum wall-clock wait. Default 30 minutes.

.PARAMETER PollSeconds
    Poll interval. The final sleep is shortened to respect the timeout.

.EXAMPLE
    .\Wait-GitHubActions.ps1

.EXAMPLE
    .\Wait-GitHubActions.ps1 -Commit 826a9e8 -TimeoutMinutes 20 -PollSeconds 15

.EXAMPLE
    .\Wait-GitHubActions.ps1 -WorkflowName @(
        'Format check',
        'Architecture lints',
        'Supply chain (cargo-deny)',
        'Workspace tests',
        'Script bench compile',
        'Push on main'
    )
#>

[CmdletBinding()]
param(
    [string]$Repo,
    [string]$Commit,
    [string]$Branch,
    [string[]]$WorkflowName,
    [ValidateRange(1, 240)]
    [int]$TimeoutMinutes = 30,
    [ValidateRange(5, 300)]
    [int]$PollSeconds = 30,
    [ValidateRange(20, 500)]
    [int]$Limit = 100
)

$ErrorActionPreference = 'Stop'

function Invoke-JsonCommand {
    param(
        [Parameter(Mandatory)]
        [string]$Command,
        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    $output = & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
    if (-not $output) {
        return $null
    }
    return ($output | ConvertFrom-Json)
}

function Get-DefaultWorkflowNames {
    $workflowDir = Join-Path (Get-Location).Path '.github/workflows'
    if (-not (Test-Path -LiteralPath $workflowDir)) {
        return @()
    }

    $names = New-Object System.Collections.Generic.List[string]
    Get-ChildItem -LiteralPath $workflowDir -File -Include '*.yml','*.yaml' | Sort-Object Name | ForEach-Object {
        $line = Select-String -LiteralPath $_.FullName -Pattern '^\s*name\s*:\s*(.+?)\s*$' | Select-Object -First 1
        if ($line) {
            $name = $line.Matches[0].Groups[1].Value.Trim()
            $name = $name.Trim('"', "'")
            if ($name) {
                [void]$names.Add($name)
            }
        }
    }
    return @($names)
}

function Format-RunTable {
    param([object[]]$Rows)
    $Rows |
        Sort-Object Name |
        Select-Object Name, Status, Conclusion, RunId, Url |
        Format-Table -AutoSize |
        Out-String
}

if (-not $Repo) {
    $repoInfo = Invoke-JsonCommand -Command 'gh' -Arguments @('repo', 'view', '--json', 'nameWithOwner')
    if (-not $repoInfo -or -not $repoInfo.nameWithOwner) {
        throw 'Could not infer GitHub repo. Pass -Repo owner/name.'
    }
    $Repo = $repoInfo.nameWithOwner
}

if (-not $Commit) {
    $Commit = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $Commit) {
        throw 'Could not infer commit. Pass -Commit.'
    }
}

if (-not $Branch) {
    $Branch = (& git branch --show-current).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $Branch) {
        throw 'Could not infer branch. Pass -Branch.'
    }
}

if (-not $WorkflowName -or $WorkflowName.Count -eq 0) {
    $WorkflowName = Get-DefaultWorkflowNames
}

$expected = @($WorkflowName | Where-Object { $_ } | Select-Object -Unique)
if ($expected.Count -eq 0) {
    Write-Warning 'No expected workflow names found. Waiting for all observed workflows for this commit.'
}

$deadline = (Get-Date).AddMinutes($TimeoutMinutes)
$allowedSuccess = @('success', 'skipped', 'neutral')

Write-Output ("Waiting for GitHub Actions: repo={0} branch={1} commit={2} timeout={3}m poll={4}s" -f $Repo, $Branch, $Commit, $TimeoutMinutes, $PollSeconds)
if ($expected.Count -gt 0) {
    Write-Output ("Expected workflows: {0}" -f ($expected -join ', '))
}

while ($true) {
    $now = Get-Date
    if ($now -ge $deadline) {
        Write-Output "TIMEOUT before poll deadline."
        exit 2
    }

    $runs = Invoke-JsonCommand -Command 'gh' -Arguments @(
        'run', 'list',
        '--repo', $Repo,
        '--branch', $Branch,
        '--limit', [string]$Limit,
        '--json', 'databaseId,name,workflowName,status,conclusion,headSha,createdAt,url'
    )

    $matchingRuns = @($runs | Where-Object { $_.headSha -eq $Commit })
    $sortedMatchingRuns = @($matchingRuns | Sort-Object @{ Expression = { [datetime]$_.createdAt }; Descending = $true })
    $latestByName = @{}
    foreach ($run in $sortedMatchingRuns) {
        if (-not $latestByName.ContainsKey($run.name)) {
            $latestByName[$run.name] = $run
        }
    }

    if ($expected.Count -gt 0) {
        $namesToCheck = $expected
    } else {
        $namesToCheck = @($latestByName.Keys | Sort-Object)
    }

    $rows = New-Object System.Collections.Generic.List[object]
    foreach ($name in $namesToCheck) {
        $run = $sortedMatchingRuns |
            Where-Object { $_.workflowName -eq $name -or $_.name -eq $name } |
            Select-Object -First 1
        if ($run) {
            [void]$rows.Add([pscustomobject]@{
                Name       = $run.name
                Status     = $run.status
                Conclusion = $run.conclusion
                RunId      = $run.databaseId
                Url        = $run.url
            })
        } else {
            [void]$rows.Add([pscustomobject]@{
                Name       = $name
                Status     = 'missing'
                Conclusion = ''
                RunId      = ''
                Url        = ''
            })
        }
    }

    Write-Output (Format-RunTable -Rows $rows.ToArray())

    $missing = @($rows | Where-Object { $_.Status -eq 'missing' })
    $pending = @($rows | Where-Object { $_.Status -ne 'missing' -and $_.Status -ne 'completed' })
    $failed = @($rows | Where-Object {
        $_.Status -eq 'completed' -and $_.Conclusion -notin $allowedSuccess
    })

    if ($failed.Count -gt 0) {
        Write-Output "FAILED workflow(s): $($failed.Name -join ', ')"
        exit 1
    }

    if ($missing.Count -eq 0 -and $pending.Count -eq 0 -and $rows.Count -gt 0) {
        Write-Output "All expected GitHub Actions completed successfully."
        exit 0
    }

    $remainingSeconds = [int][Math]::Floor(($deadline - (Get-Date)).TotalSeconds)
    if ($remainingSeconds -le 0) {
        Write-Output "TIMEOUT before sleep deadline."
        exit 2
    }

    $sleepSeconds = [Math]::Min($PollSeconds, $remainingSeconds)
    Write-Output ("Waiting: missing={0} pending={1}; sleeping {2}s (deadline {3:o})" -f $missing.Count, $pending.Count, $sleepSeconds, $deadline)
    Start-Sleep -Seconds $sleepSeconds
}
