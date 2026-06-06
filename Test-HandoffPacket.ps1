#Requires -Version 5.1
<#
.SYNOPSIS
    Advisory validator for AI handoff packets and optional scope envelopes.

.DESCRIPTION
    Standalone, advisory-first tooling for ADR-121. It validates only protocol
    invariants that already exist in ai_handoffs/AI_HANDOFF_PROTOCOL.md:
    footer shape, core header fields, optional sidecar consistency, closeout
    evidence, and optional TASK envelope scope checks.

    This script is deliberately not wired into .ai/dispatch.verify.ps1. By
    default it reports FAIL in output but exits 0 so it can be smoked safely.
    Pass -Blocking to make a FAIL exit 2.

.EXAMPLE
    .\Test-HandoffPacket.ps1 -PacketPath ai_handoffs\ISSUE-1_EXEC_2026-06-06_00-00-00+0300.md

.EXAMPLE
    .\Test-HandoffPacket.ps1 -PacketPath ai_handoffs\ISSUE-1_EXEC_*.md `
      -TaskPacket ai_handoffs\ISSUE-1_TASK_*.md -Integration origin/main
#>
[CmdletBinding()]
param(
    [string]$PacketPath,

    [string]$TaskPacket,

    [string[]]$PlannerOverridePacket = @(),

    [string]$Integration = 'main',

    [switch]$JsonOnly,

    [switch]$Blocking
)

$ErrorActionPreference = 'Stop'

$script:ValidHandoffStatus = @('COMPLETE', 'FAILED', 'BLOCKED', 'NEEDS_HUMAN')
$script:ValidNextRole = @('EXECUTOR_AI', 'REVIEWER_AI', 'PLANNER_AI', 'HUMAN_ARBITER', 'NONE')
$script:ValidStatus = @(
    'OPEN', 'AWAITING_REVIEW', 'BLOCKED', 'NEEDS_HUMAN', 'APPROVED',
    'NEEDS_CORRECTION', 'REJECTED', 'CORRECTION_OPEN', 'CLOSED', 'ABANDONED'
)

function Fail {
    param([string]$Message)
    [Console]::Error.WriteLine($Message)
    exit 1
}

function Normalize-HandoffPath {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Path)
    $p = ($Path -replace '\\', '/').Trim()
    while ($p.StartsWith('./')) { $p = $p.Substring(2) }
    return $p
}

function Get-HandoffFirstField {
    param(
        [Parameter(Mandatory)][string]$Text,
        [Parameter(Mandatory)][string]$Key
    )
    $escaped = [regex]::Escape($Key)
    $m = [regex]::Match($Text, "(?m)^$escaped[ \t]*:[ \t]*(.*?)[ \t]*$")
    if ($m.Success) { return $m.Groups[1].Value }
    return $null
}

function Get-HandoffRelatedFiles {
    param([Parameter(Mandatory)][string]$Text)
    $items = @()
    $inBlock = $false
    foreach ($line in ($Text -split "`r?`n")) {
        if ($line -match '^RELATED_FILES:[ \t]*$') {
            $inBlock = $true
            continue
        }
        if ($inBlock) {
            if ($line -match '^[ \t]*-[ \t]+(.+?)[ \t]*$') {
                $items += $Matches[1]
            } elseif ($line.Trim() -ne '') {
                break
            }
        }
    }
    return $items
}

function Get-HandoffPacketType {
    param([Parameter(Mandatory)][string]$Path)
    $name = Split-Path -Leaf $Path
    if ($name -match '^(?<id>.+)_(?<type>TASK|EXEC|REVIEW|CORRECT|CLOSEOUT)_(?<ts>\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}[+-]\d{4})\.md$') {
        return [ordered]@{
            valid       = $true
            dispatch_id = $Matches['id']
            packet_type = $Matches['type']
            timestamp   = $Matches['ts']
        }
    }
    return [ordered]@{
        valid       = $false
        dispatch_id = $null
        packet_type = $null
        timestamp   = $null
    }
}

function Get-HandoffFooter {
    param([Parameter(Mandatory)][string]$Text)

    $errors = @()
    $normalized = $Text -replace "`r`n", "`n"
    $lines = $normalized -split "`n", -1

    $last = $lines.Count - 1
    while ($last -ge 0 -and $lines[$last].Trim() -eq '') { $last-- }
    if ($last -lt 0 -or $lines[$last].Trim() -ne '---') {
        $errors += 'footer missing final horizontal rule at EOF'
        return [ordered]@{ ok = $false; fields = [ordered]@{}; errors = $errors }
    }

    $first = $last - 1
    while ($first -ge 0 -and $lines[$first].Trim() -ne '---') { $first-- }
    if ($first -lt 0) {
        $errors += 'footer missing opening horizontal rule'
        return [ordered]@{ ok = $false; fields = [ordered]@{}; errors = $errors }
    }

    $payload = @()
    for ($i = $first + 1; $i -lt $last; $i++) {
        $trimmed = $lines[$i].Trim()
        if ($trimmed -ne '') { $payload += $trimmed }
    }

    $expected = @('HANDOFF_STATUS', 'DISPATCH_ID', 'AUTHOR', 'NEXT_ROLE', 'EXIT_CODE')
    if ($payload.Count -ne $expected.Count) {
        $errors += "footer has $($payload.Count) non-empty key line(s); expected $($expected.Count)"
    }

    $fields = [ordered]@{}
    for ($i = 0; $i -lt $expected.Count; $i++) {
        $key = $expected[$i]
        if ($i -ge $payload.Count) {
            $errors += "footer missing key: $key"
            continue
        }
        $line = $payload[$i]
        if ($line -notmatch "^$([regex]::Escape($key)):[ \t]*(.*)$") {
            $errors += "footer key $($i + 1) must be $key"
            continue
        }
        $fields[$key] = $Matches[1].Trim()
    }

    return [ordered]@{
        ok     = ($errors.Count -eq 0)
        fields = $fields
        errors = $errors
    }
}

function Get-HandoffSectionBody {
    param(
        [Parameter(Mandatory)][string]$Text,
        [Parameter(Mandatory)][string]$HeadingRegex
    )
    $m = [regex]::Match($Text, "(?ms)^##[ \t]+$HeadingRegex.*?\r?\n(.*?)(?=^##[ \t]+|\z)")
    if ($m.Success) { return $m.Groups[1].Value.Trim() }
    return $null
}

function Test-HandoffCloseoutEvidence {
    param(
        [Parameter(Mandatory)][string]$Text,
        [Parameter(Mandatory)][string]$Status
    )

    $errors = @()
    $checks = @(
        @{ name = 'Final Commit(s)'; heading = 'Final Commit' },
        @{ name = 'Verification Gates'; heading = 'Verification Gates' },
        @{ name = 'Test Count Delta'; heading = 'Test Count Delta' },
        @{ name = 'Remaining Risks Carried Forward'; heading = 'Remaining Risks' },
        @{ name = 'Suggested Follow-On Tasks'; heading = 'Suggested Follow-On' }
    )

    foreach ($check in $checks) {
        $body = Get-HandoffSectionBody -Text $Text -HeadingRegex $check.heading
        if ([string]::IsNullOrWhiteSpace($body)) {
            $errors += "closeout missing non-empty section: $($check.name)"
            continue
        }
        if ($body -match '<[^>]+>') {
            $errors += "closeout section still has placeholder text: $($check.name)"
        }
    }

    $commitBody = Get-HandoffSectionBody -Text $Text -HeadingRegex 'Final Commit'
    if ($Status -eq 'CLOSED' -and $commitBody -and $commitBody -notmatch '\b[0-9a-fA-F]{7,40}\b') {
        $errors += 'closed closeout must list at least one commit hash'
    }

    return $errors
}

function Compare-HandoffSidecarField {
    param(
        [Parameter(Mandatory)]$Sidecar,
        [Parameter(Mandatory)][string]$Name,
        [AllowNull()]$Expected
    )
    $prop = $Sidecar.PSObject.Properties[$Name]
    if (-not $prop) { return "sidecar missing field: $Name" }
    $actual = $prop.Value
    if ($null -eq $Expected) { $Expected = '' }
    if ([string]$actual -ne [string]$Expected) {
        return "sidecar $Name mismatch: '$actual' != '$Expected'"
    }
    return $null
}

function Test-HandoffPacketFile {
    param([Parameter(Mandatory)][string]$Path)

    $errors = @()
    $warnings = @()
    if (-not (Test-Path -LiteralPath $Path)) {
        return [ordered]@{
            verdict = 'FAIL'
            errors = @("packet not found: $Path")
            warnings = @()
            packet_type = $null
            dispatch_id = $null
        }
    }

    $item = Get-Item -LiteralPath $Path
    $text = [System.IO.File]::ReadAllText($item.FullName)
    $nameInfo = Get-HandoffPacketType -Path $item.Name
    if (-not $nameInfo['valid']) {
        $errors += "not a canonical packet filename: $($item.Name)"
    }

    $headers = [ordered]@{
        DISPATCH_ID = Get-HandoffFirstField -Text $text -Key 'DISPATCH_ID'
        AUTHOR      = Get-HandoffFirstField -Text $text -Key 'AUTHOR'
        TIMESTAMP   = Get-HandoffFirstField -Text $text -Key 'TIMESTAMP'
        STATUS      = Get-HandoffFirstField -Text $text -Key 'STATUS'
    }
    foreach ($key in $headers.Keys) {
        if ([string]::IsNullOrWhiteSpace($headers[$key])) {
            $errors += "missing header field: $key"
        } elseif ($headers[$key] -match '<[^>]+>') {
            $errors += "header field still has placeholder text: $key"
        }
    }
    if ($headers['STATUS'] -and $headers['STATUS'] -notmatch '<[^>]+>' -and $headers['STATUS'] -notin $script:ValidStatus) {
        $errors += "invalid STATUS: $($headers['STATUS'])"
    }

    $relatedFiles = Get-HandoffRelatedFiles -Text $text
    if ($relatedFiles.Count -eq 0) {
        $errors += 'missing RELATED_FILES entries'
    }

    $footer = Get-HandoffFooter -Text $text
    $errors += $footer['errors']
    if ($footer['fields'].Count -gt 0) {
        if ($footer['fields']['HANDOFF_STATUS'] -notin $script:ValidHandoffStatus) {
            $errors += "invalid HANDOFF_STATUS: $($footer['fields']['HANDOFF_STATUS'])"
        }
        if ($footer['fields']['NEXT_ROLE'] -notin $script:ValidNextRole) {
            $errors += "invalid NEXT_ROLE: $($footer['fields']['NEXT_ROLE'])"
        }
        if ($footer['fields']['EXIT_CODE'] -notmatch '^-?\d+$') {
            $errors += "EXIT_CODE is not an integer: $($footer['fields']['EXIT_CODE'])"
        }
        if ($headers['DISPATCH_ID'] -and $footer['fields']['DISPATCH_ID'] -and $headers['DISPATCH_ID'] -ne $footer['fields']['DISPATCH_ID']) {
            $errors += "footer DISPATCH_ID does not match header: $($footer['fields']['DISPATCH_ID']) != $($headers['DISPATCH_ID'])"
        }
        if ($headers['AUTHOR'] -and $footer['fields']['AUTHOR'] -and $headers['AUTHOR'] -ne $footer['fields']['AUTHOR']) {
            $errors += "footer AUTHOR does not match header: $($footer['fields']['AUTHOR']) != $($headers['AUTHOR'])"
        }
    }

    if ($nameInfo['valid'] -and $headers['DISPATCH_ID'] -and $nameInfo['dispatch_id'] -ne $headers['DISPATCH_ID']) {
        $errors += "filename dispatch id does not match header: $($nameInfo['dispatch_id']) != $($headers['DISPATCH_ID'])"
    }

    if ($nameInfo['packet_type'] -eq 'CLOSEOUT') {
        $errors += Test-HandoffCloseoutEvidence -Text $text -Status $headers['STATUS']
    }

    $sidecarPath = $item.FullName -replace '\.md$', '.meta.json'
    if (Test-Path -LiteralPath $sidecarPath) {
        try {
            $sidecar = Get-Content -Raw -LiteralPath $sidecarPath | ConvertFrom-Json
            $sidecarErrors = @()
            $sidecarErrors += Compare-HandoffSidecarField -Sidecar $sidecar -Name 'dispatch_id' -Expected $headers['DISPATCH_ID']
            $sidecarErrors += Compare-HandoffSidecarField -Sidecar $sidecar -Name 'packet_type' -Expected $nameInfo['packet_type']
            $sidecarErrors += Compare-HandoffSidecarField -Sidecar $sidecar -Name 'author' -Expected $headers['AUTHOR']
            $sidecarErrors += Compare-HandoffSidecarField -Sidecar $sidecar -Name 'timestamp' -Expected $headers['TIMESTAMP']
            $sidecarErrors += Compare-HandoffSidecarField -Sidecar $sidecar -Name 'status' -Expected $headers['STATUS']
            $sidecarErrors += Compare-HandoffSidecarField -Sidecar $sidecar -Name 'handoff_status' -Expected $footer['fields']['HANDOFF_STATUS']
            $sidecarErrors += Compare-HandoffSidecarField -Sidecar $sidecar -Name 'next_role' -Expected $footer['fields']['NEXT_ROLE']
            $sidecarErrors += Compare-HandoffSidecarField -Sidecar $sidecar -Name 'exit_code' -Expected $footer['fields']['EXIT_CODE']
            $errors += @($sidecarErrors | Where-Object { $_ })
        } catch {
            $errors += "sidecar is not valid JSON: $($_.Exception.Message)"
        }
    }

    $verdict = if ($errors.Count -gt 0) { 'FAIL' } elseif ($warnings.Count -gt 0) { 'WARN' } else { 'PASS' }
    return [ordered]@{
        verdict       = $verdict
        packet        = $item.FullName
        packet_type   = $nameInfo['packet_type']
        dispatch_id   = $headers['DISPATCH_ID']
        errors        = @($errors)
        warnings      = @($warnings)
        sidecar_found = (Test-Path -LiteralPath $sidecarPath)
    }
}

function Convert-HandoffGlobBodyToRegex {
    param([Parameter(Mandatory)][string]$Glob)
    $out = New-Object System.Text.StringBuilder
    $i = 0
    while ($i -lt $Glob.Length) {
        if ($i + 3 -le $Glob.Length -and $Glob.Substring($i, 3) -eq '**/') {
            [void]$out.Append('(?:.*/)?')
            $i += 3
            continue
        }
        if ($i + 2 -le $Glob.Length -and $Glob.Substring($i, 2) -eq '**') {
            [void]$out.Append('.*')
            $i += 2
            continue
        }
        $ch = $Glob[$i]
        if ($ch -eq '*') {
            [void]$out.Append('[^/]*')
        } else {
            [void]$out.Append([regex]::Escape([string]$ch))
        }
        $i++
    }
    return $out.ToString()
}

function Convert-HandoffGlobToRegex {
    param([Parameter(Mandatory)][string]$Glob)
    $g = Normalize-HandoffPath $Glob
    if ([string]::IsNullOrWhiteSpace($g)) {
        throw 'empty glob in handoff envelope'
    }
    if ($g -match '[{}]') {
        throw "brace expansion is not supported in handoff envelope glob: $g"
    }
    if ($g.EndsWith('/**')) {
        $prefix = $g.Substring(0, $g.Length - 3)
        return '^' + (Convert-HandoffGlobBodyToRegex -Glob $prefix) + '(?:/.*)?$'
    }
    return '^' + (Convert-HandoffGlobBodyToRegex -Glob $g) + '$'
}

function Test-HandoffAnyGlob {
    param(
        [Parameter(Mandatory)][string]$Path,
        [string[]]$Globs = @()
    )
    $normalized = Normalize-HandoffPath $Path
    foreach ($glob in $Globs) {
        $rx = Convert-HandoffGlobToRegex -Glob $glob
        if ($normalized -match $rx) { return $true }
    }
    return $false
}

function Get-HandoffEnvelopeList {
    param(
        [Parameter(Mandatory)][string]$Body,
        [Parameter(Mandatory)][string]$Key
    )
    $escaped = [regex]::Escape($Key)
    $m = [regex]::Match($Body, "(?ms)^[ \t]*$escaped[ \t]*:[ \t]*(.*?)(?=^[ \t]*[A-Z_]+[ \t]*:|\z)")
    if (-not $m.Success) { return @() }
    $items = @()
    foreach ($line in ($m.Groups[1].Value -split "`r?`n")) {
        if ($line -match '^[ \t]*-[ \t]*(.*?)[ \t]*$') {
            $value = $Matches[1].Trim()
            $value = $value.Trim('`').Trim()
            if ($value -ne '' -and $value -notmatch '^#') { $items += $value }
        }
    }
    return $items
}

function Get-HandoffEnvelope {
    param([Parameter(Mandatory)][string]$TaskText)
    $m = [regex]::Match($TaskText, '(?s)<!--\s*handoff:envelope v1\s*-->(.*?)<!--\s*/handoff:envelope\s*-->')
    if (-not $m.Success) {
        return [ordered]@{ found = $false; may_edit = @(); must_not_edit = @(); incidental_ok = $false }
    }
    $body = $m.Groups[1].Value
    return [ordered]@{
        found         = $true
        may_edit      = @(Get-HandoffEnvelopeList -Body $body -Key 'MAY_EDIT')
        must_not_edit = @(Get-HandoffEnvelopeList -Body $body -Key 'MUST_NOT_EDIT')
        incidental_ok = ($body -match '(?im)^[ \t]*INCIDENTAL_OK[ \t]*:[ \t]*true[ \t]*$')
    }
}

function Test-HandoffPlannerScopeOverride {
    param([string[]]$PacketPaths = @())
    foreach ($path in $PacketPaths) {
        if (-not $path -or -not (Test-Path -LiteralPath $path)) { continue }
        $text = [System.IO.File]::ReadAllText((Get-Item -LiteralPath $path).FullName)
        $author = Get-HandoffFirstField -Text $text -Key 'AUTHOR'
        if ($author -notmatch '^Planner[ \t]*/') { continue }
        if ($text -match '(?im)^[ \t]*SCOPE_OVERRIDE[ \t]*:[ \t]*\S') { return $true }
    }
    return $false
}

function Test-HandoffScope {
    param(
        [Parameter(Mandatory)][string]$TaskPath,
        [Parameter(Mandatory)][string[]]$TouchedFiles,
        [string[]]$OverridePacket = @()
    )

    $errors = @()
    if (-not (Test-Path -LiteralPath $TaskPath)) {
        return [ordered]@{
            verdict = 'FAIL'
            unchecked = $false
            overridden = $false
            violations = @()
            errors = @("task packet not found: $TaskPath")
        }
    }

    $taskText = [System.IO.File]::ReadAllText((Get-Item -LiteralPath $TaskPath).FullName)
    $envelope = Get-HandoffEnvelope -TaskText $taskText
    if (-not $envelope['found']) {
        return [ordered]@{
            verdict = 'UNCHECKED'
            unchecked = $true
            reason = 'no handoff:envelope v1 block'
            overridden = $false
            violations = @()
            errors = @()
        }
    }
    $mayEdit = @($envelope['may_edit'])
    $mustNotEdit = @($envelope['must_not_edit'])
    $incidentalOk = [bool]$envelope['incidental_ok']

    if ($mayEdit.Count -eq 0 -and $mustNotEdit.Count -eq 0) {
        return [ordered]@{
            verdict = 'UNCHECKED'
            unchecked = $true
            reason = 'empty MAY_EDIT and MUST_NOT_EDIT envelope'
            overridden = $false
            violations = @()
            errors = @()
        }
    }

    $allGlobs = @($mayEdit + $mustNotEdit + @('ai_handoffs/**'))
    if ($incidentalOk) { $allGlobs += @('Cargo.lock', '**/*.meta.json') }
    foreach ($glob in $allGlobs) {
        try { [void](Convert-HandoffGlobToRegex -Glob $glob) } catch { $errors += $_.Exception.Message }
    }
    if ($errors.Count -gt 0) {
        return [ordered]@{
            verdict = 'FAIL'
            unchecked = $false
            overridden = $false
            violations = @()
            errors = @($errors)
        }
    }

    $violations = @()
    foreach ($file in $TouchedFiles) {
        $f = Normalize-HandoffPath $file
        if ([string]::IsNullOrWhiteSpace($f)) { continue }
        if (Test-HandoffAnyGlob -Path $f -Globs @('ai_handoffs/**')) { continue }
        if ($incidentalOk -and (Test-HandoffAnyGlob -Path $f -Globs @('Cargo.lock', '**/*.meta.json'))) { continue }

        $inMay = $true
        if ($mayEdit.Count -gt 0) {
            $inMay = Test-HandoffAnyGlob -Path $f -Globs $mayEdit
        }
        $inMustNot = Test-HandoffAnyGlob -Path $f -Globs $mustNotEdit
        if ((-not $inMay) -or $inMustNot) { $violations += $f }
    }

    $overridePaths = @($TaskPath) + @($OverridePacket)
    $overridden = $false
    if ($violations.Count -gt 0) {
        $overridden = Test-HandoffPlannerScopeOverride -PacketPaths $overridePaths
    }

    $verdict = if ($violations.Count -eq 0) { 'PASS' } elseif ($overridden) { 'WARN' } else { 'FAIL' }
    return [ordered]@{
        verdict = $verdict
        unchecked = $false
        overridden = $overridden
        violations = @($violations | Sort-Object -Unique)
        errors = @()
        may_edit = @($mayEdit)
        must_not_edit = @($mustNotEdit)
        incidental_ok = $incidentalOk
    }
}

function Get-HandoffTouchedFiles {
    param([Parameter(Mandatory)][string]$IntegrationRef)
    $base = (& git merge-base $IntegrationRef HEAD 2>$null).Trim()
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($base)) {
        throw "git merge-base failed for integration ref '$IntegrationRef'"
    }
    $files = @()
    $files += & git diff --name-only "$base..HEAD"
    $files += & git diff --name-only HEAD
    $files += & git ls-files --others --exclude-standard
    return @($files | Where-Object { $_ } | ForEach-Object { Normalize-HandoffPath $_ } | Sort-Object -Unique)
}

if ($env:RGE_HANDOFF_VALIDATOR_SKIP_MAIN -eq '1') { return }

if ([string]::IsNullOrWhiteSpace($PacketPath)) {
    Fail 'PacketPath is required unless RGE_HANDOFF_VALIDATOR_SKIP_MAIN=1 is set.'
}

$packetResult = Test-HandoffPacketFile -Path $PacketPath
$scopeResult = $null
if (-not [string]::IsNullOrWhiteSpace($TaskPacket)) {
    try {
        $touched = @(Get-HandoffTouchedFiles -IntegrationRef $Integration)
        $scopeResult = Test-HandoffScope -TaskPath $TaskPacket -TouchedFiles $touched -OverridePacket $PlannerOverridePacket
    } catch {
        $scopeResult = [ordered]@{
            verdict = 'FAIL'
            unchecked = $false
            overridden = $false
            violations = @()
            errors = @($_.Exception.Message)
        }
    }
}

$overall = $packetResult['verdict']
if ($scopeResult) {
    if ($packetResult['verdict'] -eq 'FAIL' -or $scopeResult['verdict'] -eq 'FAIL') {
        $overall = 'FAIL'
    } elseif ($packetResult['verdict'] -eq 'WARN' -or $scopeResult['verdict'] -in @('WARN', 'UNCHECKED')) {
        $overall = 'WARN'
    } else {
        $overall = 'PASS'
    }
}

$result = [ordered]@{
    verdict = $overall
    packet = $packetResult
    scope = $scopeResult
}

if (-not $JsonOnly) {
    Write-Output "HANDOFF_VALIDATE: $overall"
    if ($scopeResult) { Write-Output "SCOPE_VERDICT: $($scopeResult['verdict'])" }
}
Write-Output ($result | ConvertTo-Json -Depth 8)

if ($Blocking -and $overall -eq 'FAIL') { exit 2 }
exit 0
