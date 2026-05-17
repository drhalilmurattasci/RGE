# RGE Autonomous Dispatch — Task Brief

This file is the **authorized source of work** for the autonomous dispatch
loop (`Invoke-AiDispatchAuto.ps1`). When the queue is empty, Codex reads this
file, selects the next task, and files it as a GitHub issue that the hardened
dispatch queue then runs (plan → Claude gate → execute → verification gate →
control → publish).

> **The loop is INERT until the "Tasks" section below is armed.**
> While the `DISPATCH-TASKS-UNARMED` marker line is present, the driver
> selects nothing — a deterministic check, not a judgement call. Arming the
> loop is a deliberate act: delete that marker and fill in real tasks.

## How to fill this in

Pick **one** style:

### Style A — explicit task list (recommended, safest)
List concrete, **small, independently-shippable** tasks in priority order.
Codex takes the next un-filed one — or an earlier one if it is a dependency
("sequence necessity"). One file or one tight area per task, with a clear
done-criterion. Vague entries become vague commits.

### Style B — roadmap pointer
Instead of a list, write instructions telling Codex where to choose from,
e.g.: *"Pick the next unstarted job from the 'Next Jobs' section of
HANDOFF.md. Choose the smallest bounded one. Skip anything marked BLOCKED."*
Codex reads the repo (read-only) and decides. More autonomy, more drift risk —
prefer Style A until the loop has proven itself.

## Safety reminders

- The loop **halts** when a task is marked `ai-dispatch-failed` — that is,
  after a task fails its run *and* its one automatic retry — and also once
  `-MaxAutonomousTasks` tasks exist. Both need a human to clear/raise before
  it resumes.
- In `branch` publish mode, finished work waits on an `ai-dispatch/ISSUE-*`
  branch for you to merge. In `main` mode it auto-publishes to `origin/main`.
- Keep tasks bounded. The autonomous loop will plan, execute, verify, and
  (depending on mode) publish whatever is selected here.

## Tasks

Documentation batch 2 — small, bounded, documentation-only tasks. Each names
the single file it may touch and has a clear done-criterion. The tasks are
independent; any order is fine.

1. Documentation sync, `AI_DISPATCH_AUTOMATION.md` only. The section 4
   "Files & components" table predates several scripts. Add one table row for
   each missing entry, with a concise Role and a sensible Port? value matching
   the table's existing columns: `Invoke-AiDispatchAuto.ps1` (autonomous
   driver — Codex selects the next task from `.ai/dispatch.tasks.md`),
   `Register-AiDispatchSchedule.ps1` (registers the unattended Scheduled
   Task), `.ai/dispatch.verify.ps1` (the canonical verification gate),
   `.ai/dispatch.tasks.md` (the autonomous task brief), and
   `Get-AiDispatchHealth.ps1` (the dispatch-health readout). Edit only the
   section 4 table in `AI_DISPATCH_AUTOMATION.md`.

2. Documentation sync, `AI_DISPATCH_AUTOMATION.md` only. Add a new subsection
   "7.1 Unattended operation" under section 7 (Modes). In roughly 20-30 lines,
   describe the three layers that run on top of the loop: the queue runner
   (`Invoke-AiDispatchQueue.ps1`, consumes `ai-dispatch` GitHub issues), the
   autonomous driver (`Invoke-AiDispatchAuto.ps1`, Codex selects tasks from
   `.ai/dispatch.tasks.md`), and the scheduler
   (`Register-AiDispatchSchedule.ps1`, runs the automation on a recurring
   Scheduled Task). Cross-reference the scripts by name; do not duplicate
   their full parameter tables. Edit only `AI_DISPATCH_AUTOMATION.md`.

3. Documentation touch-up, `AGENTS.md` only. Extend the "Pointers" list with
   one concise bullet for `Get-AiDispatchHealth.ps1`: the dispatch-health
   readout — pass rate, correction rounds, and retries across the recorded
   `.ai/dispatch-*/` runs. Match the brevity of the existing pointer bullets.
   Edit only `AGENTS.md`.

4. Documentation sync, `AI_DISPATCH_AUTOMATION.md` only. Section 8 lists the
   `Invoke-AiDispatchLoop.ps1` parameter defaults (`-MaxPlanRevisions` 1,
   `-MaxCorrectionRounds` 1). Add a short note below the section 8 parameter
   table explaining that `Invoke-AiDispatchQueue.ps1` and
   `Invoke-AiDispatchAuto.ps1` default `-MaxCorrectionRounds` to 2 — coding
   tasks reach `needs_changes` more often than documentation tasks — and that
   both scripts pass `-MaxPlanRevisions` and `-MaxCorrectionRounds` through to
   the loop. Edit only `AI_DISPATCH_AUTOMATION.md`.
