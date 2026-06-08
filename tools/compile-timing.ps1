#Requires -Version 5.1
<#
.SYNOPSIS
    Warm-cache compile timing harness for RGE.

.DESCRIPTION
    Measures wall-clock time for Cargo `check` and/or `build` runs using the
    current target cache. Workspace timing remains the default. Release build
    timing can opt into the Phase 9 DefaultCleanRelease package set, resolved
    by tools/Resolve-CleanReleasePackageSet.ps1. This script intentionally
    does not delete target directories and does not run `cargo clean`;
    destructive clean-build certification must stay in a separately authorized
    task.

.EXAMPLE
    .\tools\compile-timing.ps1 -Mode both -Iterations 1

.EXAMPLE
    .\tools\compile-timing.ps1 -Mode check -AllTargets -Iterations 3 -JsonPath .ai\compile-timing.json

.EXAMPLE
    .\tools\compile-timing.ps1 -Mode build -Release -PackageSet DefaultCleanRelease -Iterations 1
#>

[CmdletBinding()]
param(
    [ValidateSet('check', 'build', 'both')]
    [string]$Mode = 'both',

    [ValidateRange(1, 50)]
    [int]$Iterations = 1,

    [switch]$AllTargets,

    [switch]$Release,

    [ValidateSet('Workspace', 'DefaultCleanRelease')]
    [string]$PackageSet = 'Workspace',

    [ValidateRange(0, 86400)]
    [int]$TimeoutSeconds = 0,

    [string]$JsonPath = '',

    [switch]$NoDefaultRustCache
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $RepoRoot

function Add-PathPrefixIfMissing {
    param([Parameter(Mandatory)][string]$PathPrefix)

    if (-not (Test-Path -LiteralPath $PathPrefix)) { return }
    $parts = @($env:PATH -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    foreach ($part in $parts) {
        if ([string]::Equals($part, $PathPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            return
        }
    }
    $env:PATH = $PathPrefix + ';' + $env:PATH
}

function Use-DefaultRustCacheIfPresent {
    if ($NoDefaultRustCache) { return }

    $cargoHome = 'A:\RustCache\cargo'
    $rustupHome = 'A:\RustCache\rustup'
    $targetDir = 'A:\RustCache\target'

    if (-not $env:CARGO_HOME -and (Test-Path -LiteralPath $cargoHome)) {
        $env:CARGO_HOME = $cargoHome
    }
    if (-not $env:RUSTUP_HOME -and (Test-Path -LiteralPath $rustupHome)) {
        $env:RUSTUP_HOME = $rustupHome
    }
    if (-not $env:CARGO_TARGET_DIR -and (Test-Path -LiteralPath $targetDir)) {
        $env:CARGO_TARGET_DIR = $targetDir
    }
    Add-PathPrefixIfMissing -PathPrefix (Join-Path $cargoHome 'bin')
}

function Invoke-NativeCapture {
    param(
        [Parameter(Mandatory)][string]$Exe,
        [string[]]$Arguments = @(),
        [int]$TimeoutSeconds = 0
    )

    $timedOut = $false

    $command = Get-Command $Exe -ErrorAction Stop
    $exePath = $command.Source
    if ([string]::IsNullOrWhiteSpace($exePath)) {
        $exePath = $Exe
    }

    $stdoutPath = [System.IO.Path]::GetTempFileName()
    $stderrPath = [System.IO.Path]::GetTempFileName()
    $output = @()
    $exitCode = 1
    $process = $null
    try {
        $tokens = @((ConvertTo-CmdToken -Value $exePath))
        foreach ($arg in $Arguments) {
            $tokens += (ConvertTo-CmdToken -Value $arg)
        }
        $cmdLine = (($tokens -join ' ') + ' 1> ' + (ConvertTo-CmdToken -Value $stdoutPath) + ' 2> ' + (ConvertTo-CmdToken -Value $stderrPath))

        $psi = New-Object System.Diagnostics.ProcessStartInfo
        $psi.FileName = if ($env:ComSpec) { $env:ComSpec } else { 'cmd.exe' }
        $psi.Arguments = '/d /c ' + $cmdLine
        $psi.UseShellExecute = $false
        $psi.CreateNoWindow = $true

        $process = New-Object System.Diagnostics.Process
        $process.StartInfo = $psi
        [void]$process.Start()

        if ($TimeoutSeconds -gt 0) {
            $exited = $process.WaitForExit($TimeoutSeconds * 1000)
            if (-not $exited) {
                $timedOut = $true
                Stop-ProcessTree -ProcessId $process.Id
                $process.WaitForExit()
            }
        } else {
            $process.WaitForExit()
        }
        $process.Refresh()

        $exitCode = $process.ExitCode
        if ($timedOut) {
            $exitCode = 124
        }

        $stdout = Get-Content -Raw -LiteralPath $stdoutPath -ErrorAction SilentlyContinue
        $stderr = Get-Content -Raw -LiteralPath $stderrPath -ErrorAction SilentlyContinue

        if (-not [string]::IsNullOrEmpty($stdout)) {
            $output += @($stdout -split "\r?\n" | Where-Object { $_ -ne '' })
        }
        if (-not [string]::IsNullOrEmpty($stderr)) {
            $output += @($stderr -split "\r?\n" | Where-Object { $_ -ne '' })
        }
    } finally {
        Remove-Item -LiteralPath $stdoutPath -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $stderrPath -Force -ErrorAction SilentlyContinue
    }

    if ($timedOut) {
        $output += "TIMEOUT: command exceeded $TimeoutSeconds seconds and was stopped."
    }

    return [pscustomobject]@{
        ExitCode = $exitCode
        TimedOut = $timedOut
        Output = @($output | ForEach-Object { [string]$_ })
    }
}

function ConvertTo-CmdToken {
    param([Parameter(Mandatory)][string]$Value)

    if ($Value -notmatch '[\s"&|<>^]') {
        return $Value
    }
    return '"' + ($Value -replace '"', '\"') + '"'
}

function Stop-ProcessTree {
    param([Parameter(Mandatory)][int]$ProcessId)

    $children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$ProcessId" -ErrorAction SilentlyContinue)
    foreach ($child in $children) {
        Stop-ProcessTree -ProcessId ([int]$child.ProcessId)
    }
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($process) {
        Stop-Process -Id $ProcessId -Force -ErrorAction SilentlyContinue
    }
}

function Format-CommandLine {
    param(
        [Parameter(Mandatory)][string]$Exe,
        [string[]]$Arguments = @()
    )

    $tokens = @($Exe)
    foreach ($arg in $Arguments) {
        if ($arg -match '\s') {
            $tokens += ('"{0}"' -f ($arg -replace '"', '\"'))
        } else {
            $tokens += $arg
        }
    }
    return ($tokens -join ' ')
}

function New-CargoArguments {
    param([Parameter(Mandatory)][ValidateSet('check', 'build')][string]$Kind)

    $args = @($Kind)
    if ($Release) { $args += '--release' }
    if ($PackageSet -eq 'DefaultCleanRelease') {
        $packageSetInfo = Get-DefaultCleanReleasePackageSet
        foreach ($packageName in @($packageSetInfo.included_package_names)) {
            $args += '-p'
            $args += [string]$packageName
        }
    } else {
        $args += '--workspace'
    }
    if ($AllTargets) { $args += '--all-targets' }
    return $args
}

function Get-DefaultCleanReleasePackageSet {
    if ($script:DefaultCleanReleasePackageSet) {
        return $script:DefaultCleanReleasePackageSet
    }

    $resolverPath = Join-Path $PSScriptRoot 'Resolve-CleanReleasePackageSet.ps1'
    if (-not (Test-Path -LiteralPath $resolverPath)) {
        throw "Missing clean-release package-set resolver: $resolverPath"
    }

    $script:DefaultCleanReleasePackageSet = & $resolverPath -SetName DefaultCleanRelease -Output Object
    return $script:DefaultCleanReleasePackageSet
}

function Invoke-TimedCargo {
    param(
        [Parameter(Mandatory)][string]$Label,
        [Parameter(Mandatory)][string[]]$Arguments
    )

    Write-Host ''
    Write-Host ('=== {0} ===' -f $Label)
    Write-Host ('    {0}' -f (Format-CommandLine -Exe 'cargo' -Arguments $Arguments))

    $started = Get-Date
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $capture = Invoke-NativeCapture -Exe 'cargo' -Arguments $Arguments -TimeoutSeconds $TimeoutSeconds
    $sw.Stop()
    $completed = Get-Date

    foreach ($line in $capture.Output) {
        Write-Host $line
    }

    $finishedLine = $null
    foreach ($line in $capture.Output) {
        if ($line -match '^\s*Finished\s+') {
            $finishedLine = $line.Trim()
        }
    }

    $result = [pscustomobject]@{
        label = $Label
        command = Format-CommandLine -Exe 'cargo' -Arguments $Arguments
        exit_code = $capture.ExitCode
        timed_out = [bool]$capture.TimedOut
        wall_seconds = [Math]::Round($sw.Elapsed.TotalSeconds, 3)
        cargo_finished_line = $finishedLine
        started_at = $started.ToString('o')
        completed_at = $completed.ToString('o')
        cargo_target_dir = $env:CARGO_TARGET_DIR
    }

    if ($capture.TimedOut) {
        Write-Host ('--- TIMEOUT: {0} ({1}s limit, {2}s wall) ---' -f $Label, $TimeoutSeconds, $result.wall_seconds)
    } elseif ($capture.ExitCode -ne 0) {
        Write-Host ('--- FAILED: {0} (exit {1}, {2}s) ---' -f $Label, $capture.ExitCode, $result.wall_seconds)
    } else {
        Write-Host ('--- ok: {0} ({1}s) ---' -f $Label, $result.wall_seconds)
    }

    return $result
}

Use-DefaultRustCacheIfPresent

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error 'cargo not found on PATH and not available through A:\RustCache\cargo\bin.'
    exit 127
}

if ($PackageSet -eq 'DefaultCleanRelease') {
    if ($Mode -ne 'build') {
        Write-Error 'PackageSet DefaultCleanRelease is only valid with -Mode build.'
        exit 2
    }
    if (-not $Release) {
        Write-Error 'PackageSet DefaultCleanRelease requires -Release.'
        exit 2
    }
}

$cargoVersion = (Invoke-NativeCapture -Exe 'cargo' -Arguments @('--version')).Output -join "`n"
$rustcVersion = ''
if (Get-Command rustc -ErrorAction SilentlyContinue) {
    $rustcVersion = (Invoke-NativeCapture -Exe 'rustc' -Arguments @('--version')).Output -join "`n"
}

$runStarted = Get-Date
Write-Host "RGE compile timing harness -- repo $RepoRoot"
Write-Host "Started $($runStarted.ToString('o'))"
Write-Host "cargo: $cargoVersion"
if (-not [string]::IsNullOrWhiteSpace($rustcVersion)) {
    Write-Host "rustc: $rustcVersion"
}
Write-Host "CARGO_HOME=$env:CARGO_HOME"
Write-Host "RUSTUP_HOME=$env:RUSTUP_HOME"
Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host 'Cache policy: warm-cache only; no target deletion and no cargo clean.'
Write-Host "Package set: $PackageSet"
if ($PackageSet -eq 'DefaultCleanRelease') {
    $packageSetInfo = Get-DefaultCleanReleasePackageSet
    Write-Host ('Package set command: {0}' -f $packageSetInfo.cargo_command)
    Write-Host ('Package set included: {0}; excluded: {1}' -f $packageSetInfo.included_package_count, ($packageSetInfo.excluded_package_names -join ', '))
    Write-Host ('Package set wasm bench decision: {0} {1}' -f $packageSetInfo.wasm_bench_decision.package, $packageSetInfo.wasm_bench_decision.decision)
}

$kinds = @()
switch ($Mode) {
    'check' { $kinds = @('check') }
    'build' { $kinds = @('build') }
    'both' { $kinds = @('check', 'build') }
}

$results = @()
$failed = $false
for ($i = 1; $i -le $Iterations; $i++) {
    foreach ($kind in $kinds) {
        $label = '{0} iteration {1}/{2}' -f $kind, $i, $Iterations
        $result = Invoke-TimedCargo -Label $label -Arguments (New-CargoArguments -Kind $kind)
        $results += $result
        if ($result.exit_code -ne 0) {
            $failed = $true
            break
        }
    }
    if ($failed) { break }
}

$payload = [pscustomobject]@{
    repo_root = $RepoRoot
    started_at = $runStarted.ToString('o')
    completed_at = (Get-Date).ToString('o')
    mode = $Mode
    iterations = $Iterations
    all_targets = [bool]$AllTargets
    release = [bool]$Release
    package_set = $PackageSet
    package_set_info = if ($PackageSet -eq 'DefaultCleanRelease') { Get-DefaultCleanReleasePackageSet } else { $null }
    timeout_seconds = $TimeoutSeconds
    no_default_rust_cache = [bool]$NoDefaultRustCache
    cargo_version = $cargoVersion
    rustc_version = $rustcVersion
    cargo_home = $env:CARGO_HOME
    rustup_home = $env:RUSTUP_HOME
    cargo_target_dir = $env:CARGO_TARGET_DIR
    results = $results
}

Write-Host ''
Write-Host '=== Summary ==='
$results |
    Select-Object label, exit_code, timed_out, wall_seconds, cargo_finished_line |
    Format-Table -AutoSize |
    Out-String |
    Write-Host

if (-not [string]::IsNullOrWhiteSpace($JsonPath)) {
    $resolvedJsonPath = $JsonPath
    if (-not [System.IO.Path]::IsPathRooted($resolvedJsonPath)) {
        $resolvedJsonPath = Join-Path $RepoRoot $resolvedJsonPath
    }
    $jsonParent = Split-Path -Parent $resolvedJsonPath
    if (-not [string]::IsNullOrWhiteSpace($jsonParent) -and -not (Test-Path -LiteralPath $jsonParent)) {
        New-Item -ItemType Directory -Path $jsonParent | Out-Null
    }
    $payload | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $resolvedJsonPath -Encoding UTF8
    Write-Host "Wrote JSON: $resolvedJsonPath"
}

if ($failed) {
    exit 1
}
