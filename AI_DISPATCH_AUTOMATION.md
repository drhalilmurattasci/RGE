# AI Dispatch Automation — Codex-Plans / Claude-Executes / Codex-Controls

A reusable guide to the AI dispatch tools. `Invoke-AiDispatchLoop.ps1` drives a
two-model dispatch loop (OpenAI **Codex** + Anthropic **Claude**) over a Markdown
"handoff packet" protocol. `Invoke-AiDispatchQueue.ps1` wraps that loop with a
GitHub-issue work queue and publishes successful, Codex-control-passed runs
according to the selected publish mode — by default (`pr`) it pushes the
dispatch branch and opens a pull request targeting `main` for human review;
explicit `-PublishMode main` is the opt-in path that fast-forwards and pushes
`origin/main`.

> Origin: built for the RGE repository (`A:\RCAD\RGE\`). This document is written
> so the system can be lifted into another project — see **§15 Porting**.
> Tested environment: Windows PowerShell **5.1**, `codex` CLI (OpenAI Codex
> v0.130.x, npm global), `claude` CLI (Claude Code, npm global), Git.
> Synced to `Invoke-AiDispatchLoop.ps1` as of 2026-05-16.

---

## Table of contents

1. What this is
2. Mental model
3. Quick start
4. Files & components
5. Prerequisites & environment
6. The dispatch flow
7. Modes
8. Parameters
9. Exact CLI invocations
10. The prompts — copy/paste
11. Structured output and markers
12. `.mcp.json`
13. The packet protocol
14. Known gotchas & fixes (read this before porting)
15. Porting to another project
16. Operational notes
17. Seven-task arc retrospective (2026-05-22)
18. Delegated-human auto-publish policy

---

## 1. What this is

`Invoke-AiDispatchLoop.ps1` automates **one bounded task ("dispatch")** end to end:

1. **Codex** writes a precise TASK specification from a one-line goal.
2. **Claude** reviews that spec as a preflight gate; if approved, **Claude** executes it.
3. **Codex** does a read-only control review of the result.
4. If Codex asks for changes, it writes a CORRECTION packet and Claude re-executes.

It is a thin orchestration layer on top of an existing Markdown packet protocol
(`ai_handoffs/`). The loop script **automates model routing only**. It never
stages, commits, or pushes.

`Invoke-AiDispatchQueue.ps1` is the outer unattended queue runner. It reads open
GitHub issues labelled `ai-dispatch`, runs one issue at a time on
`ai-dispatch/ISSUE-<n>`, writes a detailed `ai_dispatch_logs/log_*.md` audit
file, and commits the result on that branch. When the dispatch exits 0 and
Codex control returns `pass`, the queue then publishes per the selected
`-PublishMode`: the default `pr` mode pushes the dispatch branch and opens a
pull request targeting `main` for human review, explicit `main` mode
fast-forwards and pushes `origin/main`, and `branch` / `-NoPublish` leaves
the work on the branch. Failed or blocked runs remain local and are labelled
`ai-dispatch-failed`.

**One run = one task.** It is *not* an autonomous "build the whole project"
agent. It plans → gates → executes → control-reviews (with capped retries),
prints `Dispatch loop finished.`, and exits. To advance a project unattended,
feed scoped GitHub issues to the queue runner.

---

## 2. Mental model

| Role | Model | Does |
|---|---|---|
| Planner | Codex | Fills the TASK packet from the goal; writes CORRECTION packets |
| Executor preflight gate | Claude | Reviews the TASK *before* execution → `approve` / `needs_changes` / `block` |
| Executor | Claude | Performs the task; writes an EXECUTION_REPORT packet |
| Controller / Reviewer | Codex | Read-only review of diff + packets → `pass` / `needs_changes` / `block` |

**Design invariants**

- **Bounded scope.** Each TASK packet enumerates explicit MAY-edit /
  MUST-NOT-edit / deliverables / verification gates / halt conditions.
- **Loop has no commit/push.** The inner loop only edits the working tree. The
  outer queue runner is responsible for commit/push policy.
- **Publish gate.** The queue runner publishes only when the loop exits 0 and
  Codex control verdict is `pass`. The action taken on a passed run is
  controlled by `-PublishMode` (default `pr` opens a PR; explicit `main` is
  the opt-in `origin/main` auto-push; `branch` / `-NoPublish` skips publish).
- **Auditable.** Every model-to-model handoff is a Markdown packet on disk
  (append-only), plus a `.meta.json` sidecar written on finalize.
- **Capped iteration.** At most `MaxPlanRevisions` plan revisions and
  `MaxCorrectionRounds` correction rounds, then it stops.
- **Fail fast & loud.** Any preflight or step failure calls `Fail` (writes to
  stderr, `exit 1`). The script uses `$ErrorActionPreference = 'Stop'`.

---

## 3. Quick start

```powershell
cd <repo-root>

# A. New task — full loop (plan -> gate -> execute -> control)
.\Invoke-AiDispatchLoop.ps1 -DispatchId MYPROJECT-TASK-001 `
  -Goal "Describe the bounded task here."

# B. Plan only — stop after Codex plans and Claude approves the TASK
.\Invoke-AiDispatchLoop.ps1 -DispatchId MYPROJECT-TASK-001 `
  -Goal "..." -PlanOnly

# C. Resume — execute an already-approved, finalized TASK (no new TASK created)
.\Invoke-AiDispatchLoop.ps1 -DispatchId MYPROJECT-TASK-001 -ResumeApprovedTask

# D. Watch a dispatch from another terminal (read-only)
.\Watch-AiDispatch.ps1 -DispatchId MYPROJECT-TASK-001
```

Add `-AllowDirtyTracked` if the working tree has pre-existing tracked
modifications (the preflight otherwise aborts). The loop is long-running — run
it in a real interactive terminal, not a wrapped/CI runner with a short timeout
(see §14.5).

---

## 4. Files & components

| Path | Role | Port? |
|---|---|---|
| `Invoke-AiDispatchLoop.ps1` | The orchestrator (this automation). | **Copy** |
| `Invoke-AiDispatchQueue.ps1` | GitHub issue queue runner; commits and publishes passed dispatches per `-PublishMode` (default `pr` opens a PR for human review; explicit `main` is the opt-in `origin/main` auto-push). | **Copy/adapt** |
| `Watch-AiDispatch.ps1` | Read-only watcher for packets and `.ai/dispatch-<ID>/` scratch while a run is live. | **Copy** |
| `new-handoff.ps1` | Packet scaffold/finalize tool. Scaffolds a `.md` packet; on `-Finalize` parses a completed packet and writes its `.meta.json` sidecar. | **Copy** |
| `Invoke-AiDispatchAuto.ps1` | Autonomous driver: Codex selects the next task from `.ai/dispatch.tasks.md` and runs it through the dispatch queue. | **Copy/adapt** |
| `Register-AiDispatchSchedule.ps1` | Registers, queries, and removes the unattended Windows Scheduled Task that triggers dispatch automation on a recurring interval. | **Copy/adapt** |
| `Get-AiDispatchHealth.ps1` | Read-only dispatch-health readout over retained `.ai/dispatch-*` run directories. | **Copy/adapt** |
| `Wait-GitHubActions.ps1` | Read-only GitHub Actions waiter for the latest workflow run per workflow name on one commit, with hard timeout enforcement. | **Copy/adapt** |
| `.mcp.json` | MCP server config passed to `claude`. | **Copy/adapt** |
| `.ai/codex_control.schema.json` | Schema for Codex's control review. | **Copy** |
| `.ai/dispatch.verify.ps1` | Canonical post-execution verification gate; mirrors CI before Codex control and publish. | **Copy/adapt** |
| `.ai/dispatch.tasks.md` | Autonomous task-selection brief/backlog consumed by the autonomous driver. | **Copy/adapt** |
| `ai_handoffs/AI_HANDOFF_PROTOCOL.md` | The packet protocol spec. | **Copy/adapt** |
| `ai_handoffs/templates/*.md` | Packet templates (TASK_PACKET, EXECUTION_REPORT, REVIEW_REPORT, CORRECTION_PACKET, FINAL_CLOSEOUT). | **Copy** |
| `ai_handoffs/<ID>_<TYPE>_<TS>.md` (+ `.meta.json`) | Generated packets (per dispatch). | generated |
| `.ai/dispatch-<ID>/` | Per-run scratch: prompt files, raw model logs, Claude marker records, Codex control JSON. | generated — **git-ignore** |
| `ai_dispatch_logs/log_*.md` | Queue-run audit log committed before any auto-push; includes file changes, generated artifacts, marker summary, control JSON, and loop output. | generated |

The orchestrator hard-requires (preflight aborts if missing): `new-handoff.ps1`,
`.mcp.json`, `.ai/codex_control.schema.json`, and `ai_handoffs/AI_HANDOFF_PROTOCOL.md`.

---

## 5. Prerequisites & environment

- **Windows PowerShell 5.1+** (`#Requires -Version 5.1`). The gotchas in §14 are
  specific to 5.1's native-command handling.
- **`codex` CLI** on `PATH`, installed and **authenticated**.
- **`claude` CLI** (Claude Code) on `PATH`, installed and **authenticated** —
  see §14.4. A logged-out `claude` invoked headlessly *hangs*.
- **`git`** on `PATH`. The repo must be a git repo. If an `origin/main` remote
  exists, the preflight requires the branch to be in sync with it.
- `new-handoff.ps1` + the `ai_handoffs/` packet protocol present in the repo.
- `.gitignore` should ignore the per-run scratch dir, e.g. `.ai/dispatch-*/`.

---

## 6. The dispatch flow

```
                        Invoke-AiDispatchLoop.ps1
  preflight ─ tools on PATH? required files? git synced? tree clean?
            └ Test-ClaudeCliReady (claude auth probe — fail fast if logged out)

  ── PLAN PHASE ── (skipped entirely in -ResumeApprovedTask mode) ───────────
  scaffold TASK packet            new-handoff.ps1 -PacketType TASK
        │
        ▼
  Codex fills the TASK   ◄──┐     codex exec --sandbox workspace-write
        │                  │
        ▼                  │
  validate TASK            │     new-handoff.ps1 -Finalize -DryRun
        │                  │
        ▼                  │ needs_changes (up to MaxPlanRevisions)
  Claude plan gate ────────┘     claude -p --permission-mode plan
        │  approve
        ▼
  finalize TASK (.meta.json)      new-handoff.ps1 -Finalize
        │
        │  (-PlanOnly: stop here)
        ▼
  ── EXECUTION PHASE ───────────────────────────────────────────────────────
  Claude executes the packet ◄─┐ claude -p --permission-mode acceptEdits
        │  writes EXEC packet  │
        ▼                      │
  Codex control review ────────┘ codex exec --sandbox read-only
        │  pass                │ needs_changes → Codex writes CORRECTION
        ▼                      │ packet, Claude re-executes
  Dispatch loop finished.      │ (up to MaxCorrectionRounds)
  NO commit. NO push.
```

**Step detail**

1. **Preflight.** `Require-Command` for `git`/`codex`/`claude`; verify required
   files exist; reject `-PlanOnly` + `-ResumeApprovedTask` together; resolve the
   repo root; verify branch sync with `origin/main` (skipped if no remote);
   verify no dirty *tracked* files unless `-AllowDirtyTracked`; run
   `Test-ClaudeCliReady`.
2. **Scaffold TASK** — `new-handoff.ps1` creates `ai_handoffs/<ID>_TASK_<TS>.md`
   from the template.
3. **Codex fills the TASK** — `codex exec` (workspace-write sandbox) edits *only*
   the TASK packet, turning the goal into a bounded spec.
4. **Validate** — `new-handoff.ps1 -Finalize -DryRun` confirms the packet is
   complete and parseable.
5. **Claude plan gate** — `claude -p` (plan permission mode) reviews the TASK and
   returns a structured verdict. `needs_changes` loops back to step 3 (capped by
   `MaxPlanRevisions`); `block` aborts.
6. **Finalize TASK** — on `approve`, `new-handoff.ps1 -Finalize` writes the
   `.meta.json` sidecar. A finalized TASK = an approved TASK.
7. *(stop here if `-PlanOnly`)*
8. **Claude executes** — `claude -p` (acceptEdits) performs the task and writes
   an EXECUTION_REPORT packet. The loop then auto-finalizes that packet's
   `.meta.json` sidecar — unless the active packet's text forbids sidecar
   creation (`Test-PacketForbidsSidecar`), in which case the finalize is
   skipped.
9. **Codex control review** — `codex exec` (read-only sandbox) reviews the diff,
   packets, and verification claims; returns a structured verdict.
10. `pass` → loop ends. `needs_changes` → Codex writes a CORRECTION packet,
    Claude re-executes (capped by `MaxCorrectionRounds`). `block` → abort.
11. **End.** Prints the task path, latest EXEC packet, control verdict, and
    commit-readiness. **No commit or push is performed.**

### 6.1 The verification gate

`Invoke-AiDispatchLoop.ps1` runs the canonical verification gate,
`.ai/dispatch.verify.ps1`, after Claude execution and before the Codex control
review. The script mirrors the repository's GitHub Actions workflows
one-for-one — `fmt.yml` (format check), `architecture.yml` (architecture lints
and lint tests), `deny.yml` (supply-chain `cargo deny`), and `tests.yml`
(workspace tests and doctests) — so a passing gate means "CI would pass."

Exit code 0 lets the Codex control review proceed. A non-zero exit fails the
dispatch before publish: no control review runs on that result and the queue
runner does not commit or push it. Within `MaxCorrectionRounds` a failed gate
routes a CORRECTION packet back to Claude instead; once those rounds are
exhausted the dispatch aborts.

---

## 7. Modes

| Mode | Invocation | Behavior |
|---|---|---|
| New dispatch | `-DispatchId X -Goal "..."` (or `-GoalFile path`) | Full flow: plan → gate → execute → control. |
| Plan only | add `-PlanOnly` | Stops after the TASK is approved and finalized. No execution. |
| Resume approved TASK | `-DispatchId X -ResumeApprovedTask` | Skips scaffold/plan/gate/finalize. Locates the existing finalized TASK for `X` (must have a `.meta.json` sidecar = proof it was approved) and runs only the execution + control phase. **No new TASK is created.** Mutually exclusive with `-PlanOnly`. |

### 7.1 Unattended operation

Three scripts layer on top of `Invoke-AiDispatchLoop.ps1` to run dispatches
without a human at the keyboard: a scheduler fires a runner, the runner selects
one unit of work, and each script processes exactly one dispatch per tick.

`Invoke-AiDispatchQueue.ps1` is the **GitHub issue-queue layer**. Each
invocation pulls the oldest open issue labelled `ai-dispatch`, runs it through
the full dispatch loop on a per-issue `ai-dispatch/ISSUE-<n>` branch **inside
an isolated git worktree sibling to the primary repo** (see
[§7.2 Queue-runner worktree isolation](#72-queue-runner-worktree-isolation)),
relabels the issue, and posts a result comment. Publishing is gated: only a
run that exits 0 with a `pass` control verdict is published, and the published
action is the one named by the selected `-PublishMode` — by default (`pr`) the
queue pushes the dispatch branch and opens a pull request targeting `main` for
human review; explicit `-PublishMode main` fast-forwards and pushes
`origin/main`; `branch` / `-NoPublish` leaves the dispatch on its branch.
Failed or blocked runs stay local for inspection. A temp-dir lock file stops
a new invocation from colliding with one still in flight.

#### Mid-run progress comments

In addition to the durable final result comment, the queue posts short,
deterministic **progress comments** to the GitHub issue at the four major
queue/orchestrator stage boundaries so a human watching the issue thread can
see where an active dispatch is without reading local run-dir logs:

| Stage | When | Includes |
|---|---|---|
| `issue-claimed` | Right after the queue adds `ai-dispatch-running` to the issue. | Dispatch id, branch name. |
| `loop-starting` | Just before `Invoke-AiDispatchLoop.ps1` is invoked. | Dispatch id, branch name, local loop-log path. |
| `loop-finished` | Right after the inner dispatch loop returns. | Loop exit code and Codex control verdict (`unknown` when no verdict exists). |
| `publish-decision` | After the loop and before the publish flow runs. | Which of `pr` mode (default — push branch and open a PR), `auto-publish` (explicit `-PublishMode main`), `-NoPublish` branch mode, not-eligible (loop exit / verdict was not `pass`), or no-commit applies. |

Progress comments are concise: they identify the issue, dispatch id, branch,
and stable local log/audit identifiers where available, and they never
include full logs, loop-output tails, model transcripts, diffs, or control
JSON. The final result comment remains the only comment that carries the
loop-output tail.

**Progress-comment failures are non-terminal warnings.** A failure to post a
progress comment emits a clear `WARNING:` line and continues the dispatch; it
never fails the run, triggers a retry, changes labels, alters the publish
decision, or otherwise affects dispatch outcome. The final result comment
and the #227 terminal label reconciliation remain the authoritative durable
outcome signals.

`Invoke-AiDispatchAuto.ps1` is the **task-selection layer** above the queue
runner. When no `ai-dispatch` issue is pending, Codex reads the task brief
(`.ai/dispatch.tasks.md`), picks the next task, files an issue for it, and
hands off to `Invoke-AiDispatchQueue.ps1`. Its `-PublishMode` decides what
happens to a passed task: `pr` (default — ISSUE-239) pushes the dispatch
branch and opens a GitHub pull request targeting `main` without merging,
without pushing `origin/main`, and without closing the source issue;
`branch` leaves the work on its branch for a human to merge; and `main`
auto-publishes to `origin/main` as an explicit, bounded opt-in. It also halts
for human review once a capped number of autonomous issues exist. The bounded
conditions under which `-PublishMode main` may be used are spelled out in
**§18 Delegated-human auto-publish policy**; `-PublishMode pr` is the
human-mediated default — automation handles the branch push and PR creation,
but a human still reviews and merges the PR and decides whether to close the
source issue.

#### Queue-runner publish modes

`Invoke-AiDispatchQueue.ps1` exposes the same three publish modes as a single
`-PublishMode` switch, and preserves the legacy `-NoPublish` switch as an
alias for branch-only mode:

| Mode | Queue invocation | Behavior on a control-passed run |
|---|---|---|
| `pr` (default) | `.\Invoke-AiDispatchQueue.ps1` or `-PublishMode pr` | Push `ai-dispatch/ISSUE-<n>` to `origin` and open a pull request targeting `main` via `gh pr create`. Never runs `git merge --ff-only`, never pushes `origin/main`, never closes the source issue. The PR body links to the issue with `Refs #<n>` (not `Closes #<n>`). A human reviewer merges the PR and decides whether to close the source issue. |
| `main` | `-PublishMode main` | Fast-forward local `main` to the dispatch commit and push `origin main`; close the source issue. Explicit opt-in only — see §18 Delegated-human auto-publish policy. |
| `branch` | `-NoPublish` or `-PublishMode branch` | Commit on the `ai-dispatch/ISSUE-<n>` branch and stop; nothing is pushed, no PR is opened, the issue stays open for human review. |

ISSUE-239 changed the mechanical no-flag default from `main` to `pr`: a queue
call without a publish flag now pushes the dispatch branch and opens a pull
request for human review rather than fast-forwarding `origin/main`.
`-PublishMode main` remains supported as an explicit operator choice for
delegated-human auto-publish batches. `-NoPublish` continues to mean
branch-only mode and remains valid; combining `-NoPublish` with `-PublishMode
main` or `-PublishMode pr` is a parameter error and the queue fails fast.

PR mode runs only after a branch commit exists, the dispatch loop exits 0,
and Codex control returns `pass`. A branch push or `gh pr create` failure in
PR mode is treated as a publish-pipeline failure: the run is labelled
`ai-dispatch-failure-publish`, the local dispatch branch is preserved for
human recovery, and the run is **not** automatically retried. PR mode never
falls back to pushing `origin/main`.

### 7.2 Queue-runner worktree isolation

By default, every queue dispatch runs the inner Codex/Claude loop, scope
guard, audit-log generation, staging, and commit **inside an isolated git
worktree**, while the primary checkout stays on `main`. This is the queue's
serial one-issue run boundary; it is distinct from the multi-dispatch fan-
out pattern documented in **`AI_DISPATCH_PARALLEL.md`**, which uses a
similar worktree primitive but layers it under an outer fan-out runner.

#### Worktree convention

The queue creates the worktree at
`../dispatch-worktrees/<DispatchId>` — a sibling directory to the primary
repo. For issue #231 that resolves to `../dispatch-worktrees/ISSUE-231` on
the per-issue branch `ai-dispatch/ISSUE-231`. The path is deterministic
and computed by the pure helper `Resolve-DispatchWorktreePath` covered
under `tools/dispatch-tests/**`.

#### Why worktree isolation

The pre-ISSUE-231 queue ran the loop on the primary checkout, which
required parking untracked clutter with `git stash`, switching the primary
checkout to the dispatch branch, running the loop there, committing,
checking back out to `main`, and popping the stash. With worktree
isolation, all of that is gone:

- **Primary stays on `main`.** A primary checkout that starts clean ends
  on clean `main` in sync with `origin/main` after a successful run; any
  pre-existing allowed untracked clutter on the primary is left where it
  was and is **not** staged into the dispatch commit, because the dispatch
  edits a different working tree.
- **Scope guard, audit log, staging, and commit all run inside the
  worktree** via `git -C <worktreePath>`, so the dispatch commit captures
  only the dispatch's own output (validated against the active TASK
  packet's positive surface).
- **Publish gates stay exactly as documented above.** `main`, `branch`,
  `pr`, and legacy `-NoPublish` semantics match the ISSUE-230 publish-mode
  contract; only the run boundary changed.

#### Cleanup: success vs. failure

On a terminal success run, the queue removes the isolated worktree only
after the branch commit is preserved and the selected publish action has
completed. The ordering is mandatory: `git` refuses to delete a branch
that is checked out by a linked worktree, so the worktree removal must
precede the branch delete in `main` mode. The exact dispositions are:

| Outcome | Worktree disposition |
|---|---|
| Terminal success in `main` mode (publish + push completed) | Worktree removed; merged branch deleted. |
| Terminal success in `pr` mode (PR opened) | Worktree removed; dispatch branch kept on `origin` as the PR's head. |
| Terminal success in `branch` mode | Worktree removed; dispatch branch kept locally for human review. |
| Terminal success with no commit (loop produced nothing to commit) | Worktree removed; empty branch deleted. |
| Retry-eligible failure (accidental loop/verify failure, first attempt) | Worktree and branch archived in lockstep under `.attempt<N>` so the retry can claim a fresh path. |
| Terminal failure / `EXEC_STATUS: blocked` / publish-pipeline failure | Worktree **preserved** at its original path for human inspection. |
| Interrupted run discovered by orphan recovery | Worktree and branch archived in lockstep under `.interrupt<N>` so the retry tick can claim a fresh path. |

In every failure or interrupt case the queue's local stdout, the dispatch
audit log, and the GitHub result comment carry a deterministic worktree-
status line (formatted by the pure helper `Format-DispatchWorktreeStatus`)
naming the worktree's exact path and the `git -C <path> status` /
`git -C <path> log --oneline -5` / `git worktree remove <path>` commands a
human needs to inspect, recover, or clean it up. The cleanup decision
itself is centralized in the pure helper
`Test-DispatchWorktreeCleanupDecision` and covered by
`tools/dispatch-tests/**`.

#### Durable worktree-status reporting

The queue surfaces the surviving worktree path through every artifact a
human reaches for after a failed, blocked, interrupted, or orphan-recovered
run, so the path is never trapped in process stdout that scrolled off
screen:

- **Local stdout** prints the disposition line (preserved / archived /
  removed) plus the exact worktree path before the queue exits.
- **The committed dispatch audit log** (`ai_dispatch_logs/log_*.md`)
  carries an `## Isolated Worktree` section formatted by the pure helper
  `Format-DispatchWorktreeAuditSection`. That section is written from
  inside `Write-DispatchLog` whenever a `-WorktreeRoot` is supplied, and
  names the worktree path plus the deterministic inspection / removal
  commands. Using `-WorktreeRoot` as the log location / git-scope flag is
  not sufficient on its own — the audit log content has to identify the
  worktree explicitly so the on-branch artifact remains useful after the
  worktree is removed, archived, or preserved.
- **The final GitHub result comment** on the source issue includes the
  same disposition line, so a human watching the issue thread can see
  where the surviving state lives without reading local logs.
- **Orphan-recovery GitHub comments** posted by `Invoke-OrphanRecovery`
  are built by the pure helper `Format-DispatchOrphanRecoveryComment`.
  When `Save-OrphanDispatchWorktree` archives an interrupted worktree
  under `.interrupt<N>`, the archive path returned by that helper is
  threaded into the comment body alongside the deterministic
  `git -C "<archive>" status --short --branch`,
  `git -C "<archive>" log --oneline -5`, and
  `git worktree remove "<archive>"` commands. Comments for runs whose
  recovery did not produce an archive (e.g. no worktree existed) fall
  back to the legacy comment text without inventing a path.

The reporting surface is covered by Pester under `tools/dispatch-tests/**`:
`Format-DispatchOrphanRecoveryComment.Tests.ps1` pins the orphan-comment
text and the source-level invariant that `Invoke-OrphanRecovery` actually
consumes the formatter with the archive path,
`Format-DispatchWorktreeAuditSection.Tests.ps1` pins the audit-log
section content and that `Write-DispatchLog` embeds it when a worktree
is supplied, and `Format-DispatchWorktreeStatus.Tests.ps1` covers the
result-comment / stdout line.

#### Collision safety

The queue refuses to clobber a leftover worktree or branch on a fresh
attempt:

- Before creating its own worktree the queue checks that
  `../dispatch-worktrees/<DispatchId>` does not already exist; if it does
  the queue fails fast and tells the human how to inspect or remove it.
- The dispatch branch existence check (existing pre-ISSUE-231 behavior) is
  preserved alongside the worktree-path check.
- The retry archival (`.attempt<N>`) and orphan-recovery archival
  (`.interrupt<N>`) move both the worktree and the branch together so the
  archive slot is always usable as a single recovery surface — `git -C
  <path>.attempt<N> status` (or `.interrupt<N>`) shows the prior attempt's
  working tree and history rooted at the archived branch.

`Register-AiDispatchSchedule.ps1` is the **recurring-trigger layer**. It
registers a Windows Scheduled Task that fires one of the two runners on a fixed
interval — the issue queue by default, or the autonomous driver with
`-Autonomous`. Because the queue's single-run lock makes any tick that overlaps
a still-running dispatch skip, a long dispatch never stacks up behind ticks.

---

## 8. Parameters

| Parameter | Type | Default | Notes |
|---|---|---|---|
| `-DispatchId` | string, **mandatory** | — | Must match `^[A-Za-z0-9._-]+$`. Used for packet filenames and the run dir. |
| `-Goal` | string | — | Mandatory in the `GoalText` set. The task goal in plain language. |
| `-GoalFile` | string | — | Mandatory in the `GoalFile` set. Path to a file holding the goal. |
| `-ResumeApprovedTask` | switch | — | Mandatory switch of the `ResumeTask` set. Selects resume mode. |
| `-MaxPlanRevisions` | int 0–5 | `1` | Max Codex re-plan rounds if Claude gates `needs_changes`. |
| `-MaxCorrectionRounds` | int 0–5 | `1` | Max CORRECTION→re-execute rounds if Codex controls `needs_changes`. |
| `-ClaudePermissionMode` | enum | `acceptEdits` | One of `acceptEdits`/`auto`/`bypassPermissions`/`default`/`dontAsk`/`plan`. Used for the *execution* call (the gate call is always `plan`). |
| `-CodexModel` | string | `''` | Optional `--model` override for `codex`. |
| `-ClaudeModel` | string | `''` | Optional `--model` override for `claude`. |
| `-AllowDirtyTracked` | switch | off | Permit running when tracked files are already modified. |
| `-PlanOnly` | switch | off | Stop after the approved TASK. |

Queue-runner defaults differ from the loop default above:
`Invoke-AiDispatchQueue.ps1` and `Invoke-AiDispatchAuto.ps1` default
`-MaxCorrectionRounds` to `2` (not the loop's `1`) because coding tasks more
often reach `needs_changes`. Both queue-layer scripts pass `-MaxPlanRevisions`
and `-MaxCorrectionRounds` through to `Invoke-AiDispatchLoop.ps1`.

Parameter sets: `GoalText` (default) requires `-Goal`; `GoalFile` requires
`-GoalFile`; `ResumeTask` requires `-ResumeApprovedTask`. `-DispatchId` is
mandatory in all three.

---

## 9. Exact CLI invocations

The orchestrator shells out to three commands. All external-CLI calls are
wrapped in a localized `$ErrorActionPreference = 'Continue'` — see §14.1.

**Codex** (`Invoke-CodexPrompt`) — prompt is piped via **stdin** (`-`):

```powershell
# plan-fill / correction-packet (writes a packet)
Get-Content -Raw <prompt-file> | codex exec --cd <repo-root> --sandbox workspace-write [--model <m>] -

# control review (returns structured JSON natively — see §11)
Get-Content -Raw <prompt-file> | codex exec --cd <repo-root> --sandbox read-only [--model <m>] `
  --output-schema <schema.json> --output-last-message <out.json> -
```

**Claude** (`Invoke-ClaudeMarker`) — prompt is passed as a **trailing positional
argument** (never via stdin — see §14.2). Claude still uses
`--output-format json`, but only for the CLI envelope; the payload is free-form
prose with final marker lines that the orchestrator extracts:

```powershell
claude -p --mcp-config <.mcp.json> --permission-mode <plan|acceptEdits|...> `
  --output-format json [--model <m>] "<prompt with final marker contract>"
```

**Claude readiness probe** (`Test-ClaudeCliReady`):

```powershell
claude -p --output-format json "Return exactly: ready"
```

**new-handoff.ps1**:

```powershell
new-handoff.ps1 -DispatchId <id> -PacketType <TASK|EXEC|CORRECT> -Author "<author>"   # scaffold
new-handoff.ps1 -Finalize -PacketPath <packet.md>                                     # write .meta.json
new-handoff.ps1 -Finalize -PacketPath <packet.md> -DryRun                             # validate only
```

---

## 10. The prompts — copy/paste

These are the verbatim prompt templates the orchestrator sends. `$name` tokens
are PowerShell interpolations — substitute the real value when reusing a prompt
by hand. **The prompts hard-code the string `RGE` and the path
`ai_handoffs/AI_HANDOFF_PROTOCOL.md`; replace those when porting.**

### 10.1 Codex — plan-fill (sandbox: `workspace-write`)

```text
You are Planner / OpenAI Codex in the RGE repository.

Fill or revise this TASK_PACKET only:

$taskRel

User goal:

$script:GoalText

Revision number: $RevisionNumber

Prior Claude gate result, if any:

$gateContext

Rules:
- Edit only the TASK_PACKET above.
- Do not edit source, docs, schemas, scripts, .gitignore, or any other packet.
- Replace every placeholder.
- Make scope precise: MAY edit, MUST NOT edit, deliverables, gates, halt conditions.
- If the task is audit-only, make that explicit and set MAY edit to none.
- The TASK packet must preserve every top-level header field and the full
  machine-readable completion footer even if you cannot read
  ai_handoffs/templates/TASK_PACKET.md during planning. The rules below are
  authoritative for that header/footer contract.
- The TASK header at the top of the packet body must contain all of these
  fields, in this order, each on its own line with a non-empty value:
  DISPATCH_ID, AUTHOR, TIMESTAMP, RELATED_FILES, STATUS. RELATED_FILES is
  a bulleted list of repo-relative paths or globs that follows the field
  label on the next lines. The finalizer
  ``new-handoff.ps1 -Finalize -DryRun`` rejects any TASK packet that omits
  one of these header fields and names the missing field in its failure
  text.
- The machine-readable completion footer at the bottom of the packet must
  contain all of these fields, each on its own line with a non-empty value:
  HANDOFF_STATUS, DISPATCH_ID, AUTHOR, NEXT_ROLE, EXIT_CODE. The footer
  must be preserved in full even if the template file is unavailable.
- Every path or glob token listed under ``### MAY edit`` and
  ``### MAY add new files`` must be wrapped in Markdown backticks so the queue
  scope guard can parse it as an explicit code token. Example of a valid
  bullet: ``- ``Invoke-AiDispatchLoop.ps1`` ``.
- Bare-bulleted paths or globs in ``### MAY edit`` and ``### MAY add new files``
  (for example ``- Invoke-AiDispatchLoop.ps1`` with no backticks) are invalid
  for the queue scope guard and must not appear in the generated TASK packet.
- Footer must be:
  HANDOFF_STATUS: COMPLETE
  DISPATCH_ID: <same as header>
  AUTHOR: <same as header, e.g. Planner / OpenAI Codex>
  NEXT_ROLE: EXECUTOR_AI
  EXIT_CODE: 0
- The packet must pass new-handoff.ps1 -Finalize -DryRun.
```

Placeholders: `$taskRel` = repo-relative path of the scaffolded TASK packet;
`$script:GoalText` = the `-Goal`/`-GoalFile` text; `$RevisionNumber` = 0-based
revision index; `$gateContext` = the prior Claude gate prose/marker record, or
`No prior Claude gate.` on revision 0.

### 10.2 Claude — plan gate (permission mode: `plan`)

```text
You are Claude acting as Executor preflight gate for RGE.

Review the TASK_PACKET:

$taskRel

You must not edit files. Read the packet, inspect only the repo context needed
to decide whether the plan is executable, bounded, and protocol-safe.

Write your review as free-form prose. Cover, in whatever structure you prefer:
- the verdict reasoning,
- any blocking reasons,
- recommended changes to the TASK packet,
- the commands you actually ran.

End your response with exactly one line, by itself, anchored at column 1:

GATE_VERDICT: approve

Substitute one of these values for 'approve':
- approve        the task is safe to execute as written.
- needs_changes  Codex should revise the TASK packet first.
- block          execution must not proceed without human arbitration.

That GATE_VERDICT line must be the final line of your response. Do not wrap it
in Markdown, quotes, or a code block.
```

The orchestrator saves the verbatim response to
`.ai/dispatch-<ID>/claude.plan_gate.rev<N>.md` and branches only on the
`GATE_VERDICT:` marker.

### 10.3 Claude — execute (permission mode: `-ClaudePermissionMode`, default `acceptEdits`)

```text
You are Executor / Claude in the RGE repository.

Read and execute this $PacketKind packet:

$packetRel

Protocol rules:
- Execute only the enumerated scope.
- Do not commit.
- Do not push.
- If a halt condition triggers, stop and write an EXECUTION_REPORT with
  STATUS: BLOCKED or NEEDS_HUMAN as appropriate.
- If execution proceeds, write an EXECUTION_REPORT using:
  .\new-handoff.ps1 -DispatchId $DispatchId -PacketType EXEC -Author "Executor / Claude"
- Fill the EXEC packet completely.
- If the active packet allows sidecar creation, run:
  .\new-handoff.ps1 -Finalize -PacketPath <exec packet path>
- If the active packet forbids sidecar .meta.json creation, do not finalize
  the EXEC packet; note that deliberate skip in your summary.

Write a free-form prose summary of the execution: what changed, the
verification commands you ran and their results, the final git status, and
any notes for the reviewer.

End your response with exactly these two lines, by themselves, anchored at
column 1:

EXEC_STATUS: executed
EXEC_PACKET: ai_handoffs/<EXECUTION_REPORT file name>.md

Substitute one EXEC_STATUS value for 'executed':
- executed  the enumerated scope was carried out.
- blocked   a halt condition stopped execution.
- failed    execution was attempted but did not complete.

For EXEC_PACKET give the repo-relative path to the EXECUTION_REPORT you wrote,
or the single word none if no report was written. These two lines must be the
final lines of your response. Do not wrap them in Markdown, quotes, or a code
block.
```

Placeholders: `$PacketKind` = `TASK` or `CORRECTION`; `$packetRel` = repo-relative
path of the active packet; `$DispatchId` = the dispatch id. The orchestrator
saves the verbatim response to `.ai/dispatch-<ID>/claude.execute.round<N>.md`
and branches on `EXEC_STATUS:` plus the optional `EXEC_PACKET:` path. If
Claude omits `EXEC_STATUS:` but writes/finalizes a canonical EXEC packet, the
loop falls back to that packet's footer markers (`HANDOFF_STATUS:` /
`EXIT_CODE:`) instead of failing before Codex control.

### 10.4 Codex — control review (sandbox: `read-only`)

```text
You are Codex Controller / Reviewer for an automated RGE dispatch loop.

Review without editing anything.

Task packet:
$taskRel

Latest execution report:
$execRel

Also inspect:
- git status --short --branch
- git diff
- relevant changed files
- verification claims in the EXECUTION_REPORT
- ai_handoffs/AI_HANDOFF_PROTOCOL.md if protocol interpretation matters

Return schema-compliant JSON only. Use:
- verdict=pass only if the work is ready for queue commit/publish.
- verdict=needs_changes if Codex should write a CORRECTION_PACKET and route it
  back to Claude.
- verdict=block if human arbitration is required.

Do not edit files. Do not stage. Do not commit. Do not push.
```

`$execRel` = repo-relative path of the latest EXEC packet, or `<none>`.

### 10.5 Codex — correction packet (sandbox: `workspace-write`)

```text
You are Planner / OpenAI Codex in the RGE repository.

Write a CORRECTION_PACKET only. Edit only this file:

$packetRel

Codex control review result:

$controlJson

Rules:
- Enumerate only the fixes approved by the control review.
- Do not expand scope.
- Do not edit any source, docs, schemas, scripts, or other packets.
- Fill every placeholder.
- Footer must be:
  HANDOFF_STATUS: COMPLETE
  NEXT_ROLE: EXECUTOR_AI
  EXIT_CODE: 0
```

`$packetRel` = the scaffolded CORRECTION packet; `$controlJson` = the Codex
control-review JSON from §10.4.

### 10.6 Claude marker contract

Claude's `--json-schema` flag is unusable here (§14.3), and strict prompt-level
JSON validation proved brittle for gate/execute steps. The loop now hard-depends
only on the fields it actually consumes and extracts them as line-anchored
markers from Claude's prose:

- Plan gate: final line `GATE_VERDICT: approve|needs_changes|block`
- Execute: final lines `EXEC_STATUS: executed|blocked|failed` and
  `EXEC_PACKET: <repo-relative path|none>`

The full prose remains the audit record and revision context. The markers are
the preferred Claude payload data used for control flow. Execute has one
bounded fallback: if Claude omits `EXEC_STATUS:` but leaves an EXEC packet with
canonical footer markers, `HANDOFF_STATUS: COMPLETE` + `EXIT_CODE: 0` maps to
`executed`; blocked/failed packet footer states map to `blocked`/`failed`.
This mirrors the packet protocol's footer-marker style (`HANDOFF_STATUS:` /
`NEXT_ROLE:` / `EXIT_CODE:`) without reintroducing fuzzy prose parsing.

---

## 11. Structured output and markers

**Asymmetry — important.** Codex returns structured JSON *natively* via
`--output-schema <file> --output-last-message <out>`; the orchestrator reads
that `<out>` file directly for the control review. Claude uses
`--output-format json` only to obtain the CLI envelope; the envelope's `result`
payload is saved as prose and parsed only for the markers in §10.6. Do **not**
"fix" this by re-adding `claude --json-schema` or prompt-level JSON schemas for
Claude.

### 11.1 Claude markers

`Get-MarkerValue` extracts the last line matching `NAME: value`, tolerating
minor Markdown/list/quote decoration around the line. Required enum markers
cause a loud failure if missing or outside their allowed set. Optional markers
return `$null`; `EXEC_PACKET:` is optional because the loop can fall back to the
latest matching EXEC packet on disk. `EXEC_STATUS:` is normally enum-checked
from Claude's response; when absent, `Resolve-ExecStatusFromPacket` may derive
the status from the latest EXEC packet's exact footer markers.

### 11.2 `.ai/codex_control.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Codex control review",
  "type": "object",
  "additionalProperties": false,
  "required": ["verdict", "summary", "task_packet", "exec_packet", "changed_files", "required_fixes", "verification", "commit_readiness", "commands_run"],
  "properties": {
    "verdict": { "type": "string", "enum": ["pass", "needs_changes", "block"] },
    "summary": { "type": "string" },
    "task_packet": { "type": "string" },
    "exec_packet": { "type": ["string", "null"] },
    "changed_files": { "type": "array", "items": { "type": "string" } },
    "required_fixes": { "type": "array", "items": { "type": "string" } },
    "verification": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["command", "result", "exit_code"],
        "properties": {
          "command": { "type": "string" },
          "result": { "type": "string" },
          "exit_code": { "type": ["integer", "null"] }
        }
      }
    },
    "commit_readiness": { "type": "string", "enum": ["ready_for_publish", "not_ready", "no_commit_needed"] },
    "commands_run": { "type": "array", "items": { "type": "string" } }
  }
}
```

---

## 12. `.mcp.json`

Passed to every `claude` call via `--mcp-config`. As shipped it exposes the
`codex` CLI as an MCP server to Claude:

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

Tradeoff: this gives the Claude executor a live back-channel to Codex. It is
**not** part of the packet protocol and is redundant with it — a mid-execution
"ask Codex" call is not captured in any packet, so it sits outside the audit
trail. For strict packet-only auditability, set the file to
`{ "mcpServers": {} }`; the orchestrator still requires the file to exist and
still passes `--mcp-config`.

---

## 13. The packet protocol

The loop sits on a Markdown packet protocol (`ai_handoffs/`). Packets are the
canonical handoff; `.meta.json` sidecars are generated on finalize.

- **Packet types:** `TASK`, `EXEC` (execution report), `REVIEW`, `CORRECT`
  (correction), `CLOSEOUT`.
- **Filename:** `ai_handoffs/<DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.md` and the
  matching `.meta.json`.
- **Machine-readable footer** every packet ends with:
  `HANDOFF_STATUS:` / `NEXT_ROLE:` / `EXIT_CODE:`.
- **Finalize** (`new-handoff.ps1 -Finalize`) parses a completed packet and writes
  its `.meta.json`; an unfilled template is rejected.
- A TASK packet with a `.meta.json` sidecar has, by construction, passed the
  Claude gate — that is the signal `-ResumeApprovedTask` relies on.

The orchestrator drives the protocol; the canonical spec lives in
`ai_handoffs/AI_HANDOFF_PROTOCOL.md`.

---

## 14. Known gotchas & fixes (read this before porting)

These are hard-won. Every one cost a failed run before it was understood.

### 14.1 PowerShell 5.1: native stderr becomes a terminating error

**Symptom:** the script dies with `NativeCommandError` the instant `codex`/
`claude` print anything to stderr — even a harmless version banner.
**Cause:** the script sets `$ErrorActionPreference = 'Stop'`, and that
preference is **inherited by child `.ps1` scripts** — including the npm shims
`codex.ps1` / `claude.ps1`. When their inner `node` process writes to stderr,
PS 5.1 raises it as a *terminating* `NativeCommandError`.
**Fix:** wrap every external-CLI call in a localized
`$ErrorActionPreference = 'Continue'` (`try { ... } finally { restore }`) and
detect real failure with `$LASTEXITCODE`. Applied in `Invoke-CodexPrompt`,
`Invoke-ClaudeMarker`, and `Test-ClaudeCliReady`.

### 14.2 `claude -p` over a stdin pipe hangs

**Symptom:** the Claude call never returns; the run is eventually killed; the
output files are empty.
**Cause:** piping the prompt (`$Prompt | & claude ...`) does not deliver a clean
stdin EOF under PS 5.1, so `claude` waits for input. (`claude` also prints
`Warning: no stdin data received in 3s, proceeding without it` when stdin is not
a TTY.)
**Fix:** pass the prompt as a **trailing positional argument**
(`& claude @args $Prompt`), never via stdin.

### 14.3 `claude --json-schema` hangs

**Symptom:** `claude -p --output-format json --json-schema <schema> "..."` hangs
indefinitely — reproduced even with a tiny one-line schema and no MCP, so it is
**not** quote-mangling, auth, the repo, or `.mcp.json`. It is intrinsic to the
flag in this environment.
**Fix:** do **not** use `--json-schema`. Use plain `--output-format json`, read
the `result` field of the JSON envelope, save it verbatim as prose, and extract
line-anchored markers from that prose (§10.6).

### 14.4 `claude` CLI must be authenticated

**Symptom:** with a logged-out `claude`, a headless `claude -p` call hangs (it
tries to start an interactive login it cannot complete).
**Fix:** authenticate the CLI once — run `claude` interactively and `/login`, or
`claude setup-token` (a long-lived token, good for headless use), or set
`ANTHROPIC_API_KEY`. The `Test-ClaudeCliReady` preflight runs a probe and aborts
with a clear message if the CLI is not ready, *before* any packet is scaffolded.

### 14.5 The loop exceeds short command timeouts

**Symptom:** a run wrapped in a tool/CI runner with a ~10-minute cap is killed
mid-flight (exit 255, empty output).
**Cause:** a Codex agentic plan-fill alone takes several minutes; a full
dispatch (plan + gate + execute + control) routinely exceeds 10 minutes.
**Fix:** run the orchestrator in a **real interactive terminal** with no
timeout. `-PlanOnly` is shorter but can still be borderline.

### 14.6 Codex structured output works; Claude's does not

`codex exec --output-schema` is reliable and is used for the control review.
Only Claude's `--json-schema` is broken (§14.3), so Claude gate/execute output
uses markers instead of local JSON schema validation. Do not assume symmetry.

### 14.7 The inner loop never commits

By design there is no `git commit`/`git push` anywhere. Every run ends with
work uncommitted in the tree. `Invoke-AiDispatchQueue.ps1` is the outer
multi-dispatch runner and owns publishing: it writes a detailed timestamped log,
commits the branch, and pushes only after a clean loop exit and `pass` control
verdict. Keep this separation: model routing belongs to the loop; queueing,
commit, push, issue comments, and labels belong to the queue runner.

### 14.8 Salvaging an autonomous issue requires removing `ai-auto`

**Symptom:** after manually closing or salvaging an autonomous dispatch
issue — even after prefixing the title with `[SALVAGED ...]` or
`[COMPLETED-SALVAGE ...]` — the next autonomous tick's Codex selector
still treats the original task as "already filed" and refuses to
re-select it from the brief.

**Cause:** `Invoke-AiDispatchAuto.ps1` builds the "ALREADY FILED" list
passed to Codex via `gh issue list --label ai-auto --state all`. Any
issue carrying the `ai-auto` label — open or closed, regardless of
title — appears in that list, and Codex's task-selection step matches
brief entries against it semantically (not by exact title string).
Renaming the title alone does not remove the issue from the "already
filed" set.

**Fix:** when salvaging an autonomous dispatch that did not pass
control cleanly, remove the `ai-auto` label in addition to scrubbing
`ai-dispatch-failed` / `ai-dispatch-retry`:

```powershell
gh issue edit <num> `
  --remove-label ai-auto `
  --remove-label ai-dispatch-failed `
  --remove-label ai-dispatch-retry
```

Keep `ai-dispatch-done` if the dispatch's substantive deliverable
landed (positive salvage); strip it if the dispatch was abandoned. A
renamed title is still useful for the human audit trail, but the
`ai-auto` removal is the mechanically-enforceable signal that
re-arms the selector for that task entry. Confirmed by re-testing
on `#92` → `#93` after the title-only rename of `#91` failed to
re-arm; `#91` only re-armed once `ai-auto` was stripped.

**Manual salvage protocol.** When a dispatch fails, blocks, or otherwise
needs human recovery but produced work worth preserving, follow this
protocol so the audit trail stays coherent with the queue's automatic
label states (§14.13) and the taxonomy reference in that section:

- **Salvage branch.** Preserve the useful work on a clearly named
  salvage branch such as `ai-dispatch/ISSUE-<n>.salvage`, branched
  from the dispatch's original `ai-dispatch/ISSUE-<n>` branch. Do
  not overwrite, force-reset, or rename the original dispatch
  branch — the failure state is part of the durable record (§18.5).
- **Audit log.** If the normal queue audit log under
  `ai_dispatch_logs/` is missing or incomplete (e.g. the queue
  runner aborted before its commit step), add or keep a branch-local
  `ai_dispatch_logs/log_*.md` capturing the original dispatch id,
  the salvage reason, the commands the human ran, the
  `.ai/dispatch.verify.ps1` result that was available, and the
  Codex control verdict if one was produced before salvage.
- **No direct merge.** Do not fast-forward, rebase-merge, or
  otherwise land the salvage branch on `main` until a human reviewer
  has audited the diff against the original task scope. The
  autonomous loop's auto-publish gate does not run for salvage
  branches.
- **Issue comment.** Comment on the GitHub issue with: the salvage
  branch name, the commit SHA(s), the verification gate result, the
  Codex control verdict (or a note that none was available), and
  the manual recovery reason in plain language. This is the
  human-readable counterpart to the queue runner's automatic result
  comment and the only on-issue record of why automation was
  bypassed.
- **Label cleanup.** Strip misleading queue labels according to the
  outcome, alongside the `ai-auto` removal documented above. Remove
  `ai-dispatch-running`, retry labels (`ai-dispatch-retry`), and any
  stale `ai-dispatch-failure-*` taxonomy labels that no longer
  reflect the salvage outcome. Remove `ai-dispatch-failed` as well
  when the failure is being superseded by positive salvage; keep it
  for abandoned salvage so the terminal-failure record stays
  legible. Keep `ai-dispatch-done` only for **positive salvage** —
  i.e. when the useful work is merged or has been explicitly
  accepted by a human reviewer; strip it otherwise.
- **Issue close policy.** Close the issue as **completed** only
  after the useful work is merged or has been explicitly accepted.
  Otherwise either leave the issue open for follow-up, or close it
  as **not planned** with a one-line reason in the closing comment.

This protocol complements §14.8 above (the `ai-auto` selector
re-arming requirement) and §14.13 below (terminal label-state
reconciliation): the queue runner's `Get-DispatchTerminalLabelPlan`
helper owns the labels for queue-driven terminal states, while the
salvage protocol describes the human-owned transition out of a
failed or blocked terminal state.

### 14.9 Test crates that build real `wgpu` resources need a per-binary serialization guard

**Symptom:** the canonical workspace verification gate (`cargo test
--workspace --all-targets --no-fail-fast -j 1`) intermittently
abnormally exits a per-crate test binary with Windows
`STATUS_ACCESS_VIOLATION (0xc0000005)` AFTER all visible tests
report `ok`. The crash is in post-test teardown, not in any
individual assertion. Observed in `rge-editor` (8 GPU-bearing
end-to-end tests) and `rge-gfx --lib` (180 unit tests, ~30 of
which construct `GfxContext::new_headless()`).

**Cause:** cargo's test harness runs tests *within one binary* on a
thread pool. Multiple `#[test]` functions that build their own
`wgpu::Instance` / `wgpu::Device` / `wgpu::Queue` instances
concurrently expose a Windows-side concurrent-lifecycle bug in
wgpu's teardown path. The `-j 1` flag added in §14 serializes
*linker invocations*, not in-process test parallelism, so it does
NOT prevent this.

**Fix:** every test crate that builds real `wgpu` resources MUST
introduce a per-test-binary serialization guard. Canonical pattern
(landed in `editor/rge-editor/src/main.rs` and
`crates/gfx/src/lib.rs::test_lock`):

```rust
#[cfg(test)]
pub(crate) mod test_lock {
    use std::sync::{Mutex, MutexGuard};
    static GPU_TEST_LOCK: Mutex<()> = Mutex::new(());
    pub(crate) fn guard() -> MutexGuard<'static, ()> {
        GPU_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }
}
```

Every GPU-bearing test acquires the guard at its TOP:

```rust
#[test]
fn renders_something() {
    let _gpu_lock = crate::test_lock::guard();
    let ctx = ctx_or_skip!();   // or equivalent GfxContext build
    // ... rest of test body ...
}
```

Rust drops local bindings in *reverse declaration order*, so
`_gpu_lock` (declared first) drops LAST — after the test's
`GfxContext`. This serializes both *init* AND *teardown* across the
whole binary, which is where the access violation lives.

Poisoned-mutex recovery (`unwrap_or_else(|p| p.into_inner())`) is
mandatory: a single panicking GPU test would otherwise deadlock
every subsequent GPU test in the binary.

Any new test crate that adds a `wgpu` dependency and creates GPU
resources in unit/integration tests MUST follow this pattern, or
the intermittent access violation will re-emerge under the
canonical verify gate.

### 14.10 `EXEC_STATUS: blocked` is terminal, not retryable

**Symptom:** a read-only audit or scope-fenced task correctly halts on a
task-defined boundary (`EXEC_STATUS: blocked`), but the queue treats the
non-zero loop exit like an accidental execution failure and re-queues the
same issue for retry.

**Cause:** the inner dispatch loop must exit non-zero for blocked execution
because no verification or Codex control review may run after a halt
condition. The queue used to look only at the loop exit code, so it could
not distinguish an intentional halt from a broken executor run.

**Fix:** `Invoke-AiDispatchQueue.ps1` reads the newest
`.ai/dispatch-<ID>/claude.execute.round<N>.md` marker and treats
`EXEC_STATUS: blocked` as terminal human-review work. The branch is still
committed locally for inspection, but the issue is not automatically
retried. Accidental `failed` executions remain eligible for the one retry.

### 14.11 Read-only audit tasks: log access vs. log artifacts

**Symptom:** a CI-audit-style read-only task is written with a literal
"do not download artifacts or write logs to disk" constraint, but the
only practical way to access GitHub Actions log content — when per-job
`/logs` endpoints return `HTTP 404 BlobNotFound` and
`gh run view --log-failed` returns `log not found` — is the run-level
ZIP at `gh api repos/<owner>/<repo>/actions/runs/<id>/logs > *.zip`
followed by `unzip -p`. The executor either has to violate the literal
constraint to do the audit, or halt without the log evidence the audit
was created to gather.

**Cause:** the constraint conflates two different intents:

- *Real intent*: no log artifacts in the committed or tracked tree —
  the PR diff and worktree must stay clean of `.zip` / unzipped log
  files.
- *Impractical literal reading*: no disk writes during audit — but
  GitHub's log API requires ZIP retrieval when step-level logs are
  missing, and there is no in-memory equivalent path.

**Fix:** in read-only audit task briefs that allow `gh api .../logs`
inspection, phrase the constraint as:

> *No log artifacts in committed or tracked files. Temp extraction is
> allowed when required to inspect GitHub Actions logs (e.g.
> `gh api repos/<owner>/<repo>/actions/runs/<id>/logs > $tmpdir/run.zip`
> + `unzip -p`), and the temp files must be cleaned up before the
> EXEC report finishes — `git status --short --untracked-files=all`
> must show no `.zip` files or unzipped log directories at audit
> completion.*

Precedent: ISSUE-100 (task #8, 2026-05-23) needed run-level ZIP
access because per-job `/logs` returned `HTTP 404 BlobNotFound` for
every job; the executor wrote and cleaned up five transient ZIPs,
Codex control returned `pass`, and no log artifacts appeared in the
PR diff. The literal text in that task's brief said "Do not download
artifacts or write logs to disk," which the executor was forced to
violate. The phrasing above resolves the ambiguity for future audits.

### 14.12 Task-specific verification gates must not outrun CI by accident

**Symptom:** a dispatch implements the requested change and passes the
canonical repository gate (`.ai/dispatch.verify.ps1`), but halts with
`EXEC_STATUS: blocked` because the task brief added a broad extra
verification command that is not part of CI and fails on unrelated
pre-existing warnings outside the task scope.

**Cause:** focused task gates are useful, but a task-authored gate can
accidentally become a new repository policy. For example, task #25
(ISSUE-134) required
`cargo clippy -p rge-editor-shell --all-targets -- -D warnings`. That
command traversed dependencies of `rge-editor-shell` and failed on many
pre-existing workspace warnings outside the allowed edit set, while the
canonical dispatch gate and GitHub Actions were green. The executor
correctly halted rather than edit forbidden files, but the halt was
caused by an over-broad task gate, not by a bad implementation.

**Fix:** task briefs SHOULD keep publish gates aligned with
`.ai/dispatch.verify.ps1` and the relevant focused tests/builds for the
allowed edit surface. Add extra gates only when they are:

- already part of CI / `.ai/dispatch.verify.ps1`;
- narrow to the allowed file or crate surface and known to be clean; or
- the explicit subject of the task (for example, a task whose purpose is
  to make a clippy lane green).

If a task needs a stronger gate than CI, first verify that gate is
already clean on current `main`, or make the task explicitly about
clearing that gate. Do not hide a broad new policy inside an otherwise
small implementation dispatch.

**Salvage rule:** when a branch is blocked solely by an over-broad
task-authored gate, a human may positive-salvage the substantive change:
review the diff, tighten tests as needed, run the canonical verify gate,
merge with a clean commit message, scrub misleading failure labels, and
document the reason in the issue. This is distinct from §14.10: a
designed `EXEC_STATUS: blocked` halt is terminal; an accidentally bad
task gate is task-author error and can be corrected by human review.

### 14.13 Terminal label-state reconciliation is single-source

**Symptom:** an issue ends a queue run carrying a label combination that
no single state allows — e.g. `ai-dispatch-done` alongside a stale
`ai-dispatch-failure-timeout` from a prior attempt, or `ai-dispatch-retry`
alongside `ai-dispatch-failed` after a corrected re-queue — and the
autonomous driver or human triage cannot tell which state actually won.

**Cause:** the terminal `gh issue edit` was previously built up from
per-branch `if` clauses that only added or removed the labels relevant
to that branch. Stale labels left over from earlier attempts (or from
the prior taxonomy classification of the same issue) were never
explicitly removed.

**Fix:** `Invoke-AiDispatchQueue.ps1` builds the terminal label add/remove
plan through one pure helper, `Get-DispatchTerminalLabelPlan`, which
reconciles all three states deterministically:

- **Terminal success** (`runFailed = $false`, `willRetry = $false`):
  adds `ai-dispatch-done`; removes `ai-dispatch`, `ai-dispatch-running`,
  `ai-dispatch-retry`, `ai-dispatch-failed`, and every
  `ai-dispatch-failure-*` taxonomy label.
- **Terminal failure** (`runFailed = $true`, `willRetry = $false`):
  adds `ai-dispatch-done`, `ai-dispatch-failed`, and the selected
  taxonomy labels; removes `ai-dispatch`, `ai-dispatch-running`,
  `ai-dispatch-retry`, and every non-selected `ai-dispatch-failure-*`
  taxonomy label.
- **Retry** (`willRetry = $true`): keeps or adds `ai-dispatch` and
  `ai-dispatch-retry`; removes `ai-dispatch-running`, `ai-dispatch-done`,
  `ai-dispatch-failed`, and every `ai-dispatch-failure-*` taxonomy label
  (taxonomy labels describe a terminal failure, not queued retry state).

Post-mutation verification asserts both presence of every planned add
label and absence of every planned remove label; a partial `gh edit`
result fails the run non-zero so the autonomous driver halts rather
than loops on contradictory labels. The helper itself is pure and
covered by Pester under `tools/dispatch-tests/**` so the convention is
exercised without any live GitHub calls.

**Terminal failure-taxonomy labels.** Task #52 in
`.ai/dispatch.tasks.md` introduced a small label-only taxonomy that
classifies terminal failed runs after the queue records
`ai-dispatch-failed`. These labels are descriptive triage signals
layered **on top of** `ai-dispatch-failed`; they are **not** retry
selectors by themselves, and they participate in neither issue
selection, cap counting, halt checks, retry eligibility, publishing,
branch archival, nor cleanup:

- `ai-dispatch-failure-stall` — Codex CLI watchdog tripped (no log
  growth / `codex exec stalled` wording); distinct from a generic
  timeout.
- `ai-dispatch-failure-timeout` — wall-clock timeout was reached
  without a Codex watchdog stall signature.
- `ai-dispatch-failure-blocked` — execution halted because the
  executor reported `EXEC_STATUS: blocked` (a designed halt; see
  §14.10).
- `ai-dispatch-failure-verification` — `.ai/dispatch.verify.ps1` (or
  its CORRECTION-retry exhaustion) failed.
- `ai-dispatch-failure-control` — Codex control review returned
  `block`, or exhausted `MaxCorrectionRounds` while still on
  `needs_changes`.
- `ai-dispatch-failure-publish` — the loop succeeded but the queue's
  commit/push step failed (e.g. fast-forward refused, push auth or
  network failure).
- `ai-dispatch-failure-unknown` — terminal failure that did not
  match any classification above.

These taxonomy labels are reconciled by `Get-DispatchTerminalLabelPlan`
described above: the helper adds only the classified label(s) and
strips every other `ai-dispatch-failure-*` label, so a re-classified
retry-and-fail run does not leave the previous attempt's taxonomy
label behind. Selection, retry, cap counting, halt behavior, and
auto-publish remain driven by `ai-dispatch-failed` / `ai-dispatch` /
`ai-dispatch-retry` / `ai-dispatch-done`; the taxonomy labels exist
only to make terminal-failure triage and watchdog/retry tuning
legible to humans and to later policy work. During manual salvage
(§14.8 above), stale taxonomy labels must also be stripped as part
of label cleanup.

---

## 15. Porting to another project

1. **Copy** the files marked **Copy** in §4 into the new repo (preserve the
   `.ai/` and `ai_handoffs/` layout).
2. **Install + authenticate** the `codex` and `claude` CLIs on the machine that
   will run the loop (§5, §14.4).
3. Ensure the target is a **git repo**. If it has an `origin/main`, the branch
   must be in sync at preflight; otherwise the sync check is skipped.
4. **Edit the prompts** in `Invoke-AiDispatchLoop.ps1` (§10): replace the
   literal `RGE` repo name and the `ai_handoffs/AI_HANDOFF_PROTOCOL.md` path
   with the new project's equivalents. The prompts live in `@"..."@`
   here-strings inside `Invoke-PlanFill`, `Invoke-ClaudePlanGate`,
   `Invoke-ClaudeExecute`, `Invoke-CodexControl`, `Invoke-CorrectionPacket`.
5. **Adapt `.mcp.json`** (§12) — keep, empty, or add MCP servers as you wish.
6. Add `.ai/dispatch-*/` to `.gitignore`.
7. Confirm **PowerShell 5.1+** and that the §14 fixes are intact in your copy of
   the script (EAP isolation; positional Claude prompt; marker extraction; no
   `--json-schema`; `Test-ClaudeCliReady`).
8. **Rehearse:** run a `-PlanOnly` dispatch with a small audit-only goal first.
   Verify it reaches `Claude plan gate rev 0: approve` and `TASK finalized`
   before trusting a full run.

---

## 16. Operational notes

- **Per-run scratch** lands in `.ai/dispatch-<DISPATCH_ID>/`: `codex.prompt.md`,
  `codex.*.log`, `claude.*.envelope.json`, `claude.*.stderr.txt`,
  `claude.plan_gate.rev*.md` / `claude.execute.round*.md` (verbatim Claude prose
  + markers), `claude.ready.*`, `codex.control.round*.json`, and finalize
  dry-run logs. This is where to look when a run fails.
- **Generated packets** land in `ai_handoffs/` as `<ID>_<TYPE>_<TS>.md`. TASK
  and CORRECTION packets are finalized (a `.meta.json` sidecar is written). The
  EXEC packet is auto-finalized too — *unless* the active packet's text forbids
  sidecar creation (detected by `Test-PacketForbidsSidecar`), in which case
  finalization is deliberately skipped and the run prints
  `EXEC sidecar finalization skipped; ...`.
- **What to inspect after a run:** the TASK packet; the EXEC packet; the Codex
  control verdict + `commit_readiness`; for `-PlanOnly`, the gate prose and
  `GATE_VERDICT:` marker at `.ai/dispatch-<ID>/claude.plan_gate.rev0.md`.
- **Failure is loud:** every abort prints a `Fail` message naming the log to
  read, and exits non-zero.
- **A failed/aborted run leaves debris** (an unfinalized TASK packet, a run
  dir). Re-running with a fresh `-DispatchId` is the clean recovery; old debris
  is harmless and can be cleaned up later.
- **Resuming:** once a dispatch has an approved + finalized TASK, re-run with
  `-ResumeApprovedTask` to (re-)run only the execution phase without
  re-planning.
- **Waiting for CI:** after pushing a publish or follow-up commit, prefer
  `.\Wait-GitHubActions.ps1` over ad hoc `gh run list` polling. It filters to a
  single commit SHA, keeps only the latest run per workflow name, and enforces
  the timeout before each poll and sleep so the watcher cannot drift past its
  deadline while duplicate or stale runs are still visible.

## 17. Seven-task arc retrospective (2026-05-22)

A seven-task arc through `Invoke-AiDispatchAuto.ps1` ran from
2026-05-21 to 2026-05-22 to validate the autonomous loop across
distinct task shapes. This appendix records what landed, what
doctrine changed because of failures, and the current operating
policy. It is not a status page — read §14 for the
mechanically-enforced gotchas the arc fed back into.

### 17.1 The arc

| # | Task shape                       | Issue       | PR  | Outcome                                  |
|---|----------------------------------|-------------|-----|------------------------------------------|
| 1 | New feature (watcher in editor)  | #85         | #86 | salvaged — infra fixes extracted         |
| 2 | Test fixture + visual assertion  | #87         | #88 | clean                                    |
| 3 | Test-only regression coverage    | #89         | #90 | clean                                    |
| 4 | Read-only architectural audit    | #91 → #92   | #93 | salvaged — scope-preserving halt added   |
| 5 | Production-source adapter        | #94         | #95 | clean — first source-code dispatch       |
| 6 | Test-only follow-up coverage     | #96         | #97 | clean                                    |
| 7 | Read-only cache-surface preflight | #98         | #99 | clean blocked audit — no adapter scoped  |

Task #4 first fired as ISSUE-91 and was salvaged after the
orchestrator's verify gate caught an unrelated workspace test
failure that the auto-routed CORRECTION packet would have expanded
into source edits. Audit content was re-filed and landed as
ISSUE-92.

Task #7 intentionally halted as `STATUS: BLOCKED` /
`HANDOFF_STATUS: BLOCKED` after proving that `rge-io-image` has a
stub cache surface but no reachable cache consumer or content-addressed
`Image` identity. Its audit content landed via PR #99; the queue-runner
retry bug that surfaced on this clean block is encoded in §14.10.

### 17.2 Doctrine that changed because of failures

Four lessons are encoded as mechanical gotchas in §14 — read
those for full reproduction steps. Operational summary:

- **§14.8 — salvage requires removing `ai-auto`.** Title rename
  alone is insufficient; the selector's "already filed" list is
  built from `gh issue list --label ai-auto --state all`. When
  salvaging an autonomous issue you MUST scrub
  `ai-auto` + `ai-dispatch-failed` + `ai-dispatch-retry`.
  Discovered on the #91 → #92 re-arm.

- **§14.9 — GPU test serialization.** Test crates that build real
  `wgpu` resources need a per-binary `test_lock::guard()` mutex
  with poisoned-recovery; multiple `GfxContext` instances tearing
  down concurrently triggers Windows `STATUS_ACCESS_VIOLATION`
  in post-test cleanup. Reference impls live at
  `editor/rge-editor/src/main.rs::test_lock` and
  `crates/gfx/src/lib.rs::test_lock`. Discovered during the
  ISSUE-91 verify gate; the missing guard is what blocked the
  original task #4 from passing canonical verify.

- **Scope-preserving halt clause for read-only audits.** The
  orchestrator's canonical verify gate runs even on read-only
  audits. If verify fails on a target OUTSIDE the audit scope
  and the auto-routed CORRECTION asks the executor to fix it,
  the executor MUST halt with `NEEDS_HUMAN` rather than execute
  the correction. The clause lives in task #4's brief entry in
  `.ai/dispatch.tasks.md`; precedent (#91 → #92) is documented
  inline there.

- **§14.10 — deliberate blocked executions are not retryable.**
  `EXEC_STATUS: blocked` means a halt condition fired by design. The
  queue must commit the branch for human review and stop, not inject
  "prior failure" feedback into a retry that would pressure the executor
  to violate scope. Discovered on task #7 (#98 → #99).

Plus two infra fixes that pre-dated the arc but were validated
across it: PS 5.1 single-item array unrolling (`return ,$items`
in `Get-IssuesJson` of `Invoke-AiDispatchAuto.ps1`), and the
GlbWatcher bare-relative-path bug (`Path::parent() -> Some("")`
on a filename without a directory prefix).

### 17.3 What each task shape demonstrated

Treat this as the prior for what to expect on the next dispatch
of the same shape, not as a celebration.

- **Feature + new dep + cross-crate plumbing (#1):** the loop
  can carry a small feature through plan / execute / verify, but
  surface-level bugs in the substrate (path handling, async
  watcher) only surface under salvage, not under the loop's own
  verify gate. Treat anything novel as a draft that needs
  reviewer eyes.
- **Fixture-binary + visual assertion (#2):** fixture-path
  pinning in the brief (exact filename, not just directory) is
  necessary; otherwise the selector substitutes a shape it finds
  easier to generate (cube/quad) and the assertion semantics
  break by construction.
- **Test-only regression (#3):** smallest reliable task shape.
  Clean across all phases without special doctrine.
- **Read-only audits (#4, #7):** land cleanly with explicit halt
  semantics. #4 validated the scope-preserving verify-failure halt;
  #7 validated the "blocked is terminal, not retryable" queue path.
  Without those clauses, the loop pressures an audit into source work.
- **Production source (#5):** validated end-to-end in `branch`
  mode. The opt-in adapter pattern + per-task carve-out for a
  single dep edge held. First source dispatch the loop carried
  without salvage; n=1 — not yet a sample size that justifies
  relaxing policy.
- **Test-only follow-up to source (#6):** same code area as #5,
  zero Cargo churn, clean across all phases. Confirms a finished
  source dispatch can be safely followed by tightening tests in
  the same file without re-opening adapter design.

### 17.4 Current operating policy

- **`-PublishMode pr` is the mechanical default across the queue,
  the autonomous driver, and the autonomous scheduler (ISSUE-239).**
  A no-flag dispatch pushes the per-issue `ai-dispatch/ISSUE-<n>`
  branch and opens a pull request targeting `main` for human
  review; nothing is fast-forwarded into `origin/main` and the
  source issue stays open. `-PublishMode branch` remains available
  for fully-local human-gated review. `-PublishMode main` is
  reserved for docs and test-only tasks, and only after a dry-run
  confirms the issue body carries the right hard gates; production
  source dispatches stay off `main` until more dispatches pass
  review cleanly — n=1 (#94/#95) is not enough.
- **Verbatim review-gate strings are mandatory** for any task
  whose scope is bounded by named files, named constants, or
  named code shapes. The selector copies them into the issue
  body character-for-character; a packet that lacks any one of
  them is bounced at review without further reading. Pattern
  examples live in `.ai/dispatch.tasks.md` (tasks #1–#7 entries
  all carry them).
- **One task per dispatch, not a batch.** `-MaxAutonomousTasks`
  is raised one at a time, not in bulk. Each task gets a dry-run
  review against an explicit gate checklist before the real run.
- **The push / PR / review / merge path is human-owned.** The
  autonomous loop produces a passing branch with
  `commit_readiness: ready_for_publish`; the human reviews the
  diff against the brief, pushes the branch, opens the PR, and
  rebase-merges for linear history. The orchestrator itself
  does not push.
- **Brief stays minimal.** `.ai/dispatch.tasks.md` holds only
  pending tasks plus DONE markers for the consumed ones; do
  not seed a new task in the same commit as a retrospective or
  a doctrine update. Mixing them muddles the audit trail.

---

## 18. Delegated-human auto-publish policy

This section makes the bounded conditions under which `-PublishMode main` may
run legible and auditable. It is **policy documentation only**. Nothing in this
section enables, registers, schedules, runs, or defaults `-PublishMode main`,
and nothing here weakens an existing stop condition, finite cap, verification
gate, control gate, or human-review boundary documented elsewhere in this
file. It complements — does not replace — §7.1 (unattended operation), §14
(known gotchas), and §17.4 (current operating policy).

### 18.1 Default mode and the meaning of delegated-human authorization

- **`-PublishMode pr` is the mechanical default** for the queue runner,
  `Invoke-AiDispatchAuto.ps1`, and `Register-AiDispatchSchedule.ps1
  -Autonomous` (ISSUE-239). A passing dispatch pushes its per-issue
  `ai-dispatch/ISSUE-<n>` branch and opens a pull request targeting `main`,
  but never merges the PR, never pushes `origin/main`, and never closes the
  source issue — a human reviewer owns the merge and the issue close.
  `-PublishMode branch` remains available as a fully-local, no-push mode
  for human-gated review without involving the GitHub PR surface.
- **`-PublishMode main` is not a standing authorization, not a recommended
  default, and not a scheduler behavior.** It is allowed only as an
  **explicit, bounded, opt-in human authorization** for a single named batch
  of work. The human reviewer inspects the batch scope and gates first,
  selects `-PublishMode main` for that batch only, and lets the batch run to
  its finite cap; the next batch requires a fresh authorization.
- **A "delegated-human" run** is therefore: one human, one named batch, one
  finite cap, one review of the scope and gates beforehand, one auditable
  trail afterwards. It is not "set and forget," it is not blanket approval
  for all future tasks, and it is not a recurring schedule.
- **Repeated bounded batches are the only meaning of "indefinite" operation.**
  Running the autonomous loop "for a while" means a human authorizes batch 1,
  reviews the result, authorizes batch 2, and so on. It never means removing
  the finite cap or a stop condition.

### 18.2 Allowed publish surfaces

Not every surface is appropriate for auto-publish even under delegated-human
authorization. Surfaces split into two risk tiers:

- **Lower-risk surfaces** (most appropriate for `-PublishMode main` under a
  delegated-human authorization):
  - Documentation-only changes (`*.md` outside `ai_handoffs/` /
    `ai_dispatch_logs/` audit artifacts, ADRs, retrospectives) whose diff is
    fully contained and easily reverted.
  - Test-only additions or regression-coverage changes whose diff is confined
    to test crates / test modules and does not touch production code.

- **Higher-risk surfaces** (default to `-PublishMode branch` and a
  human-owned PR + merge path, even when a delegated-human authorization is
  in effect for the batch):
  - Production Rust source under `crates/**`, `kernel/**`, `editor/**`,
    `tools/**`.
  - Scripts (`*.ps1` at the repo root, including
    `Invoke-AiDispatchLoop.ps1` / `Invoke-AiDispatchQueue.ps1` /
    `Invoke-AiDispatchAuto.ps1` / `Register-AiDispatchSchedule.ps1` /
    `Get-AiDispatchHealth.ps1` / `Watch-AiDispatch.ps1` /
    `Wait-GitHubActions.ps1` / `new-handoff.ps1`).
  - GitHub Actions workflows under `.github/workflows/**`.
  - JSON schemas under `.ai/*.schema.json`.
  - Scheduler configuration (registered Windows Scheduled Tasks, the
    `Register-AiDispatchSchedule.ps1` artifacts).
  - Dependency edges and Cargo metadata: `Cargo.toml`, `Cargo.lock`, every
    `**/Cargo.toml`.
  - The autonomous task brief `.ai/dispatch.tasks.md` and the canonical
    verification gate `.ai/dispatch.verify.ps1`.

A delegated-human authorization SHOULD scope `-PublishMode main` to the
lower-risk surface. A batch that drifts into a higher-risk surface mid-run
SHOULD trip a stop condition (see §18.4) rather than silently auto-publish.

### 18.3 Cap rules

Every cap that bounds a single dispatch or a single autonomous batch remains
finite under delegated-human authorization. None of them may be raised to
"unlimited" or removed in the name of running longer unattended.

- **`-MaxAutonomousTasks`** (on `Invoke-AiDispatchAuto.ps1`) MUST remain a
  finite integer. The auto driver halts the batch after that many issues are
  filed and processed; the next batch requires a fresh human authorization.
- **`-MaxPlanRevisions`** (on `Invoke-AiDispatchLoop.ps1`, 0–5, default `1`)
  MUST remain finite. Once exhausted, the dispatch aborts rather than loop.
- **`-MaxCorrectionRounds`** (on `Invoke-AiDispatchLoop.ps1`, 0–5, default
  `1`; queue/auto layers default `2`) MUST remain finite. Once exhausted, the
  dispatch aborts rather than loop.
- **One issue, one branch, one dispatch per queue tick.** The single-run lock
  in `Invoke-AiDispatchQueue.ps1` is not waived.
- **"Indefinite" autonomous operation** is **only** the pattern: human
  authorizes a finite batch → batch runs to its cap → human reviews →
  human authorizes the next finite batch. The caps themselves stay finite
  forever.

### 18.4 Stop conditions

The autonomous loop MUST halt the current batch — without auto-publishing
the in-flight dispatch and without silently advancing to the next task — when
any of the following occurs. These are minimums; a delegated-human
authorization may add more, never fewer.

- The canonical verification gate `.ai/dispatch.verify.ps1` exits non-zero
  (CI would not pass).
- Codex control returns `needs_changes` beyond `-MaxCorrectionRounds`, or
  returns `block` at any point.
- The Claude plan gate returns `needs_changes` beyond `-MaxPlanRevisions`, or
  returns `block`.
- Scope drift: the diff touches paths the TASK packet listed as MUST NOT
  edit, or expands beyond MAY edit.
- Dirty or unexpected diffs: tracked files outside the dispatch's declared
  surface are modified; unexpected untracked files appear; a `.meta.json`
  sidecar is missing where one is required, or present where the TASK forbids
  it.
- Missing audit artifacts: the queue audit log
  (`ai_dispatch_logs/log_*.md`), the EXEC packet, or the control verdict
  cannot be located for the run.
- Branch / base mismatch: the dispatch is not on the expected
  `ai-dispatch/ISSUE-<n>` branch, or `origin/main` has diverged from the
  expected base, or a fast-forward into `main` is no longer possible.
- Auth or remote failure: `git push`, `gh`, `claude`, or `codex` fails on
  auth, network, or rate limits.
- Any attempted change to scheduler configuration, default
  `-PublishMode`, the canonical verification gate, schemas, workflows, or
  any other higher-risk surface from §18.2 that the batch was not explicitly
  authorized to touch.
- Any attempted edit to a file the TASK packet enumerated as forbidden, or
  to an existing handoff/log artifact the TASK packet did not authorize.

A halt under any of these conditions MUST leave the work on its branch for
human review, MUST NOT fast-forward `main`, and MUST NOT proceed to the next
batch task without a fresh human authorization.

### 18.5 Rollback behavior

Delegated-human authorization does not waive normal rollback discipline.

- **Unpushed local changes** (work that halted before the auto-publish step,
  or work on an `ai-dispatch/ISSUE-<n>` branch that was committed locally but
  not fast-forwarded into `main`):
  - Leave the branch in place for human inspection by default. The work,
    packets, and audit log are the diagnostic record.
  - A human may reset, drop, rename, or salvage the branch as appropriate
    (see §14.8 for salvage label hygiene and §14.12 for positive-salvage of
    work blocked by an over-broad task gate).
  - The autonomous loop MUST NOT silently overwrite, force-reset, or discard
    a halted branch.
- **Already-published `main` updates** (a passing dispatch that fast-forwarded
  into `origin/main` before a downstream problem was discovered):
  - Stop further autonomous batches immediately — revoke the delegated-human
    authorization until the regression is understood.
  - Use **normal git / GitHub revert and audit practice**:
    `git revert <commit>` or a PR-based revert on top of `main`, with a
    descriptive commit message and an issue/PR link explaining the rollback.
    Do not rewrite history on `main`, do not force-push `main`, do not
    silently mutate published commits.
  - Cross-reference the rollback in the original dispatch's issue, the
    autonomous task brief if relevant, and any retrospective.
- In both cases, the rollback path is the same path a human would take by
  hand; the automation has no special privilege to bypass it.

### 18.6 Audit requirements

A delegated-human authorization MUST produce a complete, durable audit trail
for the batch — separate from any chat or terminal scrollback. Each
autonomous tick that processed an issue MUST leave:

- A committed `ai_dispatch_logs/log_*.md` queue audit log on the
  dispatch's branch, including the file change list, generated artifacts,
  Claude marker summary, Codex control JSON, and loop output.
- The dispatch's handoff packets in `ai_handoffs/`: at minimum the TASK
  packet and at least one EXEC packet, plus any CORRECTION packets the
  orchestrator wrote for the run (and the matching `.meta.json` sidecars
  where the packet permits them).
- The Codex control verdict JSON (`verdict`, `commit_readiness`,
  `verification`, `required_fixes`, `changed_files`, `commands_run`) for
  the run, either via the queue log's inlined block or in the run's scratch
  directory under `.ai/dispatch-<DispatchId>/`.
- The verification gate output for the run (the `.ai/dispatch.verify.ps1`
  result that the orchestrator captured).
- The full linkage chain: the GitHub issue number, the
  `ai-dispatch/ISSUE-<n>` branch, the commit SHA(s) the dispatch landed,
  and — if `-PublishMode main` published it — the fast-forward into
  `origin/main`.

In addition, the **human authorization itself MUST be recorded** for each
bounded batch. At minimum:

- Who authorized the batch.
- The batch's named scope (which task IDs / issues, which surfaces).
- The cap (`-MaxAutonomousTasks` value used for that batch).
- The publish mode selected (`pr`, `branch`, or `main`).
- A note that the scope and gates were reviewed beforehand.

This authorization record can live in the issue comments, the dispatch
brief, a commit message, a retrospective, or another committed artifact —
but it MUST be retrievable from `git` / `gh` history after the fact.
Verbal-only or chat-only authorizations are not sufficient for a
delegated-human run.

### 18.7 What this dispatch does and does not do

This dispatch (`ISSUE-208`) is a documentation-only change to
`AI_DISPATCH_AUTOMATION.md`. Explicitly:

- It does **not** enable `-PublishMode main` anywhere.
- It does **not** register, modify, run, or test a Windows Scheduled Task.
- It does **not** edit `Invoke-AiDispatchAuto.ps1`,
  `Invoke-AiDispatchQueue.ps1`, `Invoke-AiDispatchLoop.ps1`,
  `Register-AiDispatchSchedule.ps1`, `Get-AiDispatchHealth.ps1`,
  `Watch-AiDispatch.ps1`, `Wait-GitHubActions.ps1`,
  `.ai/dispatch.verify.ps1`, `.ai/dispatch.tasks.md`, any
  `.ai/*.schema.json`, any GitHub Actions workflow, any Cargo metadata,
  any Rust crate or test, or any existing handoff/log artifact.
- It does **not** change a default `-PublishMode` anywhere, and does
  **not** imply a standing authorization for `-PublishMode main`.
- It does **not** broaden the autonomous queue's behavior, the
  scheduler's behavior, the orchestrator's behavior, or the
  verification gate's behavior.

The policy documented above takes effect only when a human, in a future
session, both reviews the batch scope and gates and explicitly invokes
`Invoke-AiDispatchAuto.ps1` with the corresponding `-PublishMode` /
`-MaxAutonomousTasks` arguments for that batch.

---

*This document describes `Invoke-AiDispatchLoop.ps1`, `Invoke-AiDispatchQueue.ps1`,
and the surrounding packet protocol. The inner loop automates model routing; the
outer queue runner commits successful control-passed work and publishes it per
the selected `-PublishMode` — by default opening a PR for human review, with
explicit `main` mode as the opt-in `origin/main` auto-push path.*
