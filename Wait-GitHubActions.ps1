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

    PR-side external checks (ISSUE-241). Some org/default checks -- the
    canonical example is the org-managed CodeQL workflow -- are not visible
    through `gh run list` but do appear on the pull request rollup
    (`gh pr view <PR> --json statusCheckRollup`). The waiter exposes an
    explicit opt-in for accepting one or more named PR-side checks:

        .\Wait-GitHubActions.ps1 -PullRequest 240 -AcceptPrSideCheck CodeQL

    Strict rules apply:
      * The opt-in only relaxes resolution for the explicitly named checks.
        Tracked workflows discovered through `gh run list` are still required
        to pass under the default semantic.
      * Each named PR-side check is accepted only when the PR rollup's head
        commit SHA equals the script's `-Commit` argument.
      * Only an explicit `SUCCESS` conclusion is accepted as pass. Failed,
        cancelled, skipped, timed-out, neutral, or otherwise non-success
        conclusions are not accepted as pass.
      * A failing conclusion on an accepted PR-side check fails the wait
        with exit code 1.
      * A missing PR-side check, a PR head SHA mismatch, or a check still
        in progress keeps the waiter polling and ultimately surfaces as
        exit code 2 if the timeout fires before resolution.

.PARAMETER Repo
    GitHub repository in owner/name form. Defaults to the GitHub slug parsed
    from `origin`; falls back to `gh repo view`.

.PARAMETER Commit
    Commit SHA to wait on. Defaults to `git rev-parse HEAD`. An abbreviated
    SHA is accepted: it is matched as a prefix of the full 40-char SHA
    reported by `gh run list` / `gh pr view`.

.PARAMETER Branch
    Branch passed to `gh run list --branch`. Defaults to the current branch.

.PARAMETER WorkflowName
    Workflow names to wait for. Defaults to the `name:` values in
    `.github/workflows/*.yml`. Pass extra names such as `Push on main` if
    repository-level workflows not tracked in this repo should be included.

.PARAMETER PullRequest
    Pull request number whose `statusCheckRollup` may be consulted for
    explicitly named external checks (see `-AcceptPrSideCheck`). Without
    `-AcceptPrSideCheck`, this parameter has no effect and PR rollups are
    not queried.

.PARAMETER AcceptPrSideCheck
    Names of external checks that may be satisfied through the
    pull-request `statusCheckRollup` for `-PullRequest`. Each name is
    added to the expected wait set automatically -- callers do not need
    to repeat the name in `-WorkflowName`. Requires `-PullRequest`.
    PR-side evidence is accepted only when the PR head commit SHA
    equals `-Commit` and the named check's conclusion is `SUCCESS`.

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

.EXAMPLE
    # Wait for the seven tracked workflows AND the org/default CodeQL check
    # that only appears on the PR rollup. CodeQL is added to the expected
    # set automatically; it does not need to be repeated in -WorkflowName.
    .\Wait-GitHubActions.ps1 `
        -Commit (git rev-parse HEAD) `
        -PullRequest 240 `
        -AcceptPrSideCheck CodeQL
#>

[CmdletBinding()]
param(
    [string]$Repo,
    [string]$Commit,
    [string]$Branch,
    [string[]]$WorkflowName,
    [int]$PullRequest,
    [string[]]$AcceptPrSideCheck,
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

function Convert-OriginUrlToGitHubRepoSlug {
    param([AllowNull()][string]$OriginUrl)

    if ([string]::IsNullOrWhiteSpace($OriginUrl)) {
        return $null
    }

    $url = $OriginUrl.Trim()
    if ($url -notmatch 'github\.com[:/](?<slug>[^?#]+?)(?:\.git)?/?$') {
        return $null
    }

    $slug = $Matches.slug.Trim('/')
    $parts = @($slug -split '/')
    if ($parts.Count -ne 2 -or -not $parts[0] -or -not $parts[1]) {
        return $null
    }

    return "$($parts[0])/$($parts[1])"
}

function Resolve-DefaultGitHubRepo {
    $originUrl = (& git remote get-url origin 2>$null | Out-String).Trim()
    if ($LASTEXITCODE -eq 0) {
        $originSlug = Convert-OriginUrlToGitHubRepoSlug -OriginUrl $originUrl
        if ($originSlug) {
            return $originSlug
        }
    }

    $repoInfo = Invoke-JsonCommand -Command 'gh' -Arguments @('repo', 'view', '--json', 'nameWithOwner')
    if (-not $repoInfo -or -not $repoInfo.nameWithOwner) {
        throw 'Could not infer GitHub repo from origin or gh repo view. Pass -Repo owner/name.'
    }
    return $repoInfo.nameWithOwner
}

function Test-ShaMatch {
    <#
    .SYNOPSIS
        True when two commit-SHA strings refer to the same commit.
    .DESCRIPTION
        `gh run list` and `gh pr view` always emit full 40-char SHAs, but
        callers (and this script's own examples) routinely pass an
        abbreviated SHA. Treat the two as matching when they are equal or
        one is a case-insensitive prefix of the other. Empty/null on either
        side never matches, so a run with no headSha is never accepted and
        an empty -Commit never matches everything.
    #>
    [CmdletBinding()]
    param(
        [Parameter()][AllowNull()][string]$Left,
        [Parameter()][AllowNull()][string]$Right
    )
    if ([string]::IsNullOrEmpty($Left) -or [string]::IsNullOrEmpty($Right)) {
        return $false
    }
    $l = $Left.ToLowerInvariant()
    $r = $Right.ToLowerInvariant()
    if ($l.Length -le $r.Length) {
        return $r.StartsWith($l)
    }
    return $l.StartsWith($r)
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

function Get-PrSideCheckRollup {
    <#
    .SYNOPSIS
        Fetch the PR rollup (head SHA + statusCheckRollup) for a pull request.

    .DESCRIPTION
        Returns `$null` if `gh pr view` produces no parsable JSON. Throws
        if the underlying gh call fails for any other reason -- callers
        decide whether to retry on the next poll.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Repo,
        [Parameter(Mandatory)][int]$PullRequest
    )

    return (Invoke-JsonCommand -Command 'gh' -Arguments @(
        'pr', 'view', [string]$PullRequest,
        '--repo', $Repo,
        '--json', 'headRefOid,statusCheckRollup'
    ))
}

function Resolve-PrSideCheckEvidence {
    <#
    .SYNOPSIS
        Classify a single named PR-side check against the expected commit SHA.

    .DESCRIPTION
        Returns a PSCustomObject with these properties:
          State      -- one of: pass, fail, pending, missing, sha-mismatch
          Conclusion -- raw conclusion / state string (or '' / 'missing')
          PrHeadSha  -- the PR rollup head SHA (or '')

        Acceptance rules (ISSUE-241 strict semantic):
          * Accepted only when the PR rollup head SHA equals -Commit.
          * Only an explicit conclusion of SUCCESS counts as `pass`.
          * Skipped / cancelled / failed / timed-out / neutral / action-required
            count as `fail` (terminal but not a pass).
          * In-progress / queued / pending counts as `pending` (keep polling).
          * Missing from the rollup or null rollup counts as `missing`.
    #>
    [CmdletBinding()]
    param(
        [Parameter()][AllowNull()]$Rollup,
        [Parameter(Mandatory)][string]$ExpectedHeadSha,
        [Parameter(Mandatory)][string]$CheckName
    )

    $missing = [pscustomobject]@{
        State      = 'missing'
        Conclusion = 'missing'
        PrHeadSha  = ''
    }

    if (-not $Rollup) { return $missing }

    $prHeadSha = ''
    if ($Rollup.PSObject.Properties.Name -contains 'headRefOid' -and $Rollup.headRefOid) {
        $prHeadSha = [string]$Rollup.headRefOid
    }

    if (-not $prHeadSha) {
        return $missing
    }

    if (-not (Test-ShaMatch -Left $ExpectedHeadSha -Right $prHeadSha)) {
        return [pscustomobject]@{
            State      = 'sha-mismatch'
            Conclusion = 'sha-mismatch'
            PrHeadSha  = $prHeadSha
        }
    }

    $checks = @()
    if ($Rollup.PSObject.Properties.Name -contains 'statusCheckRollup' -and $Rollup.statusCheckRollup) {
        $checks = @($Rollup.statusCheckRollup)
    }
    if ($checks.Count -eq 0) {
        return [pscustomobject]@{
            State      = 'missing'
            Conclusion = 'missing'
            PrHeadSha  = $prHeadSha
        }
    }

    foreach ($check in $checks) {
        if (-not $check) { continue }

        $name = $null
        if ($check.PSObject.Properties.Name -contains 'name' -and $check.name) {
            $name = [string]$check.name
        } elseif ($check.PSObject.Properties.Name -contains 'context' -and $check.context) {
            $name = [string]$check.context
        } elseif ($check.PSObject.Properties.Name -contains 'workflowName' -and $check.workflowName) {
            $name = [string]$check.workflowName
        }
        if (-not $name) { continue }
        if ($name -ne $CheckName) { continue }

        $status = ''
        if ($check.PSObject.Properties.Name -contains 'status' -and $check.status) {
            $status = [string]$check.status
        }
        $conclusion = ''
        if ($check.PSObject.Properties.Name -contains 'conclusion' -and $check.conclusion) {
            $conclusion = [string]$check.conclusion
        }
        if (-not $conclusion -and $check.PSObject.Properties.Name -contains 'state' -and $check.state) {
            $conclusion = [string]$check.state
            # StatusContext entries (`context` + `state`) use the legacy
            # commit-status state vocabulary. PENDING / EXPECTED are
            # unresolved -- they have not reached a terminal conclusion,
            # so the waiter must keep polling instead of misreading them
            # as a failing completion.
            if ($conclusion.ToUpperInvariant() -in 'PENDING','EXPECTED') {
                return [pscustomobject]@{
                    State      = 'pending'
                    Conclusion = $conclusion
                    PrHeadSha  = $prHeadSha
                }
            }
            if (-not $status) { $status = 'COMPLETED' }
        }

        $statusUpper = $status.ToUpperInvariant()
        if ($statusUpper -and $statusUpper -ne 'COMPLETED') {
            return [pscustomobject]@{
                State      = 'pending'
                Conclusion = $status
                PrHeadSha  = $prHeadSha
            }
        }

        if (-not $conclusion) {
            return [pscustomobject]@{
                State      = 'fail'
                Conclusion = ''
                PrHeadSha  = $prHeadSha
            }
        }

        $concUpper = $conclusion.ToUpperInvariant()
        if ($concUpper -eq 'SUCCESS') {
            return [pscustomobject]@{
                State      = 'pass'
                Conclusion = $conclusion
                PrHeadSha  = $prHeadSha
            }
        }

        return [pscustomobject]@{
            State      = 'fail'
            Conclusion = $conclusion
            PrHeadSha  = $prHeadSha
        }
    }

    return [pscustomobject]@{
        State      = 'missing'
        Conclusion = 'missing'
        PrHeadSha  = $prHeadSha
    }
}

function Test-PrSideAcceptanceConfig {
    <#
    .SYNOPSIS
        Validate that -PullRequest / -AcceptPrSideCheck are coherent.

    .DESCRIPTION
        * -AcceptPrSideCheck without a positive -PullRequest is rejected.
        * Returns the deduplicated, non-empty AcceptPrSideCheck list (or @()).

        Callers add the returned names to the expected-workflow set so the
        opt-in works directly without forcing operators to duplicate the
        same name in -WorkflowName.
    #>
    [CmdletBinding()]
    param(
        [int]$PullRequest,
        [string[]]$AcceptPrSideCheck
    )

    $accepted = @($AcceptPrSideCheck | Where-Object { $_ } | Select-Object -Unique)
    if ($accepted.Count -eq 0) {
        return @()
    }

    if (-not $PullRequest -or $PullRequest -le 0) {
        throw '-AcceptPrSideCheck requires -PullRequest <PR#>.'
    }

    return $accepted
}

function Invoke-WaitForGitHubActions {
    <#
    .SYNOPSIS
        Pure entry-point used by the CLI wrapper and by Pester tests.

    .DESCRIPTION
        Returns the intended process exit code (0/1/2) instead of calling
        `exit`. Tests inject -Clock / -Sleeper / -RunListProvider /
        -PrRollupProvider to drive the loop deterministically without
        shelling out to `gh` or sleeping for real.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Repo,
        [Parameter(Mandatory)][string]$Commit,
        [Parameter(Mandatory)][string]$Branch,
        [string[]]$WorkflowName,
        [int]$PullRequest,
        [string[]]$AcceptPrSideCheck,
        [ValidateRange(1, 240)][int]$TimeoutMinutes = 30,
        [ValidateRange(5, 300)][int]$PollSeconds = 30,
        [ValidateRange(20, 500)][int]$Limit = 100,
        [scriptblock]$Clock = { Get-Date },
        [scriptblock]$Sleeper = { param($seconds) Start-Sleep -Seconds $seconds },
        [scriptblock]$RunListProvider,
        [scriptblock]$PrRollupProvider
    )

    $acceptedPrSide = Test-PrSideAcceptanceConfig `
        -PullRequest $PullRequest `
        -AcceptPrSideCheck $AcceptPrSideCheck

    # PR-side acceptance is itself an explicit opt-in for a named check, so
    # the accepted names join the expected wait set automatically. This keeps
    # the advertised `-PullRequest <PR#> -AcceptPrSideCheck <Name>` call
    # working without forcing operators to repeat the name in -WorkflowName.
    $expected = @(@($WorkflowName) + @($acceptedPrSide) |
        Where-Object { $_ } |
        Select-Object -Unique)

    if ($expected.Count -eq 0) {
        Write-Warning 'No expected workflow names found. Waiting for all observed workflows for this commit.'
    }

    $deadline = (& $Clock).AddMinutes($TimeoutMinutes)
    $allowedSuccess = @('success', 'skipped', 'neutral')

    Write-Output ("Waiting for GitHub Actions: repo={0} branch={1} commit={2} timeout={3}m poll={4}s" -f $Repo, $Branch, $Commit, $TimeoutMinutes, $PollSeconds)
    if ($expected.Count -gt 0) {
        Write-Output ("Expected workflows: {0}" -f ($expected -join ', '))
    }
    if ($acceptedPrSide.Count -gt 0) {
        Write-Output ("Accepting PR-side checks for PR #{0}: {1}" -f $PullRequest, ($acceptedPrSide -join ', '))
    }

    while ($true) {
        $now = & $Clock
        if ($now -ge $deadline) {
            Write-Output 'TIMEOUT before poll deadline.'
            return 2
        }

        if ($RunListProvider) {
            $runs = & $RunListProvider $Repo $Branch $Limit
        } else {
            $runs = Invoke-JsonCommand -Command 'gh' -Arguments @(
                'run', 'list',
                '--repo', $Repo,
                '--branch', $Branch,
                '--limit', [string]$Limit,
                '--json', 'databaseId,name,workflowName,status,conclusion,headSha,createdAt,url'
            )
        }

        $matchingRuns = @($runs | Where-Object { Test-ShaMatch -Left $Commit -Right $_.headSha })
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
                    Source     = 'gh-run-list'
                })
            } else {
                [void]$rows.Add([pscustomobject]@{
                    Name       = $name
                    Status     = 'missing'
                    Conclusion = ''
                    RunId      = ''
                    Url        = ''
                    Source     = 'gh-run-list'
                })
            }
        }

        # PR-side resolution for explicitly accepted external checks.
        # Only fires when the corresponding row is `missing` -- this preserves
        # the default strict semantic for any expected workflow that is
        # visible through `gh run list`.
        if ($acceptedPrSide.Count -gt 0) {
            $prSideNeeded = @($rows | Where-Object {
                $_.Status -eq 'missing' -and ($acceptedPrSide -contains $_.Name)
            })
            if ($prSideNeeded.Count -gt 0) {
                $rollup = $null
                $rollupError = $null
                try {
                    if ($PrRollupProvider) {
                        $rollup = & $PrRollupProvider $Repo $PullRequest
                    } else {
                        $rollup = Get-PrSideCheckRollup -Repo $Repo -PullRequest $PullRequest
                    }
                } catch {
                    $rollupError = $_
                    Write-Output ("PR-side rollup fetch failed for PR #{0}: {1}" -f $PullRequest, $_.Exception.Message)
                }

                foreach ($row in $prSideNeeded) {
                    $evidence = Resolve-PrSideCheckEvidence `
                        -Rollup $rollup `
                        -ExpectedHeadSha $Commit `
                        -CheckName $row.Name

                    $row.Source = 'pr-rollup'
                    switch ($evidence.State) {
                        'pass' {
                            $row.Status = 'completed'
                            $row.Conclusion = 'success'
                            $row.Url = ("pr#{0}" -f $PullRequest)
                        }
                        'fail' {
                            $row.Status = 'completed'
                            $row.Conclusion = if ($evidence.Conclusion) { $evidence.Conclusion.ToLowerInvariant() } else { 'failure' }
                            $row.Url = ("pr#{0}" -f $PullRequest)
                        }
                        'pending' {
                            $row.Status = if ($evidence.Conclusion) { $evidence.Conclusion } else { 'in_progress' }
                            $row.Conclusion = ''
                            $row.Url = ("pr#{0}" -f $PullRequest)
                        }
                        'sha-mismatch' {
                            $row.Status = 'missing'
                            $row.Conclusion = ("sha-mismatch (pr={0})" -f $evidence.PrHeadSha)
                        }
                        default {
                            $row.Status = 'missing'
                            if ($rollupError) {
                                $row.Conclusion = 'rollup-error'
                            } else {
                                $row.Conclusion = 'missing-on-pr'
                            }
                        }
                    }
                }
            }
        }

        Write-Output (Format-RunTable -Rows $rows.ToArray())

        $missing = @($rows | Where-Object { $_.Status -eq 'missing' })
        $pending = @($rows | Where-Object { $_.Status -ne 'missing' -and $_.Status -ne 'completed' })
        $failed  = @($rows | Where-Object {
            $_.Status -eq 'completed' -and (
                ($_.Source -eq 'gh-run-list' -and $_.Conclusion -notin $allowedSuccess) -or
                ($_.Source -eq 'pr-rollup'   -and $_.Conclusion -ne 'success')
            )
        })

        if ($failed.Count -gt 0) {
            Write-Output ("FAILED workflow(s): {0}" -f (($failed | ForEach-Object { $_.Name }) -join ', '))
            return 1
        }

        if ($missing.Count -eq 0 -and $pending.Count -eq 0 -and $rows.Count -gt 0) {
            Write-Output 'All expected GitHub Actions completed successfully.'
            return 0
        }

        $remainingSeconds = [int][Math]::Floor(($deadline - (& $Clock)).TotalSeconds)
        if ($remainingSeconds -le 0) {
            Write-Output 'TIMEOUT before sleep deadline.'
            return 2
        }

        $sleepSeconds = [Math]::Min($PollSeconds, $remainingSeconds)
        Write-Output ("Waiting: missing={0} pending={1}; sleeping {2}s (deadline {3:o})" -f $missing.Count, $pending.Count, $sleepSeconds, $deadline)
        & $Sleeper $sleepSeconds
    }
}

if ($env:RGE_WAIT_GITHUB_ACTIONS_SKIP_MAIN) {
    return
}

if (-not $Repo) {
    $Repo = Resolve-DefaultGitHubRepo
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

$exitCode = Invoke-WaitForGitHubActions `
    -Repo $Repo `
    -Commit $Commit `
    -Branch $Branch `
    -WorkflowName $WorkflowName `
    -PullRequest $PullRequest `
    -AcceptPrSideCheck $AcceptPrSideCheck `
    -TimeoutMinutes $TimeoutMinutes `
    -PollSeconds $PollSeconds `
    -Limit $Limit

exit $exitCode
