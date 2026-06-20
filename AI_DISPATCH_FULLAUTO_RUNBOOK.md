# AI Dispatch — Full-Automation Hardening Runbook

Status of branch `feat/dispatch-full-auto-hardening` (built in the isolated
worktree `A:/RCAD/RGE-fullauto`; the live loop on the main checkout was never
touched). Goal: harden the dispatch loop's deadlock/recovery/replay paths, then
add **default-OFF** scaffolding to convert it to "human = Codex" full autonomy
**without arming it**. Nothing here is armed: no `-PublishMode main`, no
scheduler re-registration, no autonomy switch defaults flipped.

---

## 1. What landed (Phase-0 — the deadlock/recovery/replay fixes)

All verified with `pwsh` parse checks + Pester 5.7.1. These are net-positive for
the loop **as it runs today** and change no default behavior.

| Commit | Fix | Why it mattered |
|---|---|---|
| `a14ab64` | Guard stop-pattern drift + coverage + drift-pin test | The guard's finite `-DriverTicks` batch never early-stopped on the cap/seatbelt halts (Gap-5); now fixed and pinned against the driver's real output strings. |
| `5f64a76` | `RGE_AI_DISPATCH_VERIFY_SKIP_MAIN` skip is loud, not silent | A skipped build/test gate could read as a green pass; now emits a `VERIFY SKIPPED` signal the guard can act on. |
| `b95152b` | `MaxPlanRevisions` 1→2 + `plan-gate` taxonomy label | A single stochastic Rule-8 NACK terminally failed a dispatch and hid in `unknown` (the task-166 class). |
| `55a9f30` | Bounded flaky-gate recovery tier | **The ~2.75h deadlock.** verification/control/plan-gate now auto-recover once (own marker) instead of bricking the loop; blocked/publish/unknown still halt for a human. |
| `f9f3c71` | Stale-issue replay guards | No old issue body is requeued after a brief amendment: recovery declines superseded issues; selection skips already-published ones. |
| `5804a9e` | Terminal relabel: bounded idempotent retry + REST fallback | A partial `gh` edit no longer leaves an issue mis-routed; backstopped by the replay guard. |
| `c6723b1` | `Register` exposes `-SeatbeltInterval` | The human-checkpoint cadence is now tunable for autonomy. |

### Recovery semantics (bounded + taxonomy-specific)
- **TRANSIENT** (`stall`, `timeout`) → one-shot via `ai-dispatch-recovered-transient`.
- **FLAKY** (`verification`, `control`, `plan-gate`) → one-shot via `ai-dispatch-recovered-flaky`.
- **NEVER auto-recovered**: `blocked`, `publish`, `unknown` — always halt for a human.
- Bound: at most one recovery **per tier per issue** (≤ 2 total), each gated by its own marker. A superseded issue (a newer ai-auto issue exists) is never recovered.

---

## 1b. Review-fix round (2026-06-21) — merge-blocker fixes

An independent review found 5 High + 5 Medium blockers before the branch was safe
to arm. All are fixed (small commits, each parse + Pester checked); default-OFF and
fail-closed are preserved throughout.

| Commit | Sev | Fix |
|---|---|---|
| `1b26369` | High | Surface-split / diff-cap diff the dispatch **branch**, not the parked primary `HEAD` (the size cap was failing **open** on an empty diff). |
| `e6da11a` | High | Plumb `-SurfaceSplitPublish`/`-MaxDiffFiles`/`-MaxDiffLines` + the autonomy switches end-to-end **Register → Guard → Auto → Queue** so the guarded/scheduled path can actually reach surface-split (was unreachable; `-PublishMode main` published source directly). |
| `e78763b` | High | `VERIFY SKIPPED` is now a guard hard-rule abort (was emitted but unenforced). |
| `7a8712f` | High | Guard publish-confirmation runs for **all** postures; any `origin/main` advance under pr/branch is an anomaly (was main-only). |
| `36157d1` | High | Self-rearm verifies the **authored** task's MAY-edit surface ⊆ ceiling + requires a MUST-NOT section (was instructed, not enforced). |
| `d91a84f` | Med | Surface-split can only **downgrade** a main posture (never promote branch/pr → main); the brief is a **control surface** (brief-only changeset → PR, never auto-merge). |
| `7cc992a` | Med | Stale-replay **supersession** guard (older pending issue with a stale body); seatbelt review fail-closed on empty/truncated evidence; halt-clear **re-validates + verifies** the sentinel deletion. |

New test seam: `RGE_AI_DISPATCH_GUARD_SKIP_OOB_SHA=1` keeps the guard's all-posture
publish-confirmation offline in hermetic mock runs.

---

## 2. How to review

```
# from the main checkout:
git log --oneline main..feat/dispatch-full-auto-hardening
git diff main...feat/dispatch-full-auto-hardening -- Invoke-AiDispatchAuto.ps1
git diff main...feat/dispatch-full-auto-hardening -- Invoke-AiDispatchQueue.ps1
git diff main...feat/dispatch-full-auto-hardening -- Invoke-AiDispatchGuard.ps1 .ai/dispatch.verify.ps1 Invoke-AiDispatchLoop.ps1 Register-AiDispatchSchedule.ps1
git diff main...feat/dispatch-full-auto-hardening -- tools/dispatch-tests/
```

## 3. How to test

Requires Pester 5 (`Install-Module Pester -MinimumVersion 5.5.0 -Scope CurrentUser -Force -SkipPublisherCheck`).

> IMPORTANT — run the suite with the **current working directory set to the repo
> root you are testing** (the worktree, if testing the branch). `Test-HandoffPacket.ps1`
> resolves changed paths against the process cwd; running a worktree's tests from a
> different cwd spuriously fails one path-exclusion test.

```powershell
Set-Location 'A:\RCAD\RGE-fullauto'                       # the worktree (repo under test)
[System.Environment]::CurrentDirectory = 'A:\RCAD\RGE-fullauto'
Import-Module Pester -MinimumVersion 5.0.0 -Force
$cfg = New-PesterConfiguration
$cfg.Run.Path = 'tools/dispatch-tests'                    # whole dispatch-test suite
$cfg.Output.Verbosity = 'Detailed'
Invoke-Pester -Configuration $cfg
```
Current result: **599 pass, 1 fail** (the review-fix round added ~23 tests) — the
single failure (`sweeps a dead queue-owned claim before its TTL expires`) is the
same **pre-existing flaky timing test** in `AutonomousCodexExecutorDryRun.Tests.ps1`
(passes isolated + on baseline; not touched by this branch). Each `.ps1` also parses
clean via `[System.Management.Automation.Language.Parser]::ParseFile`.

## 4. How to roll back

- **Before merge (current state):** nothing to roll back — the live loop runs the
  main checkout, which is unchanged. Just don't merge the branch. Remove the
  worktree with `git worktree remove A:/RCAD/RGE-fullauto` and delete the branch.
- **After merge:** `git revert <commit>` per fix (each commit is self-contained),
  or `git revert 5804a9e..c6723b1` for the range. None of these commits changes a
  runtime default, so reverting is low-risk.

## 5. What stops the system (sentinels & labels)

| Lever | Effect | Clear by |
|---|---|---|
| `.ai/dispatch.auto-halt` (file) | **Master kill switch.** Every Auto tick exits at the top while it exists. | Delete the file. |
| `ai-dispatch-failed` label on an open ai-auto issue | Halts the driver (`--state all`). Non-recoverable classes (blocked/publish/unknown) land here. | Remove the label (closing the issue does NOT clear it). |
| `ai-dispatch-recovered-transient` / `ai-dispatch-recovered-flaky` label | One-shot recovery marker; a second failure of that tier halts instead of re-recovering. | Remove the marker to allow another recovery (rarely wanted). |
| `.ai/dispatch.auto-seatbelt.json` counter ≥ `-SeatbeltInterval` | Pauses for human review (writes the halt sentinel). | Review, then delete `.ai/dispatch.auto-halt`. |
| `.ai/dispatch.guard-stop` (file) | **Always-on guard kill switch** — the guard aborts (taskkill child tree + abort report, exit 2) within one `-PollIntervalSec`, before each tick and on every poll. Not gated by any switch. | Delete the file. |
| `.ai/dispatch.auto-halt` with `CLASS: consec-fail` | Written after `-MaxConsecutiveFailures` consecutive failed ticks. **Human-only** — `-AllowCodexClearHalt` refuses to clear it. | Investigate the recurring failure, then delete the file. |
| `.ai/dispatch.auto-consecutive-failures.json` | The consec-fail counter (increments on a failed tick, resets on a clean one). Only written when `-MaxConsecutiveFailures > 0`. | Delete to reset (or let a clean tick reset it). |

**Default-OFF autonomy switches** (all inert unless explicitly passed; off-path = current behavior): `-AllowCodexSelfRearm` + `-AutoRearmCeilingSurface` (Auto), `-DelegateSeatbeltReview` (Auto), `-AllowCodexClearHalt` (Auto), `-SurfaceSplitPublish` + `-MaxDiffFiles`/`-MaxDiffLines` (Queue), `-MaxConsecutiveFailures` (Auto). Only `-AllowCodexClearHalt` ever auto-clears a sentinel, and only the `seatbelt`/`recovery` classes.

---

## 6. Conversion scaffolding — COMPLETE (default-OFF, dormant)

Every switch is default-OFF and fail-closed; with no flags the loop behaves exactly
as before. The system was designed for human=codex (`AUTONOMOUS_WATCH.md`); the
guard (`Invoke-AiDispatchGuard.ps1`) is the cross-model supervisor.

| Capability | Switch (default OFF) | Pure core (Pester-tested) | Codex/IO seam (armed only) |
|---|---|---|---|
| Self-re-arm | `-AllowCodexSelfRearm` + `-AutoRearmCeilingSurface` | Test-AutoApprovableRecommendation, Get-BriefRecommendationBlock, Test-SelfRearmPostConditions | Invoke-CodexSelfRearm (`codex exec workspace-write` + git verify/revert) |
| Seatbelt review | `-DelegateSeatbeltReview` | Test-SeatbeltReviewContinue | Invoke-CodexSeatbeltReview (`codex exec read-only`) |
| Halt auto-clear | `-AllowCodexClearHalt` | Get-HaltClearEligibility, Get-HaltSentinelClass, Test-HaltClearAnswer | Invoke-CodexHaltClear (`codex exec read-only`) |
| Surface-split publish | `-SurfaceSplitPublish` | Get-DispatchSurfaceRouting | git diff routing in the queue publish path |
| Diff-size cap | `-MaxDiffFiles` / `-MaxDiffLines` | Test-DiffSizeWithinCap | git numstat in the queue |
| Consec-fail cap | `-MaxConsecutiveFailures` | Test-ConsecutiveFailureCapReached | counter file in Auto |
| Publish confirm | (always-on, main posture) | Test-PublishConfirmation | out-of-band origin/main SHA in the guard |
| Kill switch | (always-on) | Test-GuardStopRequested | sentinel poll in the guard |

Fail-closed everywhere: missing marker / ambiguous labels / stale body / unknown
taxonomy / repeated same-class failure / missing guard confirmation => halt or PR,
never main.

## 7. Smoke commands (safe — dry-run / branch / PR only; NEVER live `-PublishMode main`)

The cores + the off-path are exercised by the Pester suite (Section 3). Beyond that:

```powershell
# Guard hermetic dry-run (no child, no codex, no publish):
.\Invoke-AiDispatchGuard.ps1 -DryRun -DispatchId SMOKE-DRY

# Guard against a mock driver (no real model/publish), multi-tick early-stop:
.\Invoke-AiDispatchGuard.ps1 -DispatchId SMOKE-MOCK -DriverCommand .\mock.ps1 -MockAssess -DriverTicks 3 -PollIntervalSec 2

# Kill switch: drop the sentinel, confirm the guard aborts (exit 2 + abort-report):
New-Item .ai\dispatch.guard-stop -ItemType File -Force

# Auto driver dry-run (selects, never files/publishes):
.\Invoke-AiDispatchAuto.ps1 -DryRun

# Surface-split / cap smoke: -PublishMode branch or pr ONLY:
.\Invoke-AiDispatchQueue.ps1 -PublishMode pr -SurfaceSplitPublish -MaxDiffFiles 40 -MaxDiffLines 1500
```

Do NOT pass `-PublishMode main` (or arm `-SurfaceSplitPublish` so it routes to main)
during smoke. The guard publish-confirmation path engages only under main posture;
validate it last, under supervision, after everything else is green.

## 8. Arming sequence (do NOT run yet — pause for review first)

1. Review + merge `feat/dispatch-full-auto-hardening`; `git pull` the main checkout
   so the live driver picks up the new scripts.
2. Run the full `tools/dispatch-tests` suite (Section 3) + the dry-run/mock guard
   smokes (Section 7). All green except the known flaky claim-TTL test.
3. Run the guard live with `-PublishMode pr` and `-DriverTicks > 1` to exercise the
   multi-tick batch and confirm cap/seatbelt early-stop.
4. Arm incrementally, one switch at a time, lowest-risk first:
   `-DelegateSeatbeltReview` -> `-AllowCodexSelfRearm` (+ a NARROW
   `-AutoRearmCeilingSurface`) -> `-AllowCodexClearHalt` -> `-SurfaceSplitPublish`
   (+ `-MaxDiffFiles`/`-MaxDiffLines`) -> `-MaxConsecutiveFailures`. Keep the guard
   supervising and the `.ai/dispatch.guard-stop` kill switch documented.
5. ONLY after all the above is validated under the guard: enable `-PublishMode main`
   for the surface-split low-risk path, and re-register the scheduled task (needs an
   elevated shell) so it forwards the new args incl. `-MaxPlanRevisions 2`, the
   tightened `-SeatbeltInterval`, and the autonomy switches per policy.

> Note: the **already-registered** scheduled task still passes the old
> `-MaxPlanRevisions 1` until re-registered (step 5). The guard-launched and manual
> paths pick up the new defaults immediately. The new autonomy / surface-split
> switches are now **plumbed end-to-end** (Register → Guard → Auto → Queue, commit
> `e6da11a`); re-registering with the desired flags (step 5) is all that is needed to
> forward them — `Register-AiDispatchSchedule.ps1` fail-closes if any are passed
> without `-Autonomous`.
