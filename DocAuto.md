# RGE Autonomous Dispatch Operating Manual

This file is the operator manual for the RGE AI automation system. It describes
how work is selected, routed through Codex and Claude, verified, committed, and
published. Use this file when starting, watching, debugging, or re-arming the
automation.

The system has three layers:

1. `Invoke-AiDispatchAuto.ps1` selects work.
2. `Invoke-AiDispatchQueue.ps1` runs GitHub issues through the dispatch loop and
   owns commit, merge, push, comments, labels, retries, and logs.
3. `Invoke-AiDispatchLoop.ps1` routes one task through Codex plan, Claude gate,
   Claude execute, verification, and Codex control. It never commits or pushes.

For no-human publishing, use the autonomous driver or schedule with
`-PublishMode main`, or use the queue directly without `-NoPublish`.

## Quick Commands

Run one autonomous tick and auto-publish passing work:

```powershell
cd A:\RCAD\RGE
.\Invoke-AiDispatchAuto.ps1 -PublishMode main -MaxAutonomousTasks 20 -MaxCorrectionRounds 2
```

Register the recurring autonomous loop:

```powershell
cd A:\RCAD\RGE
.\Register-AiDispatchSchedule.ps1 -Autonomous -PublishMode main -IntervalMinutes 30 -MaxRunHours 6 -MaxAutonomousTasks 20 -MaxCorrectionRounds 2
```

Check the scheduled task:

```powershell
.\Register-AiDispatchSchedule.ps1 -Status
```

Start the scheduled task immediately:

```powershell
Start-ScheduledTask -TaskName RGE-AiDispatch
```

Watch the latest dispatch:

```powershell
.\Watch-AiDispatch.ps1 -Latest -Tail
```

Check dispatch health:

```powershell
.\Get-AiDispatchHealth.ps1 -IncludeIncomplete
```

Dry-run the autonomous selector:

```powershell
.\Invoke-AiDispatchAuto.ps1 -DryRun -PublishMode main
```

Dry-run the issue queue:

```powershell
.\Invoke-AiDispatchQueue.ps1 -DryRun
```

Stop the recurring scheduled task:

```powershell
.\Register-AiDispatchSchedule.ps1 -Unregister
```

## Source Of Work

The autonomous source of work is:

```text
.ai/dispatch.tasks.md
```

`Invoke-AiDispatchAuto.ps1` reads that file when the queue is empty. Codex then
selects exactly one bounded task and files it as a GitHub issue with these
labels:

```text
ai-dispatch
ai-auto
```

The queue then runs that issue through the hardened path.

The task brief can work in two styles:

1. Explicit list: write concrete tasks in priority order. This is safest.
2. Roadmap pointer: tell Codex where to choose from, for example `HANDOFF.md` or
   a plan section. This is more autonomous and has more drift risk.

The brief is inert if it contains this exact marker on its own line:

```text
DISPATCH-TASKS-UNARMED
```

Delete that marker to arm the autonomous selector.

The system does not choose randomly. The only automatic choice source is
`.ai/dispatch.tasks.md`, plus already-open GitHub issues labelled
`ai-dispatch`.

## Work Modes

### Autonomous Mode

Entry point:

```powershell
.\Invoke-AiDispatchAuto.ps1 -PublishMode main
```

What it does:

1. Checks for halt conditions.
2. If an `ai-dispatch` issue already exists, drains the queue and selects no new
   work.
3. If the queue is empty, reads `.ai/dispatch.tasks.md`.
4. Asks Codex, read-only, to select one task.
5. Creates a GitHub issue labelled `ai-dispatch` and `ai-auto`.
6. Runs `Invoke-AiDispatchQueue.ps1`.

Important defaults:

- Default `-PublishMode` is `branch`.
- Use `-PublishMode main` for no-human auto-publish.
- Default `-MaxAutonomousTasks` is `5`.
- A task cap stops creation of new autonomous issues once that many `ai-auto`
  issues exist.
- Already queued work is still drained even if the cap is reached.

### Issue Queue Mode

Entry point:

```powershell
.\Invoke-AiDispatchQueue.ps1
```

What it does:

1. Selects the oldest open GitHub issue labelled `ai-dispatch`.
2. Creates branch `ai-dispatch/ISSUE-<number>`.
3. Runs the dispatch loop.
4. Writes a detailed `ai_dispatch_logs/log_<timestamp>.md`.
5. Stages and commits the result branch.
6. If the loop exits `0` and Codex control verdict is `pass`, fast-forwards
   `main`, pushes `origin/main`, deletes the branch, comments, relabels, and
   closes the issue.
7. If the run fails, keeps the branch or archives it for retry, comments, and
   relabels.

Queue mode auto-publishes by default. Use this only when you want to keep a
successful run local:

```powershell
.\Invoke-AiDispatchQueue.ps1 -NoPublish
```

### Direct Loop Mode

Entry point:

```powershell
.\Invoke-AiDispatchLoop.ps1 -DispatchId SOME-ID-001 -Goal "Do one bounded task."
```

Direct loop mode is useful for local experiments. It does not commit, merge,
push, label issues, or close issues. It leaves changes in the working tree.

For the autonomous system, prefer Auto or Queue mode.

## Required Tools

The automation expects these commands on PATH:

```text
git
gh
codex
claude
powershell.exe
cargo
```

`gh` must be authenticated and able to access the GitHub repo:

```powershell
gh auth status
```

`claude` must be authenticated. The loop runs a readiness probe before the real
dispatch:

```text
claude -p --output-format json "Return exactly: ready"
```

The verification gate also requires:

```text
nightly rustfmt
cargo-deny
```

One-time setup if missing:

```powershell
rustup toolchain install nightly
cargo install cargo-deny --locked
```

The repo-local MCP config used by Claude is:

```text
.mcp.json
```

It starts the Codex MCP server:

```json
{
  "mcpServers": {
    "codex": {
      "type": "stdio",
      "command": "codex",
      "args": ["mcp-server"]
    }
  }
}
```

## Git Preconditions

The queue runner is strict before it starts:

1. Current branch must be `main`.
2. Tracked files must be clean.
3. Local `HEAD` must equal `origin/main`.
4. GitHub CLI must be authenticated.
5. `Invoke-AiDispatchLoop.ps1` must exist in the repo root.

Untracked files are allowed. The queue parks them with:

```text
git stash push --include-untracked --message "ai-dispatch-queue park: ISSUE-<n>"
```

After the queue returns to `main`, it restores the stash with `git stash pop`.
If stash restore fails, the issue comment and terminal output warn that the
stash is still present.

Tracked dirty files abort the queue. Commit, stash, or discard them before
running automation.

## GitHub Labels

The queue creates these labels idempotently:

```text
ai-dispatch          queued work
ai-dispatch-running  currently running
ai-dispatch-done     terminal processed state
ai-dispatch-failed   failed terminal state
ai-dispatch-retry    one automatic retry is queued
ai-auto              task was selected by the autonomous driver
```

Label lifecycle for a successful auto-published issue:

```text
ai-dispatch
-> ai-dispatch + ai-dispatch-running
-> ai-dispatch-done
-> issue closed
```

Label lifecycle for a first execution failure:

```text
ai-dispatch
-> ai-dispatch + ai-dispatch-running
-> ai-dispatch + ai-dispatch-retry
-> next queue tick retries once with prior feedback
```

Label lifecycle after retry failure or hard publish failure:

```text
ai-dispatch-running removed
ai-dispatch removed
ai-dispatch-done added
ai-dispatch-failed added
```

Autonomous mode halts while any `ai-auto` task also has
`ai-dispatch-failed`. Remove `ai-dispatch-failed` only after investigating.

## Successful Auto-Publish Path

The full no-human path is:

```text
Windows Scheduled Task
  -> Invoke-AiDispatchAuto.ps1 -PublishMode main
    -> Codex selects one task from .ai/dispatch.tasks.md
    -> gh issue create --label ai-dispatch --label ai-auto
    -> Invoke-AiDispatchQueue.ps1
      -> preflight clean synced main
      -> label issue ai-dispatch-running
      -> branch ai-dispatch/ISSUE-<n>
      -> Invoke-AiDispatchLoop.ps1
        -> scaffold TASK packet
        -> Codex fills TASK
        -> Claude gate reviews TASK
        -> finalize TASK sidecar
        -> Claude executes
        -> Claude writes EXEC packet
        -> verification gate runs
        -> Codex control reviews diff, packets, and verification
      -> write ai_dispatch_logs/log_<timestamp>.md
      -> git add -A
      -> git commit
      -> git checkout main
      -> git fetch origin main
      -> git merge --ff-only ai-dispatch/ISSUE-<n>
      -> git push origin main
      -> delete local issue branch
      -> comment on issue
      -> relabel ai-dispatch-done
      -> close issue
```

The publish gate is exact:

```text
loop exit code == 0
and Codex control verdict == pass
and -NoPublish is not set
```

If any part is false, the queue does not push to `origin/main`.

## Inner Dispatch Loop Details

`Invoke-AiDispatchLoop.ps1` handles exactly one `DispatchId`.

Inputs:

```powershell
-DispatchId <ID>
-Goal "<goal text>"
-GoalFile <path>
-MaxPlanRevisions <0..5>
-MaxCorrectionRounds <0..5>
-ClaudePermissionMode acceptEdits
-CodexModel <optional>
-ClaudeModel <optional>
-AllowDirtyTracked
-PlanOnly
-VerifyScript <path>
-SkipVerification
-ModelTimeoutSec <seconds>
-VerifyTimeoutSec <seconds>
-ResumeApprovedTask
```

Normal loop:

1. Requires `git`, `codex`, and `claude`.
2. Resolves repo root with `git rev-parse --show-toplevel`.
3. Requires:
   - `new-handoff.ps1`
   - `.mcp.json`
   - `.ai/codex_control.schema.json`
   - `ai_handoffs/AI_HANDOFF_PROTOCOL.md`
4. Creates `.ai/dispatch-<DispatchId>/`.
5. Refuses unsynced branches by checking `origin/main...HEAD`.
6. Refuses tracked dirty files unless `-AllowDirtyTracked` is set.
7. Runs Claude readiness probe.
8. Creates a TASK packet with `new-handoff.ps1`.
9. Codex fills the TASK packet only.
10. Claude plan gate returns final marker `GATE_VERDICT: approve`,
    `needs_changes`, or `block`.
11. If approved, TASK is finalized to `.meta.json`.
12. Claude executes and writes an EXEC packet.
13. Claude should end with:

```text
EXEC_STATUS: executed
EXEC_PACKET: ai_handoffs/<DispatchId>_EXEC_<timestamp>.md
```

14. If Claude omits `EXEC_STATUS`, the loop falls back to the canonical EXEC
    packet footer markers:

```text
HANDOFF_STATUS: COMPLETE
EXIT_CODE: 0
```

15. The verification gate runs.
16. Codex control returns schema-validated JSON.
17. If Codex says `needs_changes`, Codex writes a CORRECTION packet and the
    loop sends it back to Claude, up to `-MaxCorrectionRounds`.
18. If Codex says `pass`, the loop exits `0`.
19. The loop prints "No commit or push was performed."

The inner loop never commits or pushes. Commit and push belong only to the
queue runner.

## Verification Gate

Default verification script:

```text
.ai/dispatch.verify.ps1
```

The loop runs it after Claude execution and before Codex control.

Checks:

```text
cargo +nightly fmt --all -- --check
cargo run -q -p rge-tool-architecture-lints -- all
cargo test -p rge-tool-architecture-lints --all-targets
cargo deny check
cargo test --workspace --all-targets --no-fail-fast
cargo test --workspace --doc --no-fail-fast
```

If verification fails, Codex control does not run for that result. The loop
either creates a correction packet and retries, or fails when correction rounds
are exhausted.

Do not use `-SkipVerification` for auto-publish runs. It can allow unverified
changes to publish.

## Model Output Contracts

Claude is not required to return strict JSON. It writes prose and ends with
markers.

Plan gate marker:

```text
GATE_VERDICT: approve
```

Allowed values:

```text
approve
needs_changes
block
```

Execution markers:

```text
EXEC_STATUS: executed
EXEC_PACKET: ai_handoffs/<DispatchId>_EXEC_<timestamp>.md
```

Allowed `EXEC_STATUS` values:

```text
executed
blocked
failed
```

Codex control does use structured JSON with `.ai/codex_control.schema.json`.
Allowed verdicts:

```text
pass
needs_changes
block
```

Allowed `commit_readiness` values:

```text
ready_for_publish
not_ready
no_commit_needed
```

The queue publishes based on the control verdict `pass`, not on prose.

## Runtime Artifacts

### Run Directory

Each loop creates:

```text
.ai/dispatch-<DispatchId>/
```

This directory is local scratch and should stay gitignored.

Typical files:

```text
codex.prompt.md
codex.plan.rev0.log
claude.ready.envelope.json
claude.ready.stderr.txt
claude.plan_gate.rev0.md
claude.plan_gate.rev0.envelope.json
claude.plan_gate.rev0.stderr.txt
claude.execute.round0.md
claude.execute.round0.envelope.json
claude.execute.round0.stderr.txt
verification.round0.log
codex.control.round0.json
codex.control.round0.log
codex.correct.round0.log
correct.finalize-dryrun.round0.log
```

### Handoff Packets

Canonical packet directory:

```text
ai_handoffs/
```

Canonical packet names:

```text
<DispatchId>_TASK_<timestamp>.md
<DispatchId>_EXEC_<timestamp>.md
<DispatchId>_CORRECT_<timestamp>.md
<DispatchId>_REVIEW_<timestamp>.md
<DispatchId>_CLOSEOUT_<timestamp>.md
```

Finalized packets have matching sidecars:

```text
<same name>.meta.json
```

Sidecars are created by:

```powershell
.\new-handoff.ps1 -Finalize -PacketPath <packet>
```

### Queue Logs

Every queue run writes a committed detailed log before publish:

```text
ai_dispatch_logs/log_<timestamp>.md
```

The log contains:

- dispatch id
- issue number, title, and URL
- branch
- loop exit code
- Codex control verdict
- process trace
- `git status --short --untracked-files=all`
- `git diff --name-status`
- `git diff --stat`
- generated run files
- Claude marker summary
- Codex control JSON
- tail of loop output

This is the file to read when asking "what did automation do?"

## Branches And Commits

For issue `#12`, the queue uses:

```text
DispatchId: ISSUE-12
Branch:     ai-dispatch/ISSUE-12
```

Commit subject:

```text
ai-dispatch ISSUE-12: <issue title>
```

Commit body includes:

```text
Loop exit code: <n>
Control verdict: <verdict>
Source: <GitHub issue URL>
Detailed log: ai_dispatch_logs/log_<timestamp>.md
Publish policy: auto-push to origin/main only when loop exit code is 0 and Codex control verdict is pass.
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

Publish sequence:

```text
git fetch origin +main:refs/remotes/origin/main
git merge --ff-only ai-dispatch/ISSUE-<n>
git push origin main
git branch -d ai-dispatch/ISSUE-<n>
```

If push fails after local merge, the queue resets local `main` back to the
pre-merge SHA and keeps the issue branch for review.

## Retry And Failure Behavior

The queue gives one automatic retry for dispatch-execution failures.

First failure:

1. Branch commit is kept if one exists.
2. Failed branch is renamed to `ai-dispatch/ISSUE-<n>.attempt1`.
3. Issue keeps `ai-dispatch`.
4. Issue gets `ai-dispatch-retry`.
5. Next queue tick retries it.
6. Prior control or verification feedback is injected into the new goal.

Retry failure:

1. Issue loses `ai-dispatch` and `ai-dispatch-running`.
2. Issue gets `ai-dispatch-done`.
3. Issue gets `ai-dispatch-failed`.
4. Autonomous mode halts until a human investigates and removes
   `ai-dispatch-failed`.

Hard publish failures do not get an automatic retry. Re-running publish can
duplicate or corrupt branch state, so the queue marks the issue failed and keeps
the branch for inspection.

## Orphan Recovery

At the start of each queue tick, `Invoke-AiDispatchQueue.ps1` runs orphan
recovery.

It handles:

- issues stuck in `ai-dispatch-running`
- leftover `ai-dispatch/ISSUE-*` branches
- parked `ai-dispatch-queue park:` stashes
- interrupted publish where local `main` is ahead of `origin/main`
- stale `.ai/dispatch-<ID>/` scratch directories

If the repo is on a queue-owned branch, it can force back to `main`. If the repo
is on another branch, it refuses to proceed because that branch may contain
human work.

If a prior commit already reached `origin/main`, orphan recovery marks the issue
done and closes it instead of re-running the work.

## Halts

Autonomous mode halts on:

1. `.ai/dispatch.auto-halt` exists.
2. Any `ai-auto` issue has `ai-dispatch-failed`.
3. `-MaxAutonomousTasks` cap is reached and the queue is empty.
4. The task brief is missing, empty, or contains `DISPATCH-TASKS-UNARMED`.
5. Codex cannot return a parseable task block.

Queue mode halts/fails on:

1. Not on `main`.
2. Tracked dirty files.
3. Local `main` not equal to `origin/main`.
4. Missing required commands.
5. Missing required scripts or schemas.
6. `gh` not authenticated.
7. Existing issue branch in inconsistent state.
8. Model command failure or timeout.
9. Verification failure with exhausted correction rounds.
10. Codex control `block`.
11. Label finalization cannot be verified.

## Clearing A Halt

Inspect the issue and log first:

```powershell
gh issue view <number> --repo RustCADs/RGE --json number,title,state,labels,comments,url
.\Watch-AiDispatch.ps1 -DispatchId ISSUE-<number> -Once -Tail
Get-Content ai_dispatch_logs\log_<timestamp>.md -Tail 220
```

If `.ai/dispatch.auto-halt` exists and the cause is understood:

```powershell
Remove-Item .ai\dispatch.auto-halt
```

If an auto issue is failed and the failure is handled:

```powershell
gh issue edit <number> --repo RustCADs/RGE --remove-label ai-dispatch-failed
```

If the autonomous cap is reached, either leave it halted for review or
re-register/run with a higher cap:

```powershell
.\Register-AiDispatchSchedule.ps1 -Autonomous -PublishMode main -MaxAutonomousTasks 20
```

## Watching Progress

Use watcher:

```powershell
.\Watch-AiDispatch.ps1 -Latest
.\Watch-AiDispatch.ps1 -Latest -Tail
.\Watch-AiDispatch.ps1 -DispatchId ISSUE-12 -Once -Tail
```

The watcher is read-only. It shows:

- current git branch and sync
- packet table
- latest run file
- plan gate verdict
- execution status and exec packet
- Codex control verdict and summary
- tail of latest run file when `-Tail` is set

Use health readout:

```powershell
.\Get-AiDispatchHealth.ps1
.\Get-AiDispatchHealth.ps1 -IncludeIncomplete
```

Healthy runs usually have:

```text
Outcome PASS
PlanRevs 0
Corrections 0
VerifyRuns 1
```

Rising correction rounds mean selected tasks are too broad, prompts are unclear,
or verification is catching legitimate misses.

## Manual Queue Feeding

To feed the queue manually without autonomous selection:

```powershell
gh issue create --repo RustCADs/RGE --title "Small bounded task" --body "Goal and done criteria." --label ai-dispatch
.\Invoke-AiDispatchQueue.ps1
```

This still uses full automation after the issue exists.

## Updating The Autonomous Plan

To change what auto does next, edit:

```text
.ai/dispatch.tasks.md
```

Recommended task format:

```text
1. Imperative task title, one tight area only.
   Goal: ...
   In scope: ...
   Out of scope: ...
   Done when: ...
   Verification: ...
```

Good task shape:

- one file or one tight module area
- independently shippable
- clear done criteria
- clear verification
- no vague "improve everything"
- no hidden publish decision

Bad task shape:

- "fix all issues"
- "continue roadmap"
- "make it better"
- large cross-cutting refactor without a verification plan
- task that depends on external accounts or manual UI interaction

If you want the system to follow the main plan, put an explicit ordered list
from the main plan into `.ai/dispatch.tasks.md`. Do not rely on a chat session
remembering the plan.

## When To Use Each Entry Point

Use this for normal no-human operation:

```powershell
.\Invoke-AiDispatchAuto.ps1 -PublishMode main
```

Use this when you already created or labelled GitHub issues:

```powershell
.\Invoke-AiDispatchQueue.ps1
```

Use this to inspect the next issue only:

```powershell
.\Invoke-AiDispatchQueue.ps1 -DryRun
```

Use this to test task selection only:

```powershell
.\Invoke-AiDispatchAuto.ps1 -DryRun
```

Use this for a one-off direct experiment that should not commit or push:

```powershell
.\Invoke-AiDispatchLoop.ps1 -DispatchId EXPERIMENT-001 -Goal "..."
```

Use this to leave successful work on a branch instead of pushing:

```powershell
.\Invoke-AiDispatchAuto.ps1 -PublishMode branch
.\Invoke-AiDispatchQueue.ps1 -NoPublish
```

## Do Not Do These

- Do not manually relay Codex and Claude between chat windows. The scripts are
  the automation.
- Do not run the inner loop and expect commits or pushes. The inner loop never
  publishes.
- Do not use `-SkipVerification` for a publish-capable run.
- Do not leave tracked files dirty on `main`.
- Do not start from a branch other than `main`.
- Do not delete handoff packets unless intentionally pruning audit history.
- Do not edit `.ai/dispatch-<ID>/` run files to fake a pass.
- Do not clear `ai-dispatch-failed` without reading the issue comment and log.
- Do not put broad roadmap prose in `.ai/dispatch.tasks.md` unless you accept
  that Codex will choose a bounded interpretation.

## File Map

Core:

```text
Invoke-AiDispatchAuto.ps1
Invoke-AiDispatchQueue.ps1
Invoke-AiDispatchLoop.ps1
Register-AiDispatchSchedule.ps1
Watch-AiDispatch.ps1
Get-AiDispatchHealth.ps1
new-handoff.ps1
```

Task source and verification:

```text
.ai/dispatch.tasks.md
.ai/dispatch.verify.ps1
```

Contracts and config:

```text
.mcp.json
.ai/codex_control.schema.json
.ai/codex_review.schema.json
.ai/claude_brief.schema.json
.ai/handoff.schema.json
```

Packet protocol:

```text
ai_handoffs/AI_HANDOFF_PROTOCOL.md
ai_handoffs/templates/TASK_PACKET.md
ai_handoffs/templates/EXECUTION_REPORT.md
ai_handoffs/templates/CORRECTION_PACKET.md
ai_handoffs/templates/REVIEW_REPORT.md
ai_handoffs/templates/FINAL_CLOSEOUT.md
```

Generated audit:

```text
.ai/dispatch-*/
ai_handoffs/<DispatchId>_*.md
ai_handoffs/<DispatchId>_*.meta.json
ai_dispatch_logs/log_*.md
```

Project state inputs:

```text
Status.md
HANDOFF.md
plans/PLAN.md
plans/IMPLEMENTATION.md
```

These are not the automation itself, but they are useful inputs for writing
`.ai/dispatch.tasks.md`.

## Known Failure Messages

`Tracked files are dirty on main`

Fix: commit or stash tracked changes. Queue refuses to run with tracked dirty
files.

`Local main is not in sync with origin/main`

Fix: fetch/pull or push until `HEAD == origin/main`.

`A dispatch-queue run is already in progress`

Meaning: lock file has a live owner. Let the current run finish.

`Lock is stale`

Meaning: previous process died. Queue will remove the stale lock and continue.

`claude readiness probe timed out`

Fix: check Claude CLI/auth. The loop has not started the task yet.

`Verification gate failed`

Meaning: source did not pass the canonical CI-equivalent gate. The loop will
try correction rounds if budget remains.

`Codex requested changes`

Meaning: Codex control found the work not ready. The loop writes a correction
packet and routes it back to Claude if correction budget remains.

`Codex control blocked the dispatch`

Meaning: human arbitration is required. Queue will not publish.

`issue labels did not finalize`

Meaning: GitHub label mutation could not be verified. The queue exits non-zero
so autonomous mode halts rather than looping incorrectly.

## Minimum Checklist Before Leaving It Running

1. `gh auth status` works.
2. `claude` readiness works.
3. `git status --short` has no tracked dirty files.
4. `git rev-parse HEAD` equals `git rev-parse origin/main`.
5. `.ai/dispatch.tasks.md` contains real bounded tasks and no
   `DISPATCH-TASKS-UNARMED` marker.
6. `.\Invoke-AiDispatchAuto.ps1 -DryRun -PublishMode main` selects the expected
   task or reports none.
7. `.\Register-AiDispatchSchedule.ps1 -Autonomous -PublishMode main ...` is
   registered if recurring unattended operation is desired.
8. `.\Register-AiDispatchSchedule.ps1 -Status` shows the expected task.
9. `.\Watch-AiDispatch.ps1 -Latest -Tail` is available for observation.

## Current Intended Use

For the current no-human-layer workflow, the intended recurring setup is:

```powershell
cd A:\RCAD\RGE
.\Register-AiDispatchSchedule.ps1 `
  -Autonomous `
  -PublishMode main `
  -IntervalMinutes 30 `
  -MaxRunHours 6 `
  -MaxAutonomousTasks 20 `
  -MaxPlanRevisions 1 `
  -MaxCorrectionRounds 2
```

Then the only regular maintenance is:

1. Keep `.ai/dispatch.tasks.md` filled with small ordered tasks.
2. Watch `ai_dispatch_logs/log_*.md` and `Get-AiDispatchHealth.ps1`.
3. Investigate and clear `ai-dispatch-failed` only after reading the failed
   issue and its log.

