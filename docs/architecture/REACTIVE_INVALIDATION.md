# REACTIVE_INVALIDATION

| Companion to | PLAN.md §1.6 (cad-projection invalidation), §1.5.4.3 (topology lineage), §1.5.4.5 (cad-projection split), §13.2 (cross-architecture coherence gate); ADR-115 §"Sub-decision 4" (event-sourced `GraphEvent` enum); cad-core `CheckpointHistory` + `OperatorGraph::effective_hash_and_label` substrate |
|---|---|
| Status | Doctrine-tier v0; binding architectural rule. Substrate listed today is partial (cross-review #2 2026-05-10 elevated the gap) — Layers 1–3 are implemented; Layer 4 plus the cross-layer event stream (ADR-115 phase-4) are anticipated future work. |
| Audience | Subsystem authors landing reactive caches (cad-projection-style) or cross-substrate invalidation hooks; reviewers deciding whether a proposed dirty-tracking design follows the four-invariant rule; future ADR-115 phase-1 implementers wiring `GraphEvent` into `kernel/graph-foundation::Graph<N, E>` |
| Sibling docs | `docs/§18/CAD_PROJECTION.md` (the canonical Layer-3 consumer), `docs/§18/CAD_CORE_MODEL.md` (the Layer-1 hash recursion + Layer-2 lineage ground truth), `docs/§18/GRAPH_FOUNDATION.md` (the `Invalidation` BFS router + adjacency caches), `docs/§18/PIE_SNAPSHOT.md` (the cross-substrate participant boundary) |
| Reference impls | `crates/cad-projection/src/projection_cache/mod.rs::ProjectionCache::observe_checkpoint` (the head-advance dirty trigger) · `crates/cad-core/src/graph/operator_graph.rs::effective_hash_and_label` (the recursive hash that propagates label-bitmap upward) · `crates/cad-core/src/tessellation/cache.rs::CacheKey` (the `(structural_hash, tolerance)` memoization key) · `crates/cad-core/src/checkpoints/mod.rs::CadGraph` (the `begin_operation` / `commit` / `rollback` / `restore_to` substrate) · `kernel/graph-foundation/src/invalidation.rs::Invalidation` (the BFS dirty-bit router with `dependents_of` closure) |

> *Doctrine-tier doc — binding architectural rule, not a substrate reference. The §18 docs above describe the substrate; this doc describes the rule the substrate is required to obey across all current and future reactive layers.*

## 1. Purpose

A deterministic reactive runtime is the moat (PLAN §0.2): the engine recomputes only what is downstream of an actual change, byte-identically across runs, and surfaces every recomputation point as an observable event so editor overlays, distributed CAD synchronisation, and probabilistic-recovery diagnostics can subscribe.

Reactive invalidation is the **shape that this moat takes** in code. Without an explicit doctrine, eight subsystems would each invent their own dirty-bit recipe — some polling on tick, some propagating eagerly through hidden hooks, some implicit through "rebuild everything every frame". The result would be (a) replay non-determinism (poll timing is wall-clock dependent), (b) cross-layer drift (Layer-3 thinks Layer-2 advanced when it didn't), (c) editor responsiveness collapse during interactive editing of medium-complexity scenes.

This doc fixes the shape. The substrate is partial today (Layers 1–3 implemented; Layer 4 anticipated; the unified cross-layer event stream lands with ADR-115 phase-4); the doctrine is what every layer's design MUST satisfy regardless of when it lands.

The cross-review #2 2026-05-10 framing: "Do NOT compute metrics ad hoc from full graph scans. That becomes catastrophic later. You want incremental metrics / deterministic accumulation / event-driven updates / stable snapshot semantics. Otherwise rebuild latency explodes / reactive pipelines stall / editor responsiveness dies / distributed synchronization becomes difficult." The same warning applies one level deeper to invalidation itself: ad-hoc full-graph scans on the invalidation hot path produce the same failure mode. The four-invariant rule (§4) closes the warning into structural impossibility — every reactive layer's invalidation path is required to be explicit / revision-aware / observable / deterministic, which forecloses the full-scan shape.

## 2. The four-layer invalidation hierarchy

Reactive recomputation in RGE is organised into four layers, ordered by mutation-rate ascending and authority descending. Each layer's invalidation drives the next; lower layers MUST NOT invalidate upper layers (§4 invariant `no-inversion`).

### Layer 1 — Graph mutations (operator changes / constraint changes / parameter edits)

The authoritative origin of every reactive ripple. The `cad-core::OperatorGraph` is mutated inside a `CadGraph::begin_operation` / `commit` bracket per PLAN §1.5.4 and `docs/§18/CAD_CORE_MODEL.md`; mutation outside an open transaction is a hard-error (`CheckpointError::MutationOutsideOperation`). Each commit advances `CheckpointHistory::head` to a new `CheckpointId` and captures a new `GraphSnapshot` per `kernel/graph-foundation::Graph<N, E>` substrate.

The recompute trigger is `OperatorGraph::effective_hash_and_label`: a recursive BLAKE3 over the operator's local `structural_hash` + per-port `(port_index, upstream_hash)` + the upstream-labeled-bitmap. Any change at any upstream node produces a different effective hash at every dependent node — the hash recursion IS the change-propagation mechanism. The recursion is read-only (non-mutating) and cycle-guarded by an ancestor-set `HashSet<NodeId>` per `effective_hash_and_label_inner`.

### Layer 2 — Topology evolution (lineage updates / semantic identity shifts)

Hash-based persistent identity is unstable under boolean reorder, tolerance healing, edge merge/split, kernel switching. ADR-098 + `docs/§18/CAD_TOPOLOGY_LINEAGE.md` introduce the `TopologyEvolution` enum (`Preserved` / `Split` / `Merged` / `Deleted` / `Reinterpreted`) so the operator graph mutation produces an explicit lineage edge alongside its hash advance. The lineage advance is what makes constraints / replication / undo-visualization survive Layer-1 hash changes that are NOT identity-preserving.

Layer 2 is downstream of Layer 1 in the strict sense that the lineage edge is emitted only inside the `CadGraph::commit` that closed Layer 1's transaction. The ordering is enforced by the substrate: there is no Layer-2 path that bypasses Layer-1's transaction bracket.

### Layer 3 — Tessellation rebuilds (geometry regeneration / mesh extraction)

The `cad-core::TessellationCache` per `crates/cad-core/src/tessellation/cache.rs` memoizes `Tessellation` results keyed on `CacheKey { structural_hash, tolerance }`. The `structural_hash` field receives Layer-1's `effective_hash_and_label` output, so a Layer-1 advance at any upstream node produces a cache miss at every dependent node automatically — the "any change upstream causes a cache miss at every dependent node" property is realized by the hash recursion, not by an explicit invalidation walk.

Above the tessellation cache lives the projection layer: `crates/cad-projection/src/projection_cache/mod.rs::ProjectionCache::observe_checkpoint(head, all_entities)`. When the cad-graph head advances (i.e. Layer-1 committed), every entity in the projection's known-entity set is marked dirty and the next `CadProjection::tick` re-projects them per `docs/§18/CAD_PROJECTION.md` §6 and §7. The granularity is coarse-by-design today (head-advanced ⇒ everything dirty); per-node fine-grained dependency tracking is documented as deferred future-dispatch work.

### Layer 4 — GPU uploads (viewport redraws / render synchronization)

The `crates/gfx` substrate is single-threaded headless rendering today per `docs/§18/GFX_RENDER_TIER.md` §1; the `gfx.render-snapshot` `SnapshotParticipate` participant is anticipated alongside the Phase 6 frame-graph + render-snapshot separation per PLAN §1.5.2. The doctrine fixes the shape ahead of implementation: gfx render state is downstream-only, reproducible from the Layer-3 `ProjectedMesh`, never authoritative for geometry.

When Layer 4 lands, its invalidation trigger is the Layer-3 `ProjectionCache` reporting which `Arc<ProjectedMesh>` allocations changed since the prior render-tick — the GPU upload path consumes the projection layer's output, applies sim/render-thread snapshot semantics, and produces a frame. The doctrine forbids any path where Layer-4 mutates Layer-1, Layer-2, or Layer-3 state (§4 invariant `no-inversion`).

The audit-3 H3 SnapshotParticipate audit + discriminate (closed 2026-05-09 per HANDOFF.md) explicitly removed `gfx` from `STATEFUL_TIER2_CRATES` precisely because today's gfx state is GPU resource state (wgpu device / queue / pipelines / buffers) — non-Send-serializable AND reproducible from upstream scene state on next render. The participant lands ALONGSIDE the future frame-graph + render-snapshot separation, NOT before. Until then, gfx is genuinely stateless from the PIE perspective: capture replays from cad-graph, restores via cad-projection re-tick, and gfx rebuilds its GPU-side state from the resulting `ProjectedMesh`es on the next frame. The doctrine here ratifies that decision and binds the Phase 6+ implementation to consume Layer-3 output rather than fork an independent geometry source.

## 3. Pipeline diagram

```
Layer 1 (Graph)           cad-core::OperatorGraph
                          + CadGraph::commit advances CheckpointHistory::head
                              |
                              | effective_hash_and_label recursion
                              v
Layer 2 (Topology)        cad-core::topo_lineage emits TopologyEvolution edges
                          (Preserved / Split / Merged / Deleted / Reinterpreted)
                              |
                              | head-advance signal (Layer-3 observes via tick)
                              v
Layer 3 (Tessellation)    cad-core::TessellationCache miss on (effective_hash, tolerance)
                          ProjectionCache::observe_checkpoint marks dirty entities
                          CadProjection::tick re-projects via projection_geometry::project
                              |
                              | Arc<ProjectedMesh> change (Phase 6+ render-snapshot)
                              v
Layer 4 (GPU)             gfx pipeline-internal (today: GfxPlugin canary records frames;
                          future: gfx.render-snapshot SnapshotParticipate participant)
```

Read top-to-bottom: each layer's invalidation drives the next. Read bottom-to-top: forbidden — `no-inversion` invariant.

Each arrow is a substrate boundary that obeys the four invariants: explicit edges (the dependency arrow is declared in code, not inferred); revision-aware (the receiver compares against its last-seen revision); observable (the transition is or will be a typed event); deterministic (BTreeMap-backed iteration on the receiving side; recomputable inputs on the sending side).

### Cross-substrate boundary case — PIE round-trip

The PIE round-trip is the load-bearing test of the doctrine because it exercises every invariant simultaneously:

```
capture:    Layer 1 (cad-core.cad-graph)         -> RGEP envelope payload
            Layer 3 (cad-projection.brep-handles) -> RGEP envelope payload
            (Layers 2 and 4 are reactive-derived; not captured)
            
to_bytes -> from_bytes (deterministic envelope ordering by ParticipantId)

restore:    Layer 1 (cad-core.cad-graph)         <- payload (authoritative)
            Layer 3 (cad-projection.brep-handles) <- payload (entity↔node map only)
                                                     + clean-slate cache
                                                     + mark every entity dirty
            next tick:
              Layer 1 head observed
              Layer 3 re-projects every entity from Layer 1 + Layer 2
              Layer 2 lineage edges re-emerge from Layer 1 commit history
              Layer 4 (when implemented) re-derives from Layer 3 ProjectedMesh
```

`cross_substrate_determinism::pie_three_participant_round_trip_50_iter` pins this shape across 50 iterations including a third participant (`physics.rapier-rigid-bodies`). The byte-identity of the captured envelope across runs is the proof that the four invariants compose correctly.

## 4. The four invariants

Every invalidation path in RGE — current substrate or future ADR-115 phase-4 event stream — MUST satisfy all four. They are load-bearing for the moat.

### 4.1 Explicit

No implicit dependency rewiring. The dependency edges that drive recomputation MUST be the same edges that the consumer reads — visible at the type level, not derived behind the consumer's back.

Concrete substrate: `cad-core::OperatorGraph` stores edges as `kernel/graph-foundation::Graph<OperatorNode, EdgeKind>`; `effective_hash_and_label` walks ONLY those declared edges. The projection layer's `EntityCadMap` declares each entity's bound `NodeId` explicitly; `ProjectionCache::observe_checkpoint` walks ONLY the entities it has been told about via `CadProjection::spawn_brep_entity` or `remap_entity`. There is no path that creates a hidden dependency.

The principle generalises: when `kernel/graph-foundation::Invalidation::mark_dirty(root, dependents_of)` is called, the `dependents_of` closure is the explicit declaration of which nodes depend on which. The substrate refuses to infer dependencies — the consumer always supplies them. Anti-pattern: a reactive cache that introspects the graph and decides for itself which nodes are downstream. That is "implicit dependency rewiring" and is rejected at design review.

### 4.2 Revision-aware

Every reactive cache or subscriber MUST be able to detect that the upstream graph has advanced since its last observation, without polling.

Concrete substrate today: `CheckpointId` (monotonic `u64`) advances on every `CadGraph::commit`. `ProjectionCache::last_seen_checkpoint: Option<CheckpointId>` is the consumer-side last-seen tag; `observe_checkpoint(head, ...)` compares against it. ADR-115 phase-3 generalises this to `(graph_revision, metrics_revision)` pairs across all metric snapshots. The doctrine: every reactive consumer carries a revision tag and the substrate exposes the upstream's current revision so the consumer can compare.

This invariant is what distinguishes reactive invalidation from "rebuild-everything-every-frame" or "rebuild-on-poll-timer". The consumer NEVER asks "should I rebuild?" by inspecting the graph contents — it asks by comparing revision tags. Two consequences: (a) replay determinism is preserved (revision comparison is wall-clock-independent); (b) the substrate may evolve its internal representation freely as long as the revision contract holds. The future ADR-115 `RevisionId` lift extends this contract uniformly across every metric / cache / subscriber surface.

### 4.3 Observable

Recomputation events MUST be emittable as a typed event stream, not buried inside an opaque mutation. The cross-review #2 2026-05-10 framing: "metric subscribers consume events via a thin trait surface" — the same mechanism extends to invalidation.

Today's substrate: `kernel/graph-foundation::Invalidation` exposes `register(Box<dyn InvalidationListener>)` + `mark_dirty(root, dependents_of)` + BFS propagation with visited-set dedup. `kernel/audit-ledger::Event` provides the cross-process content-addressed event identity. ADR-115 phase-4 will land the unified `GraphEvent` enum (`#[non_exhaustive]` per the cross-review #1 hardening doctrine) emitted by `Graph<N, E>` mutations; metric subscribers and invalidation subscribers will consume the same stream.

The invariant is enforced: no future invalidation path may bury a dirty-bit flip inside private state without surfacing the corresponding event. The cross-review's "the editor reactive-overlay layer cannot land without a metric-source surface" is the same constraint stated in metric language.

Subscribers are intentionally `Send + 'static` per `kernel/graph-foundation::InvalidationListener: Send + 'static`. This anticipates future cross-thread propagation — when the Phase 6 sim/render-thread split lands per PLAN §1.5.2, the invalidation router may be instantiated on the sim thread and dispatch to a render-thread subscriber. The doctrine's `Observable` invariant locks the trait shape now so future thread-boundary work doesn't require a substrate redesign.

### 4.4 Deterministic

Replay byte-identity. Two runs feeding the same sequence of `CadGraph` mutations to a fresh substrate MUST produce byte-identical reactive caches at every observation point.

Concrete substrate: PLAN §1.6.8 Replay-Stable v1.0 commits to deterministic `kernel/ecs` iteration via `BTreeMap` everywhere; `kernel/graph-foundation::Graph<N, E>` uses `BTreeMap` for nodes, edges, and adjacency caches per `docs/§18/GRAPH_FOUNDATION.md` §3. The `cross_substrate_determinism::pie_three_participant_round_trip_50_iter` integration test (post-round-6 close) pins 3-way PIE envelope byte-identity for `cad-core.cad-graph` + `cad-projection.brep-handles` + `physics.rapier-rigid-bodies` across 50 iterations.

The exception is `TessellationCache` — it uses `HashMap` internally per `crates/cad-core/src/tessellation/cache.rs` line 14 module-doc: "determinism is not a requirement here (the cache key fully encodes the inputs and the value is always recomputable on miss)". The cache's *contents* are not part of the determinism story; the *recomputed values for the same input* are.

The `ProjectionCache` payload deliberately drops `next_mesh_id` from its `SnapshotParticipate` capture per `docs/§18/CAD_PROJECTION.md` §8 ("the receiving side starts at 0; fresh ids are assigned on re-projection"). This is the doctrine's `Deterministic` invariant in action: id allocation is recomputable from the deterministic mutation sequence, so the id stream itself need not round-trip through the wire format. The same pattern applies to every reactive cache: serialize the authoritative inputs (entity↔node mapping, last-seen checkpoint), drop the derivable outputs (mesh ids, mesh contents, hit-rate counters), let the next post-restore tick re-derive everything else.

## 5. Critical constraints (load-bearing invariants)

### 5.1 Renderer invalidation MUST NOT mutate canonical semantic state

`crates/gfx` is downstream-only. The constitutional principle #8 (PLAN §0.3 line 41) states "Editor extends runtime, never replaces" and the same shape holds for the renderer: gfx may read `Arc<ProjectedMesh>` (Layer-3 output) but may not write to `CadGraph` or `EntityCadMap` or any Layer-1/2/3 substrate. The `forbidden-dep` architecture lint partially enforces this; `kernel-isolation` enforces the kernel side; the doctrine is the authority for the gfx side until a dedicated lint surfaces.

### 5.2 Lower layers MUST NOT invalidate upper layers (no inversion)

Layer 4 cannot trigger Layer 3 recomputation; Layer 3 cannot trigger Layer 1 recomputation. The hierarchy is unidirectional. A "render needs to refresh, please re-tessellate" path is a doctrine violation — the renderer asks the projection layer "did anything change", not "please change something".

This is realized today by API shape: `ProjectionCache::observe_checkpoint` is `pub(crate)` (only the `CadProjection` orchestrator can call it); gfx has no API to mutate `CadGraph` (the `forbidden-dep` lint blocks the import).

### 5.3 Cross-substrate invalidation MUST round-trip through SnapshotParticipate

When PIE (PLAN §6.13) captures cross-substrate state and restores it later, every reactive cache MUST be re-derivable from the captured authoritative state. The participant's `restore` method clean-slates the cache, replaces the authoritative state from the payload, and marks every known entity dirty so the next tick re-projects everything — see `crates/cad-projection/src/lib.rs::CadProjection::restore` for the canonical pattern (`docs/§18/CAD_PROJECTION.md` §8). This is the §13.2 cross-architecture coherence quality gate.

The doctrine: a reactive cache MUST be classified per the failure-class taxonomy as `recoverable` (transient; rebuilt on next tick from upstream) or `snapshot-recoverable` (PIE-participating; survives Play/Stop). It MUST NOT be classified as authoritative (`session-fatal` corruption recovery) because by definition a reactive cache is downstream of an authoritative source.

The audit-3 H3 SnapshotParticipate audit + discriminate is the doctrine in action: the `STATEFUL_TIER2_CRATES = {cad-core, cad-projection, particles, physics, sculpt}` lint heuristic per `tools/architecture-lints/src/snapshot_participate.rs` reflects which crates own authoritative state vs which are purely downstream. `audio` / `editor-actions` / `editor-state` / `gfx` were REMOVED from the heuristic by source-truth audit (see `docs/§18/PIE_SNAPSHOT.md` §11.1) precisely because their state is reactive-downstream, not authoritative — restoring upstream re-derives them. The doctrine's classification rule: ask "if I throw this state away and replay the upstream, will it come back byte-identical?" — if yes, it's reactive (no PIE participation needed). If no, it's authoritative (PIE participant required).

### 5.4 Invalidation must terminate

A reactive cycle (Layer-3 invalidates Layer-2 invalidates Layer-3 …) is forbidden by §5.2 `no-inversion`, but a cycle WITHIN a layer is also forbidden. PLAN §1.13's failure-class table line "projection invalidation cycle (>1000 iterations)" promotes the bound to a snapshot-recoverable failure: if the invalidation walk runs >1000 iterations without converging, the orchestrator rolls back to the last known-good checkpoint. The doctrine: every layer's invalidation walk MUST have a fixed iteration bound + cycle-source diagnostic emit path. Today's substrate satisfies this through the head-advance coarse trigger (one pass per tick); future per-node fine-grained trackers MUST preserve the same property.

## 6. How RGE realizes these today (substrate snapshot)

- **Layer 1** — `cad-core::OperatorGraph::evaluate` calls `effective_hash_and_label` which folds the upstream-labeled-bitmap into a 32-byte BLAKE3. The audit-2 Phase 2 fix (commit referenced in `crates/cad-core/src/graph/operator_graph.rs:258` doc-comment as "audit-2 finding A1.4 / A5.2 / Pairing N2") closed the cache-key uniqueness gap that previously let two different upstream-labeled-state subgraphs collide on the same `(structural_hash, tolerance)` slot. The recursion is `pub` per `crates/cad-core/src/graph/operator_graph.rs:327` exactly so integration tests can verify the cache-key uniqueness contract without piggy-backing on `evaluate` — the doctrine's `Observable` invariant gets a test-side surface even before ADR-115 phase-4 lands the formal event stream.
- **Layer 2** — `cad-core::topo_lineage` ships the `TopologyEvolution` enum and v0 lineage substrate per ADR-098. `Persistent*Id` derivation per Phase 7.2; `OperatorId` + `SemanticScore` + per-edge / per-vertex lineage are deferred per the ADR's "v0 simplifications". Lineage edges are the structural backbone for distributed CAD per PLAN §6.17 (authoritative-server CAD sends operations + lineage diffs; clients reconcile via lineage walk) — which is why Layer 2 cannot be folded into Layer 1: lineage carries semantic-continuity information that the BLAKE3 hash alone discards.
- **Layer 3** — `crates/cad-projection/src/lib.rs::CadProjection::tick` observes the cad head, dirty-marks per-entity, and re-projects each dirty entity through `projection_geometry::project`. The projection cache is keyed-recompute, not size-bounded LRU; future eviction policy is documented as deferred. The lib-level module-doc step-by-step at the `tick` method documents the canonical sequence: (1) observe head and mark dirty, (2) re-project dirty entities, (3) clear dirty set; an early-failed re-projection does NOT roll back earlier successes within the same tick — they remain valid; only the failing entity is left in its previous state.
- **Layer 4** — `gfx` `GfxPlugin` canary records frames per `docs/§18/GFX_RENDER_TIER.md` §11; the future `gfx.render-snapshot` participant lands alongside Phase 6 frame-graph + render-snapshot separation.
- **Cross-layer event stream** — anticipated; ADR-115 phase-4 lands `GraphEvent` + `MetricSubscriber`. Today the streams are layer-local: `kernel/graph-foundation::Invalidation` for ad-hoc subscribers; `kernel/audit-ledger::Event` for projectable cross-process events; `Diagnostic` emit for human-visible telemetry. The unification target per ADR-115 §"Sub-decision 4" is: `Graph<N, E>` mutation emits a `#[non_exhaustive]` `GraphEvent` enum (`NodeAdded` / `NodeRemoved` / `EdgeAdded` / `EdgeRemoved` / `OperatorExecuted` / `TopologySplit` / …), metric and invalidation subscribers consume the same stream, the stream replays byte-identically per the `kernel/audit-ledger::Event::compute` BLAKE3 content addressing.

### 6.1 Test coverage anchoring the doctrine

The doctrine is not pinned by a dedicated lint (yet — a candidate post-ADR-115-phase-4 architecture lint would flag invalidation paths that bypass `GraphEvent` emission). It is pinned today by an integration-test cluster:

- `crates/cad-projection/tests/cad_projection_smoke.rs::invalidation_within_one_tick` — head-advance triggers re-projection inside the same tick (PLAN §13.6 line 1140 "cad-projection updates within one tick of cad-core commit").
- `crates/cad-projection/tests/cross_substrate_determinism.rs::pie_three_participant_round_trip_50_iter` — 3-way PIE byte-identity (the `Deterministic` invariant on the cross-substrate boundary).
- `crates/cad-projection/tests/fault_injection.rs::stale_topology_reference_surfaces_projection_error_not_panic` — `validate_handles` post-restore guard rejects orphan handles deterministically (audit-3 H3 closure).
- `crates/cad-core/tests/labeled_tessellation_pipeline.rs` — labeled-bitmap effect on `effective_hash` propagation through Layer-1 hash recursion.
- The future ADR-115-phase-4 `GraphEvent` stream replay-determinism gate (per ADR-115 phase-3 acceptance criterion) will close the `Observable` invariant with a dedicated test.

## 7. Source / spec inconsistencies

Mirrors the §18-pack honesty discipline (every pack surfaced 4–6 source/spec inconsistencies; this doctrine doc surfaces four).

- **Cache-key shape**: an early sketch of this doctrine described `CacheKey` as carrying three fields (`structural_hash, tolerance, labeled-state-bitmap`). Source-truth at `crates/cad-core/src/tessellation/cache.rs:113` is two fields (`structural_hash`, `tolerance`). The labeled-state-bitmap is FOLDED INTO the `structural_hash` itself by `effective_hash_and_label_inner` at `crates/cad-core/src/graph/operator_graph.rs:377-388` (line 388: `hasher.update(&upstream_labeled_bitmap.to_le_bytes())`). The mechanism realises the same uniqueness contract via a different shape — there is no separate bitmap in the cache key.
- **Per-node invalidation granularity**: the cross-review #2 framing of "incremental invalidation" anticipates per-node fine-grained dependency tracking. Source-truth at `crates/cad-projection/src/projection_cache/mod.rs:159-176` is coarse: head-advanced ⇒ every known entity dirtied. The module doc explicitly flags this as Phase 7.3 design ("Per-node fine-grained dependency tracking … is a future-dispatch concern"). The doctrine here ratifies that decision: coarse-but-correct is cheaper than fine-but-buggy at v0; ADR-115 phase-2 structural metrics (incremental SCC / depth / fanout) provide the building blocks for fine-grained tracking when the cost surfaces.
- **`Invalidation` substrate vs head-advance**: `kernel/graph-foundation::Invalidation` exposes `mark_dirty(root, dependents_of)` with BFS propagation. `cad-projection` does NOT use it — the projection layer relies on head-advance via `observe_checkpoint(head, all_entities)` instead. This is intentional under §4.1 (explicit dependency wiring): the projection layer's dependency surface is "every known entity depends on the cad head", which is more expensive in the worst case but trivially correct. When per-node tracking lands, `Invalidation` becomes the candidate substrate.
- **`TessellationCache` u32 bitmap modulo wrap**: source-truth at `crates/cad-core/src/graph/operator_graph.rs:381-388` notes the upstream-labeled-bitmap is `u32` with port indices wrapped modulo 32 (line 383: `acc | (1u32 << (i % 32))`). The doc-comment at line 376 acknowledges "no current operator exceeds arity 2; future ops are unlikely to exceed 32" but the wraparound is a defensive collision class. Round-6 audit-6 logged this as C2 ("cache-key bitmap modulo collision — defensive forward-compat; trigger when first arity-33+ operator ships"); deferred per IMPLEMENTATION.md phase order. The doctrine flags this as a known forward-compat boundary: when arity-33+ operators surface, a wider bitmap (u64 / u128) is the upgrade path.

## 8. Reviewer guidance for new reactive layers

When a future subsystem proposes a new reactive cache or invalidation path, the doctrine's reviewer checklist:

1. **Identify the layer.** Is it Layer-1 (graph mutation), Layer-2 (topology evolution), Layer-3 (geometry rebuild), Layer-4 (GPU upload), or a new horizontal slice within an existing layer? A "new layer" claim crossing the §0.6 freeze policy threshold requires an ADR.
2. **Identify the upstream authority.** Which existing substrate is this cache derived FROM? If the answer is "no upstream, this is authoritative", the substrate is misclassified — reactive caches always have an upstream. The exception is Layer-1 itself; everything else is reactive.
3. **Apply the four invariants.** Walk through Explicit / Revision-aware / Observable / Deterministic in order. If any invariant cannot be satisfied with current substrate, identify the gap and decide whether to add substrate (small) or defer the layer (often the right call).
4. **Apply the load-bearing constraints.** §5.1 (no canonical mutation), §5.2 (no inversion), §5.3 (PIE round-trip), §5.4 (termination bound). These are stricter than the four invariants — they govern composition, not just shape.
5. **Identify failure-class classification.** Per `docs/§18/RECOVERY_MODEL.md`, the cache's worst-case failure must be `recoverable` or `snapshot-recoverable`. If it would be `session-fatal`, it is authoritative, not reactive.
6. **Specify the test surface.** A 1-tick observation test (the cache reflects a Layer-1 mutation), a PIE round-trip test (the cache re-derives from a captured upstream), and a determinism test (50-iter byte-identity) are the canonical three. Layer 4 will add a 4th: a render-frame golden test (Phase 6+).

Each item satisfied is a single line in the dispatch's "design notes" section. Items not satisfied are blocking review feedback, not `// TODO`s.

### 8.1 Anti-patterns rejected at design review

The doctrine codifies what NOT to do at the same time it codifies what to do. The five anti-patterns:

1. **Polling timer.** "Every 100ms re-check whether the upstream changed." Violates `Revision-aware` (the timer is a polling mechanism) and `Deterministic` (timer firing is wall-clock-dependent).
2. **Implicit dependency inference.** "The cache walks the graph and figures out which nodes are downstream." Violates `Explicit` — the dependency edges must be declared, not inferred.
3. **Hidden recomputation hook.** "Mutating an upstream automatically calls a global rebuild closure." Violates `Observable` — the rebuild is not a typed event the consumer can subscribe to or skip.
4. **Cross-layer up-call.** "The renderer notices a missing texture and asks the projection layer to re-tessellate." Violates `no-inversion` (§5.2) — Layer 4 cannot drive Layer 3.
5. **Authoritative reactive cache.** "The cache IS the source of truth; if you lose it you lose data." Violates `recoverable` / `snapshot-recoverable` classification — by definition a reactive cache is downstream and re-derivable.

The five are listed not as future hypotheticals but because each has been considered (or attempted) in similar systems. The doctrine forecloses each by making the alternative path strictly easier to design.

### 8.2 Cross-substrate composition discipline

When two reactive layers compose (e.g. Layer 3 cad-projection observes Layer 1 cad-graph), the composition itself is doctrine-bound:

- **Single source of truth per layer.** The `EntityCadMap` is the unique entity↔node FK store post-Pairing-6 closure (2026-05-08); `BRepHandle` does NOT carry a redundant `cad_node: NodeId` field. Two-place storage is two-place drift.
- **Atomic transitions across layers.** `CadProjection::spawn_brep_entity` writes the ECS entity AND the map entry AND the dirty bit in one fallible step that rolls back the spawn on map-insert failure. Multi-step transitions through a reactive boundary MUST be atomic at the API boundary, even if internally they touch multiple layers.
- **Validate-on-restore.** Every reactive cache that participates in PIE MUST expose a `validate_*` method that detects orphan references after a divergent-state restore. `CadProjection::validate_handles(&CadGraph) -> Vec<(EntityId, NodeId)>` is the canonical pattern (CRITICAL #1 closure 2026-05-07). The doctrine: orphan references from a divergent-state PIE payload MUST surface as observable diagnostics, not silently produce stale outputs.

## 9. Cross-references

- **PLAN.md §0.2** — moat statement (deterministic reactive CAD runtime).
- **PLAN.md §0.3 line 41** — constitutional principle #8 (editor extends runtime, never replaces).
- **PLAN.md §1.5.2** — render-side snapshot staging (Layer 4 anticipated).
- **PLAN.md §1.5.4** — CAD transactional core (Layer 1 substrate).
- **PLAN.md §1.5.4.3** — topology lineage graph (Layer 2 substrate; ADR-098).
- **PLAN.md §1.5.4.5** — cad-projection internal split (Layer 3 substrate).
- **PLAN.md §1.6** — invalidation references throughout (cad-projection invalidation density entropy metric).
- **PLAN.md §1.6.8** — Replay-Stable v1.0 determinism mode (invariant 4.4).
- **PLAN.md §1.10.4** — incremental invalidation radius + cad-projection invalidation density entropy metrics.
- **PLAN.md §13.2** — cross-architecture coherence quality gate (constraint 5.3).
- **PLAN.md §13.10** — graph invalidation propagation depth entropy metric.
- **ADR-098** — topology lineage substrate (Layer 2 doctrine).
- **ADR-115** — graph-metrics substrate design; sub-decision 4 (event-sourced `GraphEvent`); the cross-layer event stream that lands the `Observable` invariant for ALL layers.
- **`docs/§18/CAD_CORE_MODEL.md`** — Layer 1 + Layer 3 substrate reference.
- **`docs/§18/CAD_TOPOLOGY_LINEAGE.md`** — Layer 2 substrate reference.
- **`docs/§18/CAD_PROJECTION.md`** — Layer 3 substrate reference (canonical reactive consumer).
- **`docs/§18/GFX_RENDER_TIER.md`** — Layer 4 substrate reference + Phase 6 deferral framing.
- **`docs/§18/GRAPH_FOUNDATION.md`** §6 — `Invalidation` BFS router substrate.
- **`docs/§18/PIE_SNAPSHOT.md`** — cross-substrate participant boundary (constraint 5.3).
- **`docs/§18/RECOVERY_MODEL.md`** — failure-class taxonomy (recoverable vs snapshot-recoverable cache classification).
- **`crates/cad-projection/src/projection_cache/mod.rs`** — `ProjectionCache::observe_checkpoint` (head-advance dirty trigger).
- **`crates/cad-core/src/graph/operator_graph.rs`** — `effective_hash_and_label` (recursive hash propagation).
- **`crates/cad-core/src/tessellation/cache.rs`** — `CacheKey { structural_hash, tolerance }` (Layer-3 memoization key).
- **`crates/cad-core/src/checkpoints/mod.rs`** — `CheckpointHistory` + `begin_operation` / `commit` / `rollback` / `restore_to`.
- **`kernel/graph-foundation/src/invalidation.rs`** — `Invalidation` BFS router (anticipated Layer-1 / Layer-2 subscriber substrate).
- **2026-05-10 ChatGPT cross-review #1 + #2 archives in `change.md`** — the architectural design pressure that motivated this doctrine doc.
