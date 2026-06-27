#Requires -Version 5.1
<#
.SYNOPSIS
    Canonical verification gate for the RGE AI dispatch loop.

.DESCRIPTION
    Invoke-AiDispatchLoop.ps1 runs this script after the Claude execution
    step and before Codex control. Exit code 0 means the working tree is
    verified green; any non-zero exit fails the dispatch -- no Codex control
    review runs and the queue will not publish.

    The checks below mirror the five GitHub Actions workflows one-for-one, so
    a verified dispatch means "CI would pass":
        .github/workflows/fmt.yml           cargo +nightly fmt --check
        .github/workflows/architecture.yml  architecture lints + lint tests
        .github/workflows/deny.yml          cargo deny check
        .github/workflows/tests.yml         workspace tests + doctests
        .github/workflows/bench.yml         cargo bench -p rge-script-bench --no-run

    ADR-121 handoff packet validation runs as an advisory-only section after
    the CI-parity checks. It is deliberately not counted as a verification step
    and cannot fail this gate unless a later ADR/dispatch explicitly promotes
    it to blocking behavior.

    FIRST-RUN SETUP. Run this script once by hand before relying on it:
        .\.ai\dispatch.verify.ps1
    It needs the `nightly` toolchain (for rustfmt's nightly-only options) and
    `cargo-deny` installed -- both are mandatory, because CI runs both and a
    skipped check could let a dispatch pass here yet fail CI:
        rustup toolchain install nightly
        cargo install cargo-deny --locked

    TUNING. The two `cargo test --workspace` steps are the slow part. For a
    faster gate, comment them out below (build + lints still run) or point the
    loop at a trimmed copy with `Invoke-AiDispatchLoop.ps1 -VerifyScript`.

.NOTES
    The loop invokes:  powershell.exe -NoProfile -ExecutionPolicy Bypass -File <this>
#>

$ErrorActionPreference = 'Stop'
$script:VerifyWasDotSourced = ($MyInvocation.InvocationName -eq '.')

# --- Locate the repo root (this script lives in <repo>/.ai/) ---------------
$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $RepoRoot

# --- Step runner -----------------------------------------------------------
$script:StepIndex = 0
function Invoke-Step {
    param([string]$Label, [string]$Exe, [string[]]$Arguments)
    $script:StepIndex++
    Write-Output ''
    Write-Output ('=== [{0}] {1} ===' -f $script:StepIndex, $Label)
    Write-Output ('    {0} {1}' -f $Exe, ($Arguments -join ' '))
    $started = Get-Date
    # PS 5.1 turns a native command's stderr into a terminating error under
    # EAP=Stop; cargo banners progress to stderr, so isolate it.
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & $Exe @Arguments
    } finally {
        $ErrorActionPreference = $prevEap
    }
    $code = $LASTEXITCODE
    $elapsed = [int]((Get-Date) - $started).TotalSeconds
    if ($code -ne 0) {
        Write-Output ('--- STEP FAILED: {0} (exit {1}, {2}s) ---' -f $Label, $code, $elapsed)
        Write-Output ('VERIFY FAIL: {0}' -f $Label)
        exit $code
    }
    Write-Output ('--- ok: {0} ({1}s) ---' -f $Label, $elapsed)
}

function Test-CommandRuns {
    param([string]$Exe, [string[]]$Arguments)
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        & $Exe @Arguments *> $null
    } catch {
        $ErrorActionPreference = $prevEap
        return $false
    }
    $ok = ($LASTEXITCODE -eq 0)
    $ErrorActionPreference = $prevEap
    return $ok
}

function Resolve-HandoffAdvisoryDispatchId {
    param([string]$BranchName = '')

    if (-not [string]::IsNullOrWhiteSpace($env:RGE_AI_DISPATCH_ID)) {
        return $env:RGE_AI_DISPATCH_ID.Trim()
    }
    if ([string]::IsNullOrWhiteSpace($BranchName)) {
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $BranchName = (& git branch --show-current 2>$null)
        } finally {
            $ErrorActionPreference = $prevEap
        }
    }
    $BranchName = ([string]$BranchName).Trim()
    if ($BranchName -match '^ai-dispatch/(.+)$') {
        return $Matches[1]
    }
    return $null
}

function Resolve-HandoffAdvisoryPacket {
    param(
        [Parameter(Mandatory)][string]$HandoffDir,
        [Parameter(Mandatory)][string]$DispatchId,
        [Parameter(Mandatory)][ValidateSet('TASK', 'EXEC', 'CORRECT')][string]$PacketType
    )

    if (-not (Test-Path -LiteralPath $HandoffDir)) { return $null }
    $idPattern = [regex]::Escape($DispatchId)
    $typePattern = [regex]::Escape($PacketType)
    $namePattern = "^$idPattern`_$typePattern`_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}[+-]\d{4}\.md$"
    return Get-ChildItem -LiteralPath $HandoffDir -File -Filter '*.md' |
        Where-Object { $_.Name -match $namePattern } |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
}

function Resolve-HandoffAdvisoryPackets {
    param(
        [Parameter(Mandatory)][string]$HandoffDir,
        [Parameter(Mandatory)][string]$DispatchId,
        [Parameter(Mandatory)][ValidateSet('CORRECT')][string]$PacketType
    )

    if (-not (Test-Path -LiteralPath $HandoffDir)) { return @() }
    $idPattern = [regex]::Escape($DispatchId)
    $typePattern = [regex]::Escape($PacketType)
    $namePattern = "^$idPattern`_$typePattern`_\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}[+-]\d{4}\.md$"
    return @(Get-ChildItem -LiteralPath $HandoffDir -File -Filter '*.md' |
        Where-Object { $_.Name -match $namePattern } |
        Sort-Object LastWriteTimeUtc)
}

function Write-HandoffAdvisoryLine {
    param([AllowNull()][object]$Message)
    [Console]::Out.WriteLine([string]$Message)
}

function Invoke-HandoffPacketAdvisoryValidation {
    param(
        [Parameter(Mandatory)][string]$RepoRoot,
        [Parameter(Mandatory)][string]$HandoffDir,
        [Parameter(Mandatory)][string]$ValidatorPath,
        [string]$IntegrationRef = 'main',
        [string]$BranchName = ''
    )

    Write-HandoffAdvisoryLine ''
    Write-HandoffAdvisoryLine '=== [advisory] ADR-121 handoff packet validation ==='

    $dispatchId = Resolve-HandoffAdvisoryDispatchId -BranchName $BranchName
    if ([string]::IsNullOrWhiteSpace($dispatchId)) {
        Write-HandoffAdvisoryLine 'HANDOFF_ADVISORY: SKIP (no dispatch id in RGE_AI_DISPATCH_ID or ai-dispatch/<id> branch)'
        return [pscustomobject]@{ Status = 'SKIP'; DispatchId = $null; ExitCode = 0; Packet = $null; TaskPacket = $null }
    }
    if (-not (Test-Path -LiteralPath $ValidatorPath)) {
        Write-HandoffAdvisoryLine "HANDOFF_ADVISORY: SKIP (validator not found: $ValidatorPath)"
        return [pscustomobject]@{ Status = 'SKIP'; DispatchId = $dispatchId; ExitCode = 0; Packet = $null; TaskPacket = $null }
    }

    $packet = Resolve-HandoffAdvisoryPacket -HandoffDir $HandoffDir -DispatchId $dispatchId -PacketType 'EXEC'
    if (-not $packet) {
        $packet = Resolve-HandoffAdvisoryPacket -HandoffDir $HandoffDir -DispatchId $dispatchId -PacketType 'TASK'
    }
    $taskPacket = Resolve-HandoffAdvisoryPacket -HandoffDir $HandoffDir -DispatchId $dispatchId -PacketType 'TASK'
    if (-not $packet) {
        Write-HandoffAdvisoryLine "HANDOFF_ADVISORY: SKIP (no TASK or EXEC packet found for $dispatchId)"
        return [pscustomobject]@{ Status = 'SKIP'; DispatchId = $dispatchId; ExitCode = 0; Packet = $null; TaskPacket = $null }
    }

    $args = @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $ValidatorPath,
        '-PacketPath', $packet.FullName,
        '-Integration', $IntegrationRef
    )
    if ($taskPacket) {
        $args += @('-TaskPacket', $taskPacket.FullName)
        $overridePackets = Resolve-HandoffAdvisoryPackets -HandoffDir $HandoffDir -DispatchId $dispatchId -PacketType 'CORRECT'
        if ($overridePackets.Count -gt 0) {
            $args += '-PlannerOverridePacket'
            foreach ($override in $overridePackets) { $args += $override.FullName }
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
        $args += @('-ExcludeTouchedPath', $env:CARGO_TARGET_DIR)
    }

    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        $output = & powershell.exe @args 2>&1
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $prevEap
    }
    $reportedVerdict = $null
    foreach ($line in $output) {
        Write-HandoffAdvisoryLine $line
        if ([string]$line -match '^HANDOFF_VALIDATE:\s*(PASS|WARN|FAIL)\s*$') {
            $reportedVerdict = $Matches[1]
        }
    }
    if ($code -eq 0 -and $reportedVerdict -eq 'PASS') {
        Write-HandoffAdvisoryLine 'HANDOFF_ADVISORY: PASS (non-blocking)'
        return [pscustomobject]@{
            Status = 'PASS'; DispatchId = $dispatchId; ExitCode = 0
            Packet = $packet.FullName; TaskPacket = if ($taskPacket) { $taskPacket.FullName } else { $null }
        }
    }

    if ($code -eq 0 -and $reportedVerdict -in @('WARN', 'FAIL')) {
        Write-HandoffAdvisoryLine "HANDOFF_ADVISORY: WARN (validator reported $reportedVerdict; advisory only, verify remains green)"
    } elseif ($code -eq 0) {
        Write-HandoffAdvisoryLine 'HANDOFF_ADVISORY: WARN (validator verdict missing; advisory only, verify remains green)'
    } else {
        Write-HandoffAdvisoryLine "HANDOFF_ADVISORY: WARN (validator exited $code; advisory only, verify remains green)"
    }
    return [pscustomobject]@{
        Status = 'WARN'; DispatchId = $dispatchId; ExitCode = $code
        Packet = $packet.FullName; TaskPacket = if ($taskPacket) { $taskPacket.FullName } else { $null }
    }
}

function Test-VerifyLoadOnlyRequested {
    return (($env:RGE_AI_DISPATCH_VERIFY_LOAD_ONLY -eq '1') -and $script:VerifyWasDotSourced)
}

function Test-VerifySkipMainRequested {
    return ($env:RGE_AI_DISPATCH_VERIFY_SKIP_MAIN -eq '1')
}

function Stop-VerifySkipMainRequested {
    if (-not (Test-VerifySkipMainRequested)) { return }
    # LOUD, signal-classifiable banner. Get-RecordSource (Invoke-AiDispatchGuard.ps1)
    # classifies any line starting with 'VERIFY ' as a status signal. Exit non-zero
    # so unguarded Auto/Queue runs cannot mistake a skipped gate for a real pass.
    Write-Output 'VERIFY SKIPPED: RGE_AI_DISPATCH_VERIFY_SKIP_MAIN=1 -- the build/test gate did NOT run. Interactive-debug only; MUST NOT be set on an autonomous or main-publish run. This is NOT a real pass.'
    exit 1
}

function Stop-VerifyLoadOnlyMisuse {
    if ($env:RGE_AI_DISPATCH_VERIFY_LOAD_ONLY -ne '1') { return }
    if ($script:VerifyWasDotSourced) { return }
    Write-Output 'VERIFY LOAD_ONLY BLOCKED: RGE_AI_DISPATCH_VERIFY_LOAD_ONLY=1 is only valid for dot-sourced test helper loading. This is NOT a real pass.'
    exit 1
}

Stop-VerifySkipMainRequested
Stop-VerifyLoadOnlyMisuse

if (Test-VerifyLoadOnlyRequested) { return }

# --- Ensure cargo is reachable ---------------------------------------------
# Cargo is not always on PATH in unattended sessions; on this machine the
# Rust install lives on the RustCache volume.
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $cargoBin = 'A:\RustCache\cargo\bin'
    if (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe')) {
        $env:PATH = $cargoBin + ';' + $env:PATH
    }
}
if (-not $env:CARGO_HOME -and (Test-Path -LiteralPath 'A:\RustCache\cargo')) {
    $env:CARGO_HOME = 'A:\RustCache\cargo'
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Output 'VERIFY FAIL: cargo not found on PATH and not at A:\RustCache\cargo\bin.'
    exit 127
}

$env:CARGO_TERM_COLOR = 'always'
$env:RUST_BACKTRACE = '1'

Write-Output "RGE dispatch verification -- repo $RepoRoot"
Write-Output "Started $((Get-Date).ToString('o'))"

# --- 0. Worktree-cache hygiene (prevents stale CARGO_MANIFEST_DIR poisoning) --
# A dispatch runs in a linked git worktree that shares one cargo target cache
# (CARGO_TARGET_DIR) with the main checkout. Rust bakes env!("CARGO_MANIFEST_DIR")
# into fixture-reading test binaries at compile time; when a sibling worktree is
# later pruned/merged, those binaries linger in the shared target and a later
# `cargo test` reuses them, panicking "The system cannot find the path specified"
# on assets under the dead worktree path (see ISSUE-258 / AI_DISPATCH_AUTOMATION
# section 7.2). When this gate runs inside a LINKED worktree, reconcile the
# registry and force the fixture-reading crates to recompile against THIS
# worktree's path. The main checkout is skipped (git-dir equals git-common-dir),
# so manual verifies pay nothing.
#
# Regenerate $bakingPkgs with:
#   git grep -l CARGO_MANIFEST_DIR -- "crates/**" "tools/**" "editor/**" "runtime/**"
# mapped to owning [package] names (10 crates as of 2026-06-02). The rule is
# "clean EVERY CARGO_MANIFEST_DIR-embedding crate" with NO per-crate exceptions —
# a per-crate "is this one harmful?" judgement is exactly what let PR #288 fail.
# rge-tool-architecture-lints was added 2026-06-02: its
# forbidden_dep::tests::current_workspace_passes_all_rules walks up from
# CARGO_MANIFEST_DIR to the workspace root, so a poisoned binary fails the gate
# with "expected workspace root Cargo.toml at <dead path>" (PR #288; recurred #289).
# rge-editor's CARGO_MANIFEST_DIR uses are ALL inside its #[cfg(test)] mod tests
# (main.rs ~1698/1757/1798/1844): golden-project + fixture path reads
# (golden-projects/simple-scene/.rge-project, crates/io-gltf .../cube.glb). The
# step-4 `cargo test --workspace --all-targets` compiles and RUNS those tests, so
# a stale-path rge-editor test binary is a real poisoning vector -- the same
# fixture-reading class as the original 8, not a launch-path/binary special case.
$prevEap0 = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    $gitDir = (& git rev-parse --git-dir 2>$null)
    $gitCommonDir = (& git rev-parse --git-common-dir 2>$null)
    if ($gitDir -and $gitCommonDir -and ($gitDir -ne $gitCommonDir)) {
        Write-Output ''
        Write-Output '=== [0] worktree-cache hygiene (linked worktree) ==='
        & git worktree prune 2>$null
        $bakingPkgs = @(
            'rge-data', 'rge-scene-loader', 'rge-runtime-headless',
            'rge-ui-fonts', 'rge-ui-icons', 'rge-io-gltf', 'rge-io-image', 'rge-ui-theme',
            'rge-tool-architecture-lints', 'rge-editor'
        )
        foreach ($p in $bakingPkgs) { & cargo clean -p $p 2>$null }
        Write-Output ('--- ok: pruned registry + refreshed {0} path-embedding crate(s) ---' -f $bakingPkgs.Count)
    }
} finally {
    $ErrorActionPreference = $prevEap0
}

# --- 1. Format (mirrors fmt.yml) -------------------------------------------
# rustfmt.toml uses nightly-only options, so fmt runs on the nightly channel.
Invoke-Step -Label 'cargo +nightly fmt --check' -Exe 'cargo' `
    -Arguments @('+nightly', 'fmt', '--all', '--', '--check')

# --- 2. Architecture lints (mirrors architecture.yml) ----------------------
# `cargo run ... -- all` exits 1 on lint violations, 2 on a tooling error.
Invoke-Step -Label 'architecture lints' -Exe 'cargo' `
    -Arguments @('run', '-q', '-p', 'rge-tool-architecture-lints', '--', 'all')
Invoke-Step -Label 'architecture-lint test suite' -Exe 'cargo' `
    -Arguments @('test', '-p', 'rge-tool-architecture-lints', '--all-targets')

# --- 3. Supply chain (mirrors deny.yml) ------------------------------------
# cargo-deny is mandatory: CI runs it, so a verified dispatch must too.
# A missing local cargo-deny is a hard fail (install it once), not a skip --
# skipping it would let a dispatch pass here yet fail CI on the deny job.
if (-not (Test-CommandRuns -Exe 'cargo' -Arguments @('deny', '--version'))) {
    $script:StepIndex++
    Write-Output ''
    Write-Output ('=== [{0}] cargo deny check ===' -f $script:StepIndex)
    Write-Output 'VERIFY FAIL: cargo-deny is not installed, but CI enforces it.'
    Write-Output 'Install it once with:  cargo install cargo-deny --locked'
    exit 127
}
Invoke-Step -Label 'cargo deny check' -Exe 'cargo' -Arguments @('deny', 'check')

# --- 4. Workspace tests (mirrors tests.yml) -- the slow steps --------------
# --all-targets covers unit + integration tests; --doc covers doctests, which
# --all-targets deliberately excludes. Both must pass for a green workspace.
#
# `-j 1`: Windows/MSVC linker OOMs under default parallelism on this
# workspace (rustc and link.exe both exceed available memory when several
# crates link simultaneously -- observed exit codes 0xc0000142 / 1102 /
# STATUS_STACK_BUFFER_OVERRUN). Serialized workspace tests are slower
# (~10-15 min) but deterministic for unattended dispatch; flaky parallel
# linker crashes would otherwise fail this gate intermittently and block
# every autonomous run.
Invoke-Step -Label 'cargo test --workspace --all-targets' -Exe 'cargo' `
    -Arguments @('test', '--workspace', '--all-targets', '--no-fail-fast', '-j', '1')
Invoke-Step -Label 'cargo test --workspace --doc' -Exe 'cargo' `
    -Arguments @('test', '--workspace', '--doc', '--no-fail-fast', '-j', '1')

# --- 5. Script bench compile (mirrors bench.yml) ---------------------------
# Compile-only: --no-run builds the bench harnesses without executing them,
# matching the bench.yml job which only verifies that benchmarks still build.
Invoke-Step -Label 'cargo bench -p rge-script-bench --no-run' -Exe 'cargo' `
    -Arguments @('bench', '-p', 'rge-script-bench', '--no-run')

$null = Invoke-HandoffPacketAdvisoryValidation `
    -RepoRoot $RepoRoot `
    -HandoffDir (Join-Path $RepoRoot 'ai_handoffs') `
    -ValidatorPath (Join-Path $RepoRoot 'Test-HandoffPacket.ps1') `
    -IntegrationRef 'main'

Write-Output ''
Write-Output ('VERIFY OK: all {0} verification step(s) passed.' -f $script:StepIndex)
exit 0
