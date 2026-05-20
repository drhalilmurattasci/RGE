# ADR-097: cad-projection internal module split

| Status | Accepted (PLAN.md §1.5.4.5 NEW v0.7); current crate layout at `crates/cad-projection/src/` realises the six-module split; this ADR is a documentation backfill |
|---|---|
| Date | 2026-05-20 (backfill — original decision accepted as part of PLAN v0.7) |
| Deciders | (RGE architecture review) |
| PLAN references | §1.5.4.5 (cad-projection internal split), §1.5.4 (cad-core), §1.6.8 (deterministic iteration), §1.8 (forbidden-dep DAG — `projection_structural` cannot import `projection_runtime` or `projection_editor`), §0.6 (freeze policy — 6-way split conserved), §10.4 (dogfood rule — `CadProjectionPlugin`), §13.2 (`SnapshotParticipate` quality gate) |
| ADR references | ADR-098 (topology lineage substrate — referenced by the `BRepHandle` SSoT refactor design), ADR-104 (capability surface), ADR-114 (`PluginContext` owned handoff — substrate behind the cad-projection plugin canary) |
| Implementation phase | Phase 7 — CAD Spike (cad-projection ECS view layer; the six modules are live in `crates/cad-projection/src/` with three filled in and three documented stubs) |

## Context

PLAN.md §1.5.4.5 (added in v0.7) commits the workspace to splitting `cad-projection` internally before it can become a "god bridge". Bridge layers, left to themselves, accumulate hidden policy, silently become orchestration engines, and become impossible to refactor late — the exact failure mode `cad-core` was structured to avoid. `cad-projection` is uniquely exposed to this risk because PLAN §1.8 designates it the **only** Tier-2 crate allowed to import `cad-core` (every ECS-side consumer of CAD goes through it), which structurally invites every cad-adjacent concern to land there. Without a split rule on the books, every future "we just need this one small projection helper" lands in the same module file until refactoring it stops being possible inside a single dispatch.

PLAN §1.5.4.5 names six projection categories — structural, geometric, semantic, runtime, editor, and cache — chosen because each has a *different mutation frequency and a different invalidation rule*. Structural mappings change rarely (entity add/remove); geometric tessellations invalidate on cad-core operator commit; semantic metadata invalidates on annotation or selection change; runtime collision/visibility feeders churn per-frame; editor gizmos and picking churn per-frame in editor mode only and are stripped on cook; cache memoization piggybacks on all of the above. Forcing those concerns into one module file would entangle five distinct invalidation lifecycles in one file's tick path.

The current layout at `crates/cad-projection/src/` realises the six-module split as live Rust modules. Three are implemented (`projection_structural`, `projection_geometry`, `projection_cache`) per the Phase 7.3 dispatch; three (`projection_semantic`, `projection_runtime`, `projection_editor`) are documented stubs per PLAN §0.6 freeze policy, conserved as named modules rather than collapsed away. The `projection-modules` architecture lint enforces the structural-cannot-import-runtime-or-editor rule out of `tools/architecture-lints/src/projection_modules.rs` (per `docs/§18/ARCHITECTURE_LINTS.md` §2 / §3 — `projection-modules`).

No accepted ADR existed for this decision until this backfill. The decision was load-bearing across `plans/PLAN.md` §1.5.4.5 / §1.8 / §1.6.8, `docs/§18/CAD_PROJECTION.md`, `docs/§18/ARCHITECTURE_LINTS.md`, and the live crate layout — but the ADR slot was empty, and several adjacent ADRs (ADR-098, ADR-104) cross-link to ADR-097 as the canonical source for the split rationale. This ADR fills that slot. It introduces no new architecture; it documents what PLAN §1.5.4.5 already accepts.

## Decision

**`cad-projection` is one crate split internally into six modules, not a multi-crate split. The six modules are `projection_structural`, `projection_geometry`, `projection_semantic`, `projection_runtime`, `projection_editor`, and `projection_cache`. A `projection-modules` architecture lint enforces `projection_structural`'s isolation from `projection_runtime` and `projection_editor`. Adding a seventh category requires a future ADR.**

Four sub-decisions follow.

1. **One crate, six modules — not six crates.** The split is internal to `crates/cad-projection/`. Each category lives in its own module directory under `crates/cad-projection/src/` (`projection_structural/`, `projection_geometry/`, etc.). The crate's `lib.rs` re-exports per module and owns the top-level orchestrator (`CadProjection`). PLAN §1.5.4.5 explicitly allows refactoring to per-category crates later "if growth demands"; today the boundary is module-level. This keeps the split-cost low (no Cargo manifest proliferation, no inter-crate dependency declaration ceremony) while still placing every category behind a module boundary that the `projection-modules` lint can enforce.

2. **Six categories, each owning a single mutation-frequency / invalidation-rule profile.** Per PLAN §1.5.4.5 table:

   | Module | Owns | Updates on |
   |---|---|---|
   | `projection_structural` | entity existence; `BRepHandle` ECS component; `EntityCadMap` bidirectional entity↔cad-node mapping; hierarchy emission | cad-core entity add/remove |
   | `projection_geometry` | tessellation projection; `ProjectedMesh` payload; bounds; `project()` entry-point; `CheckpointTag` proxy | cad-core operator commit (tessellation cache invalidate) |
   | `projection_semantic` | material slot bindings; selection-set membership; layer info | cad-core annotation changes; user selection |
   | `projection_runtime` | collision proxies for physics; visibility filters; render queue input | per-frame, throttled |
   | `projection_editor` | gizmo bindings; picking handles; debug visualisers/overlays | per-frame, editor-only (stripped on cook) |
   | `projection_cache` | memoised `Arc<ProjectedMesh>` storage; dirty-bit set; head-tracking; `CacheStats` | piggyback on all of the above |

   The category ownership in `docs/§18/CAD_PROJECTION.md` §2 / §3 / §4 / §5 / §6 / §7 matches this table for the three implemented modules. The three stub modules are reserved for the named ownership and left empty pending concrete dispatch work.

3. **`projection_structural` cannot import `projection_runtime` or `projection_editor`.** This is the layering invariant PLAN §1.5.4.5 + §1.8 accept. Importing from `projection_geometry` and `projection_cache` is permitted (per `docs/§18/CAD_PROJECTION.md` §2). The intuition: structural mappings are the substrate every other category depends on, so they may not depend on the higher-frequency / editor-only layers — otherwise the dependency cycle ("structural needs runtime for X, runtime needs structural for Y") collapses the split. The architecture-lint `projection-modules` (per `docs/§18/ARCHITECTURE_LINTS.md` §2 / §3) inspects `crates/cad-projection/src/projection_structural/**` for imports referencing `projection_runtime` or `projection_editor` and FAILs the CI gate on violation. The lint returns an empty passing report if `crates/cad-projection/src/` doesn't exist (so it stays no-op during early phases).

4. **Each module documents "what triggers me" and "what I emit", and adding a seventh category requires a future ADR.** Per PLAN §1.5.4.5 CI rules. Module-level Rustdoc on each `mod.rs` is the canonical home for the trigger/emission contract — the editor-ui or runtime author wiring a new feed reads the doc-comment, not a separate diagram. The seventh-category restraint is the freeze rule that keeps the split honest: every PR proposing a new projection category lands as an ADR amendment first, not as a quiet seventh module folder.

## Consequences

### Positive

- **Each category has one tick rhythm, not five.** `projection_structural` ticks on entity add/remove; `projection_geometry` ticks on cad-core commit; `projection_runtime` ticks per-frame. With the split, each module's tick path is the only tick path in that file. Without the split, one file's tick path is the union of five rhythms — every change has to reason about all of them simultaneously.
- **Invalidation rules are localised.** `projection_cache` owns dirty-bit propagation and piggybacks on the other categories' commits. Its invariants are stated in one module (`projection_cache/mod.rs`); the cad-core commit listener does not need to reach into structural or geometric to invalidate them, because the cache is the single point of bookkeeping.
- **Editor-only code is structurally isolatable.** `projection_editor` ownership of gizmos / picking / debug visualisers gives the future "cook strips editor code" workflow a single module to strip rather than chasing editor-only branches across the bridge. The dependency-direction rule (structural cannot import editor) keeps the cookable substrate clean by construction.
- **God-bridge risk is mitigated mechanically.** Without an ADR-defined split + lint, PR review would have to catch every "tiny helper that doesn't really belong anywhere" landing in one big bridge module. With the split, the helper has to land in *a* named category — and if it doesn't fit any, the seventh-category ADR rule forces the author to name what's missing.

### Negative / risks

- **Module-level split is weaker than crate-level isolation.** A determined author can break the `projection_structural` isolation rule by routing through `projection_geometry` (which is allowed to import everything else). The lint catches direct violations; transitive policy violations rely on review discipline. PLAN §1.5.4.5 leaves the "refactor to crates if growth demands" door open precisely because module-level enforcement has this transitive blind spot.
- **Stub modules carry maintenance cost.** Three modules (`projection_semantic`, `projection_runtime`, `projection_editor`) are documented stubs today. They show up in `pub mod` lists, in the architecture-lint scan list, in IDE module trees, etc. — visual noise per PLAN §0.6 conservation in exchange for keeping the named slot reserved. Per `docs/§18/CAD_PROJECTION.md` §2, that trade is explicit.
- **Seventh-category restraint can pressure naming.** Future projection-adjacent work that doesn't cleanly map to one of the six (e.g. "lineage projection", "PIE participate adapters") may either stretch an existing category name or trigger a seventh-category ADR. The restraint is intentional — the ADR cost surfaces "is this really a new category, or just a feature of an existing one?" — but it does shift the cost forward.

### Mitigations

- **`projection-modules` architecture lint as the structural enforcement.** Per `docs/§18/ARCHITECTURE_LINTS.md` §2 / §3 — `projection-modules` (308L, 0 inline tests + 8 integration tests at `tools/architecture-lints/tests/projection_modules_test.rs`). The lint is wired into `cargo run -p rge-tool-architecture-lints -- all` and runs in `.github/workflows/architecture.yml`. Violations FAIL the CI gate.
- **Module-level Rustdoc as the trigger/emission contract.** Per PLAN §1.5.4.5 CI rules, each module's `mod.rs` `//!` block names "what triggers me" and "what I emit". `crates/cad-projection/src/lib.rs` carries the canonical narration of the six-module split and the tick contract.
- **Seventh-category ADR as the freeze gate.** PLAN §0.6 + §1.5.4.5 jointly require any addition of a seventh projection category to ship as an ADR. The ADR-097 → ADR-N amendment trail is the audit log of category growth; without it, a seventh module landing quietly is structurally impossible at the lint + freeze-policy level.

## Alternatives explicitly NOT chosen and why

**One bridge module, no split.** This was the v0.6 PLAN shape. It fails for the reasons PLAN §1.5.4.5 names: every cad-projection concern accumulates into one file, five mutation-frequency profiles entangle in one tick path, and refactoring it later becomes prohibitively expensive ("god bridge"). Rejected by PLAN v0.7.

**Six separate crates from day one (`cad-projection-structural`, `cad-projection-geometry`, …).** Crate-level isolation would give the strongest enforcement, but at six × the manifest, dependency, and version-bump cost. Module-level split inside one crate is the weaker-but-cheaper option, with the explicit PLAN §1.5.4.5 escape hatch of refactoring later if growth demands. The break-even point hasn't been reached. Crate-level split is the deferred future shape, not the wrong shape; it's the shape that's premature today.

**A different number of categories (four / five / seven).** Four (collapsing semantic into structural; collapsing editor into runtime) under-resolves the per-category mutation-frequency story — editor code and runtime code have profoundly different lifecycles (editor-only is stripped on cook), and material/selection metadata behaves differently from raw entity existence. Seven preemptively (carving out, say, "lineage projection" or "PIE adapter" as their own category) violates the freeze rule before any concrete dispatch needs them — and the seventh-category ADR rule exists precisely to discourage premature growth. Six is the PLAN-§1.5.4.5-accepted number; this ADR ratifies it.

**Stricter `projection_structural` isolation (also blocked from `projection_semantic` / `projection_cache`).** PLAN §1.5.4.5 names only `projection_runtime` and `projection_editor` as forbidden imports for `projection_structural`. Tighter rules (e.g. "structural may only depend on cad-core") would be defensible but were not the accepted PLAN shape; structural's dependence on `projection_geometry` (e.g. for `CheckpointTag` re-exports) and `projection_cache` (e.g. for dirty-bit interaction during entity insert) is permitted and used by the live code. This ADR follows PLAN, not a tighter alternative.

**Per-file rather than per-module split (e.g. one big `projection.rs` with category sections).** Section comments inside one file are not enforceable by the architecture lint (the lint inspects module-path imports). Per-module split is what allows mechanical enforcement; per-file split is what becomes a "god module" with extra comments. Rejected for the same god-bridge reason that motivated the split at all.

## Implementation guidance

### Module layout

```
crates/cad-projection/src/
├── lib.rs                        # re-exports; CadProjection orchestrator; SnapshotParticipate impl; CadProjectionPlugin entry
├── plugin_adapter.rs             # Tier-2 plugin canary shim (per PLAN §10.4 dogfood)
├── picking.rs                    # picking surface helpers consumed by future projection_editor work
├── render_adapter.rs             # render-side adapter consumed by future projection_runtime / render-handoff work
├── projection_structural/        # entity existence; BRepHandle; EntityCadMap; hierarchy emission
│   └── mod.rs
├── projection_geometry/          # tessellation projection; ProjectedMesh; bounds; project(); CheckpointTag
│   └── mod.rs
├── projection_semantic/          # STUB — material slots; selection-set membership; layer info
│   └── mod.rs
├── projection_runtime/           # STUB — collision proxies; visibility filters; render queue input
│   └── mod.rs
├── projection_editor/            # STUB — gizmos; picking handles; debug visualisers
│   └── mod.rs
└── projection_cache/             # ProjectionCache; dirty bits; head-tracking; CacheStats
    └── mod.rs
```

The implemented modules and stub modules together form the live six-module split. `lib.rs`, `plugin_adapter.rs`, `picking.rs`, and `render_adapter.rs` are top-level concerns (orchestration / plugin shim / category-adjacent helpers) and do not count as a seventh category — they are the seams between the six categories and the outside world.

### Module Rustdoc contract (per PLAN §1.5.4.5)

Each `mod.rs` carries a module-level `//!` doc block that names:

- **What triggers me.** The mutation events this category reacts to (e.g. `projection_geometry` reacts to cad-core operator commit + tessellation cache invalidate; `projection_runtime` reacts to per-frame throttled ticks).
- **What I emit.** The ECS-side artefacts this category produces (e.g. `projection_structural` emits `BRepHandle` and `EntityCadMap` updates; `projection_cache` emits `ProjectedMeshId`s and dirty-bit transitions).
- **Failure class.** Inherited from the crate's `Failure class: snapshot-recoverable` declaration in `lib.rs` per PLAN §1.13.

### `projection-modules` architecture-lint enforcement

Per `docs/§18/ARCHITECTURE_LINTS.md` §3 — `projection-modules`:

- Implementation file: `tools/architecture-lints/src/projection_modules.rs` (308L).
- Test fixtures: `tools/architecture-lints/tests/projection_modules_test.rs` (8 integration tests).
- Rule: inside `crates/cad-projection/src/`, the `projection_structural` module may not import from `projection_runtime` or `projection_editor`.
- Empty-tree behaviour: if `crates/cad-projection/src/` does not exist, the lint emits an empty passing report (so it's no-op during early phases or in test fixtures that don't realise the crate).
- CI wiring: `cargo run -p rge-tool-architecture-lints -- all` (via `.github/workflows/architecture.yml`).

This ADR does not modify the lint, the fixtures, or the CI wiring. They are referenced here as the existing enforcement substrate.

### Interplay with `BRepHandle` SSoT (post-Pairing-6)

The `BRepHandle` SSoT refactor (2026-05-08, Pairing-6 closure per `docs/§18/CAD_PROJECTION.md` §3) keeps the entity↔cad-node FK exclusively in `EntityCadMap` (owned by `projection_structural`). This is structurally consistent with this ADR: structural is the substrate, and the bidirectional map is the canonical structural artefact. The geometric and cache modules read through `CadProjection::node_for(entity)` rather than carrying their own copy of the FK. Future projection categories that need entity↔cad-node lookup MUST go through `node_for`, not duplicate the field.

### Interplay with `SnapshotParticipate` + the `cad-projection.brep-handles` participant

Per `docs/§18/CAD_PROJECTION.md` §8 and PLAN §13.2, `CadProjection` (the top-level orchestrator in `lib.rs`) implements `SnapshotParticipate` and registers the `cad-projection.brep-handles` participant. The participant payload carries the structural state (`EntityCadMap` + `last_seen_checkpoint`); geometric / cache state re-derives on the next tick. Category boundaries are preserved across PIE snapshots: structural is captured/restored, geometric and cache are recomputed from cad-core + structural. Future stub-module fills (semantic / runtime / editor) must decide their own capture/recompute split independently.

### Interplay with `CadProjectionPlugin` (Tier-2 dogfood canary)

Per `docs/§18/CAD_PROJECTION.md` §9 and PLAN §10.4, `CadProjectionPlugin` (at `crates/cad-projection/src/plugin_adapter.rs`) is the first Tier-2 plugin canary. The plugin wraps `CadProjection` (the orchestrator) and drives `tick` through the type-erased `PluginContext`. The plugin shim is module-level (alongside `lib.rs`), not inside any one category, because it operates on the orchestrator as a whole. Future per-category plugin shims (e.g. an editor-specific gizmo plugin layered on `projection_editor`) would land in `projection_editor/plugin_adapter.rs` or similar — not a new top-level shim.

## Followups / open questions

- **Stub-module fills (semantic / runtime / editor).** Three modules are documented stubs today. Each will be filled by a future dispatch as concrete use cases arrive: `projection_semantic` when material-slot binding or selection-set membership lands as a real cross-system concern (likely alongside scene-extraction or asset-system work); `projection_runtime` when collision proxies or render-queue feeders need an ECS-side bridge (likely alongside the render-handoff dispatches that ADR-117 / ADR-118 anchor); `projection_editor` when gizmos / picking surfaces formalise their ECS substrate (likely alongside editor-actions / editor-state dispatches). No specific schedule; the stubs hold the slot until pulled.
- **Refactor to crates if growth demands.** PLAN §1.5.4.5 leaves open the escape hatch of splitting some or all of the six modules into separate crates if a module grows beyond what one crate can responsibly carry. Likely triggers: `projection_runtime` accreting a non-trivial per-frame feed graph; `projection_editor` accreting editor-only dependencies that the cook step needs to strip cleanly; or a transitive-import policy gap surfacing in production. The refactor is mechanical (mostly Cargo manifest + path adjustments); the ADR amendment would be to ratify the per-crate boundary, not to introduce a new architecture.
- **Per-node fine-grained dirty tracking.** `projection_cache` today marks every known entity dirty when the cad-core head advances (head-advanced ⇒ everything dirty). PLAN §1.5.4.5 leaves room for finer-grained "which entities depend on which cad nodes" tracking; whether to land that in `projection_cache` or in a new helper is a future dispatch's call. This ADR does not commit to the implementation strategy.
- **Seventh-category proposals.** Any future proposal to add a seventh projection category (e.g. "lineage projection", "PIE adapter as its own category", "physics-binding projection") must ship as an ADR amendment to this one. The amendment specifies: the category name; the mutation frequency / invalidation rule profile; what triggers it; what it emits; the `projection-modules` lint update (if any); the rationale for not folding it into an existing category. Per PLAN §0.6 freeze policy.
- **Tighter `projection_structural` isolation.** Today the rule blocks `projection_structural` from importing `projection_runtime` or `projection_editor` only. A stricter "structural may only depend on cad-core" rule was considered (see Alternatives above) but rejected as not the accepted PLAN shape. If a future audit shows transitive policy violations are happening through `projection_geometry` or `projection_cache`, the tighter rule can be ratified by ADR amendment + lint update.

## References

- **PLAN.md §1.5.4.5** — cad-projection internal split (the accepted decision this ADR backfills); the six-category table; the CI rules (`projection_structural` cannot import `projection_runtime` or `projection_editor`; each module documents trigger/emission; seventh category requires ADR).
- **PLAN.md §1.5.4** — cad-core scope (this ADR's substrate above which the bridge sits).
- **PLAN.md §1.6.8** — determinism modes (`BTreeMap` / `BTreeSet` convention for deterministic iteration across the projection categories).
- **PLAN.md §1.8** — forbidden-dependency rules; the rule "`projection_structural` cannot import `projection_runtime` or `projection_editor`" is enumerated alongside the rest of the Tier-1/Tier-2/Tier-3 DAG.
- **PLAN.md §0.6** — freeze policy; the six-way split is conserved (stub modules are documented stubs, not collapsed away).
- **PLAN.md §10.4** — dogfood rule; `CadProjectionPlugin` as the Tier-2 plugin canary that exercises the orchestrator.
- **PLAN.md §13.2** — `SnapshotParticipate` quality gate; the `cad-projection.brep-handles` participant rides on top of `projection_structural` state.
- **PLAN.md §1.13** — failure-class taxonomy; `cad-projection` declares `Failure class: snapshot-recoverable` at crate level, inherited by every module.
- **`docs/§18/CAD_PROJECTION.md`** — companion §18 doc; §2 module split table, §3–§7 per-module ownership, §8 `SnapshotParticipate`, §9 plugin canary, §10 failure class.
- **`docs/§18/ARCHITECTURE_LINTS.md`** — companion §18 doc; §2 nine-lint table, §3 `projection-modules` lint module-doc, §4 exemptions-registry policy.
- **ADR-098** — topology lineage substrate; referenced by the `BRepHandle` SSoT refactor design that landed inside `projection_structural`.
- **ADR-104** — capability surface; cross-links to ADR-097 from its PLAN-references row.
- **ADR-114** — `PluginContext` owned-handoff design; the substrate behind `CadProjectionPlugin`'s adapter shim.
- **`crates/cad-projection/src/lib.rs`** — top-level orchestrator + `pub mod` declarations for all six modules + the `SnapshotParticipate` impl + the failure-class declaration.
- **`crates/cad-projection/src/projection_structural/mod.rs`** — `BRepHandle` ECS component; `EntityCadMap` bidirectional map; `EntityCadMapError`.
- **`crates/cad-projection/src/projection_geometry/mod.rs`** — `ProjectedMesh`; `ProjectedMeshId`; `CheckpointTag` proxy; the `project()` function; `ProjectionError`.
- **`crates/cad-projection/src/projection_semantic/mod.rs`** — stub module (material slots, selection sets).
- **`crates/cad-projection/src/projection_runtime/mod.rs`** — stub module (collision proxies, render-queue feeders).
- **`crates/cad-projection/src/projection_editor/mod.rs`** — stub module (gizmos, picking).
- **`crates/cad-projection/src/projection_cache/mod.rs`** — `ProjectionCache`; dirty-bit set; `CacheStats`.
- **`tools/architecture-lints/src/projection_modules.rs`** — the `projection-modules` lint implementation that enforces the structural-isolation rule.
- **`tools/architecture-lints/tests/projection_modules_test.rs`** — eight integration tests covering the lint's fixture-based behaviour.
