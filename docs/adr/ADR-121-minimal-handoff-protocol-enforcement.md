# ADR-121: Minimal AI handoff protocol enforcement

| Status | Accepted for advisory implementation 2026-06-06 |
|---|---|
| Date | 2026-06-06 |
| Deciders | Human arbiter, Planner |
| Related docs | `ai_handoffs/AI_HANDOFF_PROTOCOL.md`, `AI_DISPATCH_AUTOMATION.md`, `AI_DISPATCH_PARALLEL.md` |
| Amends | `ai_handoffs/AI_HANDOFF_PROTOCOL.md` Non-Goals for advisory tooling only |

## Context

The AI handoff protocol is intentionally lightweight. It is an append-only
Markdown record-keeping convention for Planner, Executor, Reviewer, and Human
Arbiter collaboration. Completion is detectable with a line-anchored footer
grep, or with a `.meta.json` sidecar when present.

The protocol's current Non-Goals say it is not CI integration, not automated
routing, not a machine-readable dispatch state machine, and not enforcement
beyond Reviewer and Planner discipline plus human arbitration. That posture has
kept the protocol portable across Codex, Claude Code, shell scripts, and CI.

Two gaps now have enough operational pressure to deserve a narrow advisory
exception:

1. **Unenforced mandatory invariants.** Rules already require an EOF-anchored
   completion footer, executor adherence to the `MAY edit` / `MUST NOT edit`
   envelope, and closeout records with gates, test deltas, commits, risks, and
   follow-ons. Today, these remain discipline-only checks.
2. **No dispatch claim primitive.** Parallel dispatch and autonomous queue paths
   can both target the same dispatch id or branch. Append-only packets preserve
   the audit trail after the fact, but they do not prevent duplicate execution
   from starting.

This ADR is accepted for advisory implementation only. It permits standalone
tooling and tests, but it does not wire verification, change packet templates,
or alter dispatch behavior.

## Decision

Add two minimal mechanisms that preserve the protocol's grep-and-JSON altitude:

1. An advisory-first packet/scope validator for invariants the protocol already
   mandates.
2. A claim/lock convention that prevents duplicate execution while preserving
   an append-only audit trail.

No live verification gate may become blocking until a later implementation
dispatch proves the checks on real historical samples and records the false
positive posture.

### D1. Accepted advisory slice before integration

This ADR deliberately separates advisory tooling from live integration. The
safe sequence is:

1. Land this ADR as `Proposed`.
2. Accept it for advisory-only validator tooling.
3. Smoke the validator across hand-authored envelopes for representative
   historical dispatches.
4. Only then consider protocol/template changes.
5. Only after another explicit decision consider blocking verification.

Acceptance of this ADR was not permission to make blocking verification,
template, queue, schema, or scheduler changes. `Test-HandoffPacket.ps1` landed
first as standalone tooling. A later advisory-only slice wires it into
`.ai/dispatch.verify.ps1` after the historical smoke suite exists; that hook is
non-counted, non-blocking, and reports `SKIP`/`WARN` without changing the gate's
exit code.

### D2. Thin validator scope

A future validator may check only obligations that already exist in
`ai_handoffs/AI_HANDOFF_PROTOCOL.md`:

- Footer present at EOF, with required keys and valid enum/integer fields.
- Footer and `.meta.json` sidecar consistency when a sidecar exists.
- `CLOSEOUT` packet includes Rule-5 closeout evidence: gates and results,
  test-count delta, commit hash or hashes, remaining risks, and follow-ons.
- `EXEC` touched-file set stays inside the `TASK` envelope, except explicitly
  permitted protocol artifacts and incidental files.
- Rule-8 absence/zero/unchanged claims carry falsifying searches. This starts
  advisory-only even after the rest of the validator is trusted.

The validator must emit both a grep-able one-line result and structured JSON,
for example:

```text
HANDOFF_VALIDATE: PASS
```

or:

```text
SCOPE_VERDICT: WARN
```

The JSON result is for scripts; the one-line result is for humans and simple
watchers.

### D3. Machine-readable scope envelope

Future TASK packets may include this marker-delimited block:

```markdown
<!-- handoff:envelope v1 -->
MAY_EDIT:
  - crates/rge-foo/src/**
  - crates/rge-foo/tests/**
MUST_NOT_EDIT:
  - crates/rge-foo/src/generated/**
INCIDENTAL_OK: true
<!-- /handoff:envelope -->
```

The Markdown prose remains readable, but the marker gives simple tools a stable
parse target. A packet without the block is `UNCHECKED`, never `FAIL`, for
scope validation.

The glob dialect is gitignore-like and intentionally narrow:

- `**` means any depth.
- `*` means within one path segment.
- trailing `/**` means the whole subtree.
- no brace expansion.
- no implicit path rewriting beyond normalizing `\` to `/`.

Authors must list path alternatives explicitly instead of relying on brace
syntax such as `crates/{a,b}/**`.

Initial optional template guidance now lives in
`ai_handoffs/templates/TASK_PACKET.md`, and the `Invoke-AiDispatchLoop.ps1`
Planner prompt instructs Codex to mirror the human-readable `### MAY edit` /
`### MAY add new files` and `### MUST NOT edit` / `### MUST NOT add new files`
sections into the envelope when the scope can be represented safely. The
human-readable scope remains authoritative for the queue scope guard; the
envelope is advisory validator input.

### D4. Scope matching semantics

The validator computes the whole dispatch diff, not just the latest commit:

```text
base = git merge-base <integration-ref> HEAD
touched = git diff --name-only base..HEAD
        + git diff --name-only HEAD
        + git ls-files --others --exclude-standard
```

For each touched file:

- `MUST_NOT_EDIT` wins over `MAY_EDIT`.
- Empty `MAY_EDIT` plus non-empty `MUST_NOT_EDIT` means denylist mode:
  everything is allowed except denied paths.
- Empty `MAY_EDIT` and empty `MUST_NOT_EDIT` means `UNCHECKED`, not deny-all.
- `ai_handoffs/**` is exempt because protocol packets are expected dispatch
  artifacts.
- If `INCIDENTAL_OK: true`, `Cargo.lock` and `**/*.meta.json` are exempt.
- Incidental exemptions are path-based only. No content-diff heuristic tries to
  infer "formatter-only" output.

The result is:

- `PASS` when no violations exist.
- `WARN` when violations exist but an authorized Planner override exists.
- `FAIL` when violations exist and no authorized Planner override exists.
- `UNCHECKED` when no envelope is available.

### D5. Planner-owned scope override

Scope overrides must be Planner-owned. An Executor must not be able to waive its
own out-of-envelope edits from the `EXEC` packet alone.

A future implementation may recognize an override only from the original
`TASK`, a later `CORRECT` packet, or another Planner-authored packet that names
the exact paths and reason. The `EXEC` packet may cite that override in
`Deviations from Task Packet`, but it is not the authority.

This keeps the validator aligned with the protocol's existing arbitration
model: scope expansion flows through the Planner and remains visible in the
audit trail.

### D6. Claim and lock convention

A future claim mechanism has two layers:

1. **Live lock.** A generated live lock under `.ai/` is acquired atomically,
   for example by creating a dispatch-specific directory. The live lock may be
   removed or replaced when released or expired. It is operational state, not
   the permanent protocol record.
2. **Append-only claim event.** Each claim, renew, release, expire, or reclaim
   writes a new JSON event under an append-only claim-event path, for example:

```text
ai_handoffs/claims/<DISPATCH_ID>_<TIMESTAMP>_<EVENT>.json
```

Each event records:

```json
{
  "dispatch_id": "ISSUE-123",
  "event": "claim",
  "actor": "Claude Code",
  "harness": "Invoke-AiDispatchLoop.ps1",
  "branch": "ai-dispatch/ISSUE-123",
  "timestamp": "2026-06-06T00:00:00+03:00",
  "ttl_seconds": 3600
}
```

An actor finding a live, unexpired claim it does not own must halt before
execution and report `HANDOFF_STATUS: BLOCKED` or equivalent queue failure
metadata. A stale claim can be reclaimed only by writing a reclaim event and
acquiring a fresh live lock.

This design does not pretend one TTL file is append-only. The live lock is
mutable operational state; the claim events are the durable audit record.

Initial standalone tooling now lives in `Invoke-HandoffClaim.ps1`. It supports
`Status`, `Claim`, `Renew`, `Release`, and `Reclaim` actions over the live
`.ai/handoff-claims/<DISPATCH_ID>/` directory plus append-only
`ai_handoffs/claims/*.json` events. It is not wired into any dispatch runner.

### D7. Rollout and smoke requirements

The rollout is advisory-first:

1. Implement a standalone validator, not wired into the canonical verify gate.
2. Run it manually against representative hand-authored envelopes for at least
   five recent dispatches with known diffs.
3. Confirm expected `PASS` results.
4. Inject one deliberate out-of-envelope touched file and confirm `FAIL`.
5. Confirm legacy packets without envelopes return `UNCHECKED`.
6. Confirm Planner-owned override downgrades `FAIL` to `WARN`.
7. Confirm Executor-only override does not downgrade `FAIL`.

Initial executable smoke coverage now lives in
`tools/dispatch-tests/HandoffScopeHistoricalSmoke.Tests.ps1`. It hand-authors
envelopes for five representative recent commits, reads their touched files
from `git diff-tree`, and pins expected `PASS`, injected `FAIL`,
Planner-owned `WARN`, and legacy `UNCHECKED` behavior without rewriting
historical packets.

Those results are now recorded, so advisory integration into
`.ai/dispatch.verify.ps1` is permitted. Blocking integration still requires a
separate decision after advisory output has run cleanly.

## Consequences

### Positive

- Mandatory protocol rules become mechanically checkable without requiring a
  new runtime or shared service.
- Parallel and autonomous dispatches gain a collision-prevention mechanism.
- The protocol remains readable Markdown plus small JSON records.
- Legacy handoff packets remain valid.
- Scope expansion remains Planner-arbitrated instead of Executor-waived.

### Negative

- This partially reverses the current Non-Goal that excludes enforcement beyond
  Planner, Reviewer, and human discipline.
- The scope validator can false-positive if the glob dialect or diff base is
  wrong.
- Claim handling introduces generated operational state under `.ai/`.
- Template and sidecar changes will eventually be needed if the envelope proves
  useful.

### Mitigations

- Advisory-first rollout.
- `UNCHECKED`, not `FAIL`, for legacy packets.
- No brace glob support.
- Whole-dispatch diff from merge-base.
- Planner-owned override only.
- Append-only claim events separate from mutable live locks.
- Blocking mode requires another explicit decision.

## Alternatives considered

| Alternative | Pros | Cons | Outcome |
|---|---|---|---|
| Do nothing | Preserves the current protocol exactly | Leaves scope and duplicate-claim gaps discipline-only | Rejected if parallel/autonomous execution continues |
| Full state machine | Strongest enforcement | Contradicts the Markdown governance protocol and Non-Goals | Rejected |
| Adopt external task ledger tooling | Reuses prior art | Adds a runtime dependency and reduces cross-agent portability | Rejected |
| Single mutable `<DISPATCH_ID>.claim.json` | Simple to read | TTL refresh/reclaim conflicts with append-only audit semantics | Rejected |
| Advisory-only validator forever | Low risk | Never closes the structural gap | Deferred fallback if smoke results are noisy |

## Implementation guidance

The first implementation slice was documentation/tooling-only and did not
modify `.ai/dispatch.verify.ps1`.

Suggested first slice:

- Add a standalone validator script under a tooling path.
- Add unit/smoke tests for envelope parsing and glob matching.
- Add sample synthetic fixtures, not historical packet rewrites.
- Record smoke results in the dispatch EXEC/CLOSEOUT.
- Leave all live automation behavior unchanged.

The second slice updates packet templates and Planner prompt guidance with the
optional envelope block. It does not require old packets to gain envelopes and
does not change queue scope-guard behavior.

The third slice may wire advisory output into the canonical verification gate.
That integration must remain non-blocking and non-counted unless a later
decision explicitly promotes it.

Blocking behavior is deliberately out of scope until advisory output has proven
stable.
