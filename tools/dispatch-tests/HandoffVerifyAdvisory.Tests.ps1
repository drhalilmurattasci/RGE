#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0' }

BeforeAll {
    $script:OriginalLocation = Get-Location
    $script:TestsRoot = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:VerifyPath = Join-Path $script:RepoRootForTest '.ai\dispatch.verify.ps1'
    if (-not (Test-Path -LiteralPath $script:VerifyPath)) {
        throw "dispatch.verify.ps1 not found at $script:VerifyPath"
    }

    $script:OldVerifyLoadOnly = $env:RGE_AI_DISPATCH_VERIFY_LOAD_ONLY
    $env:RGE_AI_DISPATCH_VERIFY_LOAD_ONLY = '1'
    try {
        . $script:VerifyPath
    } finally {
        if ($null -ne $script:OldVerifyLoadOnly) { $env:RGE_AI_DISPATCH_VERIFY_LOAD_ONLY = $script:OldVerifyLoadOnly }
        else { Remove-Item Env:RGE_AI_DISPATCH_VERIFY_LOAD_ONLY -ErrorAction SilentlyContinue }
        Set-Location -LiteralPath $script:OriginalLocation
    }

    function Write-Utf8NoBomFile {
        param(
            [Parameter(Mandatory)][string]$Path,
            [Parameter(Mandatory)][AllowEmptyString()][string]$Text
        )
        [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
    }

    function Set-TestEnvVar {
        param(
            [Parameter(Mandatory)][string]$Name,
            [AllowNull()][string]$Value
        )
        if ($null -eq $Value) { Remove-Item "Env:$Name" -ErrorAction SilentlyContinue }
        else { Set-Item "Env:$Name" -Value $Value }
    }
}

AfterAll {
    Set-Location -LiteralPath $script:OriginalLocation
}

Describe 'Resolve-HandoffAdvisoryDispatchId' {
    It 'prefers RGE_AI_DISPATCH_ID over the branch name' {
        $old = $env:RGE_AI_DISPATCH_ID
        try {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value 'ISSUE-FROM-ENV'

            Resolve-HandoffAdvisoryDispatchId -BranchName 'ai-dispatch/ISSUE-FROM-BRANCH' |
                Should -Be 'ISSUE-FROM-ENV'
        } finally {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $old
        }
    }

    It 'falls back to ai-dispatch branch naming' {
        $old = $env:RGE_AI_DISPATCH_ID
        try {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $null

            Resolve-HandoffAdvisoryDispatchId -BranchName 'ai-dispatch/ISSUE-321' |
                Should -Be 'ISSUE-321'
        } finally {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $old
        }
    }

    It 'returns null outside dispatch context' {
        $old = $env:RGE_AI_DISPATCH_ID
        try {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $null

            Resolve-HandoffAdvisoryDispatchId -BranchName 'main' | Should -BeNullOrEmpty
        } finally {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $old
        }
    }
}

Describe 'Resolve-HandoffAdvisoryPacket' {
    It 'selects the latest canonical packet for the dispatch and type' {
        $handoffDir = Join-Path $TestDrive 'ai_handoffs'
        New-Item -ItemType Directory -Path $handoffDir -Force | Out-Null
        $older = Join-Path $handoffDir 'ISSUE-VERIFY_EXEC_2026-06-06_01-00-00+0300.md'
        $newer = Join-Path $handoffDir 'ISSUE-VERIFY_EXEC_2026-06-06_02-00-00+0300.md'
        $other = Join-Path $handoffDir 'ISSUE-OTHER_EXEC_2026-06-06_03-00-00+0300.md'
        Write-Utf8NoBomFile -Path $older -Text 'older'
        Write-Utf8NoBomFile -Path $newer -Text 'newer'
        Write-Utf8NoBomFile -Path $other -Text 'other'
        (Get-Item -LiteralPath $older).LastWriteTimeUtc = [datetime]'2026-06-06T01:00:00Z'
        (Get-Item -LiteralPath $newer).LastWriteTimeUtc = [datetime]'2026-06-06T02:00:00Z'
        (Get-Item -LiteralPath $other).LastWriteTimeUtc = [datetime]'2026-06-06T03:00:00Z'

        $packet = Resolve-HandoffAdvisoryPacket -HandoffDir $handoffDir -DispatchId 'ISSUE-VERIFY' -PacketType 'EXEC'

        $packet.Name | Should -Be 'ISSUE-VERIFY_EXEC_2026-06-06_02-00-00+0300.md'
    }
}

Describe 'Invoke-HandoffPacketAdvisoryValidation' {
    It 'skips when no dispatch context is available' {
        $old = $env:RGE_AI_DISPATCH_ID
        try {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $null

            $result = Invoke-HandoffPacketAdvisoryValidation -RepoRoot $TestDrive `
                -HandoffDir (Join-Path $TestDrive 'ai_handoffs') `
                -ValidatorPath (Join-Path $TestDrive 'validator.ps1') `
                -BranchName 'main'

            $result.Status | Should -Be 'SKIP'
            $result.ExitCode | Should -Be 0
        } finally {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $old
        }
    }

    It 'downgrades a validator non-zero exit to advisory WARN' {
        $old = $env:RGE_AI_DISPATCH_ID
        try {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value 'ISSUE-VERIFY'
            $handoffDir = Join-Path $TestDrive 'ai_handoffs'
            New-Item -ItemType Directory -Path $handoffDir -Force | Out-Null
            $task = Join-Path $handoffDir 'ISSUE-VERIFY_TASK_2026-06-06_01-00-00+0300.md'
            $exec = Join-Path $handoffDir 'ISSUE-VERIFY_EXEC_2026-06-06_02-00-00+0300.md'
            Write-Utf8NoBomFile -Path $task -Text 'task'
            Write-Utf8NoBomFile -Path $exec -Text 'exec'
            $validator = Join-Path $TestDrive 'validator.ps1'
            Write-Utf8NoBomFile -Path $validator -Text @'
param(
    [string]$PacketPath,
    [string]$TaskPacket,
    [string[]]$PlannerOverridePacket,
    [string]$Integration
)
Write-Output "HANDOFF_VALIDATE: FAIL"
Write-Output "PacketPath=$PacketPath"
Write-Output "TaskPacket=$TaskPacket"
exit 2
'@

            $result = Invoke-HandoffPacketAdvisoryValidation -RepoRoot $TestDrive `
                -HandoffDir $handoffDir `
                -ValidatorPath $validator `
                -IntegrationRef 'main' `
                -BranchName 'main'

            $result.Status | Should -Be 'WARN'
            $result.ExitCode | Should -Be 2
            Split-Path -Leaf $result.Packet | Should -Be 'ISSUE-VERIFY_EXEC_2026-06-06_02-00-00+0300.md'
            Split-Path -Leaf $result.TaskPacket | Should -Be 'ISSUE-VERIFY_TASK_2026-06-06_01-00-00+0300.md'
        } finally {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $old
        }
    }

    It 'downgrades an advisory FAIL verdict even when the validator exits zero' {
        $old = $env:RGE_AI_DISPATCH_ID
        try {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value 'ISSUE-VERIFY'
            $handoffDir = Join-Path $TestDrive 'ai_handoffs'
            New-Item -ItemType Directory -Path $handoffDir -Force | Out-Null
            Write-Utf8NoBomFile -Path (Join-Path $handoffDir 'ISSUE-VERIFY_TASK_2026-06-06_01-00-00+0300.md') -Text 'task'
            Write-Utf8NoBomFile -Path (Join-Path $handoffDir 'ISSUE-VERIFY_EXEC_2026-06-06_02-00-00+0300.md') -Text 'exec'
            $validator = Join-Path $TestDrive 'validator-zero-fail.ps1'
            Write-Utf8NoBomFile -Path $validator -Text @'
param(
    [string]$PacketPath,
    [string]$TaskPacket,
    [string[]]$PlannerOverridePacket,
    [string]$Integration
)
Write-Output "HANDOFF_VALIDATE: FAIL"
exit 0
'@

            $result = Invoke-HandoffPacketAdvisoryValidation -RepoRoot $TestDrive `
                -HandoffDir $handoffDir `
                -ValidatorPath $validator `
                -IntegrationRef 'main' `
                -BranchName 'main'

            $result.Status | Should -Be 'WARN'
            $result.ExitCode | Should -Be 0
        } finally {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $old
        }
    }

    It 'passes the active cargo target dir to the advisory validator as an excluded touched path' {
        $oldDispatch = $env:RGE_AI_DISPATCH_ID
        $oldTarget = $env:CARGO_TARGET_DIR
        try {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value 'ISSUE-VERIFY'
            Set-TestEnvVar -Name 'CARGO_TARGET_DIR' -Value (Join-Path $TestDrive 'target-issue-verify')
            $handoffDir = Join-Path $TestDrive 'ai_handoffs'
            New-Item -ItemType Directory -Path $handoffDir -Force | Out-Null
            Write-Utf8NoBomFile -Path (Join-Path $handoffDir 'ISSUE-VERIFY_TASK_2026-06-06_01-00-00+0300.md') -Text 'task'
            Write-Utf8NoBomFile -Path (Join-Path $handoffDir 'ISSUE-VERIFY_EXEC_2026-06-06_02-00-00+0300.md') -Text 'exec'
            $validator = Join-Path $TestDrive 'validator-exclude-target.ps1'
            Write-Utf8NoBomFile -Path $validator -Text @'
param(
    [string]$PacketPath,
    [string]$TaskPacket,
    [string[]]$PlannerOverridePacket,
    [string[]]$ExcludeTouchedPath,
    [string]$Integration
)
if ($ExcludeTouchedPath -notcontains $env:CARGO_TARGET_DIR) {
    Write-Output "HANDOFF_VALIDATE: FAIL"
    Write-Output "ExcludeTouchedPath=$($ExcludeTouchedPath -join ',')"
    exit 2
}
Write-Output "HANDOFF_VALIDATE: PASS"
exit 0
'@

            $result = Invoke-HandoffPacketAdvisoryValidation -RepoRoot $TestDrive `
                -HandoffDir $handoffDir `
                -ValidatorPath $validator `
                -IntegrationRef 'main' `
                -BranchName 'main'

            $result.Status | Should -Be 'PASS'
            $result.ExitCode | Should -Be 0
        } finally {
            Set-TestEnvVar -Name 'RGE_AI_DISPATCH_ID' -Value $oldDispatch
            Set-TestEnvVar -Name 'CARGO_TARGET_DIR' -Value $oldTarget
        }
    }
}
