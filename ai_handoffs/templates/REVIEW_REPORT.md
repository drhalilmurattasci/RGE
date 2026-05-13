# Review Report

DISPATCH_ID: <same as the TASK_PACKET / EXECUTION_REPORT>
AUTHOR: Reviewer / <AI identity, must differ from the Executor>
TIMESTAMP: <ISO-8601 local>
RELATED_FILES:
- <files actually inspected, not just claimed>
STATUS: APPROVED | NEEDS_CORRECTION | REJECTED

## References

- Task Packet: `ai_handoffs/<DISPATCH_ID>_TASK_<TIMESTAMP>.md`
- Execution Report: `ai_handoffs/<DISPATCH_ID>_EXEC_<TIMESTAMP>.md`
- <Any prior REVIEW or CORRECT packets in this dispatch>

## Independently Re-Run Gates

The Reviewer MUST re-run each verification gate the Executor claimed and
record the actual observed result here. Gates that could not be re-run
(environment / time / sandbox limits) MUST be explicitly noted with reason.

- `cargo +nightly fmt --check -p <crate>` → <observed result> (matches | differs)
- `cargo test -p <crate> --lib --no-fail-fast` → <observed result>
- `cargo run -q -p rge-tool-architecture-lints -- all` → <observed result>
- `cargo test --workspace --no-fail-fast` → <observed result | NOT RE-RUN — reason>
- <other gates>

## Findings

### Correct

- <Specific Executor claim verified, with evidence (file:line, expected
   vs observed value, test output snippet)>
- <...>

### Needs Correction

- **<short title>** — <file:line reference>. <Why it's wrong, with concrete
   evidence>. Recommended fix: <one sentence>.
- <...>

### Latent Risks (Not Blocking)

- <Risk> — <why it is not v0-blocking, what would trigger it, suggested
   future dispatch to address>.
- <...>

## Test Coverage Assessment

- **Strong**: <tests that pin concrete invariants>
- **Weak / Missing**: <gaps in coverage and what tests would close them>

## Doc Accuracy Check

- <Specific claims in `Status.md` / `HANDOFF.md` / ADR / `change.md` that
   match the implementation, with file:line evidence>
- <Specific overclaim or underclaim risks, if any>

## Recommended Action

One of:

- **APPROVE for closeout** — all gates green, no `Needs Correction` items.
- **ISSUE CORRECTION_PACKET addressing**:
  1. <enumerated finding>
  2. <enumerated finding>
- **REJECT** — fundamental scope / approach problem; abandon and re-plan.
   Justification: <one paragraph>.
