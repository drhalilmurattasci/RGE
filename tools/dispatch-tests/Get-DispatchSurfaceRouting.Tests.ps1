#Requires -Version 5.1
<#
.SYNOPSIS
    Pester coverage for Get-DispatchSurfaceRouting in Invoke-AiDispatchQueue.ps1 --
    the fail-closed surface-split classifier for default-OFF auto-merge.

.DESCRIPTION
    Dot-sources the production queue through its RGE_AI_DISPATCH_QUEUE_SKIP_MAIN
    seam so the pure classifier loads without running the dispatch flow. No side
    effects; nothing here touches gh / git / the network.
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try { . $script:QueueScriptPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Get-DispatchSurfaceRouting' {
    It 'auto-merges when every path is low-risk: <Why>' -ForEach @(
        @{ Paths = @('README.md');                                              Why = 'a doc' }
        @{ Paths = @('tools/dispatch-tests/Foo.Tests.ps1');                      Why = 'a dispatch test' }
        @{ Paths = @('crates/editor-ui/src/Bar.Tests.ps1');                      Why = 'a .Tests.ps1 anywhere' }
        @{ Paths = @('ai_handoffs/ISSUE-1_TASK.md', 'ai_dispatch_logs/log.md');  Why = 'generated artifacts' }
        @{ Paths = @('docs/guide.md', 'AGENTS.md', 'ai_handoffs/x.json');        Why = 'docs + artifact mix' }
    ) {
        (Get-DispatchSurfaceRouting -ChangedPaths $Paths).Routing | Should -Be 'main'
    }

    It 'routes to PR (human merge) when any path is high-risk: <Why>' -ForEach @(
        @{ Paths = @('crates/editor-ui/src/menus/command.rs');           Why = 'product source' }
        @{ Paths = @('Cargo.toml');                                      Why = 'Cargo manifest' }
        @{ Paths = @('Cargo.lock');                                      Why = 'Cargo lock' }
        @{ Paths = @('.github/workflows/tests.yml');                     Why = 'CI workflow' }
        @{ Paths = @('Invoke-AiDispatchAuto.ps1');                       Why = 'an automation script' }
        @{ Paths = @('.ai/dispatch.verify.ps1');                         Why = 'the verify gate' }
        @{ Paths = @('schemas/handoff.schema.json');                     Why = 'a schema' }
        @{ Paths = @('README.md', 'crates/cad-core/src/lib.rs');         Why = 'a doc mixed with source -> any high-risk wins' }
    ) {
        (Get-DispatchSurfaceRouting -ChangedPaths $Paths).Routing | Should -Be 'pr'
    }

    It 'lists the high-risk paths and excludes the low-risk ones' {
        $r = Get-DispatchSurfaceRouting -ChangedPaths @('README.md', 'crates/x/src/a.rs', 'Cargo.toml')
        $r.Routing | Should -Be 'pr'
        $r.HighRiskPaths | Should -Contain 'crates/x/src/a.rs'
        $r.HighRiskPaths | Should -Contain 'Cargo.toml'
        $r.HighRiskPaths | Should -Not -Contain 'README.md'
    }

    It 'fail-closed on an empty changeset (nothing to auto-merge)' {
        $r = Get-DispatchSurfaceRouting -ChangedPaths @()
        $r.Routing | Should -Be 'pr'
        $r.Reason | Should -Match 'no changed paths'
    }

    It 'STRICT (default): any brief change routes to PR, even alongside low-risk work: <Why>' -ForEach @(
        @{ Paths = @('.ai/dispatch.tasks.md');                                   Why = 'brief alone' }
        @{ Paths = @('.ai/dispatch.tasks.archive.md');                           Why = 'archive alone' }
        @{ Paths = @('.ai/dispatch.tasks.md', 'Status.md', 'ai_handoffs/x.md');  Why = 'brief + low-risk docs' }
    ) {
        (Get-DispatchSurfaceRouting -ChangedPaths $Paths).Routing | Should -Be 'pr'
    }

    It 'STRICT (default): a brief-FREE low-risk changeset still auto-merges to main' {
        (Get-DispatchSurfaceRouting -ChangedPaths @('Status.md', 'ai_handoffs/ISSUE-9_EXEC.md')).Routing | Should -Be 'main'
    }

    Context 'with -AllowBriefRideAlong (operator opt-in to ride-along)' {
        It 'a brief re-arm riding along with genuine low-risk work auto-merges to main' {
            $r = Get-DispatchSurfaceRouting -ChangedPaths @('.ai/dispatch.tasks.md', 'Status.md', 'ai_handoffs/ISSUE-9_EXEC.md') -AllowBriefRideAlong $true
            $r.Routing | Should -Be 'main'
            $r.Reason | Should -Match 'brief re-arm'
        }
        It 'a brief-ONLY changeset still routes to PR (the control surface never auto-merges itself)' {
            $r = Get-DispatchSurfaceRouting -ChangedPaths @('.ai/dispatch.tasks.md') -AllowBriefRideAlong $true
            $r.Routing | Should -Be 'pr'
            $r.Reason | Should -Match 'brief'
        }
        It 'a brief + high-risk source still routes to PR' {
            (Get-DispatchSurfaceRouting -ChangedPaths @('.ai/dispatch.tasks.md', 'crates/editor-ui/src/menus/default_menu.rs') -AllowBriefRideAlong $true).Routing |
                Should -Be 'pr'
        }
    }

    It 'normalizes backslash paths before matching' {
        (Get-DispatchSurfaceRouting -ChangedPaths @('ai_handoffs\ISSUE-1_TASK.md')).Routing | Should -Be 'main'
        (Get-DispatchSurfaceRouting -ChangedPaths @('crates\x\src\a.rs')).Routing | Should -Be 'pr'
    }
}

Describe 'Test-DiffSizeWithinCap' {
    It 'is within when under both caps' {
        (Test-DiffSizeWithinCap -FilesChanged 3 -LinesChanged 50 -MaxFiles 10 -MaxLines 200).Within | Should -BeTrue
    }
    It 'is unlimited when caps are 0 (disabled)' {
        (Test-DiffSizeWithinCap -FilesChanged 999 -LinesChanged 99999 -MaxFiles 0 -MaxLines 0).Within | Should -BeTrue
    }
    It 'exceeds when files over the file cap' {
        $d = Test-DiffSizeWithinCap -FilesChanged 11 -LinesChanged 1 -MaxFiles 10 -MaxLines 0
        $d.Within | Should -BeFalse
        $d.Reason | Should -Match 'files changed'
    }
    It 'exceeds when lines over the line cap' {
        $d = Test-DiffSizeWithinCap -FilesChanged 1 -LinesChanged 300 -MaxFiles 0 -MaxLines 200
        $d.Within | Should -BeFalse
        $d.Reason | Should -Match 'lines changed'
    }
    It 'is within exactly at the cap boundary' {
        (Test-DiffSizeWithinCap -FilesChanged 10 -LinesChanged 200 -MaxFiles 10 -MaxLines 200).Within | Should -BeTrue
    }
}

Describe 'Measure-DiffNumstatOutput' {
    It 'counts files and changed lines from normal numstat output' {
        $m = Measure-DiffNumstatOutput -NumstatOutput ("10`t2`tREADME.md`n0`t7`ttools/test.ps1")
        $m.ParseOk | Should -BeTrue
        $m.FilesChanged | Should -Be 2
        $m.LinesChanged | Should -Be 19
    }

    It 'marks binary or malformed numstat rows unparseable so the cap downgrades to PR' {
        $m = Measure-DiffNumstatOutput -NumstatOutput ("-`t-`timage.png`n3`t1`tREADME.md")
        $m.ParseOk | Should -BeFalse
    }

    It 'marks overflowing numeric fields unparseable instead of throwing or undercounting' {
        $m = Measure-DiffNumstatOutput -NumstatOutput ("999999999999999999999999`t1`tlarge.txt")
        $m.ParseOk | Should -BeFalse
    }
}

Describe 'Test-PendingIssueSuperseded (stale-body selection guard)' {
    It 'is superseded only when a newer ai-auto issue has the same normalized title' {
        $auto = @(
            [pscustomobject]@{ number = 101; title = '  Repeat this task   now ' }
        )
        Test-PendingIssueSuperseded -IssueNumber 100 -IssueTitle 'Repeat this task now' -AutoIssues $auto |
            Should -BeTrue
    }

    It 'is NOT superseded by a newer unrelated ai-auto issue (bare number compare is fail-open)' {
        $auto = @(
            [pscustomobject]@{ number = 101; title = 'Different task' }
        )
        Test-PendingIssueSuperseded -IssueNumber 100 -IssueTitle 'Real unrun task' -AutoIssues $auto |
            Should -BeFalse
    }

    It 'is NOT superseded when the all-auto evidence is missing (published-SHA guard remains)' {
        Test-PendingIssueSuperseded -IssueNumber 100 -IssueTitle 'Real unrun task' -AutoIssues $null |
            Should -BeFalse
    }

    It 'is NOT superseded when its title is blank' {
        $auto = @(
            [pscustomobject]@{ number = 101; title = 'Anything' }
        )
        Test-PendingIssueSuperseded -IssueNumber 100 -IssueTitle '   ' -AutoIssues $auto |
            Should -BeFalse
    }

    It 'returns the newest same-title issue for the production close comment' {
        $auto = @(
            [pscustomobject]@{ number = 101; title = 'Same task' }
            [pscustomobject]@{ number = 104; title = 'Same task' }
            [pscustomobject]@{ number = 105; title = 'Other task' }
        )
        (Select-PendingIssueSupersedingAutoIssue -IssueNumber 100 -IssueTitle 'Same task' -AutoIssues $auto).number |
            Should -Be 104
    }
}
