# AI Dispatch Automation — Codex-Plans / Claude-Executes / Codex-Controls

A reusable guide to `Invoke-AiDispatchLoop.ps1`: a PowerShell orchestrator that
drives a two-model dispatch loop (OpenAI **Codex** + Anthropic **Claude**) over a
Markdown "handoff packet" protocol, with a mandatory **human gate** before any
commit.

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
11. Structured-output schemas
12. `.mcp.json`
13. The packet protocol
14. Known gotchas & fixes (read this before porting)
15. Porting to another project
16. Operational notes

---

## 1. What this is

`Invoke-AiDispatchLoop.ps1` automates **one bounded task ("dispatch")** end to end:

1. **Codex** writes a precise TASK specification from a one-line goal.
2. **Claude** reviews that spec as a preflight gate; if approved, **Claude** executes it.
3. **Codex** does a read-only control review of the result.
4. If Codex asks for changes, it writes a CORRECTION packet and Claude re-executes.

It is a thin orchestration layer on top of an existing Markdown packet protocol
(`ai_handoffs/`). It **automates model routing only**. It never stages, commits,
or pushes — a human authorizes every git publish step.

**One run = one task.** It is *not* an autonomous "build the whole project"
agent. It plans → gates → executes → control-reviews (with capped retries),
prints `Dispatch loop finished.`, and exits. To advance a project you run it
repeatedly — one scoped task per run, with a human commit between each.

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
- **No commit, no push.** The loop only edits the working tree. A human
  authorizes publishing.
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
| `new-handoff.ps1` | Packet scaffold/finalize tool. Scaffolds a `.md` packet; on `-Finalize` parses a completed packet and writes its `.meta.json` sidecar. | **Copy** |
| `.mcp.json` | MCP server config passed to `claude`. | **Copy/adapt** |
| `.ai/claude_plan_gate.schema.json` | Schema for Claude's gate verdict. | **Copy** |
| `.ai/claude_execution_result.schema.json` | Schema for Claude's execution result. | **Copy** |
| `.ai/codex_control.schema.json` | Schema for Codex's control review. | **Copy** |
| `ai_handoffs/AI_HANDOFF_PROTOCOL.md` | The packet protocol spec. | **Copy/adapt** |
| `ai_handoffs/templates/*.md` | Packet templates (TASK_PACKET, EXECUTION_REPORT, REVIEW_REPORT, CORRECTION_PACKET, FINAL_CLOSEOUT). | **Copy** |
| `ai_handoffs/<ID>_<TYPE>_<TS>.md` (+ `.meta.json`) | Generated packets (per dispatch). | generated |
| `.ai/dispatch-<ID>/` | Per-run scratch: prompt files, raw model logs, model JSON. | generated — **git-ignore** |

The orchestrator hard-requires (preflight aborts if missing): `new-handoff.ps1`,
`.mcp.json`, the three `.ai/*.schema.json` files, and
`ai_handoffs/AI_HANDOFF_PROTOCOL.md`.

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

---

## 7. Modes

| Mode | Invocation | Behavior |
|---|---|---|
| New dispatch | `-DispatchId X -Goal "..."` (or `-GoalFile path`) | Full flow: plan → gate → execute → control. |
| Plan only | add `-PlanOnly` | Stops after the TASK is approved and finalized. No execution. |
| Resume approved TASK | `-DispatchId X -ResumeApprovedTask` | Skips scaffold/plan/gate/finalize. Locates the existing finalized TASK for `X` (must have a `.meta.json` sidecar = proof it was approved) and runs only the execution + control phase. **No new TASK is created.** Mutually exclusive with `-PlanOnly`. |

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

**Claude** (`Invoke-ClaudeJson`) — prompt is passed as a **trailing positional
argument** (never via stdin — see §14.2):

```powershell
claude -p --mcp-config <.mcp.json> --permission-mode <plan|acceptEdits|...> `
  --output-format json [--model <m>] "<wrapped-prompt>"
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
- Footer must be:
  HANDOFF_STATUS: COMPLETE
  NEXT_ROLE: EXECUTOR_AI
  EXIT_CODE: 0
- The packet must pass new-handoff.ps1 -Finalize -DryRun.
```

Placeholders: `$taskRel` = repo-relative path of the scaffolded TASK packet;
`$script:GoalText` = the `-Goal`/`-GoalFile` text; `$RevisionNumber` = 0-based
revision index; `$gateContext` = the prior Claude gate JSON, or
`No prior Claude gate.` on revision 0.

### 10.2 Claude — plan gate (permission mode: `plan`)

```text
You are Claude acting as Executor preflight gate for RGE.

Review the TASK_PACKET:

$taskRel

You must not edit files. Read the packet, inspect only the repo context needed
to decide whether the plan is executable, bounded, and protocol-safe.

Return structured JSON only. Use:
- verdict=approve if the task is safe to execute as written.
- verdict=needs_changes if Codex should revise the TASK packet first.
- verdict=block if execution should not proceed without human arbitration.

Include commands_run with any commands you actually ran.
```

This text is then wrapped — see §10.6.

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
- If the active packet forbids sidecar `.meta.json` creation, do not finalize
  the EXEC packet; mention that deliberate skip in the returned JSON notes.
- Return structured JSON only, including exec_packet as the repo-relative
  path to the EXECUTION_REPORT if one was written.
```

Placeholders: `$PacketKind` = `TASK` or `CORRECTION`; `$packetRel` = repo-relative
path of the active packet; `$DispatchId` = the dispatch id. Wrapped per §10.6.

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
- verdict=pass only if the work is ready for human commit authorization.
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

### 10.6 Claude JSON wrapper (applied to every Claude prompt)

Because `claude --json-schema` is unusable here (§14.3), the schema is appended
to the prompt instead and a strict output-contract preamble is prepended. Every
Claude prompt above is sent wrapped as:

```text
CRITICAL OUTPUT CONTRACT:
- Your final terminal response must be exactly one JSON object.
- Do not return prose, Markdown, a summary, or a table outside that JSON object.
- If you need to perform repo work first, do the work, then make the final
  response only the JSON object.
- The JSON object must match the schema below.

<one of the Claude prompts from §10.2 / §10.3>

Return exactly one JSON object matching this schema. Do not wrap it in Markdown.
Do not include explanatory text outside the JSON object.

Schema:
<contents of the matching .ai/*.schema.json file>
```

---

## 11. Structured-output schemas

**Asymmetry — important.** Codex returns structured JSON *natively* via
`--output-schema <file> --output-last-message <out>`; the orchestrator reads
that `<out>` file directly. Claude's equivalent flag (`--json-schema`) **hangs**
in this environment (§14.3), so Claude gets the schema embedded in its prompt
(§10.6), the orchestrator reads the `result` field of Claude's
`--output-format json` envelope, strips any Markdown code fences, parses it, and
validates it against the schema **locally** (the `Test-JsonSchemaSubset` /
`Convert-ClaudeResultJson` functions). Do **not** "fix" this by re-adding
`--json-schema`.

### 11.1 `.ai/claude_plan_gate.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Claude plan gate",
  "type": "object",
  "additionalProperties": false,
  "required": ["verdict", "summary", "blocking_reasons", "recommended_changes", "commands_run"],
  "properties": {
    "verdict": { "type": "string", "enum": ["approve", "needs_changes", "block"] },
    "summary": { "type": "string" },
    "blocking_reasons": { "type": "array", "items": { "type": "string" } },
    "recommended_changes": { "type": "array", "items": { "type": "string" } },
    "commands_run": { "type": "array", "items": { "type": "string" } }
  }
}
```

### 11.2 `.ai/claude_execution_result.schema.json`

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Claude execution result",
  "type": "object",
  "additionalProperties": false,
  "required": ["status", "summary", "exec_packet", "changed_files", "verification", "final_git_status", "notes"],
  "properties": {
    "status": { "type": "string", "enum": ["executed", "blocked", "failed"] },
    "summary": { "type": "string" },
    "exec_packet": { "type": ["string", "null"] },
    "changed_files": { "type": "array", "items": { "type": "string" } },
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
    "final_git_status": { "type": "string" },
    "notes": { "type": "array", "items": { "type": "string" } }
  }
}
```

### 11.3 `.ai/codex_control.schema.json`

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
    "commit_readiness": { "type": "string", "enum": ["ready_for_human_commit", "not_ready", "no_commit_needed"] },
    "commands_run": { "type": "array", "items": { "type": "string" } }
  }
}
```

The local validator (`Test-JsonSchemaSubset`) supports a *subset* of JSON Schema:
`type`, `enum`, `required`, `additionalProperties: false`, `properties`, and
`items`. Keep schemas within that subset.

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
`Invoke-ClaudeJson`, and `Test-ClaudeCliReady`.

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
**Fix:** do **not** use `--json-schema`. Use plain `--output-format json`, embed
the schema in the prompt (§10.6), read the `result` field of the JSON envelope,
strip Markdown fences, and validate against the schema locally in PowerShell.

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
Only Claude's `--json-schema` is broken (§14.3). Do not assume symmetry.

### 14.7 The loop never commits

By design there is no `git commit`/`git push` anywhere. Every run ends with
work uncommitted in the tree. A human reviews and commits. If you build an outer
multi-dispatch runner, keep this gate or uncommitted work will pile up.

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
   the script (EAP isolation; positional Claude prompt; no `--json-schema`;
   `Test-ClaudeCliReady`).
8. **Rehearse:** run a `-PlanOnly` dispatch with a small audit-only goal first.
   Verify it reaches `Claude plan gate rev 0: approve` and `TASK finalized`
   before trusting a full run.
9. Keep schemas within the validator's supported subset (§11).

---

## 16. Operational notes

- **Per-run scratch** lands in `.ai/dispatch-<DISPATCH_ID>/`: `codex.prompt.md`,
  `codex.*.log`, `claude.*.envelope.json`, `claude.*.stderr.txt`,
  `claude.*.json` (validated result), `claude.ready.*`, finalize dry-run logs.
  This is where to look when a run fails.
- **Generated packets** land in `ai_handoffs/` as `<ID>_<TYPE>_<TS>.md`. TASK
  and CORRECTION packets are finalized (a `.meta.json` sidecar is written). The
  EXEC packet is auto-finalized too — *unless* the active packet's text forbids
  sidecar creation (detected by `Test-PacketForbidsSidecar`), in which case
  finalization is deliberately skipped and the run prints
  `EXEC sidecar finalization skipped; ...`.
- **What to inspect after a run:** the TASK packet; the EXEC packet; the Codex
  control verdict + `commit_readiness`; for `-PlanOnly`, the gate JSON at
  `.ai/dispatch-<ID>/claude.plan_gate.rev0.json`.
- **Failure is loud:** every abort prints a `Fail` message naming the log to
  read, and exits non-zero.
- **A failed/aborted run leaves debris** (an unfinalized TASK packet, a run
  dir). Re-running with a fresh `-DispatchId` is the clean recovery; old debris
  is harmless and can be cleaned up later.
- **Resuming:** once a dispatch has an approved + finalized TASK, re-run with
  `-ResumeApprovedTask` to (re-)run only the execution phase without
  re-planning.

---

*This document describes `Invoke-AiDispatchLoop.ps1` and its surrounding packet
protocol. The orchestrator automates model routing only — it never commits or
pushes; a human authorizes every publish.*
