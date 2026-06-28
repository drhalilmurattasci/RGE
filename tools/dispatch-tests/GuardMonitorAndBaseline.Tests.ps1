#Requires -Modules @{ ModuleName = 'Pester'; ModuleVersion = '5.0' }

# Coverage for two Invoke-AiDispatchGuard.ps1 safety fixes (bug-sweep H + D).
# Dot-sources the guard via its RGE_AI_DISPATCH_GUARD_SKIP_MAIN seam (no child,
# no claude, no git, no gh).
#
#  H: Invoke-MonitorModel ignored claude's exit code -- a failed claude that still
#     printed 'ok' parsed as a PASS. Now routed through the pure, fail-closed
#     Convert-MonitorRawToText (nonzero exit -> '' -> abort fail-safe).
#  D: Get-OriginMainSha returned '' on any failure, and the call site skipped the
#     out-of-band publish check on '' SILENTLY -- indistinguishable from the test
#     seam. Now: bounded retry + a WARN that fires only on a REAL baseline failure
#     (not the seam).

BeforeAll {
    $env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN = '1'
    $env:RGE_AI_DISPATCH_GUARD_SKIP_OOB_SHA = '1'
    $script:GuardPath = Join-Path $PSScriptRoot '..\..\Invoke-AiDispatchGuard.ps1'
    . $script:GuardPath -DispatchId 'PESTER' -WatchRoot $TestDrive
    $ErrorActionPreference = 'Continue'
}

AfterAll {
    Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_MAIN -ErrorAction SilentlyContinue
    Remove-Item Env:RGE_AI_DISPATCH_GUARD_SKIP_OOB_SHA -ErrorAction SilentlyContinue
}

Describe 'Convert-MonitorRawToText (H: fail closed on nonzero claude exit)' {
    It 'returns empty on a NONZERO exit even when stdout printed a pass token' {
        # The core fix: a failed claude that still emitted "ok" must NOT pass.
        Convert-MonitorRawToText -ExitCode 1 -Raw 'ok'                    | Should -Be ''
        Convert-MonitorRawToText -ExitCode 1 -Raw '{"verdict":"ok"}'      | Should -Be ''
        Convert-MonitorRawToText -ExitCode 137 -Raw 'ok'                  | Should -Be ''
    }

    It 'returns the trimmed stdout on a clean (zero) exit' {
        Convert-MonitorRawToText -ExitCode 0 -Raw "  ok `n" | Should -Be 'ok'
        Convert-MonitorRawToText -ExitCode 0 -Raw '{"verdict":"abort"}' | Should -Be '{"verdict":"abort"}'
    }

    It 'returns empty for a clean exit with empty/null stdout (still abort fail-safe)' {
        Convert-MonitorRawToText -ExitCode 0 -Raw ''    | Should -Be ''
        Convert-MonitorRawToText -ExitCode 0 -Raw $null | Should -Be ''
    }
}

Describe 'Get-OriginMainSha + out-of-band baseline (D: visible, not silent)' {
    It 'still short-circuits to empty under the hermetic OOB test seam' {
        # Seam is set in BeforeAll, so the real git path never runs here.
        Get-OriginMainSha | Should -Be ''
    }

    Context 'source contract' {
        BeforeAll { $script:GuardSrc = Get-Content -LiteralPath $script:GuardPath -Raw }

        It 'bounded-retries the baseline read before giving up' {
            $script:GuardSrc | Should -Match 'for \(\$attempt = 1; \$attempt -le 3; \$attempt\+\+\)'
        }

        It 'WARNs when the baseline is unreadable for a REAL reason (not the test seam)' {
            # The skip must distinguish a genuine failure from RGE_AI_DISPATCH_GUARD_SKIP_OOB_SHA.
            $script:GuardSrc | Should -Match 'elseif \(\$env:RGE_AI_DISPATCH_GUARD_SKIP_OOB_SHA -ne ''1''\)'
            $script:GuardSrc | Should -Match 'out-of-band publish detection skipped for this tick'
        }
    }
}
