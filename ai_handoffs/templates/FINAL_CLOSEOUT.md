# Final Closeout

DISPATCH_ID: <same as the TASK_PACKET>
AUTHOR: Planner / <AI identity, or explicitly-designated closer>
TIMESTAMP: <ISO-8601 local>
RELATED_FILES:
- <every file touched across the dispatch (source + tests + docs)>
STATUS: CLOSED | ABANDONED

## Dispatch Summary

<One paragraph: what the dispatch shipped (or why it was abandoned), why it
matters, how it relates to the broader chapter / phase / roadmap.>

## Full Packet Chain

In order:

- `ai_handoffs/<DISPATCH_ID>_TASK_<TIMESTAMP>.md`
- `ai_handoffs/<DISPATCH_ID>_EXEC_<TIMESTAMP>.md`
- `ai_handoffs/<DISPATCH_ID>_REVIEW_<TIMESTAMP>.md`
- `ai_handoffs/<DISPATCH_ID>_CORRECT_<TIMESTAMP>.md` (if any; list each
   correction round in order)
- `ai_handoffs/<DISPATCH_ID>_EXEC_<TIMESTAMP>.md` (per correction round)
- `ai_handoffs/<DISPATCH_ID>_REVIEW_<TIMESTAMP>.md` (per correction round)
- (this file)

## Final Commit(s)

- `<hash>` — <subject line>
- <... in chronological order; include all commits that landed under this
   dispatch>

If `ABANDONED`: write `none` and explain in `Dispatch Summary` why no
commits were retained.

## Verification Gates — Final Results

- `cargo +nightly fmt --check -p <crate>` → exit 0
- `cargo build -p <crate>` → exit 0
- `cargo test -p <crate> --lib --no-fail-fast` → <N> passed / 0 failed / <K> ignored
- `cargo test --workspace --no-fail-fast` → <N> passed / 0 failed / 19 ignored
- `cargo run -q -p rge-tool-architecture-lints -- all` → exit 0
  (9 enforcement + 1 supplementary PASS)
- <any dispatch-specific gates>

For `ABANDONED` dispatches: record the gate states at the time of
abandonment (these are not required to be green).

## Test Count Delta

- Per-crate: <previous> → <new> (+<delta> net)
- Workspace: <previous> → <new> (+<delta> net)
- <Note any unusual movements: e.g. "3 reject-tests flipped to accept-tests
   at the boundary lift; the test count stays in the total but the
   semantics changed">

## Remaining Risks Carried Forward

Explicitly enumerate, even if the answer is `none known`:

1. **<risk>** — <why it is deferred>; <what would trigger it>;
   <suggested future dispatch or trigger condition>.
2. <...>

This section MUST exist and MUST be non-empty (use `none known` if truly
none — but think hard before writing that).

## Suggested Follow-On Tasks

- <Specific next-dispatch candidate, with one-sentence framing>
- <...>

Use `none` if the chapter / phase is truly closed and no follow-on is
warranted.

## Sign-Off

Planner: <role> / <AI identity>
Timestamp: <ISO-8601 local>
Status: <CLOSED | ABANDONED>
