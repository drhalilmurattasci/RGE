#Requires -Version 5.1
<#
.SYNOPSIS
    Resolves the Phase 9 default clean-release package set.

.DESCRIPTION
    Uses `cargo metadata --format-version 1 --no-deps` to build a
    machine-readable package set for the default clean-release build. The
    default set excludes the Wasmtime scripting stack from release timing while
    keeping the wasm-named bench wrapper explicit.

.EXAMPLE
    .\tools\Resolve-CleanReleasePackageSet.ps1

.EXAMPLE
    .\tools\Resolve-CleanReleasePackageSet.ps1 -Output Command
#>

[CmdletBinding()]
param(
    [ValidateSet('DefaultCleanRelease')]
    [string]$SetName = 'DefaultCleanRelease',

    [ValidateSet('Json', 'Object', 'Command', 'IncludedNames')]
    [string]$Output = 'Json'
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $RepoRoot

$MandatoryExcludedPackages = @(
    'rge-runtime-wasmtime',
    'rge-runtime-wasmtime-engine',
    'rge-script-host',
    'rge-expr-wasm',
    'rge-script-bench'
)

$WasmBenchPackage = 'rge-tool-wasm-bench'
$WasmBenchRationale = 'Included because the package is a wasm-named wrapper but current metadata shows no dependency on wasmtime or the excluded scripting packages; excluding it by name alone would hide a current non-Wasmtime package.'

function ConvertTo-CommandLine {
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

function Get-WorkspacePackageClosure {
    param(
        [Parameter(Mandatory)][string]$PackageName,
        [Parameter(Mandatory)][hashtable]$PackagesByName
    )

    $seen = New-Object 'System.Collections.Generic.HashSet[string]'
    $queue = New-Object 'System.Collections.Generic.Queue[string]'
    $queue.Enqueue($PackageName)

    while ($queue.Count -gt 0) {
        $current = $queue.Dequeue()
        if (-not $seen.Add($current)) { continue }
        if (-not $PackagesByName.ContainsKey($current)) { continue }

        foreach ($dependency in @($PackagesByName[$current].dependencies)) {
            $dependencyName = [string]$dependency.name
            if ([string]::IsNullOrWhiteSpace($dependencyName)) { continue }
            if (-not $seen.Contains($dependencyName)) {
                $queue.Enqueue($dependencyName)
            }
        }
    }

    return @($seen)
}

function Resolve-DefaultCleanReleasePackageSet {
    $metadataText = (& cargo metadata --format-version 1 --no-deps) -join "`n"
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata --format-version 1 --no-deps failed with exit $LASTEXITCODE."
    }

    $metadata = $metadataText | ConvertFrom-Json
    $packagesById = @{}
    $packagesByName = @{}
    $duplicateNames = @()

    foreach ($package in @($metadata.packages)) {
        $name = [string]$package.name
        $id = [string]$package.id

        if ($packagesByName.ContainsKey($name)) {
            $duplicateNames += $name
        } else {
            $packagesByName[$name] = $package
        }

        $packagesById[$id] = $package
    }

    if ($duplicateNames.Count -gt 0) {
        $joined = (($duplicateNames | Sort-Object -Unique) -join ', ')
        throw "Duplicate workspace package names are not supported by the default clean-release package set: $joined"
    }

    $workspacePackages = @()
    foreach ($memberId in @($metadata.workspace_members)) {
        $id = [string]$memberId
        if (-not $packagesById.ContainsKey($id)) {
            throw "cargo metadata workspace member id was not present in packages: $id"
        }
        $workspacePackages += $packagesById[$id]
    }

    $workspaceNames = @($workspacePackages | ForEach-Object { [string]$_.name })
    $missingExcluded = @($MandatoryExcludedPackages | Where-Object { -not $packagesByName.ContainsKey($_) })
    if ($missingExcluded.Count -gt 0) {
        throw "Mandatory excluded package(s) missing from cargo metadata: $($missingExcluded -join ', ')"
    }
    if (-not $packagesByName.ContainsKey($WasmBenchPackage)) {
        throw "Required wasm bench decision package missing from cargo metadata: $WasmBenchPackage"
    }

    $excludedSet = New-Object 'System.Collections.Generic.HashSet[string]'
    foreach ($name in $MandatoryExcludedPackages) {
        [void]$excludedSet.Add($name)
    }

    $wasmBenchClosure = @(Get-WorkspacePackageClosure -PackageName $WasmBenchPackage -PackagesByName $packagesByName)
    $wasmBenchForbiddenDependencies = @(
        $wasmBenchClosure |
            Where-Object { $_ -eq 'wasmtime' -or $excludedSet.Contains($_) } |
            Sort-Object -Unique
    )
    if ($wasmBenchForbiddenDependencies.Count -gt 0) {
        throw "$WasmBenchPackage now depends on excluded Wasmtime/scripting package(s): $($wasmBenchForbiddenDependencies -join ', ')"
    }

    $includedNames = @()
    foreach ($name in $workspaceNames) {
        if (-not $excludedSet.Contains($name)) {
            $includedNames += $name
        }
    }

    $bothIncludedAndExcluded = @($includedNames | Where-Object { $excludedSet.Contains($_) })
    if ($bothIncludedAndExcluded.Count -gt 0) {
        throw "Package(s) cannot be both included and excluded: $($bothIncludedAndExcluded -join ', ')"
    }

    $cargoArguments = @('build', '--release')
    foreach ($name in $includedNames) {
        $cargoArguments += '-p'
        $cargoArguments += $name
    }

    return [pscustomobject]@{
        schema_version = 'clean-release-package-set-v1'
        set_name = $SetName
        metadata_command = 'cargo metadata --format-version 1 --no-deps'
        workspace_package_count = $workspaceNames.Count
        included_package_count = $includedNames.Count
        excluded_package_count = $MandatoryExcludedPackages.Count
        included_package_names = $includedNames
        excluded_package_names = $MandatoryExcludedPackages
        wasm_bench_decision = [pscustomobject]@{
            package = $WasmBenchPackage
            decision = 'include'
            rationale = $WasmBenchRationale
            dependency_closure_checked = $wasmBenchClosure
        }
        cargo_executable = 'cargo'
        cargo_arguments = $cargoArguments
        cargo_command = ConvertTo-CommandLine -Exe 'cargo' -Arguments $cargoArguments
        validation = [pscustomobject]@{
            all_included_names_resolved = $true
            all_excluded_names_resolved = $true
            duplicate_package_names = @()
            both_included_and_excluded = @()
            generated_command_uses_workspace_selector = $false
        }
    }
}

$result = Resolve-DefaultCleanReleasePackageSet

switch ($Output) {
    'Object' {
        $result
    }
    'Command' {
        $result.cargo_command
    }
    'IncludedNames' {
        $result.included_package_names
    }
    'Json' {
        $result | ConvertTo-Json -Depth 8
    }
}
