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

Code batch 2 — two small, low-risk, test-coverage-only tasks. Each adds tests
to one file's `#[cfg(test)] mod tests` block and changes no runtime behavior,
so any mistake fails the verification gate and publishes nothing. Each names
the single file it may touch.

1. Test coverage, `crates/audio/src/falloff.rs` only.
   `AudioFalloff::to_kira_easing` maps all four `AudioFalloff` variants to a
   `kira::Easing`, but the existing easing-map test asserts only two of them
   (`Linear` and `InverseSquare`). Extend the test coverage so all four
   variants are asserted against their documented `kira::Easing` values —
   including the `Logarithmic` and `Custom` cases — and add one test that
   `AudioFalloff::Custom` with a negative exponent does not produce a NaN or
   negative amplitude (this exercises the `exp.max(0.0)` clamp). Edit only
   `crates/audio/src/falloff.rs`.

2. Test coverage, `crates/io-image/src/format_detect.rs` only. `detect_format`
   identifies an image format by matching magic-byte prefixes. The existing
   tests cover each full magic plus the empty-input and unknown-format cases,
   but not a truncated prefix of a valid magic. Add a test asserting that a
   strict truncated prefix of the PNG signature (the PNG magic is 8 bytes — use
   its first 3) returns `None`, pinning that a partial magic is not misdetected
   as a full match. Edit only `crates/io-image/src/format_detect.rs`.
