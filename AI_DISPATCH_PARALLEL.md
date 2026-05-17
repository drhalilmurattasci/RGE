# AI Dispatch — Parallel Fan-Out (Many Dispatches at Once)

Companion to **`AI_DISPATCH_AUTOMATION.md`**. That doc covers running **one**
dispatch. This one answers: *can N unrelated dispatches run concurrently?*

> Short version: yes — but only with **per-dispatch git-worktree isolation** and
> an **outer fan-out runner**. Running the single-dispatch loop 20× against one
> working tree does **not** work. Read §2 before doing anything.
> Synced to `Invoke-AiDispatchLoop.ps1` as of 2026-05-16.

---

## Table of contents

1. Short answer
2. Why the single-dispatch loop is not parallel-safe as-is
3. The fix — one git worktree per dispatch
4. Prerequisite — the dispatch infrastructure must reach each worktree
5. Architecture — the fan-out runner
6. Concurrency — throttle it (do not literally run 20 at once)
7. Worktree lifecycle
8. Specifying the N tasks
9. Reference fan-out runner (PowerShell sketch)
10. Caveats & hard limits
11. Simpler alternative — serial batch
12. Recommendation

---

## 1. Short answer

- **20 dispatches as parallel processes against one working tree → NO.** They
  corrupt each other's files and diffs.
- **20 dispatches, each in its own git worktree, fanned out by an outer runner →
  YES.** That is the supported parallel pattern.
- **"Simultaneously" should mean a small concurrency pool (≈3–6), not literally
  20 at once** — see §6.
- It requires a one-time prerequisite (§4) and a new outer script (§5,§9). The
  per-dispatch loop (`Invoke-AiDispatchLoop.ps1`) itself does **not** change.
- The human gate does **not** scale away: 20 dispatches = 20 review-and-commit
  decisions. Parallelism speeds the *work*, not the *review*.

---

## 2. Why the single-dispatch loop is not parallel-safe as-is

`Invoke-AiDispatchLoop.ps1` is built on the assumption that it **owns the git
working tree** for the life of a dispatch. Run 20 copies in parallel against one
checkout and three things break:

1. **Concurrent source edits collide.** Each dispatch's Claude executor edits
   files in the *same* tree. Two executors touching the same file (a shared
   `Cargo.toml`, a `mod.rs`, etc. — likely even for "unrelated" tasks) produce
   interleaved/lost writes.
2. **Diff cross-contamination.** `Invoke-CodexControl` tells Codex to inspect
   `git status --short --branch` and `git diff`. With 20 dispatches live, each
   dispatch's control review sees **all 20 dispatches' changes** at once — it
   cannot tell which edits are "its own," so its `changed_files`, `verification`
   and `verdict` are meaningless.
3. **Preflight collisions.** The preflight aborts on a dirty *tracked* tree.
   Dispatch #2 starting while dispatch #1 is mid-execution sees #1's in-flight
   edits and fails preflight — or, with `-AllowDirtyTracked`, proceeds and
   inherits the contamination from (2).

**What is already parallel-safe:** packet filenames
(`<DISPATCH_ID>_<TYPE>_<TS>.md` — unique per DispatchId) and per-run scratch
(`.ai/dispatch-<DISPATCH_ID>/`). The **shared source working tree** is the one
resource that must be isolated.

---

## 3. The fix — one git worktree per dispatch

`git worktree` lets one repository have **multiple working directories** at
once. They share the repo's object store and refs, but each has its **own
working directory, its own index, its own HEAD, and its own branch**.

Give every dispatch its own worktree and the §2 problems disappear:

- Each Claude executor edits a **private** working directory — no collisions.
- Each Codex control review runs `git diff` inside that worktree — it sees
  **only its own dispatch's changes**. The review is sound again.
- Each preflight sees a **clean tracked tree** (a fresh worktree off `HEAD`).
- Unrelated tasks touch different files, so the 20 branches merge back with
  little or no conflict — "unrelated" is the *easy* case for this.

Worktrees are far lighter than 20 full clones (shared object store), which makes
them the right isolation primitive here. (20 separate `git clone`s also work and
are even more isolated, but cost 20× the disk and a fetch each.)

---

## 4. Prerequisite — the dispatch infrastructure must reach each worktree

`git worktree add` materializes only **committed** files. The orchestrator and
its dependencies are currently **untracked**, so a fresh worktree would not
contain them — and the orchestrator resolves `new-handoff.ps1`, `.mcp.json`,
`.ai/codex_control.schema.json`, and the handoff protocol files relative to its
worktree root.

Pick one:

- **Option A — commit the infrastructure** (cleanest). Commit
  `Invoke-AiDispatchLoop.ps1`, `new-handoff.ps1`, `.mcp.json`,
  `.ai/codex_control.schema.json`, `ai_handoffs/AI_HANDOFF_PROTOCOL.md`, and
  `ai_handoffs/templates/*`. Every `git worktree add` then includes them
  automatically.
- **Option B — copy the infrastructure into each fresh worktree.** The fan-out
  runner does this after `git worktree add` (see §9). Works without committing,
  but every worktree carries an untracked copy.

Either way, also ensure `.gitignore` ignores `.ai/dispatch-*/`.

---

## 5. Architecture — the fan-out runner

The fan-out runner is a **new outer script** (e.g.
`Invoke-AiDispatchFanout.ps1`) layered **on top of** the unchanged
single-dispatch loop:

```
Invoke-AiDispatchFanout.ps1
  │
  ├─ read N task goals (one goal file per task)            §8
  │
  ├─ for each task:  git worktree add  +  branch  (+copy infra, Option B)   §4,§7
  │
  ├─ fan out — launch one Invoke-AiDispatchLoop.ps1 per worktree,
  │            throttled to MaxConcurrency                  §6
  │     worktree-001 ─ Invoke-AiDispatchLoop.ps1 ─┐
  │     worktree-002 ─ Invoke-AiDispatchLoop.ps1 ─┤  (≤ MaxConcurrency
  │     worktree-003 ─ Invoke-AiDispatchLoop.ps1 ─┤   running at a time)
  │     …                                         │
  │     worktree-020 ─ Invoke-AiDispatchLoop.ps1 ─┘
  │
  ├─ wait for all, collect each run's output / verdict
  │
  └─ STOP. No commit, no merge.  ► human reviews each worktree,
                                   commits/merges per dispatch     §10
```

Each inner run is an ordinary single dispatch — same preflight, same
plan → gate → execute → control flow, same "never commits" rule — it just
happens inside an isolated worktree.

---

## 6. Concurrency — throttle it (do not literally run 20 at once)

Running all 20 truly simultaneously is inadvisable:

- **Model API rate limits.** Each dispatch makes ~5 model calls (Codex plan,
  Claude gate, Claude execute, Codex control, + possible correction). 20 at once
  = ~20 concurrent agentic sessions hammering the Codex and Claude APIs.
- **Cost.** A batch of 20 is ~20× the token cost of one dispatch. There is no
  discount for parallelism.
- **Machine load.** Each run spawns `codex` + `claude` (Node) processes that do
  real agentic work — CPU, RAM, and disk I/O multiply.

Use a **concurrency cap** (`MaxConcurrency`, default ≈4). All 20 tasks still get
processed — in waves — but only a handful run at any instant. Raise the cap only
if your API tier and machine genuinely have the headroom.

---

## 7. Worktree lifecycle

```powershell
# create — one per dispatch, each on its own branch off a synced base
git worktree add ..\dispatch-worktrees\FANOUT-001 -b dispatch/FANOUT-001 HEAD

# run — Invoke-AiDispatchLoop.ps1 executes with its working dir = the worktree
#       (git rev-parse --show-toplevel resolves to the worktree)

# inspect — each worktree holds that dispatch's uncommitted result
git -C ..\dispatch-worktrees\FANOUT-001 status --short

# the human commits / merges per dispatch (the loop never does)

# cleanup — after the result is committed or discarded
git worktree remove ..\dispatch-worktrees\FANOUT-001
git branch -d dispatch/FANOUT-001        # if merged / no longer needed
```

Create worktrees from a commit that is **in sync with `origin/main`** — the
orchestrator's preflight requires `origin/main...HEAD` to be `0 0`.

---

## 8. Specifying the N tasks

One **goal file per task**, all in a directory:

```
dispatch-goals/
  001-cleanup-untracked-artifacts.md
  002-add-frame-graph-bench.md
  003-...
  ...
  020-...
```

Each file's contents is the plain-language goal for that dispatch. The fan-out
runner assigns a `DispatchId` per file (e.g. `FANOUT-001 … FANOUT-020`) and
passes the file to the loop via the existing **`-GoalFile`** parameter. Keep the
tasks genuinely **unrelated** (different files / crates / areas) so the branches
merge back cleanly.

---

## 9. Reference fan-out runner (PowerShell 5.1 sketch)

A starting point — not a hardened tool. It has no re-run handling and no
per-task retry; treat those as TODOs.

```powershell
#Requires -Version 5.1
<#  Invoke-AiDispatchFanout.ps1 — fan a batch of unrelated dispatches out
    across isolated git worktrees, throttled to MaxConcurrency.            #>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$GoalsDir,      # dir of goal files, one per task
    [string]$DispatchPrefix = 'FANOUT',
    [ValidateRange(1, 12)][int]$MaxConcurrency = 4,
    [string]$WorktreeRoot = '',
    [string]$BaseRef = 'HEAD',
    [switch]$PlanOnly
)
$ErrorActionPreference = 'Stop'

$repoRoot = (& git rev-parse --show-toplevel).Trim()
if (-not $WorktreeRoot) {
    $WorktreeRoot = Join-Path (Split-Path $repoRoot -Parent) 'dispatch-worktrees'
}

# Infra to copy into each worktree IF it is not committed (Option B, §4).
# If you committed the infra (Option A), this list can be emptied.
$infra = @(
    'Invoke-AiDispatchLoop.ps1', 'new-handoff.ps1', '.mcp.json',
    '.ai/codex_control.schema.json', 'ai_handoffs/AI_HANDOFF_PROTOCOL.md'
)

$goalFiles = @(Get-ChildItem -LiteralPath $GoalsDir -File | Sort-Object Name)
$tasks = for ($i = 0; $i -lt $goalFiles.Count; $i++) {
    $id = '{0}-{1:D3}' -f $DispatchPrefix, ($i + 1)
    [pscustomobject]@{
        Id = $id; GoalFile = $goalFiles[$i].FullName
        Worktree = Join-Path $WorktreeRoot $id; Branch = "dispatch/$id"
    }
}
Write-Output "Fanning out $($tasks.Count) dispatches, $MaxConcurrency at a time."

# 1. one isolated worktree per task
foreach ($t in $tasks) {
    & git worktree add $t.Worktree -b $t.Branch $BaseRef
    if ($LASTEXITCODE -ne 0) { throw "git worktree add failed for $($t.Id)" }
    foreach ($rel in $infra) {                      # Option B: copy infra if missing
        $src = Join-Path $repoRoot $rel
        $dst = Join-Path $t.Worktree $rel
        if ((Test-Path -LiteralPath $src) -and -not (Test-Path -LiteralPath $dst)) {
            New-Item -ItemType Directory -Force -Path (Split-Path $dst) | Out-Null
            Copy-Item -LiteralPath $src -Destination $dst
        }
    }
}

# 2. fan out, throttled. A fresh worktree has a clean tracked tree, so the
#    inner loop does NOT need -AllowDirtyTracked.
$jobs = @()
foreach ($t in $tasks) {
    while (@(Get-Job -State Running).Count -ge $MaxConcurrency) { Start-Sleep -Seconds 5 }
    Write-Output "launch $($t.Id)"
    $jobs += Start-Job -Name $t.Id -ScriptBlock {
        param($Worktree, $Id, $GoalFile, $PlanOnly)
        Set-Location $Worktree
        $a = @('-DispatchId', $Id, '-GoalFile', $GoalFile)
        if ($PlanOnly) { $a += '-PlanOnly' }
        & (Join-Path $Worktree 'Invoke-AiDispatchLoop.ps1') @a *>&1
    } -ArgumentList $t.Worktree, $t.Id, $t.GoalFile, [bool]$PlanOnly
}

# 3. wait and collect
Wait-Job -Job $jobs | Out-Null
foreach ($j in $jobs) {
    Write-Output "==================== $($j.Name) : $($j.State) ===================="
    Receive-Job -Job $j
}
Remove-Job -Job $jobs

Write-Output ""
Write-Output "All dispatches finished. Review each worktree under $WorktreeRoot,"
Write-Output "then commit/merge per dispatch. No commit or push was performed."
```

Run it from a real terminal (the batch far exceeds any 10-minute command cap).
PowerShell 7+ users can replace the `Start-Job` pool with
`ForEach-Object -Parallel -ThrottleLimit`.

---

## 10. Caveats & hard limits

- **Cost & rate limits** — ~20× the model usage of one dispatch, much of it
  concurrent. Expect rate-limit throttling; size `MaxConcurrency` to your tier.
- **Disk** — 20 worktrees = 20 working directories (object store is shared, but
  the checked-out trees are not). For a large repo this is real space.
- **The human gate does not scale away** — 20 dispatches end as 20 worktrees,
  each with uncommitted work and a Codex `commit_readiness` verdict. That is 20
  human review-and-commit decisions. Parallelism shortens wall-clock for the
  *work*, not the *review*.
- **Integration** — 20 branches must land on `main`. Unrelated tasks → minimal
  conflict; tasks that share files **will** conflict on merge. Keep the batch
  genuinely unrelated (§8).
- **Failure isolation (a plus)** — each dispatch is its own worktree and job, so
  one dispatch failing (Claude `block`, Codex `block`, an error) does **not**
  stop the others. Collect partials; re-run only the failed ids.
- **Recurring-runner locks** — `Invoke-AiDispatchQueue.ps1` and
  `Invoke-AiDispatchAuto.ps1` each hold a single-run lock, so a scheduled tick
  and a manual run that overlap serialize instead of colliding on the shared
  working tree.
- **Shared CLI auth is fine** — one `codex` / `claude` login serves all the
  parallel runs.
- **`origin/main` sync** — every worktree's preflight checks it; create the
  worktrees from a synced base (§7).

---

## 11. Simpler alternative — serial batch

If wall-clock time is not critical, **do not** build the worktree machinery.
Run the N dispatches **one after another on the single working tree**, with a
human commit between each:

```
for each goal:  Invoke-AiDispatchLoop.ps1 -DispatchId ... -GoalFile ...
                → human reviews → human commits → next
```

This needs no worktrees, no fan-out runner, and no merge step — each dispatch
starts from a clean, committed tree. It is slower but far simpler and lower
risk, and it matches the existing `MAIN-ORDERED-*` serial-queue precedent in
`ai_handoffs/`. Parallel fan-out is only worth its complexity when wall-clock
time genuinely matters and you have the API headroom.

---

## 12. Recommendation

| You want… | Do this |
|---|---|
| 20 unrelated tasks, wall-clock matters, API headroom exists | Worktree fan-out (§5,§9), `MaxConcurrency ≈ 4`, infra committed (§4 Option A). |
| 20 tasks, wall-clock not critical | Serial batch (§11) — simpler, safer. |
| Tasks that share files | Serial — parallel branches would just conflict on merge. |

For a true 20-wide parallel run: commit the infrastructure, write
`Invoke-AiDispatchFanout.ps1` from the §9 sketch, stage 20 goal files, and run
the fan-out from a terminal with `MaxConcurrency` tuned to your rate limit. Then
budget for 20 separate human commit decisions at the end.

---

*Companion to `AI_DISPATCH_AUTOMATION.md`. The per-dispatch loop is unchanged;
parallelism is achieved purely by isolation (worktrees) + an outer runner. The
no-commit human gate still applies — once per dispatch.*
