#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0' }

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:VerifyPath      = Join-Path $script:RepoRootForTest '.ai\dispatch.verify.ps1'
    $script:AutoScriptPath  = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchAuto.ps1'
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'

    foreach ($p in @($script:VerifyPath, $script:AutoScriptPath, $script:QueueScriptPath)) {
        if (-not (Test-Path -LiteralPath $p)) {
            throw "Required dispatch script not found at $p"
        }
    }

    function Invoke-ChildPowerShell {
        param([Parameter(Mandatory)][string]$Command)
        $oldEap = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        $global:LASTEXITCODE = 0
        try {
            $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $Command 2>&1
            $code = $LASTEXITCODE
            return [pscustomobject]@{
                Code = $code
                Text = (($output | ForEach-Object { [string]$_ }) -join "`n")
            }
        } finally {
            $ErrorActionPreference = $oldEap
        }
    }
}

Describe 'RGE_AI_DISPATCH_VERIFY_SKIP_MAIN fail-closed behavior' {
    It 'makes dispatch.verify.ps1 exit non-zero instead of returning a false green pass' {
        $cmd = @"
`$env:RGE_AI_DISPATCH_VERIFY_SKIP_MAIN = '1'
& '$($script:VerifyPath)'
"@

        $result = Invoke-ChildPowerShell -Command $cmd

        $result.Code | Should -Be 1
        $result.Text | Should -Match 'VERIFY SKIPPED: RGE_AI_DISPATCH_VERIFY_SKIP_MAIN=1'
        $result.Text | Should -Match 'This is NOT a real pass'
    }

    It 'lets tests load verify helpers through the explicit load-only seam' {
        $cmd = @"
`$env:RGE_AI_DISPATCH_VERIFY_LOAD_ONLY = '1'
. '$($script:VerifyPath)'
if (Get-Command Resolve-HandoffAdvisoryDispatchId -ErrorAction SilentlyContinue) { exit 0 }
exit 99
"@

        (Invoke-ChildPowerShell -Command $cmd).Code | Should -Be 0
    }

    It 'blocks the load-only seam when dispatch.verify.ps1 is invoked as a real file' {
        $cmd = @"
`$env:RGE_AI_DISPATCH_VERIFY_LOAD_ONLY = '1'
& '$($script:VerifyPath)'
"@

        $result = Invoke-ChildPowerShell -Command $cmd

        $result.Code | Should -Be 1
        $result.Text | Should -Match 'VERIFY LOAD_ONLY BLOCKED'
        $result.Text | Should -Match 'This is NOT a real pass'
    }

    It 'blocks Auto before a publish-capable run can invoke the queue' {
        $cmd = @"
`$env:RGE_AI_DISPATCH_VERIFY_SKIP_MAIN = '1'
& '$($script:AutoScriptPath)' -PublishMode main -DryRun
"@

        $result = Invoke-ChildPowerShell -Command $cmd

        $result.Code | Should -Be 1
        $result.Text | Should -Match 'RGE_AI_DISPATCH_VERIFY_SKIP_MAIN=1 disables the verification gate'
        $result.Text | Should -Match 'refusing publish-capable Auto run'
    }

    It 'blocks Auto in its default PR publish mode too' {
        $cmd = @"
`$env:RGE_AI_DISPATCH_VERIFY_SKIP_MAIN = '1'
& '$($script:AutoScriptPath)' -DryRun
"@

        $result = Invoke-ChildPowerShell -Command $cmd

        $result.Code | Should -Be 1
        $result.Text | Should -Match 'refusing publish-capable Auto run'
        $result.Text | Should -Match 'PublishMode pr'
    }

    It 'blocks Queue before a publish-capable run can reach git or GitHub' {
        $cmd = @"
`$env:RGE_AI_DISPATCH_VERIFY_SKIP_MAIN = '1'
& '$($script:QueueScriptPath)' -PublishMode main -DryRun
"@

        $result = Invoke-ChildPowerShell -Command $cmd

        $result.Code | Should -Be 1
        $result.Text | Should -Match 'RGE_AI_DISPATCH_VERIFY_SKIP_MAIN=1 disables the verification gate'
        $result.Text | Should -Match 'refusing publish-capable queue run'
    }

    It 'blocks Queue in its default PR publish mode too' {
        $cmd = @"
`$env:RGE_AI_DISPATCH_VERIFY_SKIP_MAIN = '1'
& '$($script:QueueScriptPath)' -DryRun
"@

        $result = Invoke-ChildPowerShell -Command $cmd

        $result.Code | Should -Be 1
        $result.Text | Should -Match 'refusing publish-capable queue run'
        $result.Text | Should -Match 'resolved publish mode: pr'
    }
}
