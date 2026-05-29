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

# --- Locate the repo root (this script lives in <repo>/.ai/) ---------------
$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $RepoRoot

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
#   git grep -l CARGO_MANIFEST_DIR -- "crates/**/tests/**" "crates/ui-fonts/src/**" "runtime/**/tests/**"
# mapped to owning [package] names. rge-editor + rge-tool-architecture-lints also
# read CARGO_MANIFEST_DIR but for non-asset purposes (editor launch path /
# workspace-root discovery), so they are intentionally excluded to keep the
# per-dispatch refresh cheap.
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
            'rge-ui-fonts', 'rge-ui-icons', 'rge-io-gltf', 'rge-io-image', 'rge-ui-theme'
        )
        foreach ($p in $bakingPkgs) { & cargo clean -p $p 2>$null }
        Write-Output ('--- ok: pruned registry + refreshed {0} fixture crate(s) ---' -f $bakingPkgs.Count)
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

Write-Output ''
Write-Output ('VERIFY OK: all {0} verification step(s) passed.' -f $script:StepIndex)
exit 0
