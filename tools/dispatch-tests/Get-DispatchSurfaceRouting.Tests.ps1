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
        @{ Paths = @('.ai/dispatch.tasks.md');                                   Why = 'the brief (markdown)' }
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

    It 'normalizes backslash paths before matching' {
        (Get-DispatchSurfaceRouting -ChangedPaths @('ai_handoffs\ISSUE-1_TASK.md')).Routing | Should -Be 'main'
        (Get-DispatchSurfaceRouting -ChangedPaths @('crates\x\src\a.rs')).Routing | Should -Be 'pr'
    }
}
