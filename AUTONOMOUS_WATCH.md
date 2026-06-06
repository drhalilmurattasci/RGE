# Autonomous dispatch with a Claude watch/safety-monitor

Status: **mechanism built + smoke-tested — NOT armed.** The guard's live
supervision path is implemented and exercised against a mock driver, and the
`-Executor codex` loop swap is merged (#317). What is NOT done: the unattended
scheduler (`Register-AiDispatchSchedule.ps1`) is not registered, the guard has not
been pointed at the real driver with `-PublishMode main`, and no autonomous run has
executed against RGE. Arming is a separate, explicit operator decision.

## Goal (owner-directed reconfiguration)

Run the dispatch automation **fully autonomously with Codex as the decision +
execution layer**, and put **Claude in a dedicated watch / safety-monitor role**
that records everything and can halt the run with a report. Concretely:

- **Codex** = plans + selects the next task + **executes** + control-reviews +
  **authorizes the merge** (auto-publish to `origin/main` on its own `pass`
  verdict). Codex is "the human" — no per-PR human review in the loop.
- **Claude** = **watch only**. It does not execute. It observes the run, records
  everything to a log, assesses for anomalies, and on a problem it **terminates
  the automation and writes a report file**. Claude is the independent
  cross-AI safety backstop (since Codex now reviews its own work, the Claude
  veto is the only non-Codex check).

This **reverses the human-only merge gate** the project used previously. The
safety net for that reversal is three-layered: (1) the §18 *delegated-human
auto-publish* bounds in `AI_DISPATCH_AUTOMATION.md` (capped autonomous-issue
count, etc.); (2) Codex's `pass` control verdict; (3) the **Claude watch veto**
defined here. Any one tripping blocks/halts the publish.

## What already exists (reused, not rebuilt)

- **Codex selects the next task** — `Invoke-AiDispatchAuto.ps1` (Codex picks from
  `.ai/dispatch.tasks.md` and runs it through the queue).
- **Auto-merge on pass** — `Invoke-AiDispatchQueue.ps1 -PublishMode main`
  (fast-forward + push `origin/main` + close the issue), bounded by §18.
- **A per-phase event log** — `.ai/dispatch-trace/*.jsonl`, emitted by the queue
  + autonomous drivers, aggregated by `Get-AiDispatchTrends.ps1`. This is a
  primary signal the watch layer consumes ("it may already exist" — it does).
- **A passive read-only run dashboard** — the existing `Watch-AiDispatch.ps1`, a
  human-facing terminal watcher that summarizes packets, run-dir JSON, the Codex
  control verdict, and a progress bar. It explicitly does NOT call claude/codex,
  git-write, or kill anything. This is the other half of the "watch … log file
  (it may already exist)" the owner referred to. The NEW active guard below is
  its **action-taking sibling, not a replacement** — the passive watcher keeps
  its read-only, non-participating contract intact.
- **A Codex stall watchdog + abort-on-`block`** inside `Invoke-AiDispatchLoop.ps1`.

## What this change adds

### 1. `Invoke-AiDispatchGuard.ps1` — the Claude active safety-guard (BUILT here)

The action-taking sibling of the passive `Watch-AiDispatch.ps1` dashboard: where
the dashboard only *displays*, the guard *records + decides + halts*. It supervises
an autonomous dispatch run end to end:

1. **Launch + capture (live path, BUILT).** `Invoke-GuardLiveRun` starts the driver
   (`-DriverCommand`, default `Invoke-AiDispatchAuto.ps1`) as a child via
   `Start-Process`, redirecting its stdout/stderr to
   `.ai/dispatch-watch/<run-id>/driver.{stdout,stderr}.log` and tailing them
   incrementally into the watch log (`watch.log`) + structured events
   (`events.jsonl`). `-DryRun` instead drives a hermetic scripted sequence (no
   child, no Claude, no publish).
2. **Record + classify.** Each new output line is timestamped, sequence-numbered,
   and classified by source — `signal` (the loop/gate's own status lines:
   `VERIFY …`, `GATE_EXIT=`, `verdict=…`, `execution round N`), `command` (an
   echoed executed command), or `prose` (TASK text / rubric / model reasoning).
   Liveness is tracked by output growth + the child's process state. (The
   `.ai/dispatch-trace/*.jsonl` stream is an additional progress signal a future
   revision can correlate by pid.)
3. **Assess.** Every `-AssessIntervalSec`, the recent activity is sent to
   `claude -p` with a fixed rubric for a structured `{ "verdict": "ok"|"abort" }` —
   an unparseable/invalid response is treated as **abort** (fail-safe). Every
   verdict is recorded. `-MockAssess` substitutes a scripted verdict for offline
   smoke tests.
4. **Hard rules (deterministic floor).** Matched against *structured sources, not
   prose*, so a TASK packet or rubric that merely *mentions* a dangerous command
   can never trip them: **command** patterns (a push to the protected
   `main`/`master` ref in any form — `origin main`, `HEAD:main`, `refs/heads/main`,
   `+main`, a URL remote — or any force push) match only `command`-classified
   text; **signal** patterns (`VERIFY FAILED`, non-zero `GATE_EXIT`, a
   `BLOCKED`/`FAILED`/`NEEDS_HUMAN`/`block`/`needs_human` verdict) match only
   `signal` text. Plus numeric limits: `-MaxRunMinutes`, `-StallMinutes` (no output
   progress), `-MaxCorrectionRounds`, and a non-zero driver exit.
5. **Terminate + report.** On any abort (hard-rule or LLM `abort`), kill the child
   process **tree** (`taskkill /T /F`) and write
   `.ai/dispatch-watch/<run-id>/abort-report.md` — trigger, reason, recent-activity
   tail, and follow-up. The watch log records the final disposition
   (`completed` | `aborted`); the process exit code is `0` / `2`.

`-DryRun` is **not** the default — omit it and the guard enters the live path.
Pointing `-DriverCommand` at the real `Invoke-AiDispatchAuto.ps1` with
`-PublishMode main` is the explicit arming step; until then the live mechanism is
validated only against a mock driver (see smoke tests below).

### 2. `-Executor codex` for the loop (BUILT, default-off)

`Invoke-AiDispatchLoop.ps1` now accepts an additive executor selector:
`[ValidateSet('claude','codex')] [string]$Executor = 'claude'`. The default is
unchanged. With `-Executor codex`, the execution round calls `Invoke-CodexExecute`
instead of `Invoke-ClaudeExecute`; Codex writes the EXEC packet under the same
marker contract and through the existing Codex stall-watchdog path. Claude still
serves as the plan gate / watch-side circuit breaker; it no longer needs to be
the executor.

The selector is plumbed through `Invoke-AiDispatchAuto.ps1`,
`Invoke-AiDispatchQueue.ps1`, and `Register-AiDispatchSchedule.ps1`. Pure
argument-builder helpers expose the exact Auto -> Queue -> Loop command vectors
for dry-run tests, so the delegated-human posture can be verified without
running a live model call or publish.

## Dry-run (this change)

```
# Exercises the full watch→record→assess→terminate→report path against a mock
# child, writing the watch log + (on a simulated anomaly) an abort report. No
# real dispatch, no publish, no scheduler.
.\Invoke-AiDispatchGuard.ps1 -DryRun -DispatchId GUARD-DRYRUN                 # benign run completes
.\Invoke-AiDispatchGuard.ps1 -DryRun -DryRunOutcome hardrule -DispatchId GUARD-HR   # hard-rule abort + report
.\Invoke-AiDispatchGuard.ps1 -DryRun -DryRunOutcome llm-abort -DispatchId GUARD-LLM  # Claude-veto abort + report
```

The Codex-executor chain is also dry-run tested by Pester:

```
Invoke-Pester -Path .\tools\dispatch-tests\AutonomousCodexExecutorDryRun.Tests.ps1
```

Those tests assert that a delegated run would pass `-PublishMode main` and
`-Executor codex` from Auto to Queue, and `-Executor codex` from Queue to Loop.
They never invoke codex, claude, gh, git, the scheduler, or the publish path.

The LIVE supervision mechanism is smoke-tested against a mock driver (no real
loop / model / publish) across three outcomes — clean completion, a `signal`
abort (`VERIFY FAILED`), and a `command` abort (`git push origin main`) — and the
deterministic logic is covered by Pester:

```
Invoke-Pester -Path .\tools\dispatch-tests\GuardSafetyMonitor.Tests.ps1
```

## NOT done (explicit)

- The scheduler is NOT registered (`Register-AiDispatchSchedule.ps1` not run).
- The guard's live path is built + smoke-tested against a mock, but has NOT been
  pointed at the real `Invoke-AiDispatchAuto.ps1` with `-PublishMode main`.
- No live autonomous run against RGE has executed.

Arming any of the above is a separate, explicit operator decision.
