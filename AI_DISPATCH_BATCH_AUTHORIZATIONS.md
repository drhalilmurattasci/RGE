# AI Dispatch Batch Authorizations

Append-only record of delegated-human autonomous dispatch batches. This file
records explicit, finite operator authorizations before `-PublishMode main` or
human=Codex review/merge behavior is used.

## 2026-06-29 - human=Codex next-5 batch

- Authorized by: human operator in Codex session; Codex is delegated to perform
  the human-review role for this bounded batch.
- Batch scope: process up to the next 5 autonomous tasks selected from
  `.ai/dispatch.tasks.md`, starting from live task 172.
- Cap: finite 5-task batch (`-DriverTicks 5`; open-backlog ceiling
  `-MaxAutonomousTasks 5`).
- Executor: Codex (`-Executor codex`).
- Publish posture: guarded full automation with `-PublishMode main` plus
  `-SurfaceSplitPublish`; high-risk surfaces must downgrade to PR. Task 172 is
  explicitly PR-routed and must not be auto-merged by the queue.
- Delegated-human behavior: when a PR is required by surface routing or task
  policy, Codex may perform the human-role review/merge decision only after the
  dispatch branch is committed, verification/control are green, and the diff
  stays inside the task scope.
- Autonomy switches authorized: `-DelegateSeatbeltReview`,
  `-AllowCodexClearHalt` for auto-clearable classes only, and
  `-MaxConsecutiveFailures 2`. Manual/consec-fail halts remain human-only unless
  the operator explicitly resolves the underlying condition.
- Pre-batch state reviewed: `origin/main` at `05f2669`, tracked tree clean,
  dispatch Pester `688/688`, canonical verify exit 0 after the hardening
  burn-down, stale memmap2/RUSTSEC halt resolved by current history.
- Stop conditions: preserve all stop conditions in `AI_DISPATCH_AUTOMATION.md`
  section 18.4, including nonzero verification, Codex control block, scope
  drift, dirty unexpected diffs, auth/remote failures, branch/base mismatch,
  publish anomalies, high-risk surface auto-main attempts, and human-only halt
  classes.
