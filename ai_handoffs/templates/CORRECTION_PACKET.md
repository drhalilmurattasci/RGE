# Correction Packet

DISPATCH_ID: <same as the TASK_PACKET>
AUTHOR: Planner / <AI identity>
TIMESTAMP: <ISO-8601 local>
RELATED_FILES:
- <files affected by approved corrections>
STATUS: CORRECTION_OPEN

## References

- Task Packet: `ai_handoffs/<DISPATCH_ID>_TASK_<TIMESTAMP>.md`
- Latest Execution Report: `ai_handoffs/<DISPATCH_ID>_EXEC_<TIMESTAMP>.md`
- Latest Review Report: `ai_handoffs/<DISPATCH_ID>_REVIEW_<TIMESTAMP>.md`
- <Any prior CORRECT packets in this dispatch>

## Approved Corrections (Planner Sign-Off)

The Executor MUST act on exactly these corrections — nothing more, nothing
less. Each correction references the specific Review Report finding it
addresses.

1. **<short title>** — <Reviewer finding reference>. Required change:
   <concrete instruction with file:line if applicable>. Acceptance:
   <what proves the correction landed>.
2. <...>

## Deferred Findings (NOT Approved for This Round)

The following findings from the Review Report are NOT approved for this
correction round. They are recorded here as latent risks to be carried
forward in `FINAL_CLOSEOUT`.

1. **<short title>** — <Reviewer finding reference>. Deferred because:
   <Planner rationale>. Future trigger / dispatch: <when it should be
   addressed>.
2. <...>

## Updated Acceptance Criteria

<If the correction round changes any acceptance bar from the Task Packet,
state the updated bar here. Otherwise: "Unchanged from Task Packet.">

## Re-Verification Gates

The Executor MUST re-run these gates after the corrections:

- `cargo +nightly fmt --check -p <crate>` → expected exit 0
- `cargo test -p <crate> --lib --no-fail-fast` → expected N+delta passed / 0 failed / 0 ignored
- `cargo test --workspace --no-fail-fast` → expected N+delta passed / 0 failed / 19 ignored
- `cargo run -q -p rge-tool-architecture-lints -- all` → expected exit 0
- <any dispatch-specific gates>

## Halt Conditions (Updated if Any)

<List any halt conditions that change for this correction round. Otherwise:
"Unchanged from Task Packet.">

## Planner Notes

<Rationale for the chosen correction subset, why the deferred findings are
deferred, any new context or constraints the Executor should be aware of.>
