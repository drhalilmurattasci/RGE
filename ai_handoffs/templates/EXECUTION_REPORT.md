# Execution Report

DISPATCH_ID: <same as the TASK_PACKET>
AUTHOR: Executor / <AI identity>
TIMESTAMP: <ISO-8601 local>
RELATED_FILES:
- <path/to/file.rs> — <one-line description of edits>
- <path/to/test.rs> — <added test name(s)>
- <path/to/doc.md> — <added section>
STATUS: AWAITING_REVIEW

## Task Packet Reference

`ai_handoffs/<DISPATCH_ID>_TASK_<TIMESTAMP>.md`

## What I Changed

### Source

- `<path>`: <concise description of the edit, including line-range hint if useful>
- <...>

### Tests

- `<path>::<test_fn_name>`: <what invariant the test pins, with concrete
   expected values where applicable>
- <...>

### Docs

- `<path>`: <what was added (e.g. snapshot, ADR section, change.md row);
   one sentence>
- <...>

## Per-File Summary

<Optional — use only if the per-file edits warrant more than one line each.
Skip if the bullet lists above already say enough.>

## Verification Results

- `cargo +nightly fmt --check -p <crate>` → exit <code>
- `cargo build -p <crate>` → exit <code>
- `cargo test -p <crate> --lib --no-fail-fast` → <N> passed / <M> failed / <K> ignored
- `cargo test --workspace --no-fail-fast` → <N> passed / <M> failed / <K> ignored
- `cargo run -q -p rge-tool-architecture-lints -- all` → exit <code>
  (<W> enforcement PASS + <S> supplementary PASS)
- <any dispatch-specific gates>

## Deviations from Task Packet

<Explicit list of anything that fell outside the stated `MAY` / `MUST NOT`
envelope, or any acceptance criterion that was relaxed. If none, write
"None — execution stayed strictly within the Task Packet scope.">

## Open Questions for Reviewer

<Specific things the Executor wants the Reviewer to focus on. Examples:
- "I substituted aggregate orientation for per-triangle outward winding
   because per-triangle on a centroid-based fan over a non-coplanar boundary
   produced false-positive failures — is that the right v0 tradeoff?"
- "I left the workspace-test gate unverified due to a local rlib-cache
   problem — cad-core lib + arch-lints pass; please re-verify the workspace
   gate on your side." >

## Worktree State

- Tracked files: <clean | <list of modified-but-uncommitted files>>
- Untracked items: <list (include the prior precedent files at the repo
   root if they are still present)>
- Branch: <branch>
- Last commit: `<hash>` <subject>
