# RGE — Agent Guide

Shared orientation for any AI agent (Codex, Claude Code, or other) working in
this repository. Codex auto-loads `AGENTS.md`; Claude Code auto-loads
`CLAUDE.md`, which imports this file. Keep this file short — it loads into
every session.

## Before you reason about dispatch state

This repo runs an AI dispatch automation. Before judging packets, dispatch
status, or "is dispatch X finished," read **`AI_DISPATCH_AUTOMATION.md`** (full
spec) and **`ai_handoffs/AI_HANDOFF_PROTOCOL.md`** (packet protocol). Do not
infer the design by guessing from the `ai_handoffs/` file listing — that is the
single most common mistake.

## Orchestrator dispatch vs. manual protocol

`Invoke-AiDispatchLoop.ps1` is the orchestrator: a Codex-plans /
Claude-executes / Codex-controls loop. One run = one bounded task. It never
commits or pushes — for a standalone loop run a human authorizes the git
publish, while the `Invoke-AiDispatchQueue.ps1` queue runner auto-publishes
control-passed runs (see `AI_DISPATCH_AUTOMATION.md`).

Two packet shapes exist; do not confuse them:

- **Orchestrator dispatch** (`Invoke-AiDispatchLoop.ps1`) writes only a `TASK`
  and an `EXEC` packet to `ai_handoffs/` (plus a `CORRECT` packet only if a
  correction round runs). The Codex control review goes to a gitignored
  run-dir JSON — `.ai/dispatch-<DispatchId>/codex.control.round0.json` — never a
  packet. It emits **no `_REVIEW_` and no `_CLOSEOUT_` packets, ever.**
- **Manual `ai_handoffs/` protocol** is the explicit chain
  `TASK -> EXEC -> REVIEW -> CORRECT -> CLOSEOUT`.

**Consequence:** for an orchestrator dispatch, missing `_REVIEW_` / `_CLOSEOUT_`
packets is normal and expected — it is **not** evidence of an unfinished
dispatch. A dispatch is finished when the orchestrator reports `Dispatch loop
finished` with a `pass` control verdict and its work is committed. Completion is
a committed change, not a packet.

## Pointers — consult these, don't reconstruct them from the tree

- `AI_DISPATCH_AUTOMATION.md` — orchestrator spec, run modes, and the
  CLI/runtime gotchas (PowerShell 5.1 stderr trap, `claude` auth, why
  `--json-schema` is avoided, the ~10-minute command ceiling).
- `AI_DISPATCH_PARALLEL.md` — running many dispatches concurrently.
- `HANDOFF.md` / `Status.md` / `change.md` — engine state, next-job options,
  chronological history.
- `OLD/` and `.ai/dispatch-*/` are gitignored local scratch — ignore them when
  assessing repo state.
