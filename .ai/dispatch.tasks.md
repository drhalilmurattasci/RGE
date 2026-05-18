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

Style B — roadmap pointer. Codex selects the work autonomously each tick;
there is no hand-written task list. Read `HANDOFF.md` and pick the next piece
of unstarted work from its "Next-job options" / next-jobs section.

Selection rules, in priority order:

1. Choose the SMALLEST, most self-contained slice available. The HANDOFF
   options are multi-file; do NOT attempt a whole option in one dispatch.
   Decompose it: pick one file, one small module, or one operator's worth of
   work — never a cross-cutting change.
2. The slice must have a clear done-criterion and a concrete verification
   command (the file's own tests, or `cargo test -p <crate>`). Prefer
   additive work (new tests, one bounded function or impl) over refactors.
3. Skip anything marked BLOCKED, anything needing design sign-off or human
   arbitration, and anything requiring external accounts or manual UI steps.
4. Honor the architecture: `OperatorNode` is the canonical IR; renderer-tier
   crates (`rge-gfx*`) must not depend on game-domain crates. Narrow the
   scope whenever a choice is unclear.
5. If no sufficiently small, well-scoped slice exists, select nothing this
   tick rather than filing a vague task.

One tight area per dispatch. Vague scope is the only thing the loop reliably
fails on — keep every selected task narrow.
