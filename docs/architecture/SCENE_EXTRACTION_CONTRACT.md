# SCENE_EXTRACTION_CONTRACT

| Companion to | PLAN.md §1.5.2 (render-side snapshot staging; Phase 6 sim/render-thread split anticipated), §1.5.4.5 (cad-projection internal split), §6.13 (PIE), §13.2 (cross-architecture coherence gate); ADR-114 (PluginContext owned-handoff for the Tier-2 plugin canary substrate); future Phase 6+ frame-graph + render-snapshot ADR (anticipated when separation lands) |
|---|---|
| Status | Doctrine-tier v0; binding architectural rule. Today's substrate realises the contract through cad-projection's free `project()` function + `gfx::GfxPlugin` canary; the future Phase 6 frame-graph + sim/render-thread split per §1.5.2 will land the formal `gfx.render-snapshot` participant alongside this doctrine. |
| Audience | Subsystem authors landing scene-extraction code (cad-projection-style or future render-snapshot-style); reviewers verifying that a proposed extractor satisfies the four extraction principles + four contract invariants; future Phase 6 frame-graph implementers binding their work to the Layer-3 `ProjectedMesh` substrate |
| Sibling docs | `docs/§18/CAD_PROJECTION.md` (the canonical extractor today), `docs/§18/GFX_RENDER_TIER.md` (the canonical consumer today), `docs/§18/CAD_CORE_MODEL.md` (the authoritative Layer-1 source the extractor reads), `docs/§18/PIE_SNAPSHOT.md` (the cross-substrate boundary the contract crosses), `docs/architecture/REACTIVE_INVALIDATION.md` (sibling doctrine doc; the invalidation rules that govern WHEN extraction runs; this doc governs WHAT extraction produces and WHO owns it) |
| Reference impls | `crates/cad-projection/src/projection_geometry/mod.rs::project` (the free pure-function extractor) · `crates/cad-projection/src/projection_geometry/mod.rs::ProjectedMesh` (the wire shape extracted from `cad_core::Tessellation`) · `crates/cad-projection/src/lib.rs::CadProjection::tick` (the orchestration entry-point that drives `project()` for every dirty entity) · `crates/gfx/src/lib.rs` + `crates/gfx/src/plugin_adapter.rs::GfxPlugin` (the downstream consumer that records frames; canary today, future render-snapshot participant) · `crates/cad-projection/tests/cross_substrate_determinism.rs::pie_three_participant_round_trip_50_iter` (the cross-substrate determinism test that pins the contract empirically) |

> *Doctrine-tier doc — binding architectural rule, not a substrate reference. The §18 docs above describe the substrate; this doc describes the rule the substrate is required to obey for any current or future extraction path.*

## 1. Purpose

The cross-review #2 2026-05-10 framing: "this separation matters enormously later." Without an explicit contract on which subsystem owns geometry vs which consumes it, RGE risks "renderer drift" — the renderer becoming the de-facto source of truth for scene state because it's the most-touched code path during interactive editing, while the CAD operator pipeline's truth diverges silently from what the user sees on screen.

The contract this doc fixes: **the CAD-graph + topology layers are authoritative; the projection layer extracts a derived view; the renderer consumes that view downstream-only.** No path lets the renderer mutate geometry. No path lets the projection layer become a CAD-graph alternate. No path lets a "convenient" cache become an authoritative shadow.

The contract is binding ahead of full implementation. Today's substrate realises Layer 3 (cad-projection) cleanly; Layer 4 (gfx render-snapshot) is single-threaded headless rendering with the formal participant deferred to Phase 6 per `docs/§18/GFX_RENDER_TIER.md` §1. The doctrine fixes the shape so when Phase 6 lands, it slots into a predetermined position rather than reinventing the boundary.

## 2. The canonical pipeline

```
CAD Graph (cad-core)              authoritative
    |                             OperatorGraph + CheckpointHistory + topo_lineage
    | mutation drives upstream-hash recomputation per
    | effective_hash_and_label (REACTIVE_INVALIDATION.md Layer 1)
    v
Topology Layer                    authoritative lineage
                                  cad-core::topo_lineage (ADR-098)
                                  TopologyEvolution edges per commit
    |
    | semantic-continuity preserved across rebuild
    v
Geometry Evaluation               cad-core::OperatorGraph::evaluate
                                  recursive; cycle-guarded; produces
                                  Arc<Tessellation> via TessellationCache
    |
    | (effective_hash, tolerance) keyed memoization
    v
Tessellation                      cad-core::TessellationCache
                                  per-entity Arc<ProjectedMesh> in
                                  cad-projection::ProjectionCache
    |
    | derived view; pure function of (graph state, checkpoint, tolerance)
    v
Scene Extraction                  cad-projection::projection_geometry::project
                                  free function; pure; deterministic; replaceable
    |
    | Arc<ProjectedMesh> into ECS-side surface
    v
GPU Resources                     gfx::GfxContext + GfxPlugin canary records frames
                                  (Phase 6+ future: gfx.render-snapshot
                                  SnapshotParticipate participant for sim/render
                                  thread boundary)
```

Every arrow is unidirectional. The CAD graph never reads the GPU; the GPU never writes the CAD graph. The contract is the discipline that keeps the arrows pointing in only one direction.

## 3. Ownership rules

The authoritative-vs-derived split is the contract's load-bearing decision. Each row of the table is binding.

### 3.1 Authoritative

| Subsystem | Owns | Failure-class |
|---|---|---|
| `cad-core::CadGraph` | semantic operator graph (DAG of `OperatorNode`); persistent topo IDs; constraints; operation history per `CheckpointHistory`; `begin_operation`/`commit`/`rollback`/`restore_to` transactional bracket | `snapshot-recoverable` |
| `cad-core::topo_lineage` | lineage edges per ADR-098; `TopologyEvolution` enum (`Preserved`/`Split`/`Merged`/`Deleted`/`Reinterpreted`); semantic-continuity scoring (deferred per ADR-098 v0 simplifications) | `snapshot-recoverable` (inherits cad-core's class) |
| `cad-core::CheckpointHistory` | revision system; `CheckpointId(u64)` monotonic; `BTreeMap<CheckpointId, Checkpoint>` deterministic iteration; in-progress transaction bookkeeping | `snapshot-recoverable` |
| `kernel/audit-ledger::AuditLedger` | `Event` projection of every `Action` and `CadCheckpoint` mutation; replay-stable per PLAN §1.6.8 | `kernel-fatal` (checksum-fail per PLAN §1.13 line 573) |

These subsystems own state that, if lost, cannot be recovered from anything else in the system. They are the bottom of the dependency stack.

### 3.2 Derived

| Subsystem | Owns | Failure-class |
|---|---|---|
| `cad-core::TessellationCache` | per-`(structural_hash, tolerance)` `Arc<Tessellation>` memoization; `HashMap`-backed (determinism-via-recompute, not iteration); hit/miss counters | `snapshot-recoverable` (declared on cad-core lib.rs) |
| `cad-projection::ProjectionCache` | per-entity `Arc<ProjectedMesh>`; `BTreeMap`-backed (deterministic iteration); dirty-bit set; `last_seen_checkpoint`; `next_mesh_id` monotonic | `snapshot-recoverable` |
| `gfx` GPU buffers | `wgpu::Device` + pipeline-internal buffers / textures / pipelines; non-Send-serializable wgpu resource state | `recoverable` (declared on gfx lib.rs) |
| `editor-ui` viewport overlays | gizmo bindings, picking surfaces, selection highlights — pending; future `projection_editor` module home | `recoverable` (declared on editor-state lib.rs for the coordination state side) |

Derived subsystems are reactive — they re-derive from authoritative state on the next tick. Loss of a derived state is recoverable: the subsystem rebuilds itself from upstream. This classification is the same one `docs/architecture/REACTIVE_INVALIDATION.md` §5.3 enforces; this doc's contract makes the consequences for ownership explicit.

### 3.3 The renderer NEVER owns authoritative geometry

`crates/gfx` is a downstream consumer. The `forbidden-dep` architecture lint blocks `cad-core` imports from gfx; gfx may not call `CadGraph::commit`, may not write to the operator graph, may not invent its own geometry source. Today's `GfxPlugin` canary takes `GfxContext` + `HeadlessTarget` from `PluginContext` and records frames per `docs/§18/GFX_RENDER_TIER.md`; the future `gfx.render-snapshot` participant will consume `Arc<ProjectedMesh>` allocations through the same boundary.

The contract is enforced at three layers: (a) the architecture-lint stops the dependency import; (b) the doctrine here documents the rule for design review; (c) the §13.2 cross-architecture coherence quality gate verifies cross-substrate composition empirically. The three layers compose to a hard wall.

## 4. Extraction principles

Every scene-extraction code path — `cad-projection::project` today, the future `gfx.render-snapshot` capture path tomorrow, any further extractor that surfaces — MUST satisfy four principles.

### 4.1 Pure

The extraction is a pure function of (graph state, checkpoint, tolerance). No hidden mutation; no side channel; no environment lookup. Same inputs → same output, byte-for-byte.

Source-truth: `crates/cad-projection/src/projection_geometry/mod.rs::project(cad: &CadGraph, node: NodeId, cache: &mut TessellationCache, tolerance: Tolerance) -> Result<Arc<ProjectedMesh>, ProjectionError>` is a free function — not a method on a struct that could carry hidden state. The `cad-core` reference is `&CadGraph` (read-only); the `cache` is `&mut TessellationCache` (memoization side-effect only — value is recomputable on miss). Determinism is preserved.

### 4.2 Incremental

The extraction may rely on memoization, but the memoization key MUST fully encode the inputs. No "the cache happens to have this value because we touched it earlier" — the cache contents must be reconstructible from the input stream alone.

Concrete substrate: `TessellationCache::CacheKey { structural_hash, tolerance }` is the entire input space. Identical `(structural_hash, tolerance)` pairs MUST produce identical `Tessellation` outputs across runs. The `tolerance_equality_under_epsilon` test in `crates/cad-core/src/tessellation/cache.rs` pins the property at the f32 quantization boundary.

### 4.3 Deterministic

Two replays of the same mutation sequence MUST produce byte-identical `ProjectedMesh` allocations at every observation point. The `cross_substrate_determinism::pie_three_participant_round_trip_50_iter` integration test pins this empirically across 50 iterations, with `cad-core.cad-graph` + `cad-projection.brep-handles` + `physics.rapier-rigid-bodies` composed into a single envelope.

The doctrine: byte-identity of the extracted `Arc<ProjectedMesh>` is the proof that the extraction respects determinism. If the proof fails, either the upstream (cad-graph) is non-deterministic OR the extractor introduced state — both are bugs.

### 4.4 Replaceable

The extraction code path MUST be swappable without touching the upstream substrate or the downstream consumer. Today's `project()` free function is one such implementation; a future zero-copy variant that borrows from `Arc<Tessellation>` directly (rather than copying buffers per `crates/cad-projection/src/projection_geometry/mod.rs:170-176`) is a candidate replacement. Neither shape touches `cad-core` or `gfx`; both produce `Arc<ProjectedMesh>` shaped identically.

The principle is what makes the extraction a CONTRACT rather than an implementation detail. The implementation may evolve; the contract is fixed.

## 5. Critical contract invariants

Four load-bearing invariants. Each is enforced today by some combination of substrate shape + integration test + design review; ADR-115 phase-4 will add an architecture lint that mechanically verifies the first.

### 5.1 Mesh caches are DISPOSABLE

A reactive mesh cache MUST be re-derivable from `(cad_graph, checkpoint)`. Throwing the cache away and re-running every dirty entity through `project()` MUST produce byte-identical output to the cache contents.

Concrete substrate: the `ProjectionCache::clear_meshes` method (`crates/cad-projection/src/projection_cache/mod.rs:202`) drops every `Arc<ProjectedMesh>` while preserving stats and `last_seen_checkpoint`. The next tick re-projects every entity from upstream. The PIE round-trip is the canonical case: the wire format deliberately drops `next_mesh_id` and the cached meshes themselves per `docs/§18/CAD_PROJECTION.md` §8.

The contract makes "throw the cache away" a safe operation, not a bug. Garbage collection of cold meshes; eviction under memory pressure; a future LRU policy — all are valid because the cache is disposable.

### 5.2 Topology remains CANONICAL

The topology lineage (`cad-core::topo_lineage`) is NEVER reconstructed from the mesh. The mesh is an output of the topology; trying to reverse-engineer topology from mesh vertices/indices is the kind of "renderer becomes source of truth" failure mode the contract exists to prevent.

Concrete substrate: `Tessellation::face_labels: Option<Vec<TopologyFaceId>>` per ADR-098 is the only mesh-side topology-tag — and it is OPTIONAL because it is INHERITED from upstream operators, not derived from mesh geometry. If `face_labels` is `None`, the mesh has no topology identity; the lineage substrate is the only authority.

### 5.3 Extraction is a PURE FUNCTION of (graph state, checkpoint, tolerance)

No hidden mutation, no environment lookup, no nondeterministic side-channel. The function signature is the contract:

```rust
pub fn project(
    cad: &CadGraph,
    node: NodeId,
    cache: &mut TessellationCache,
    tolerance: Tolerance,
) -> Result<Arc<ProjectedMesh>, ProjectionError>;
```

`&CadGraph` is read-only. `&mut TessellationCache` is the memoization side-effect — the cache may insert; it may not mutate values that were already there. `tolerance` is `Copy`. `node` is a `Copy` `NodeId`. The output is `Arc<Result<...>>`.

The one allowed side-effect class — cache insertion — is recoverable: throwing the cache away rebuilds it. Anything beyond cache insertion is a contract violation surfaced at design review.

### 5.4 GPU resources have NO PIE participation

Per audit-3 H3 audit-and-discriminate (closed 2026-05-09; rationale in `docs/§18/PIE_SNAPSHOT.md` §11.1), `gfx` is REMOVED from `STATEFUL_TIER2_CRATES` per `tools/architecture-lints/src/snapshot_participate.rs`. The reasoning is doctrine: GPU buffers / pipelines / textures are non-Send-serializable AND reproducible from upstream scene state on next render. The class taxonomy classifies them as `recoverable` (declared on `crates/gfx/src/lib.rs`), not `snapshot-recoverable`.

The future `gfx.render-snapshot` participant will land alongside Phase 6 frame-graph + render-snapshot separation (§1.5.2 sim/render-thread split). When it lands, its payload is "render-side state replicated across the sim/render thread boundary" per `docs/§18/GFX_RENDER_TIER.md` §11.2 — likely camera + light + material handles + mesh handles, all anchored to cad-projection's `ProjectedMeshId` so cross-architecture coherence holds. The participant explicitly does NOT serialize wgpu device/queue/pipelines themselves; those are recoverable per the existing classification.

The contract therefore distinguishes two kinds of "render state":

- **Wire-format render state** (anchored to `ProjectedMeshId` / `MaterialId` / `CameraId`; deterministic; PIE-participating when Phase 6 lands).
- **GPU-resource render state** (non-Send wgpu handles; recoverable; never PIE-participating).

The first is the future participant's payload; the second is rebuilt on every render. The contract guarantees the second is always cheap-enough to rebuild that crashing the GPU subsystem is recoverable, not session-fatal.

## 6. Today vs anticipated

### 6.1 Today (Phase 7.3 + Phase 6.1 PBR-lite)

- **Single-threaded headless rendering.** No §1.5.2 sim/render-thread split. The `GfxPlugin` canary records frames synchronously per `crates/gfx/src/plugin_adapter.rs`.
- **Pull-based extraction.** `CadProjection::tick` runs on demand from the orchestrator (typically once per editor frame); pulls dirty entities through `project()`.
- **`Arc<ProjectedMesh>` is the wire shape.** Stored in `ProjectionCache::meshes: BTreeMap<ProjectedMeshId, Arc<ProjectedMesh>>`. Multiple readers share the allocation cheaply.
- **Buffer copy.** `project()` copies positions and indices from the upstream `Arc<Tessellation>` into a fresh `ProjectedMesh`. Per `crates/cad-projection/src/projection_geometry/mod.rs:170-176`: "correctness-first; a future optimization can have ProjectedMesh borrow from Arc<Tessellation> directly."
- **`gfx` consumes through `PluginContext`.** Per ADR-114 owned-handoff: the canary takes `GfxContext` + `HeadlessTarget` from the registry on tick, records, and puts everything back. There is NO direct gfx ↔ cad-projection coupling today; the coupling is mediated by `PluginContext`.

### 6.2 Anticipated (Phase 6+ frame-graph + render-snapshot separation)

- **Sim/render-thread split per §1.5.2.** Render thread sees an immutable snapshot of `(ECS_tick_N, CadCheckpointId_N)` while sim builds N+1.
- **`gfx.render-snapshot` participant.** New `SnapshotParticipate` impl on `gfx`; participant id `gfx.render-snapshot`. Payload anchored to `ProjectedMeshId` + `MaterialId` + `CameraId`; `wgpu` handles excluded.
- **Frame graph.** Transient resource lifetimes computed at frame begin; `TexturePool` / `BufferPool` keyed on frame index; declarative pass DAG with read/write resource declarations.
- **Render-snapshot wire format ADR.** A future ADR formalises the on-the-wire shape when Phase 6 lands. Today's doctrine pre-binds the participant's class (`snapshot-recoverable` for the wire-format part, `recoverable` for the GPU-resource part) and its anchor convention (cad-projection `ProjectedMeshId` for cross-architecture coherence).
- **Material-runtime + PSO cache.** Pipeline state objects keyed on `(shader_hash, vertex_layout)` so 100 material instances of the same shader share one PSO. `MaterialId` becomes the wire-format anchor for material state.

The doctrine binds the future shape to the present contract. Phase 6 implementers are bound by §3 (ownership rules), §4 (extraction principles), §5 (contract invariants); they do NOT need to re-derive the boundary at implementation time.

### 6.3 Migration discipline when Phase 6 lands

The transition from today's substrate to Phase 6 is bounded by the contract. Three concrete points:

- **`project()` stays.** The free function is `Replaceable` per §4.4 — Phase 6 may add a parallel `extract_render_snapshot()` path on top of it, or replace its internals with a borrowed-from-Arc variant, but the function's signature + post-conditions are stable.
- **`ProjectedMesh` is the wire shape.** The struct's fields (positions / indices / source_node / source_checkpoint) are part of the contract. Phase 6 may add new fields behind feature flags but MUST NOT remove or rename existing ones; downstream consumers (gfx, future material-runtime) bind against the existing shape.
- **The `STATEFUL_TIER2_CRATES` lint adds gfx.** When Phase 6 lands the formal `gfx.render-snapshot` participant, `tools/architecture-lints/src/snapshot_participate.rs` adds `gfx` back to the list. The audit-3 H3 audit-and-discriminate test `stateful_tier2_list_does_not_contain_audited_removals` — currently asserting gfx is NOT in the list — flips its assertion. That test is the migration trip-wire: nothing about Phase 6 lands silently.

### 6.4 Reviewer checklist for new extraction paths

When a future subsystem proposes a new extraction code path (e.g. a `projection_runtime` collision-proxy extractor or a `projection_editor` gizmo extractor), the doctrine's reviewer checklist:

1. **Identify the upstream authority.** What `cad-core` or `cad-projection` substrate does this extractor read FROM? If the answer is "nothing" or "the renderer", the extractor is misclassified.
2. **Confirm the function signature is pure.** Does it take immutable references to upstream + a memoization handle? Any `&mut` to authoritative state is a contract violation.
3. **Confirm the output shape is replaceable.** Is the returned type `Arc<T>` for some `T` whose fields are stable? If so, future implementations can swap internal mechanics freely.
4. **Confirm the failure-class is correct.** Per `docs/§18/RECOVERY_MODEL.md`, the extractor's worst-case failure must be `recoverable` or `snapshot-recoverable`, never `session-fatal`.
5. **Specify the test surface.** A pure-function unit test (same inputs → same output), a determinism integration test (50-iter byte-identity), and a PIE round-trip test (extract → capture → restore → extract → byte-identity) are the canonical three.

## 7. Why the contract matters

Cross-review #2 framing: "this separation matters enormously later." Three failure modes the contract prevents:

- **Renderer drift.** Without the contract, the renderer accumulates "convenience caches" of geometry that gradually become the de-facto source of truth. Six months later, the CAD operator pipeline produces output that disagrees with what the user sees on screen. The contract forecloses this by making the dependency direction part of the architecture lint (`forbidden-dep`) and the design-review doctrine (this doc's §3.3).
- **Authoritative cache corruption.** If a downstream cache somehow accumulates authoritative bits, a corruption bug in the cache becomes a session-fatal data loss. The contract's classification rule (§5.1, §5.4) requires every cache to be `recoverable` or `snapshot-recoverable`, never `session-fatal`. Corruption in a derived cache is recoverable by definition because the upstream re-derives it.
- **Distributed CAD divergence.** Per PLAN §6.17, authoritative-server CAD broadcasts operator-graph deltas; clients rebuild locally. If clients' "extraction" pipelines could mutate authoritative state, two clients would diverge in their representation of the same operator graph. The contract guarantees the extraction is a pure function — same authoritative state → same extracted output across clients — which is the prerequisite for the §6.17 reconciliation model.

The contract is what lets the moat (PLAN §0.2) be "deterministic reactive CAD runtime" rather than "another tool that mostly works on a single workstation".

## 8. Source / spec inconsistencies

Mirroring the §18-pack honesty discipline.

- **`ProjectedMesh` borrow vs copy**: source-truth at `crates/cad-projection/src/projection_geometry/mod.rs:170-176` copies positions and indices out of the upstream `Arc<Tessellation>`. The crate's module doc explicitly flags this as "correctness-first; a future optimization can have ProjectedMesh borrow from Arc<Tessellation> directly." The contract's §4.4 (`Replaceable`) anticipates this: a future zero-copy implementation produces the same `ProjectedMesh` shape via a different internal path; downstream consumers see no API change.
- **Free function vs method**: the dispatch spec referenced "free `project()` fn" — confirmed by source at `crates/cad-projection/src/projection_geometry/mod.rs:159`. It is genuinely a free function, not a method on `CadProjection`. The orchestrator's `tick` method calls it for every dirty entity; downstream callers can equally use it directly without going through the orchestrator. This realises §4.4 (`Replaceable`) at the type level.
- **`gfx` PIE participation**: the dispatch spec referenced "audit-3 H3 audit-and-discriminate (gfx removed from STATEFUL_TIER2_CRATES per RECOVERY_MODEL.md / EXECUTION_DOMAINS.md / GFX_RENDER_TIER.md §11)". Source-truth at `tools/architecture-lints/src/snapshot_participate.rs::STATEFUL_TIER2_CRATES` is `{cad-core, cad-projection, particles, physics, sculpt}` — gfx confirmed absent. The doctrine ratifies the audit: today's gfx state is recoverable; the future `gfx.render-snapshot` participant lands when Phase 6 brings the wire-format-vs-GPU-resource split.
- **`projection_runtime` / `projection_editor` / `projection_semantic` stubs**: source-truth at `docs/§18/CAD_PROJECTION.md` §2 confirms three of six modules are stub-only today. The contract here applies to all six modules equally — when `projection_runtime` (collision proxies / render-queue feeders) and `projection_editor` (gizmos / picking handles) gain real implementations, they MUST satisfy the same four extraction principles + four contract invariants.

## 9. Cross-references

- **PLAN.md §0.2** — moat (deterministic reactive CAD runtime).
- **PLAN.md §1.5.2** — render-side snapshot staging; sim/render-thread split.
- **PLAN.md §1.5.4** — CAD transactional core (the authoritative source).
- **PLAN.md §1.5.4.5** — cad-projection internal split.
- **PLAN.md §6.13** — PIE; the cross-substrate boundary the contract crosses.
- **PLAN.md §6.14** — subsystem integration map (CAD → projection → render flow).
- **PLAN.md §6.17** — authoritative CAD serialization (distributed reconciliation depends on the contract).
- **PLAN.md §13.2** — cross-architecture coherence quality gate.
- **PLAN.md §13.6** — B-Rep / topology / cad-core gates (render-side snapshot tested).
- **ADR-097** — cad-projection internal split.
- **ADR-098** — topology lineage substrate (the canonical authority for §3.1).
- **ADR-114** — PluginContext owned-handoff (the substrate the gfx canary uses to reach the GPU side).
- **ADR-115** — graph-metrics substrate design; sub-decision 4 anticipates the future cross-layer event stream that will surface extraction transitions as typed events.
- **`docs/§18/CAD_PROJECTION.md`** — canonical extractor reference impl.
- **`docs/§18/CAD_CORE_MODEL.md`** — authoritative source reference.
- **`docs/§18/CAD_TOPOLOGY_LINEAGE.md`** — Layer-2 authority that survives Layer-1 hash changes.
- **`docs/§18/GFX_RENDER_TIER.md`** §1, §11.2 — Phase 6 render-snapshot deferral framing.
- **`docs/§18/PIE_SNAPSHOT.md`** — the cross-substrate envelope; §11.1 audit-removals table.
- **`docs/§18/RECOVERY_MODEL.md`** — failure-class taxonomy (recoverable vs snapshot-recoverable cache classification).
- **`docs/architecture/REACTIVE_INVALIDATION.md`** — sibling doctrine doc; reactive invalidation rules govern WHEN extraction runs, this doc governs WHAT it produces.
- **`crates/cad-projection/src/projection_geometry/mod.rs`** — `project()` free function + `ProjectedMesh` + `ProjectionError`.
- **`crates/cad-projection/src/lib.rs`** — `CadProjection::tick` orchestrator; `SnapshotParticipate` impl.
- **`crates/gfx/src/lib.rs`** — gfx substrate module map.
- **`crates/gfx/src/plugin_adapter.rs`** — `GfxPlugin` canary (the future render-snapshot participant's substrate today).
- **`crates/cad-projection/tests/cross_substrate_determinism.rs`** — the empirical pin on the contract.
- **`tools/architecture-lints/src/snapshot_participate.rs`** — `STATEFUL_TIER2_CRATES` heuristic; the §3.3 enforcement layer.
- **2026-05-10 ChatGPT cross-review #1 + #2 archives in `change.md`** — the architectural design pressure that motivated this doctrine doc.
