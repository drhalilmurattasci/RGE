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

Requires Pester 5 (installed this session: `Install-Module Pester -MinimumVersion 5.5.0 -Scope CurrentUser -Force -SkipPublisherCheck`).

```
Import-Module Pester -MinimumVersion 5.0.0 -Force
$cfg = New-PesterConfiguration
$cfg.Run.Path = 'tools/dispatch-tests'   # run the whole dispatch-test suite
$cfg.Output.Verbosity = 'Detailed'
Invoke-Pester -Configuration $cfg
```
New/affected suites: `GuardSafetyMonitor.Tests.ps1` (78), `Get-FailureTaxonomyLabels.Tests.ps1` (11), `Get-RecoveryDecision.Tests.ps1` (19). Each `.ps1` also parses clean via `[System.Management.Automation.Language.Parser]::ParseFile`.

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

---

## 6. Conversion scaffolding — PENDING (default-OFF, builds next)

The system was **designed** for human=codex (`AUTONOMOUS_WATCH.md`); the guard
(`Invoke-AiDispatchGuard.ps1`) is the cross-model supervisor. The remaining work
arms nothing by default:

**Landed — pure decision cores + safety mechanisms (default-OFF / dormant, Pester-covered):**
- [x] **Surface-split classifier** `Get-DispatchSurfaceRouting` (Queue) — fail-closed; 17 tests.
- [x] **AUTO_APPROVABLE / qualifying-recommendation** `Test-AutoApprovableRecommendation` (Auto) — fail-closed; 11 tests.
- [x] **Halt-clear eligibility** `Get-HaltClearEligibility` (Auto) — fail-closed; 12 tests.
- [x] **`.ai/dispatch.guard-stop` kill switch** (Guard) — always-on; unit + integration tests.
- [x] **Guard publish-confirmation** `Test-PublishConfirmation` + live wiring (Guard, main-posture only) — real `VERIFY OK` / `Codex control passed` signals + out-of-band `origin/main` SHA; 8 tests.

**Remaining — live wiring of the switches (integration-level; needs smoke tests, not unit tests, before arming):**
- [ ] `-AllowCodexSelfRearm` (default OFF): at a `NEEDS_HUMAN_RECORDED` gate, call
      `Test-AutoApprovableRecommendation`; if it qualifies, `codex exec` authors the
      next feature task (atomic marker-neutralize + append + verify); else file
      needs-human + halt (current behavior).
- [ ] `-DelegateSeatbeltReview` (default OFF): at the seatbelt fire, a read-only
      `codex exec` CONTINUE/HOLD review stands in for the human checkpoint.
- [ ] `-AllowCodexClearHalt` (default OFF): write a `class` into the auto-halt
      sentinel at each write site; at the top-of-tick check, call
      `Get-HaltClearEligibility` and `codex exec` adjudicate clearable classes only.
- [ ] **Surface-split publish routing**: wire `Get-DispatchSurfaceRouting` into the
      queue publish path so low-risk auto-merges and high-risk opens a PR.
- [ ] Diff-size ceiling, consecutive-failure hard stop, `AUTO_APPROVABLE` brief/audit
      protocol plumbing, docs (`AUTONOMOUS_WATCH.md`, `AI_DISPATCH_AUTOMATION.md`,
      brief subsection).

## 7. Arming sequence (do NOT run yet — pause for review first)

1. Merge `feat/dispatch-full-auto-hardening` to main; `git pull` the main checkout
   so the live driver picks up the new scripts.
2. Run the full `tools/dispatch-tests` Pester suite + a guard smoke test
   (`Invoke-AiDispatchGuard.ps1 -DryRun` and against the mock driver).
3. Run the guard live with `-PublishMode pr` and `-DriverTicks > 1` to exercise
   the multi-tick batch and confirm cap/seatbelt early-stop now works.
4. Only then arm: guard with the real driver, surface-split publish, autonomy
   switches per policy, the kill-switch documented. Re-register the scheduled
   task (needs an elevated shell) so it forwards the new args incl.
   `-MaxPlanRevisions 2` and the tightened `-SeatbeltInterval`.

> Note: the **already-registered** scheduled task still passes the old
> `-MaxPlanRevisions 1` until re-registered (step 4). The guard-launched and
> manual paths pick up the new defaults immediately.
