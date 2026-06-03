# ARCHITECTURE_LINTS

| Companion to | PLAN.md §1.3 (Rule 3 — line cap, no utils) + §1.6.4 (one-import-path-per-format) + §1.8 (forbidden-dep DAG) + §1.13 (failure-class taxonomy) + §1.14 (graph-foundation substrate) + §1.15 (editor-state coordination-not-authority) + §6.16 (Command Bus); IMPLEMENTATION.md Phase 0.2 (architecture-lints DONE) |
|---|---|
| Status | Active v1; the 9 enforcement lints all PASS (per Status.md 2026-05-09 architecture lint matrix) + the supplementary `snapshot-participate` (warning-level, never fails CI); 28 entries in `exemptions.toml` (all failure-class rollout-debt; no graph-foundation or reserved entries); 121 tests across `tools/architecture-lints/{src,tests}/` (43 inline + 78 fixture-based integration); CI-wired via `.github/workflows/architecture.yml`; the workspace's only architectural-correctness gate today |
| Audience | Subsystem authors landing first real implementation (must not break a lint); reviewers verifying which rule a violation maps to; orchestrator authors adding a new lint; rollout-debt tracker authors clearing exemptions |
| Sibling doc | `GRAPH_FOUNDATION.md` — substrate behind the graph-foundation lint Check 2; `RECOVERY_MODEL.md` — the failure-class lint's enforcement target; `EXECUTION_DOMAINS.md` — kernel-isolation rule's domain context; `EDITOR_STATE_MODEL.md` — editor-state-ownership rule's owner |
| Reference impls | `tools/architecture-lints/src/main.rs` (121L; CLI dispatch) · `tools/architecture-lints/src/common.rs` (250L; `Violation` / `LintReport` / `Exemptions` / cargo-metadata helpers) · 10 lint implementation modules (9 enforcement + the supplementary `snapshot-participate`) · 10 fixture-based integration test files at `tools/architecture-lints/tests/` · `tools/architecture-lints/exemptions.toml` (28 entries — all failure-class rollout-debt; no graph-foundation or reserved) · `.github/workflows/architecture.yml` (CI invocation) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the meta-reference for the architecture-enforcement suite — 9 enforcement lints plus the supplementary warning-level `snapshot-participate`. Each lint's specific rationale is in its module-doc; this doc covers the suite-level shape, the exemptions-toml registry, the CI integration, and lint-author guidance for adding a new lint.

## 1. Why a substrate

PLAN's architectural rules are spread across §1.3, §1.6.4, §1.8, §1.13, §1.14, §1.15, and §6.16. Without a mechanical enforcement layer, every PR review would manually re-check every rule against every change — and rules that are NOT mechanically enforced will silently rot (audit-1 found three: failure-class declarations had not been added to 81 crates; graph-foundation Check 2 had not detected `kernel/asset` reinventing adjacency; forbidden-dep rules 3-6 were dead code due to the `rge-` prefix mismatch).

`tools/architecture-lints` is the canonical home for the mechanical-enforcement suite. Three load-bearing properties:

- **One CLI binary, 9 enforcement lints + 1 supplementary, single PASS/FAIL gate.** `cargo run -p rge-tool-architecture-lints -- all` runs every lint and exits non-zero if any *enforcement* lint failed (the supplementary `snapshot-participate` always exits 0). CI's `.github/workflows/architecture.yml` wires this exactly.
- **One exemptions registry, code-review-trailed.** `tools/architecture-lints/exemptions.toml` is the only place to suppress a lint against a specific file path; adding an entry requires explicit reason field + follow-up plan. No inline `#[allow(...)]` annotations scattered across the workspace.
- **Per-lint fixture-based integration tests.** Every lint has its own `tests/<lint>_test.rs` exercising synthetic fixtures against the lint's algorithm; `tests/fixtures/forbidden_dep/` holds workspace-shaped fixture trees. Integration-test count today: 78 across all lints.

## 2. The 10 lints — what each enforces

The canonical mapping (per `main.rs` lines 38-67 and Status.md "Architecture lint matrix"):

| Lint | PLAN.md rule | What it detects |
|---|---|---|
| `forbidden-dep` | §1.8 | The 7-rule dependency DAG (Tier-1↛Tier-2; Tier-2↛Tier-3; cad-core stands alone; editor-ui↛physics/audio/input; physics↛script-host; renderers↛game-domain; editor-shell stays loader-free) |
| `split-exemption` | §1.3 Rule 3 | Any `.rs` >1000 lines requires a `// SPLIT-EXEMPTION: <reason>` annotation |
| `no-utils` | §1.3 Rule 3 | No `utils.rs` / `util.rs` / `helpers.rs` / `helper.rs` files anywhere in the workspace |
| `graph-foundation` | §1.14 | Check 1: no `NodeId` / `EdgeId` / `StableHash` redefinitions outside `kernel/graph-foundation`. Check 2: no `BTreeMap<K, BTreeSet<K>>` / `HashMap<K, HashSet<K>>` adjacency reinvention |
| `editor-state-ownership` | §1.15 | Part A: `Selection` / `Hover` / `ActiveTool` / `ModalState` / `DragDrop` only defined inside `crates/editor-state/`. Part B: `editor-state` may not import authoritative content types (coordination-not-authority) |
| `command-bus` | §6.16 | `crates/**` (other than `crates/editor-actions/`) cannot import `kernel_ecs::{Commands, EntityMut, Mut, insert, remove, replace, insert_component, remove_component, despawn, spawn_with}` |
| `projection-modules` | §1.6 / §1.8 | `crates/cad-projection/src/projection_structural/` cannot import from `projection_runtime` or `projection_editor` |
| `kernel-isolation` | §1.6.4 | One-import-path-per-format: each binary asset format has exactly one `io-*` crate handling it (file is named for fileandfolderstructure.md §12 history; the rule is one-format-per-crate) |
| `failure-class` | §1.13 | Every Tier-1 + Tier-2 crate's `src/lib.rs` declares `//! Failure class: <kind>` for at least one of the 5 closed-set values |
| `snapshot-participate` | §13.2 | **Supplementary / warning-level** (never fails CI): every stateful Tier-2 crate (closed list) should carry an `impl SnapshotParticipate`; emits `info:` coverage lines and always exits 0 |

`Cmd::All` (`main.rs` lines 97-111) runs all 10 in fixed order — the 9 enforcement lints, then the supplementary `snapshot-participate`. The dispatcher prints `[<lint>] PASS (0 violations)` or `[<lint>] FAIL (N violations)` per lint, then exits 0 iff every *enforcement* lint passed (`snapshot-participate` never contributes a failure).

## 3. Lint-by-lint summary

For each lint, the algorithm sketch + what counts as a violation. Full module-docs in `tools/architecture-lints/src/<lint>.rs`.

### `forbidden-dep` (`forbidden_dep.rs`, 817L, 27 inline tests)

Walks `cargo metadata`'s direct workspace-internal dep graph (external registry deps ignored). For each rule, classifies each (consumer, dep) pair via `Tier::One` / `Tier::Two` / `Tier::Three` and checks the prohibition. Workspace package names carry the `rge-` prefix (`rge-cad-core`, `rge-physics`, etc.) — audit-6 (2026-05-09) caught that rules 3-6 were dead code because they compared against bare names; the fix was prefix-correct comparisons (per `forbidden_dep.rs` lines 14-22). The forbidden-dep `rge-` prefix discipline is now active per audit-5 closure.

### `split-exemption` (`split_exemption.rs`, 168L, 6 inline tests)

Walks every `.rs` under `kernel/`, `crates/`, `runtime/`, `editor/`, `tools/`. Files >1000 lines without a `// SPLIT-EXEMPTION:` annotation (case-sensitive; colon required) are violations. Single bare flag — no exemptions in `exemptions.toml`.

### `no-utils` (`no_utils.rs`, 46L, 0 inline tests; 7 integration tests)

Walks every `.rs` and rejects files whose base name (case-insensitive) is `utils.rs` / `util.rs` / `helpers.rs` / `helper.rs`. Catch-all "bag of tricks" files are an architecture smell; descriptive module names instead. Smallest lint in the suite.

### `graph-foundation` (`graph_foundation.rs`, 316L, 0 inline tests; 9 integration tests)

Two checks per `graph_foundation.rs` lines 1-24:

- **Check 1 (forbidden-name redefinition).** No crate outside `kernel/graph-foundation/` may define its own `NodeId` / `EdgeId` / `StableHash` (struct, enum, type alias, or trait). Uses `syn::visit` to inspect AST definitions.
- **Check 2 (adjacency-map reinvention).** Added 2026-05-09 per audit-5 deep-audit followup. No crate outside `kernel/graph-foundation/` may define a struct field of shape `BTreeMap<K, BTreeSet<K>>` or `HashMap<K, HashSet<K>>` where the outer key type equals the inner set's element type. That shape is the canonical "I'm reinventing graph storage" pattern. Without Check 2, audit-1 would have missed `kernel/asset::DependencyGraph` silently rolling its own graph.

`exemptions.toml` currently carries **no** `graph-foundation` exemption (all 28 entries are `failure-class` rollout-debt). A prior false-positive exemption for `crates/editor-ui/src/layout/node.rs` — editor-ui's `LayoutNodeId` is its own UI-tree-node identifier, semantically unrelated to graph-foundation's `NodeId` — is no longer present in the registry.

### `editor-state-ownership` (`editor_state_ownership.rs`, 500L, 0 inline tests; 16 integration tests)

Two-part per the module-doc lines 1-22:

- **Part A — Ownership.** The five coordination-state types (`Selection`, `Hover`, `ActiveTool`, `ModalState`, `DragDrop`) may only be **defined** inside `crates/editor-state/`. `use … ::Selection` (re-import) is explicitly allowed.
- **Part B — Coordination-not-authority.** `crates/editor-state/` may only import IDs and handles from the kernel tier; cannot import authoritative content types from Tier-2. Exception: `kernel/*` crates (paths starting with `kernel_`) are freely importable.

### `command-bus` (`command_bus.rs`, 318L, 0 inline tests; 7 integration tests)

Active enforcement since 2026-05-05 per the module-doc lines 12-17. Forbidden symbol list per lines 19-38 covers 10 mutation-side `kernel_ecs::*` symbols. Scope: applies only to `crates/**` (kernel/**, runtime/**, editor/**, tools/** explicitly skipped per lines 41-50). Read-only access (`Query`, `Res`, `EntityRef`) is unrestricted.

### `projection-modules` (`projection_modules.rs`, 308L, 0 inline tests; 8 integration tests)

Inside `crates/cad-projection/src/`, the `projection_structural` module may NOT import from `projection_runtime` or `projection_editor`. Returns an empty passing report immediately if `crates/cad-projection/src/` doesn't exist (so the lint is no-op during early phases).

### `kernel-isolation` (`kernel_isolation.rs`, 160L, 0 inline tests; 8 integration tests)

Naming-mismatch noted in module-doc lines 4-10: file is named for `fileandfolderstructure.md §12` history; the rule is **one-import-path-per-format** (PLAN §1.6.4). Each `io-*` crate opts in via `[package.metadata.rge] formats = [...]` in its own `Cargo.toml`. Missing-metadata policy is **Option B (pragmatic)**: emit `warning:` to stderr and continue (real workspace today has no `io-*` crate opted in; lint exits 0 to avoid stalling rollout).

### `failure-class` (`failure_class.rs`, 239L, 5 inline tests; 7 integration tests)

The lint that closes the audit-1 audit-debt registry per `RECOVERY_MODEL.md` §5. Algorithm:
1. Walk every workspace member via `cargo metadata`.
2. Skip non-Tier-1 / non-Tier-2 crates (the `classify` helper returns `Tier::One` / `Tier::Two` for `kernel/*` and `crates/*` paths only).
3. For each in-scope crate, locate `src/lib.rs`.
4. Check `exemptions.toml` for an `[[exemption]]` whose `lint = "failure-class"` and `file = "<manifest-rel-path>"`.
5. If exempt — skip. Otherwise scan every `//!` line for the prefix `Failure class:` and validate each comma-separated value against the closed set.

Closed set: `recoverable` / `snapshot-recoverable` / `plugin-fatal` / `session-fatal` / `kernel-fatal` (case-sensitive). Multi-value `//! Failure class: recoverable, snapshot-recoverable` is supported.

### `snapshot-participate` (`snapshot_participate.rs`, 377L, 5 inline tests; 5 integration tests) — supplementary / warning-level

NOT an enforcement lint. For every Tier-2 crate in the closed `STATEFUL_TIER2_CRATES` list (`cad-core`, `cad-projection`, `physics` + the forward-compat `particles` / `sculpt`), checks whether its `src/` tree contains an `impl SnapshotParticipate` (string match — the trait name is unique to this codebase, so no `syn` walk is needed). Emits an `info:` line per crate (impl present → stdout; missing → stderr) plus a one-line coverage summary, then reports PASS **regardless** — it NEVER pushes a `Violation`, so its exit code is always 0 and it cannot fail the `all` aggregate. Scaffolds the PLAN §13.2 v1.0 gate ("all stateful Tier-2 has `SnapshotParticipate`") as coverage tracking without blocking inter-Phase landings; a future dispatch flips it to error-level by pushing a `Violation` for the missing-impl case (per the `snapshot_participate.rs` module-doc "Why warning-level only").

## 4. The exemptions registry

Lives at `tools/architecture-lints/exemptions.toml`. 28 entries today (per `grep -c '^\[\[exemption\]\]'`), all `failure-class`:

- **28 failure-class rollout-debt exemptions — all 28 entries.** Per `RECOVERY_MODEL.md` §6: when the lint was introduced 2026-05-05, all 81 Tier-1 + Tier-2 crates lacked the `//! Failure class: <kind>` declaration. Rather than block landing the lint behind 81 simultaneous edits, per-crate exemptions were added; each clears as its crate gets first real implementation. **28 remain** (of the original 81) as of 2026-06-02.
- **No `graph-foundation` or "reserved" entries.** A prior `graph-foundation` false-positive (for `crates/editor-ui/src/layout/node.rs`) is no longer present; the registry today is exclusively failure-class rollout-debt.

Total: 28 (all failure-class rollout-debt).

### Exemption schema

```toml
[[exemption]]
lint = "<lint-name>"        # one of the kebab-case lint names emitted by main.rs
file = "<path>"             # workspace-relative, forward slashes (POSIX-style)
reason = "<why>"            # human-readable justification + follow-up plan
```

Adding an exemption is a deliberate architectural decision and SHOULD be accompanied by a follow-up task (see `reason` field). Removing an exemption is encouraged whenever the underlying issue is resolved.

### Failure-class rollout-debt cleanup recipe

The mechanical procedure (per `RECOVERY_MODEL.md` §6 + Status.md "Physics + Audio failure-class exemptions cleared" 2026-05-08):

1. Crate gets first real implementation per `IMPLEMENTATION.md` phase order.
2. Add `//! Failure class: <kind>` line in the crate's `src/lib.rs` (one of the 5 closed-set values).
3. Remove the matching `[[exemption]]` block from `exemptions.toml`.
4. Re-run `cargo run -p rge-tool-architecture-lints -- failure-class` — must exit 0.

The exemption removal is part of the same dispatch as the implementation; the lint then enforces the declaration on that crate immediately.

## 5. Test fixtures pattern

Every lint has unit-fixture tests + workspace-regression tests; total **121 tests** across `tools/architecture-lints/{src,tests}/`:

### Inline tests in `src/` (43 total)

- `forbidden_dep.rs`: 27 tests (rule-by-rule classification + edge cases).
- `split_exemption.rs`: 6 tests (cap-not-reached / cap-reached-with-marker / cap-reached-without-marker / nested edge cases).
- `failure_class.rs`: 5 tests (parse-extra-whitespace / wrong-case-keyword-not-parsed / multi-value-line / closed-set / lint-name-stable).
- `snapshot_participate.rs`: 5 tests (bare-name-prefix-strip / list-contains-known-impls / list-excludes-audited-removals / list-sorted / nonexistent-dir-false).

### Integration tests in `tests/` (78 total; one file per lint + shared fixtures)

- `command_bus_test.rs` (7), `editor_state_ownership_test.rs` (16), `failure_class_test.rs` (7), `forbidden_dep_test.rs` (6), `graph_foundation_test.rs` (9), `kernel_isolation_test.rs` (8), `no_utils_test.rs` (7), `projection_modules_test.rs` (8), `snapshot_participate_test.rs` (5), `split_exemption_test.rs` (5).
- `tests/fixtures/forbidden_dep/` — workspace-shaped fixture tree exercising the dep-graph traversal against synthetic Tier-1 / Tier-2 crates.

The literal `#[test]` count is **121** today — 43 inline (`src/`) + 78 integration (`tests/`) — the supplementary `snapshot-participate` lint contributed 5 inline + 5 integration, and `editor-state-ownership`'s integration suite has since grown to 16 (from 7). (Status.md's architecture-lint test tally is counted separately and may lag this figure until its next refresh.)

The fixture pattern: write a synthetic `.rs` file containing the rule-violating shape, parse it into the lint's algorithm, assert the algorithm reports the expected `Violation` (file path, line, message). Each lint test exercises the positive-case (rule violated → violation reported) AND the negative-case (rule satisfied → no violation reported); the `forbidden_dep` lint additionally exercises `tests/fixtures/forbidden_dep/` synthetic workspace trees against `cargo_metadata::MetadataCommand`.

## 6. The graph-foundation lint Check 2 (added 2026-05-09)

Per `graph_foundation.rs` lines 10-19:

> "**Check 2 — adjacency-map reinvention** (added 2026-05-09 per audit-5 deep-audit followup). No crate outside `kernel/graph-foundation/` may define a struct field of shape `BTreeMap<K, BTreeSet<K>>` or `HashMap<K, HashSet<K>>` where the outer key type equals the inner set's element type. That shape is the canonical 'I'm reinventing graph storage' pattern (an adjacency map). The proper substrate is `kernel/graph-foundation::Graph<N, E>`. Without this check, audit-1 found `kernel/asset::DependencyGraph` had silently rolled its own graph via `BTreeMap<AssetId, BTreeSet<AssetId>>` — Check 1 didn't catch it because no `NodeId` / `EdgeId` / `StableHash` redefinition was involved."

The check uses `syn::visit::Visit` on `ItemStruct` field types; the type-parameter equality check is structural (both sides parsed as `Type::Path` and compared). Two adjacency-map shapes are recognised: `BTreeMap<K, BTreeSet<K>>` and `HashMap<K, HashSet<K>>`. The check fires when the outer Map's value-type generic argument matches the inner Set's element-type generic argument — false positives arise only when two semantically-unrelated identifiers happen to alias the same name (e.g. editor-ui's `LayoutNodeId`).

Cross-ref `GRAPH_FOUNDATION.md` §6 for the substrate-reuse pattern Check 2 enforces; cross-ref `KERNEL_ASSET.md` §1 third bullet for the migration that closed the audit-1 catch.

## 7. The forbidden-dep `rge-` prefix discipline (2026-05-09 audit-5 fix)

Per `forbidden_dep.rs` lines 14-22:

> "Every workspace member's `name = "..."` field in its `Cargo.toml` carries the `rge-` prefix (e.g. `rge-cad-core`, `rge-physics`, `rge-gfx`, `rge-editor-ui`). `cargo_metadata::Package::name` returns this exact name, so all literal package-name comparisons in this lint must include the `rge-` prefix. Audit-6 (2026-05-09) caught that rules 3-6 were dead code because they compared against the bare names (`"cad-core"`, `"physics"`, etc.) which never match the prefixed reality."

Dead-code period: rules 3-6 (cad-core stands alone, editor-ui ↛ physics/audio/input, physics ↛ script-host, renderers ↛ game-domain) were inert for a stretch where the lint exited 0 not because the dep-graph satisfied the rules but because the comparisons never matched. Audit-5 (2026-05-09) caught the prefix mismatch; the fix was a single-batch update to all literal name constants.

Rules 1-2 (Tier-1 ↛ Tier-2 / Tier-2 ↛ Tier-3) were active throughout because they classify by directory path, not literal name. Rules 3-6 are now active per audit-5 closure.

## 8. CI integration via `.github/workflows/architecture.yml`

The workflow (per `architecture.yml` lines 1-23):

```yaml
name: Architecture lints
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  architecture:
    name: cargo run -p rge-tool-architecture-lints -- all
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain (per rust-toolchain.toml)
        uses: dtolnay/rust-toolchain@stable
        with: { toolchain: stable }
      - name: Cache cargo registry + target
        uses: Swatinem/rust-cache@v2
        with: { shared-key: architecture-lints }
      # ... (cargo build + run -- all)
```

Every push to `main` and every PR to `main` runs `cargo run -p rge-tool-architecture-lints -- all`. The job exits 0 iff every lint exits 0; non-zero blocks merge. The `architecture-lints` cache key keeps `target/` warm across runs. Note: per Status.md line 42, `architecture.yml` is the **only** CI workflow that runs the lint suite today; the workspace test suite (`cargo test --workspace`) is **not** wired into CI yet (gap tracked in Status.md "remaining audit-debt").

## 9. Failure-class lint enforcement progress

Per `RECOVERY_MODEL.md` §4 (the 23 declared crates) + §6 (rollout-debt frame) + Status.md 2026-05-09:

- **23 of original 81 cleared** (`kernel/{types, ecs, schedule, diagnostics, plugin-host, asset, audit-ledger, app, graph-foundation, events}` = 10 kernel + `crates/{cad-core, cad-projection, editor-actions, editor-state, gfx, physics, audio, script-host, components-{animation, audio, identity, render}, ui-theme}` = 13 crates).
- **58 remain** as of 2026-05-09. Each clears as its crate gets first real implementation per IMPLEMENTATION.md phase order.

Cross-ref `RECOVERY_MODEL.md` §4 for the per-crate class assignment table; §8 for the `PluginError` → `FailureClass` mapping; §10 for the auto-emit policy that routes plugin errors onto declared classes.

## 10. Lint-author guidance — adding a new lint

The mechanical procedure (per the suite's pattern):

1. **Add the module.** Create `tools/architecture-lints/src/<new_lint>.rs` with module-doc summarising the rule, source-truth references, and the algorithm sketch. Mirror the module-doc shape of an existing lint.
2. **Wire into the dispatcher.** Add `mod <new_lint>;` to `main.rs` (alphabetical order); add a `Cmd::<NewLint>` variant; add a match arm in `run()` for both single-invocation and `Cmd::All`.
3. **Add lint constants.** Define the canonical kebab-case name (`const LINT_NAME: &str = "<new-lint>";`) used in both `LintReport::new` and `exemptions.toml` schema.
4. **Implement `pub(crate) fn run(workspace_root: &Path) -> Result<LintReport>`.** Use `common::iter_rust_files` / `common::cargo_metadata` / `common::Exemptions::load` per the existing pattern. Return `LintReport` with one `Violation` per detected rule break.
5. **Add inline `#[cfg(test)] mod tests`** with at least 5 cases: positive-case, negative-case, edge-case (whitespace / case sensitivity / nested), exemption-honoured, exemption-malformed.
6. **Add `tests/<new_lint>_test.rs`** with 5+ fixture-based regressions exercising the lint against synthetic workspace shapes.
7. **Update `RECOVERY_MODEL.md` §1** if the new lint enforces a recovery-model property, or **add a new sibling §18 doc** if the lint enforces a new substrate.
8. **Run `cargo run -p rge-tool-architecture-lints -- all`** and confirm every lint still PASSes. If pre-existing workspace state would fail the new rule, add rollout-debt exemptions to `exemptions.toml` with a follow-up cleanup plan.
9. **Add this doc's §2 table row + §3 lint summary block.**

The shared-helpers pattern in `common.rs` (lines 1-250) is the substrate every lint sits on: `Violation` / `LintReport` / `Exemptions::is_exempt` / `cargo_metadata` / `workspace_members` / `iter_rust_files` / `source_roots` / `Tier::One` / `Tier::Two` classification / `relativize`. Reuse before adding new infrastructure.

## 11. Source / spec inconsistencies

> **Note (authoring-time reconciliation).** The bullets below reconcile the original commissioning brief against source-truth *as this doc was first authored*. The supplementary `snapshot-participate` lint and its tests post-date that reconciliation — see §1 / §2 / §5 for the current **10-lint** (9 enforcement + 1 supplementary) and **121-test** figures.

- **Brief stated "9 lint impls" + "exemptions.toml" + "1 substantive + 58 rollout-debt remaining"**; source-truth via `grep -c '\[\[exemption\]\]'` on the exemptions TOML file at that audit: **60** total entries (1 graph-foundation FP + 58 failure-class rollout-debt + 1 reserved). The brief's "1 substantive + 58" lined up with the 1 graph-foundation false-positive + 58 failure-class rollout-debt; the 1 reserved was implementation-detail capacity. (Dated finding — the registry has **since been reduced to 28 entries, all `failure-class`**: the graph-foundation false-positive and the reserved slot are gone and 30 more failure-class crates cleared as they got first real implementations. The current authoritative count is in §1 and §4; use anchored `grep -c '^\[\[exemption\]\]'` — the unanchored form over-counts by matching the schema comment.)
- **Brief stated "every lint has unit-fixture tests + workspace-regression tests; total 97 tests in tools/architecture-lints/"**; source-truth via `grep -c '#\[test\]'`: 33 inline tests across `src/` + 64 integration tests across `tests/` = 97 total. Status.md line 51 reports `rge-tool-architecture-lints | 69` — the discrepancy between 97 and 69 is *non-test* tests (the 33 inline tests are inside `mod tests` blocks but several use `#[allow(clippy::unwrap_used)]` shapes that the test-counter doesn't always pick up cleanly). The 97 figure is the literal `#[test]` count; the doc reports both for honesty.
- **Brief stated `kernel-isolation` enforces "PLAN §1.6.4 one-import-path-per-format"** — source-truth confirmed at `kernel_isolation.rs` lines 1-7. The file is named `kernel_isolation` for `fileandfolderstructure.md §12` historical reasons; the actual rule is one-format-per-crate (matching the brief). The doc surfaces the naming mismatch so future readers don't assume the lint enforces a "kernel-isolation" property.
- **Brief stated "23 of 81 cleared / 58 remain"**; source-truth via grep on `lib.rs` files (10 kernel + 13 crates = 23 with `//! Failure class:` declaration) + `grep -c 'lint = "failure-class"' exemptions.toml` (= 58). The two numbers are consistent (81 - 58 = 23). The doc reflects this in §9.
- **Brief stated "graph-foundation lint Check 2 (added 2026-05-09 to detect BTreeMap<K, BTreeSet<K>> adjacency reinvention)"**; source-truth confirmed at `graph_foundation.rs` lines 10-19. The audit-5 followup that introduced Check 2 is documented in the module-doc and re-stated in this doc's §6.
- **Brief stated "forbidden-dep `rge-` prefix discipline (rules 3-6 were dead code pre-2026-05-09 audit-5; now active)"**; source-truth confirmed at `forbidden_dep.rs` lines 14-22. The audit-6 (the brief said audit-5; module-doc says audit-6) date and dead-code period are documented verbatim. The doc reflects this in §7 and notes the audit-5 vs audit-6 brief vs source disagreement is minor (the same audit cycle in different bookkeeping).

## 12. References

- **PLAN.md §1.3** — Rule 3: line cap (1000-line `// SPLIT-EXEMPTION:` requirement) and no `utils.rs` / `helpers.rs` files.
- **PLAN.md §1.6.4** — one-import-path-per-format; the `io-*` crate-per-format rule the `kernel-isolation` lint enforces.
- **PLAN.md §1.8** — forbidden-dep DAG; the 7-rule taxonomy `forbidden-dep` enforces.
- **PLAN.md §1.13** — failure-class taxonomy; the 5-class closed set + per-crate declaration the `failure-class` lint enforces.
- **PLAN.md §1.14** — graph-foundation substrate doctrine; the substrate-redefinition + adjacency-reinvention rules `graph-foundation` enforces.
- **PLAN.md §1.15** — editor-state coordination-not-authority; the type-ownership + import-restriction rules `editor-state-ownership` enforces.
- **PLAN.md §6.16** — Command Bus; the mutation-API import restriction `command-bus` enforces.
- **IMPLEMENTATION.md Phase 0.2** — architecture-lints DONE; the lint suite this doc covers (9 enforcement + 1 supplementary).
- **`GRAPH_FOUNDATION.md`** — sibling §18 doc; the substrate behind the graph-foundation lint Check 2; documents the `Graph<N, E>` substrate that consumers should reuse.
- **`RECOVERY_MODEL.md`** — sibling §18 doc; the failure-class enforcement target; §6 documents the rollout-debt frame; §4 documents the 23 declared crates.
- **`EXECUTION_DOMAINS.md`** — sibling §18 doc; per-domain failure-class implications; the kernel-isolation rule's domain context.
- **`EDITOR_STATE_MODEL.md`** — sibling §18 doc; documents the coordination-state types `editor-state-ownership` Part A protects.
- **`EDITOR_ACTIONS_COMMAND_BUS.md`** — sibling §18 doc; the canonical consumer of the mutation API the `command-bus` lint protects.
- **`KERNEL_ASSET.md`** — sibling §18 doc; documents the substrate migration that closed the audit-1 graph-foundation Check 2 catch.
- **`tools/architecture-lints/src/main.rs`** — CLI dispatch; the all-lints runner (9 enforcement + 1 supplementary).
- **`tools/architecture-lints/src/common.rs`** — shared helpers (`Violation`, `LintReport`, `Exemptions`, `cargo_metadata`, tier classification).
- **`tools/architecture-lints/src/forbidden_dep.rs`** — 7-rule dep-graph DAG enforcement (817L, 27 inline tests).
- **`tools/architecture-lints/src/split_exemption.rs`** — 1000-line cap + `// SPLIT-EXEMPTION:` annotation requirement (168L, 6 inline tests).
- **`tools/architecture-lints/src/no_utils.rs`** — utils/helpers filename rejection (46L; smallest lint).
- **`tools/architecture-lints/src/graph_foundation.rs`** — Check 1 forbidden-name redefinition + Check 2 adjacency-map reinvention (316L).
- **`tools/architecture-lints/src/editor_state_ownership.rs`** — Part A type-ownership + Part B coordination-not-authority (500L).
- **`tools/architecture-lints/src/command_bus.rs`** — `crates/**` mutation-API import restriction (318L).
- **`tools/architecture-lints/src/projection_modules.rs`** — cad-projection structural-↛-runtime/editor split (308L).
- **`tools/architecture-lints/src/kernel_isolation.rs`** — one-import-path-per-format (160L; misleadingly-named).
- **`tools/architecture-lints/src/failure_class.rs`** — `//! Failure class: <kind>` declaration enforcement (239L, 5 inline tests).
- **`tools/architecture-lints/src/snapshot_participate.rs`** — supplementary warning-level §13.2 `SnapshotParticipate` coverage scaffold (377L, 5 inline tests; never fails CI).
- **`tools/architecture-lints/exemptions.toml`** — the 28-entry exemption registry (all failure-class rollout-debt; no graph-foundation or reserved entries).
- **`tools/architecture-lints/tests/`** — 10 per-lint integration test files + `fixtures/forbidden_dep/` synthetic workspace trees.
- **`.github/workflows/architecture.yml`** — CI invocation of `cargo run -p rge-tool-architecture-lints -- all`.
