#Requires -Version 5.1
<#
.SYNOPSIS
    Regression coverage for the stale-replay / orphan-recovery "already published"
    guard in Invoke-AiDispatchQueue.ps1.

.DESCRIPTION
    Dot-sources the queue through its RGE_AI_DISPATCH_QUEUE_SKIP_MAIN seam to load
    the two pure helpers without running the dispatch flow:
      * Get-StaleReplayPublishedShaArgs -CreatedAt        -> builds the git-log args
      * Select-StaleReplayPublishedSha  -IssueId -GitLog  -> picks the publish SHA

    Two collision classes are pinned here:

    1. MIGRATED ancient subjects. After the RustCADs/RGE -> drhalilmurattasci/RGE
       migration the new repo restarted issue numbering, so a fresh "ISSUE-4"
       collided with an imported ancient commit "ai-dispatch ISSUE-4: ..."
       (2026-05-17). Defense: the `--since=<issue.createdAt>` floor (a real publish
       is always newer than its issue).

    2. BODY-QUOTE / merge false-positives. `git log --grep` matches the WHOLE commit
       message, so any recent commit that merely QUOTES "ai-dispatch ISSUE-N:" in its
       body -- including the floor fix commit itself (b5a6fb4) and the 22 merge
       commits whose body line-starts the marker -- produced a false "already
       published". Defense: print %H<TAB>%s and prefix-match the SUBJECT only (no
       --grep / --fixed-strings, no `-n 1`). A line-start `-E "^..."` anchor was
       evaluated and REJECTED: git log --grep is multiline, so `^` matches a body
       line-start too (and with `-n 1` returns the wrong, merge, SHA).
#>

BeforeAll {
    $script:TestsRoot       = Split-Path -Parent $PSCommandPath
    $script:RepoRootForTest = Split-Path -Parent (Split-Path -Parent $script:TestsRoot)
    $script:QueueScriptPath = Join-Path $script:RepoRootForTest 'Invoke-AiDispatchQueue.ps1'
    $env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN = '1'
    try { . $script:QueueScriptPath }
    finally { Remove-Item Env:RGE_AI_DISPATCH_QUEUE_SKIP_MAIN -ErrorAction SilentlyContinue }
}

Describe 'Get-StaleReplayPublishedShaArgs (creation-floored subject-scan args)' {
    It 'builds a subject-printing, creation-floored git-log scan' {
        $a = Get-StaleReplayPublishedShaArgs -CreatedAt '2026-06-25T04:16:08Z'
        $a   | Should -Contain 'log'
        $a   | Should -Contain 'origin/main'
        $a   | Should -Contain '--format=%H%x09%s'
        ($a -join ' ') | Should -Match '--since=2026-06-25T04:16:08Z'
    }

    It 'does NOT use --grep / --fixed-strings / -n 1 (those re-open the body-quote gap)' {
        $joined = (Get-StaleReplayPublishedShaArgs -CreatedAt '2026-06-25T04:16:08Z') -join ' '
        $joined | Should -Not -Match '--grep'
        $joined | Should -Not -Match '--fixed-strings'
        $joined | Should -Not -Match '(^|\s)-n(\s|$)'
    }

    It 'returns $null when createdAt is missing (fail-CLOSED contract, not an unfloored scan)' {
        Get-StaleReplayPublishedShaArgs -CreatedAt ''   | Should -BeNullOrEmpty
        Get-StaleReplayPublishedShaArgs -CreatedAt $null | Should -BeNullOrEmpty
        Get-StaleReplayPublishedShaArgs -CreatedAt '   ' | Should -BeNullOrEmpty
    }
}

Describe 'Select-StaleReplayPublishedSha (subject-prefix selection)' {
    BeforeAll {
        # Declared in BeforeAll (not the Describe body) so they exist at RUN time, not
        # only at Pester discovery time.
        $script:sha1 = '1111111111111111111111111111111111111111'
        $script:sha2 = '2222222222222222222222222222222222222222'
        $script:sha3 = '3333333333333333333333333333333333333333'
    }

    It 'returns the SHA of a genuine subject-line publish' {
        $out = "$sha1`tai-dispatch ISSUE-4: Real publish title"
        Select-StaleReplayPublishedSha -IssueId 'ISSUE-4' -GitLogOutput $out | Should -Be $sha1
    }

    It 'ignores a commit that only mentions the marker mid-subject (not a prefix)' {
        $out = "$sha1`tFix follow-up to ai-dispatch ISSUE-4: regression"
        Select-StaleReplayPublishedSha -IssueId 'ISSUE-4' -GitLogOutput $out | Should -Be ''
    }

    It 'ignores a merge commit whose SUBJECT is not the marker (body never reaches us)' {
        # The merge commit's body line-starts "ai-dispatch ISSUE-335:", but %s is the
        # merge subject -- this is exactly what defeats the rejected `-E "^..."` anchor.
        $out = "$sha1`tMerge pull request #336 from RustCADs/ai-dispatch/ISSUE-335"
        Select-StaleReplayPublishedSha -IssueId 'ISSUE-335' -GitLogOutput $out | Should -Be ''
    }

    It 'disambiguates ISSUE-4 from ISSUE-40 (exact <id>: prefix)' {
        $out = "$sha1`tai-dispatch ISSUE-40: Different issue"
        Select-StaleReplayPublishedSha -IssueId 'ISSUE-4' -GitLogOutput $out | Should -Be ''
    }

    It 'returns the newest (first) matching line when several match' {
        $out = @(
            "$sha1`tai-dispatch ISSUE-4: newer republish"
            "$sha2`tai-dispatch ISSUE-4: original publish"
        ) -join "`n"
        Select-StaleReplayPublishedSha -IssueId 'ISSUE-4' -GitLogOutput $out | Should -Be $sha1
    }

    It 'skips non-matching lines and returns a later match; tolerates blanks/no-tab lines' {
        $out = @(
            ''
            'a-line-with-no-tab'
            "$sha2`tMerge branch 'x'"
            "$sha3`tai-dispatch ISSUE-7: the publish"
        ) -join "`n"
        Select-StaleReplayPublishedSha -IssueId 'ISSUE-7' -GitLogOutput $out | Should -Be $sha3
    }

    It 'returns empty on empty/null output' {
        Select-StaleReplayPublishedSha -IssueId 'ISSUE-4' -GitLogOutput ''    | Should -Be ''
        Select-StaleReplayPublishedSha -IssueId 'ISSUE-4' -GitLogOutput $null | Should -Be ''
    }
}

Describe 'End-to-end against a real repo (floor + subject selection together)' {
    BeforeAll {
        $script:repo = Join-Path $TestDrive 'e2e'
        New-Item -ItemType Directory -Path $script:repo -Force | Out-Null
        git -C $script:repo init -q | Out-Null
        git -C $script:repo config user.email 't@t.test' | Out-Null
        git -C $script:repo config user.name  'test' | Out-Null

        function script:Add-Commit {
            param([string]$Date, [string[]]$MsgArgs)
            Set-Content -LiteralPath (Join-Path $script:repo 'f.txt') -Value $Date -NoNewline
            git -C $script:repo add -A | Out-Null
            $env:GIT_AUTHOR_DATE = $Date; $env:GIT_COMMITTER_DATE = $Date
            git -C $script:repo commit -q @MsgArgs | Out-Null
            Remove-Item Env:GIT_AUTHOR_DATE, Env:GIT_COMMITTER_DATE -ErrorAction SilentlyContinue
        }

        # (a) ANCIENT migrated subject publish for ISSUE-4 -> floored out by --since.
        Add-Commit -Date '2026-05-17T23:59:13 +0300' -MsgArgs @('-m', 'ai-dispatch ISSUE-4: Ancient migrated publish')
        # (b) RECENT body-quote commit (the b5a6fb4 case): marker only in the BODY.
        # NB: no embedded double-quotes -- they mangle PS 5.1 native arg passing.
        Add-Commit -Date '2026-06-25T13:00:00 +0300' -MsgArgs @('-m', 'Fix stale-replay guard: floor the grep', '-m', 'collided with an ancient ai-dispatch ISSUE-4: marker in the body')
        # (c) RECENT merge commit: marker line-starts the BODY, subject is the merge line.
        Add-Commit -Date '2026-06-25T13:05:00 +0300' -MsgArgs @('-m', 'Merge pull request #336 from RustCADs/ai-dispatch/ISSUE-335', '-m', 'ai-dispatch ISSUE-335: Document the gate')
        # (d) RECENT genuine subject-line publish for ISSUE-6.
        Add-Commit -Date '2026-06-25T13:10:00 +0300' -MsgArgs @('-m', 'ai-dispatch ISSUE-6: Real recent publish')
        $script:sha_real6 = (git -C $script:repo rev-parse HEAD).Trim()

        git -C $script:repo update-ref refs/remotes/origin/main HEAD | Out-Null

        function script:Resolve-PublishedSha {
            param([string]$IssueId, [string]$CreatedAt)
            $a = Get-StaleReplayPublishedShaArgs -CreatedAt $CreatedAt
            if ($null -eq $a) { return '' }   # mirror the call sites' fail-closed glue
            # `git` returns multi-line output as string[]; the helpers (like the real
            # Invoke-Tool/.Text path) take a single string -- join before binding.
            $out = (git -C $script:repo @a) -join "`n"
            Select-StaleReplayPublishedSha -IssueId $IssueId -GitLogOutput $out
        }
    }

    It 'ISSUE-4: no false positive -- ancient floored out AND recent body-quote ignored (the live b5a6fb4 bug)' {
        Resolve-PublishedSha -IssueId 'ISSUE-4' -CreatedAt '2026-06-25T04:16:08Z' | Should -Be ''
    }

    It 'ISSUE-335: merge commit (body line-starts marker) is NOT matched (the rejected -E ^ leak)' {
        Resolve-PublishedSha -IssueId 'ISSUE-335' -CreatedAt '2026-06-25T04:16:08Z' | Should -Be ''
    }

    It 'ISSUE-6: a genuine recent subject-line publish IS detected (no over-filtering)' {
        Resolve-PublishedSha -IssueId 'ISSUE-6' -CreatedAt '2026-06-25T04:16:08Z' | Should -Be $script:sha_real6
    }

    It 'missing createdAt fails CLOSED: scan args are null -> treated as not-published' {
        Get-StaleReplayPublishedShaArgs -CreatedAt '' | Should -BeNullOrEmpty
    }
}

Describe 'Source wiring contract (both call sites floored, subject-only, fail-closed)' {
    BeforeAll {
        $script:QueueSource = Get-Content -LiteralPath $script:QueueScriptPath -Raw
    }

    It 'both issue fetches request createdAt so the floor has an input' {
        $script:QueueSource | Should -Match "--json',\s*'number,title,createdAt'"            # orphan recovery
        $script:QueueSource | Should -Match "'number,title,body,labels,url,createdAt'"        # stale-replay
    }

    It 'both call sites route through Get-StaleReplayPublishedShaArgs AND Select-StaleReplayPublishedSha' {
        # 1 definition + 2 call sites each.
        ([regex]::Matches($script:QueueSource, 'Get-StaleReplayPublishedShaArgs')).Count | Should -BeGreaterOrEqual 3
        ([regex]::Matches($script:QueueSource, 'Select-StaleReplayPublishedSha')).Count   | Should -BeGreaterOrEqual 3
    }

    It 'both call sites fail closed on null scan args' {
        ([regex]::Matches($script:QueueSource, '\$null -eq \$scanArgs')).Count | Should -BeGreaterOrEqual 2
    }

    It 'the unfloored / whole-message grep is gone for published-SHA detection' {
        # Match the code forms (quoted git args), not the explanatory comment that
        # names `--grep`/`--fixed-strings` to document why they were removed.
        $script:QueueSource | Should -Not -Match '--grep=ai-dispatch'
        $script:QueueSource | Should -Not -Match "'--fixed-strings'"
    }
}
