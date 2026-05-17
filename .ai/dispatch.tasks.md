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

Starter batch — small, bounded, documentation-only tasks. Each is independent
and names the single file it may touch.

1. Documentation sync, `AI_DISPATCH_AUTOMATION.md` only. That document
   predates the verification gate. Add a concise subsection describing
   `.ai/dispatch.verify.ps1`: it mirrors the four GitHub Actions workflows
   (format, architecture lints, cargo-deny, workspace tests); the dispatch
   loop runs it after the Claude execution step and before Codex control; a
   non-zero exit fails the dispatch before any publish. Edit only
   `AI_DISPATCH_AUTOMATION.md`, matching the document's existing style.

2. Documentation sync, `AI_DISPATCH_PARALLEL.md` only. Add a short note that
   both the dispatch queue and the autonomous driver
   (`Invoke-AiDispatchAuto.ps1`) hold their own single-run lock, so
   overlapping scheduled and manual ticks are serialized rather than
   colliding. Edit only `AI_DISPATCH_PARALLEL.md`, matching its existing
   style.

3. Documentation touch-up, `AGENTS.md` only. Extend the "Pointers" list with
   one concise bullet each for `Invoke-AiDispatchAuto.ps1` (the autonomous
   driver — Codex selects the next task from `.ai/dispatch.tasks.md`) and
   `Register-AiDispatchSchedule.ps1` (registers the unattended Scheduled
   Task). Match the brevity of the existing pointer bullets. Edit only
   `AGENTS.md`.
