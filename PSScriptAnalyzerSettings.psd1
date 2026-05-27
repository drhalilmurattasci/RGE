@{
    # PSScriptAnalyzer configuration for the .github/workflows/powershell.yml
    # CI guardrail (added in ISSUE-223). The gate runs analyzer Error and
    # Warning severities against tracked *.ps1 / *.psm1 files via git
    # ls-files. Each exclusion below is intentional and documented; the
    # excluded rules conflict with established conventions across the
    # dispatch-automation scripts and would require widespread refactors
    # rather than mechanical fixes -- broad rewrites of those scripts are
    # explicitly out of scope for the CI guardrail that introduced this
    # settings file.

    Severity = @('Error', 'Warning')

    ExcludeRules = @(
        # PSUseApprovedVerbs: many existing helpers across the dispatch
        # automation use unapproved verbs by design (Require-Command,
        # Release-Lock / Acquire-Lock, Git-Step, Finalize-Packet,
        # Release-AutoLock / Acquire-AutoLock, ...). Each is called from
        # multiple scripts and from already-written packet protocols;
        # renaming them is a coordinated cross-script refactor rather than
        # a narrow lint cleanup.
        'PSUseApprovedVerbs',

        # PSUseSingularNouns: several helpers deliberately return / operate
        # on collections and carry plural nouns
        # (Get-FailureTaxonomyLabels, Get-TaskPositiveAllowedTokens,
        # Get-QueueStatusEntries, Get-ProcessStartTicks, Get-RelatedFiles,
        # Format-Seconds, ...). Renaming them would break call sites in
        # this and adjacent scripts.
        'PSUseSingularNouns',

        # PSAvoidUsingWriteHost: Invoke-AiDispatchLoop.ps1 and
        # Watch-DispatchStages.ps1 use Write-Host deliberately to emit
        # colored interactive UI output. Replacing it with Write-Output
        # would re-route that text into the success pipeline where the
        # queue runner Tee-Object captures it as loop output.
        'PSAvoidUsingWriteHost',

        # PSUseShouldProcessForStateChangingFunctions: helpers such as
        # New-MutationSnapshot / Remove-MutationSnapshot mutate the
        # filesystem but are not user-facing cmdlets and have no callers
        # that pass -WhatIf / -Confirm. Adding the parameters would change
        # call-site behavior of every invoker rather than tighten a lint.
        'PSUseShouldProcessForStateChangingFunctions',

        # PSReviewUnusedParameter: several dispatch entry-points keep
        # parameters that are reserved for the caller contract (e.g.
        # -ClaudePermissionMode, -CodexModel, -ClaudeModel,
        # -ModelTimeoutSec, -CodexStallThreshold, -NoColor, -Tail) or are
        # consumed in nested script blocks the analyzer cannot trace.
        # The parameters are part of the public surface the scheduler /
        # autonomous driver depends on; removing them would break callers.
        'PSReviewUnusedParameter',

        # PSAvoidAssignmentToAutomaticVariable: several Invoke-* helpers in
        # Invoke-AiDispatchLoop.ps1 deliberately reuse $args as a local
        # name for the constructed CLI argument list. The functions carry
        # their own param() blocks, so the automatic $args has no role
        # there. Renaming is mechanical but spans many call sites and is
        # out of scope for this CI guardrail.
        'PSAvoidAssignmentToAutomaticVariable',

        # PSAvoidUsingEmptyCatchBlock: catch blocks across the dispatch
        # automation deliberately swallow exceptions to keep best-effort
        # paths (JSONL trace persistence, gh probes, stash pops, label
        # finalization JSON parsing, ...) from blocking dispatch progress
        # on transient noise. Each swallow already carries an inline
        # comment explaining the intent; adding a no-op statement just to
        # satisfy the rule would not improve clarity.
        'PSAvoidUsingEmptyCatchBlock',

        # PSUseBOMForUnicodeEncodedFile: Watch-DispatchStages.ps1 is
        # deliberately written without a BOM so its progress lines are
        # consumable by downstream filters that strip non-ASCII bytes.
        'PSUseBOMForUnicodeEncodedFile'
    )
}
