#Requires -Version 5.1
<#
.SYNOPSIS
    ISSUE-241 coverage for the PR-side external-check opt-in in
    Wait-GitHubActions.ps1.

.DESCRIPTION
    The waiter is hardened so a caller can explicitly opt-in to resolving
    one or more named external checks (the canonical example is the
    org/default CodeQL workflow that lands on the PR check rollup but is
    invisible to `gh run list`). Strict rules apply, and this file pins
    them so a future "make it pass" change cannot silently broaden the
    waiter:

      1. Default behaviour is unchanged: only `gh run list` evidence
         decides expected workflows. A tracked workflow with no run
         visible for the target commit keeps the waiter polling until
         the timeout fires (exit code 2).

      2. -PullRequest + -AcceptPrSideCheck is the only path that accepts
         a named external check via the PR rollup. The check must
         resolve to an explicit SUCCESS conclusion against the same
         head commit SHA that was passed to -Commit.

      3. A PR rollup whose headRefOid differs from -Commit must not be
         accepted as pass under any circumstances. The waiter treats
         the row as `missing` so the timeout, not a SHA-mismatched
         green check, decides the outcome.

      4. A failing PR-side check (FAILURE / CANCELLED / TIMED_OUT /
         SKIPPED / ACTION_REQUIRED / NEUTRAL) immediately fails the
         wait with exit code 1.

      5. A missing PR-side check (no row in statusCheckRollup for the
         requested name) keeps polling and times out as exit code 2.

      6. -AcceptPrSideCheck without -PullRequest is rejected fail-fast.
         An accepted PR-side name does NOT need to be repeated in
         -WorkflowName -- the opt-in itself adds the name to the
         expected wait set. This keeps the advertised one-flag opt-in
         working without forcing operators to duplicate the name.

      7. PR-side StatusContext entries with state = PENDING / EXPECTED
         are unresolved (the waiter keeps polling and times out as
         exit 2 if they never resolve), NOT a completed failure.

    All tests run against the dot-sourced runtime function
    `Invoke-WaitForGitHubActions` with injected `Clock`, `Sleeper`,
    `RunListProvider`, and `PrRollupProvider` scriptblocks. No live
    `gh`, `git`, or filesystem mutation occurs.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:WaitScriptPath  = Join-Path $script:RepoRootForTest 'Wait-GitHubActions.ps1'
    if (-not (Test-Path -LiteralPath $script:WaitScriptPath)) {
        throw "Wait-GitHubActions.ps1 not found at $script:WaitScriptPath"
    }

    # Dot-source the production waiter through the testability seam so its
    # helpers (Invoke-WaitForGitHubActions, Resolve-PrSideCheckEvidence,
    # Test-PrSideAcceptanceConfig, ...) land in this Pester session without
    # running the main loop.
    $env:RGE_WAIT_GITHUB_ACTIONS_SKIP_MAIN = '1'
    try {
        . $script:WaitScriptPath
    } finally {
        Remove-Item Env:RGE_WAIT_GITHUB_ACTIONS_SKIP_MAIN -ErrorAction SilentlyContinue
    }

    # --- Reusable fixture helpers ----------------------------------------

    # A frozen clock the waiter will read via -Clock. The deadline math
    # uses the *first* sample to compute deadline = first + TimeoutMinutes,
    # then each subsequent sample advances by AdvanceSeconds. Three samples
    # at 0/0/0 means deadline = first; on the second loop iteration the
    # waiter sees `now -ge deadline` and returns exit code 2 (timeout).
    function script:New-FrozenClock {
        param(
            [datetime]$Start = ([datetime]'2026-05-28T08:00:00Z'),
            [double]$AdvanceSeconds = 0
        )
        $box = [pscustomobject]@{ Now = $Start; Advance = $AdvanceSeconds }
        $clock = {
            $value = $box.Now
            $box.Now = $box.Now.AddSeconds($box.Advance)
            return $value
        }.GetNewClosure()
        return [pscustomobject]@{
            Clock = $clock
            Box   = $box
        }
    }

    # A sleeper that never actually sleeps -- but, when constructed with a
    # `$Box`, advances the frozen clock by the requested seconds. Combined
    # with a $AdvanceSeconds=0 frozen clock this lets a test step the
    # waiter forward to its deadline in a deterministic single iteration.
    function script:New-FastForwardSleeper {
        param($Box)
        if (-not $Box) {
            return { param($seconds) }
        }
        return {
            param($seconds)
            $box.Now = $box.Now.AddSeconds([double]$seconds)
        }.GetNewClosure()
    }

    function script:New-RunListProvider {
        param([object[]]$Runs)
        $snapshot = ,$Runs
        return {
            param($repo, $branch, $limit)
            return $snapshot
        }.GetNewClosure()
    }

    function script:New-PrRollupProvider {
        param([object]$Rollup)
        $snapshot = $Rollup
        return {
            param($repo, $pr)
            return $snapshot
        }.GetNewClosure()
    }

    function script:New-Run {
        param(
            [Parameter(Mandatory)][string]$Name,
            [Parameter(Mandatory)][string]$HeadSha,
            [string]$Status     = 'completed',
            [string]$Conclusion = 'success',
            [int]$DatabaseId    = 0,
            [datetime]$CreatedAt = ([datetime]'2026-05-28T07:55:00Z'),
            [string]$Url        = ''
        )
        return [pscustomobject]@{
            databaseId   = $DatabaseId
            name         = $Name
            workflowName = $Name
            status       = $Status
            conclusion   = $Conclusion
            headSha      = $HeadSha
            createdAt    = $CreatedAt
            url          = $Url
        }
    }

    function script:New-PrRollupCheck {
        param(
            [Parameter(Mandatory)][string]$Name,
            [string]$Status     = 'COMPLETED',
            [string]$Conclusion = 'SUCCESS'
        )
        return [pscustomobject]@{
            __typename = 'CheckRun'
            name       = $Name
            status     = $Status
            conclusion = $Conclusion
        }
    }

    function script:New-PrRollup {
        param(
            [Parameter(Mandatory)][string]$HeadRefOid,
            [object[]]$Checks = @()
        )
        return [pscustomobject]@{
            headRefOid        = $HeadRefOid
            statusCheckRollup = $Checks
        }
    }

    # Invoke-WaitForGitHubActions emits status lines via Write-Output (so
    # operators can capture and audit them). In a `$result = & $func`
    # assignment the int exit code lands at the end of the same pipeline.
    # The exit code is always the last non-string element. Filter for it
    # here so each test asserts on the exit code, not the human-readable
    # progress prose.
    function script:Get-WaiterExitCode {
        param([Parameter(Mandatory)][object[]]$Output)
        $tail = @($Output | Where-Object { $_ -is [int] -or $_ -is [int64] })
        if ($tail.Count -eq 0) {
            throw "Waiter pipeline did not emit an integer exit code. Output was:`n$($Output -join "`n")"
        }
        return [int]$tail[-1]
    }
}

Describe 'ISSUE-241 Resolve-PrSideCheckEvidence classifier' {

    It 'returns missing when the rollup is $null' {
        $r = Resolve-PrSideCheckEvidence -Rollup $null -ExpectedHeadSha 'aaa' -CheckName 'CodeQL'
        $r.State | Should -Be 'missing'
    }

    It 'returns missing when statusCheckRollup is empty' {
        $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @()
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'CodeQL'
        $r.State | Should -Be 'missing'
    }

    It 'returns sha-mismatch when PR head SHA differs from -Commit' {
        $rollup = New-PrRollup -HeadRefOid 'bbb' -Checks @(
            New-PrRollupCheck -Name 'CodeQL' -Conclusion 'SUCCESS'
        )
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'CodeQL'
        $r.State     | Should -Be 'sha-mismatch'
        $r.PrHeadSha | Should -Be 'bbb'
    }

    It 'returns pass for an explicit SUCCESS conclusion at the same SHA' {
        $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
            New-PrRollupCheck -Name 'CodeQL' -Conclusion 'SUCCESS'
        )
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'CodeQL'
        $r.State | Should -Be 'pass'
    }

    It 'returns pending for a not-yet-completed check at the same SHA' {
        $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
            New-PrRollupCheck -Name 'CodeQL' -Status 'IN_PROGRESS' -Conclusion ''
        )
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'CodeQL'
        $r.State | Should -Be 'pending'
    }

    It 'returns fail for a FAILURE conclusion' {
        $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
            New-PrRollupCheck -Name 'CodeQL' -Conclusion 'FAILURE'
        )
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'CodeQL'
        $r.State | Should -Be 'fail'
    }

    It 'rejects <Conclusion> as a pass (treats it as fail)' -TestCases @(
        @{ Conclusion = 'SKIPPED'         },
        @{ Conclusion = 'CANCELLED'       },
        @{ Conclusion = 'TIMED_OUT'       },
        @{ Conclusion = 'NEUTRAL'         },
        @{ Conclusion = 'ACTION_REQUIRED' },
        @{ Conclusion = 'STARTUP_FAILURE' }
    ) {
        param([string]$Conclusion)
        $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
            New-PrRollupCheck -Name 'CodeQL' -Conclusion $Conclusion
        )
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'CodeQL'
        $r.State | Should -Be 'fail'
    }

    It 'returns missing when the named check is absent from the rollup' {
        $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
            New-PrRollupCheck -Name 'Some Other Check' -Conclusion 'SUCCESS'
        )
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'CodeQL'
        $r.State | Should -Be 'missing'
    }

    It 'accepts a StatusContext entry (context+state shape) when state is SUCCESS' {
        # StatusContext rollup entries use `context` + `state` rather than
        # `name` + `conclusion`. The classifier normalises both shapes.
        $rollup = [pscustomobject]@{
            headRefOid        = 'aaa'
            statusCheckRollup = @(
                [pscustomobject]@{
                    __typename = 'StatusContext'
                    context    = 'deploy/preview'
                    state      = 'SUCCESS'
                }
            )
        }
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'deploy/preview'
        $r.State | Should -Be 'pass'
    }

    It 'returns pending for a StatusContext entry with state <State>' -TestCases @(
        @{ State = 'PENDING'  }
        @{ State = 'EXPECTED' }
    ) {
        param([string]$State)
        # PENDING and EXPECTED are unresolved StatusContext states -- the
        # check has not reached a terminal conclusion, so the waiter must
        # keep polling instead of misreading them as a failing completion.
        $rollup = [pscustomobject]@{
            headRefOid        = 'aaa'
            statusCheckRollup = @(
                [pscustomobject]@{
                    __typename = 'StatusContext'
                    context    = 'deploy/preview'
                    state      = $State
                }
            )
        }
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'deploy/preview'
        $r.State      | Should -Be 'pending'
        $r.Conclusion | Should -Be $State
    }

    It 'still returns fail for a StatusContext entry with state FAILURE' {
        # Terminal failing StatusContext states must still be terminal.
        $rollup = [pscustomobject]@{
            headRefOid        = 'aaa'
            statusCheckRollup = @(
                [pscustomobject]@{
                    __typename = 'StatusContext'
                    context    = 'deploy/preview'
                    state      = 'FAILURE'
                }
            )
        }
        $r = Resolve-PrSideCheckEvidence -Rollup $rollup -ExpectedHeadSha 'aaa' -CheckName 'deploy/preview'
        $r.State | Should -Be 'fail'
    }
}

Describe 'ISSUE-241 Test-PrSideAcceptanceConfig fail-fast validation' {

    It 'returns @() when -AcceptPrSideCheck is empty' {
        Test-PrSideAcceptanceConfig -PullRequest 0 -AcceptPrSideCheck @() |
            Should -BeNullOrEmpty
    }

    It 'rejects -AcceptPrSideCheck without -PullRequest' {
        { Test-PrSideAcceptanceConfig -PullRequest 0 -AcceptPrSideCheck @('CodeQL') } |
            Should -Throw -ExpectedMessage '*PullRequest*'
    }

    It 'accepts a name even when it is not pre-listed in -WorkflowName' {
        # The advertised opt-in (-PullRequest <PR#> -AcceptPrSideCheck <Name>)
        # must work on its own; the accepted name joins the expected wait
        # set inside Invoke-WaitForGitHubActions, so the validator no longer
        # needs the caller to list it in -WorkflowName as well.
        $r = Test-PrSideAcceptanceConfig -PullRequest 240 -AcceptPrSideCheck @('CodeQL')
        @($r).Count | Should -Be 1
        @($r)[0] | Should -Be 'CodeQL'
    }

    It 'returns the deduplicated accepted set when valid' {
        $r = Test-PrSideAcceptanceConfig `
            -PullRequest 240 `
            -AcceptPrSideCheck @('CodeQL','CodeQL')
        @($r).Count | Should -Be 1
        @($r)[0] | Should -Be 'CodeQL'
    }
}

Describe 'ISSUE-241 Invoke-WaitForGitHubActions end-to-end semantics' {

    Context 'Default strict mode (no PR-side opt-in)' {

        It 'returns 0 when every expected workflow has a matching success run' {
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $runs = @(
                New-Run -Name 'Format check'       -HeadSha 'aaa' -Conclusion 'success'
                New-Run -Name 'Architecture lints' -HeadSha 'aaa' -Conclusion 'success'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('Format check','Architecture lints') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs $runs) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 0
        }

        It 'returns 1 when an expected workflow ran but failed' {
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $runs = @(
                New-Run -Name 'Format check' -HeadSha 'aaa' -Status 'completed' -Conclusion 'failure'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('Format check') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs $runs) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 1
        }

        It 'returns 2 when an expected workflow has no matching run (timeout)' {
            # Start the clock just before the deadline; the fast-forward
            # sleeper trips the timeout on the next-iteration check.
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('Missing workflow') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs @()) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 2
        }

        It 'returns 2 when a run exists on a DIFFERENT commit (no recent-run fallback)' {
            # A green run on commit `zzz` must not satisfy a wait for `aaa`.
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $runs = @( New-Run -Name 'Format check' -HeadSha 'zzz' -Conclusion 'success' )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('Format check') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs $runs) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 2
        }
    }

    Context 'PR-side opt-in (--PullRequest + --AcceptPrSideCheck)' {

        It 'returns 0 when the PR rollup shows the accepted check SUCCESS at the same SHA' {
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $runs = @(
                New-Run -Name 'Format check' -HeadSha 'aaa' -Conclusion 'success'
            )
            $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
                New-PrRollupCheck -Name 'CodeQL' -Conclusion 'SUCCESS'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('Format check','CodeQL') `
                -PullRequest 240 -AcceptPrSideCheck @('CodeQL') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs $runs) `
                -PrRollupProvider (New-PrRollupProvider -Rollup $rollup) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 0
        }

        It 'returns 2 (timeout) when the PR rollup head SHA differs from -Commit' {
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $rollup = New-PrRollup -HeadRefOid 'bbb' -Checks @(
                New-PrRollupCheck -Name 'CodeQL' -Conclusion 'SUCCESS'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('CodeQL') `
                -PullRequest 240 -AcceptPrSideCheck @('CodeQL') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs @()) `
                -PrRollupProvider (New-PrRollupProvider -Rollup $rollup) 6>$null)
            # SHA mismatch must not satisfy the wait; the row stays missing,
            # the sleeper fast-forwards past the deadline, exit 2.
            (Get-WaiterExitCode -Output $output) | Should -Be 2
        }

        It 'returns 1 when an accepted PR-side check has a FAILURE conclusion at the same SHA' {
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
                New-PrRollupCheck -Name 'CodeQL' -Conclusion 'FAILURE'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('CodeQL') `
                -PullRequest 240 -AcceptPrSideCheck @('CodeQL') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs @()) `
                -PrRollupProvider (New-PrRollupProvider -Rollup $rollup) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 1
        }

        It 'returns 1 when an accepted PR-side check is SKIPPED (not accepted as pass)' {
            # The TASK packet explicitly forbids accepting skipped PR-side
            # checks as pass, so a SKIPPED conclusion has to surface as a
            # failing terminal state -- not silently pass.
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
                New-PrRollupCheck -Name 'CodeQL' -Conclusion 'SKIPPED'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('CodeQL') `
                -PullRequest 240 -AcceptPrSideCheck @('CodeQL') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs @()) `
                -PrRollupProvider (New-PrRollupProvider -Rollup $rollup) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 1
        }

        It 'returns 2 (timeout) when the named PR-side check is missing from the rollup' {
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
                New-PrRollupCheck -Name 'Some Other Check' -Conclusion 'SUCCESS'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('CodeQL') `
                -PullRequest 240 -AcceptPrSideCheck @('CodeQL') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs @()) `
                -PrRollupProvider (New-PrRollupProvider -Rollup $rollup) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 2
        }

        It 'accepts -PullRequest 240 -AcceptPrSideCheck CodeQL without duplicating CodeQL in -WorkflowName' {
            # This pins the advertised one-flag opt-in: the accepted name
            # must NOT need to be repeated in -WorkflowName. The opt-in
            # itself adds CodeQL to the expected wait set, and a matching
            # PR-head-SHA SUCCESS resolves the wait with exit 0.
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
                New-PrRollupCheck -Name 'CodeQL' -Conclusion 'SUCCESS'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @() `
                -PullRequest 240 -AcceptPrSideCheck @('CodeQL') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs @()) `
                -PrRollupProvider (New-PrRollupProvider -Rollup $rollup) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 0
        }

        It 'keeps polling and times out (exit 2) when the PR-side StatusContext is still PENDING' {
            # The waiter must not misread StatusContext state=PENDING as a
            # failing completion. Without resolution, the fast-forward
            # sleeper trips the deadline and exit 2 is returned.
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $rollup = [pscustomobject]@{
                headRefOid        = 'aaa'
                statusCheckRollup = @(
                    [pscustomobject]@{
                        __typename = 'StatusContext'
                        context    = 'CodeQL'
                        state      = 'PENDING'
                    }
                )
            }
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @() `
                -PullRequest 240 -AcceptPrSideCheck @('CodeQL') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs @()) `
                -PrRollupProvider (New-PrRollupProvider -Rollup $rollup) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 2
        }

        It 'does NOT relax tracked workflows: a missing tracked workflow still times out even with the PR-side opt-in' {
            # CodeQL is accepted via PR-side; the unrelated tracked workflow
            # has no run, so the waiter cannot pass.
            $clk = New-FrozenClock -Start ([datetime]'2026-05-28T08:00:00Z') -AdvanceSeconds 0
            $sleeper = New-FastForwardSleeper -Box $clk.Box
            $rollup = New-PrRollup -HeadRefOid 'aaa' -Checks @(
                New-PrRollupCheck -Name 'CodeQL' -Conclusion 'SUCCESS'
            )
            $output = @(Invoke-WaitForGitHubActions `
                -Repo 'org/repo' -Commit 'aaa' -Branch 'topic' `
                -WorkflowName @('Format check','CodeQL') `
                -PullRequest 240 -AcceptPrSideCheck @('CodeQL') `
                -TimeoutMinutes 1 -PollSeconds 5 `
                -Clock $clk.Clock -Sleeper $sleeper `
                -RunListProvider (New-RunListProvider -Runs @()) `
                -PrRollupProvider (New-PrRollupProvider -Rollup $rollup) 6>$null)
            (Get-WaiterExitCode -Output $output) | Should -Be 2
        }
    }
}

Describe 'ISSUE-241 source-level surface' {

    BeforeAll {
        $script:WaitScriptText = Get-Content -Raw -LiteralPath $script:WaitScriptPath
    }

    It 'declares -PullRequest as a script parameter' {
        $script:WaitScriptText | Should -Match '(?ms)param\([^)]*\[int\]\$PullRequest'
    }

    It 'declares -AcceptPrSideCheck as a script parameter' {
        $script:WaitScriptText | Should -Match '(?ms)param\([^)]*\[string\[\]\]\$AcceptPrSideCheck'
    }

    It 'guards the main flow behind RGE_WAIT_GITHUB_ACTIONS_SKIP_MAIN' {
        $script:WaitScriptText | Should -Match 'RGE_WAIT_GITHUB_ACTIONS_SKIP_MAIN'
    }
}
