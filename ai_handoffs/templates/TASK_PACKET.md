# Task Packet

DISPATCH_ID: <stable identifier, e.g. 2026-05-13-A or phase7-fillet-sub-eta>
AUTHOR: Planner / <AI identity, e.g. Claude | Codex | GPT-5>
TIMESTAMP: <ISO-8601 local, e.g. 2026-05-13_13-00-00+0300>
RELATED_FILES:
- <path/to/file.rs>
- <path/to/other.md>
STATUS: OPEN

## Goal

<One paragraph describing what this dispatch is trying to accomplish and why
the work is needed now. Reference prior dispatches, ADRs, or change.md entries
that motivate this packet.>

## Scope

### MAY edit
- <path/to/file.rs>
- <path/to/other.md>

### MUST NOT edit
- <path/to/forbidden.rs>
- <path/to/protected-ADR.md>

### MAY add new files
- <directory or pattern, e.g. `crates/foo/src/bar/*.rs`>

### MUST NOT add new files
- <e.g. new ADR docs, new crates, new architecture lints,
   new doctrine docs, new Cargo entries>

## Deliverables

- <Concrete artefact: function, struct, test, doc paragraph, commit>
- <...>

## Acceptance Criteria

- <Measurable, e.g. `cargo test --workspace --no-fail-fast` exit 0 with
   N+delta passed / 0 failed / 19 ignored>
- <Specific test must exist and pin specific invariant>
- <Specific lint must pass / must NOT shift exemption count>
- <...>

## Constraints / Non-Goals

- <Things this dispatch explicitly does NOT solve — list them so the
   Executor does not accidentally do extra work>
- <...>

## Verification Gates

The Executor MUST run and document the result of each of these in their
`EXECUTION_REPORT`:

- `cargo +nightly fmt --check -p <crate>` → expected exit 0
- `cargo build -p <crate>` → expected exit 0
- `cargo test -p <crate> --lib --no-fail-fast` → expected N passed / 0 failed / 0 ignored
- `cargo test --workspace --no-fail-fast` → expected N passed / 0 failed / 19 ignored
- `cargo run -q -p rge-tool-architecture-lints -- all` → expected exit 0
  (9 enforcement + 1 supplementary PASS)
- <any dispatch-specific gates>

## Halt Conditions

The Executor MUST halt (without committing) and request fresh guidance if any
of the following occur:

- <Specific failure mode 1, e.g. "any pre-existing test in
   operators::round_fillet regresses">
- <Specific failure mode 2, e.g. "any architecture-lint exemption count
   shifts">
- <Specific failure mode 3, e.g. "the implementation requires a shape
   change to RoundFilletSpec / RoundFilletPathSpec / RoundFilletError">
- <...>

## Planner Notes

<Optional: rationale, references to prior dispatches, ADRs, design tradeoffs,
known-unknowns, alternatives that were considered and rejected.>
