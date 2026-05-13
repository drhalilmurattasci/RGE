# AI Handoff Protocol

A lightweight, append-only governance protocol for AI-to-AI dispatch exchanges
in the RGE repository.

## Purpose

Multiple AI agents (Planner, Executor, Reviewer) cooperate on RGE work under
human arbitration. This protocol makes their exchanges visible, auditable, and
recoverable without any runtime tooling. It is a Markdown-based record-keeping
convention — NOT runtime automation, NOT a build-system hook, NOT a replacement
for ADRs / `Status.md` / `HANDOFF.md` / `change.md`.

Use this protocol when more than one AI agent is collaborating on a dispatch
that warrants peer review (commit-level work, multi-step chapters, design
decisions). For one-shot trivial tasks the existing repo-level docs are enough.

## Roles

### Planner AI
- Decomposes the user's intent into bounded dispatches.
- Issues `TASK_PACKET` files defining scope, acceptance criteria, halt
  conditions, and verification gates.
- Approves `CORRECTION_PACKET` content before any correction round runs.
- Issues `FINAL_CLOSEOUT` when the dispatch lands or is abandoned.
- Owns scope. The Executor never expands scope without a fresh Planner packet.

### Executor AI
- Reads the `TASK_PACKET`.
- Executes strictly within the stated `MAY edit` / `MUST NOT edit` envelope.
- Writes an `EXECUTION_REPORT` describing what shipped and verification
  results.
- Flags any uncertainty as `Open Questions for Reviewer`, never as silent
  scope expansion.

### Reviewer AI
- Reads the `TASK_PACKET` + `EXECUTION_REPORT`.
- Independently re-runs verification gates where feasible.
- Reads the actual changes (not just claims).
- Writes a `REVIEW_REPORT` with verdict
  (`APPROVED` / `NEEDS_CORRECTION` / `REJECTED`) and concrete findings.
- Recommends; does not directly order corrections.

### Human Arbiter
- The user is the final authority.
- Resolves disagreements among Planner / Executor / Reviewer.
- May override any protocol rule.
- Closes ambiguous scope decisions explicitly.

## Dispatch Lifecycle

1. **Planner → `TASK_PACKET`.** Scope, deliverables, acceptance criteria,
   verification gates, halt conditions.
2. **Executor → `EXECUTION_REPORT`.** What changed, per-file summary,
   verification results, deviations, open questions.
3. **Reviewer → `REVIEW_REPORT`.** Independently verified gates, findings
   (correct / needs-correction / latent-risks), test-coverage assessment,
   doc-accuracy check, recommended action.
4. **If `APPROVED`:** Planner writes `FINAL_CLOSEOUT`. Dispatch is `CLOSED`.
5. **If `NEEDS_CORRECTION`:** Planner writes a `CORRECTION_PACKET` enumerating
   the approved subset of review findings. Executor writes a new
   `EXECUTION_REPORT`. Reviewer writes a new `REVIEW_REPORT`. Loop until
   `APPROVED` → `CLOSEOUT`.
6. **If `REJECTED`:** Planner writes `FINAL_CLOSEOUT` with `STATUS: ABANDONED`
   and a written reason.

## Correction Loop

Corrections require Planner approval. Reviewer findings are recommendations,
not direct execution orders. The Planner:

- Decides which review findings to act on this round.
- Decides which findings to defer (record as latent risks).
- Writes the `CORRECTION_PACKET` with the approved subset enumerated.
- The Executor acts ONLY on the corrections enumerated in that packet — not
  on the raw `REVIEW_REPORT`.

This protects against reviewer-loop runaway, prevents scope creep, and keeps
the Planner accountable for what ships.

## File Naming Convention

```
ai_handoffs/<DISPATCH_ID>_<PACKET_TYPE>_<TIMESTAMP>.md
```

Where:

- `<DISPATCH_ID>`: a stable identifier for the dispatch, chosen by the
  Planner. Recommended forms:
  - Date-letter: `2026-05-13-A`, `2026-05-13-B` (multiple dispatches per day).
  - Chapter-suffix: `phase7-fillet-sub-eta`,
    `phase8-loft-curvature`.
- `<PACKET_TYPE>`: one of `TASK`, `EXEC`, `REVIEW`, `CORRECT`, `CLOSEOUT`.
- `<TIMESTAMP>`: ISO-8601 local form `YYYY-MM-DD_HH-MM-SS+TZTZ`
  (e.g. `2026-05-13_13-00-00+0300`).

### Examples

A clean single-round dispatch:

```
2026-05-13-A_TASK_2026-05-13_13-00-00+0300.md
2026-05-13-A_EXEC_2026-05-13_14-30-00+0300.md
2026-05-13-A_REVIEW_2026-05-13_14-45-00+0300.md
2026-05-13-A_CLOSEOUT_2026-05-13_15-00-00+0300.md
```

A dispatch with one correction round:

```
2026-05-13-B_TASK_2026-05-13_13-00-00+0300.md
2026-05-13-B_EXEC_2026-05-13_14-30-00+0300.md
2026-05-13-B_REVIEW_2026-05-13_14-45-00+0300.md
2026-05-13-B_CORRECT_2026-05-13_15-00-00+0300.md
2026-05-13-B_EXEC_2026-05-13_16-00-00+0300.md
2026-05-13-B_REVIEW_2026-05-13_16-15-00+0300.md
2026-05-13-B_CLOSEOUT_2026-05-13_16-30-00+0300.md
```

Multiple correction rounds are allowed; each `CORRECT` packet gets its own
unique timestamp, and the `EXEC` / `REVIEW` packets follow naturally.

## Required Markdown Sections

Every packet — regardless of type — MUST contain at minimum:

- `DISPATCH_ID`
- `AUTHOR` (role + AI identity, e.g. `Planner / Claude`,
  `Executor / Codex`, `Reviewer / Claude`)
- `TIMESTAMP`
- `RELATED_FILES`
- `STATUS`

Plus packet-type-specific structured sections — see the templates in
`ai_handoffs/templates/`.

## Rules

### 1. Append-only

Each packet is a NEW file. Packets are never modified or replaced. The
dispatch's audit trail is the ordered concatenation of its packets.

### 2. Prior files must not be rewritten

Once a packet is written, it is immutable. Errors are corrected by writing a
NEW packet (e.g. a `CORRECTION_PACKET` or a follow-up `REVIEW_REPORT`), never
by editing the original. This preserves the chain-of-custody for every
decision.

### 3. No scope expansion without explicit approval

The Executor MUST stay within the `TASK_PACKET` `MAY edit` / `MUST NOT edit`
envelope. If new work is required, the Executor writes an `EXECUTION_REPORT`
noting the gap in `Deviations from Task Packet` or `Open Questions for
Reviewer`, and the Planner issues a new `TASK_PACKET` or
`CORRECTION_PACKET`. Silent scope expansion violates the protocol.

### 4. Correction packets require Planner approval

Reviewer findings flow through the Planner. The Reviewer recommends; the
Planner decides. `CORRECTION_PACKET` files explicitly enumerate which review
findings are approved for execution and which are deferred as latent risks.
Without an approved `CORRECTION_PACKET`, no correction is performed.

### 5. Final closeout must include tests/gates and remaining risks

`FINAL_CLOSEOUT` is the only packet that closes a dispatch. It MUST include:

- All verification gates run and their results.
- Test-count delta (workspace + per-crate).
- Final commit hash(es).
- Remaining risks, explicitly enumerated (or `none known` if truly none).
- Suggested follow-on tasks (or `none` if truly none).

A `FINAL_CLOSEOUT` that omits any of these is invalid and the dispatch is
not considered closed.

## Repository Layout

```
ai_handoffs/
  AI_HANDOFF_PROTOCOL.md   # this file
  templates/
    TASK_PACKET.md
    EXECUTION_REPORT.md
    REVIEW_REPORT.md
    CORRECTION_PACKET.md
    FINAL_CLOSEOUT.md
  <DISPATCH_ID>_TASK_<TIMESTAMP>.md
  <DISPATCH_ID>_EXEC_<TIMESTAMP>.md
  ...
```

Handoff packet files are typically left untracked (consistent with the
existing precedent at the repo root) unless the Planner explicitly stages
them for audit-trail commits. Either choice is acceptable; the protocol
does not mandate one.

## Relationship to Existing Precedent

This protocol formalizes the practice established by these files at the
repository root (created 2026-05-13):

- `OPENAItoCLAUDE_2026-05-13_12-15-18+0300.md` — Codex → Claude review packet
  after the sub-epsilon hardening commit.
- `CLAUDEtoOPENAI_2026-05-13_12-22-26+0300.md` — Claude → Codex review-of-
  review.
- `CLAUDE_SUB_EPSILON_REVIEW.md` — Codex-prepared review packet for
  Claude's sub-epsilon review.

Those files remain valid historical records and are NOT migrated into
`ai_handoffs/`. Going forward, new dispatches use the structured packet
format in `ai_handoffs/`.

## Non-Goals

This is a GOVERNANCE protocol. It is NOT:

- CI integration.
- Automated packet routing or scheduling.
- A machine-readable dispatch state-machine.
- A replacement for ADRs, `Status.md`, `HANDOFF.md`, or `change.md`.
- Enforcement beyond Reviewer + Planner discipline + human arbitration.

The protocol exists to make multi-AI dispatch auditable and recoverable.
It does not replace any existing governance surface and it does not encode
any architectural doctrine beyond the rules above.
