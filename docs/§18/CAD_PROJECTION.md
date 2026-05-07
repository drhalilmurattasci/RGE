# CAD_PROJECTION

| Companion to | PLAN.md §1.5.4.5 (cad-projection ECS view layer) |
|---|---|
| Status | Stable v0 (Phase 7.3 lib + plugin canary; PIE `SnapshotParticipate` shipped 2026-05-08; `BRepHandle` SSoT refactor / Pairing-6 closure landed 2026-05-08) |
| Audience | Editor-ui authors needing entity↔cad bridges; orchestrator authors composing `CadProjection` with PIE; future `projection_runtime` / `projection_editor` module fillers |
| Sibling doc | `CAD_CORE_MODEL.md` — provides the operator graph, tessellation cache, and `CadGraph` whose state this layer projects into ECS; `PIE_SNAPSHOT.md` — substrate for the participant impl `cad-projection.brep-handles` |
| Reference impls | `crates/cad-projection/src/lib.rs` (top-level orchestrator + `SnapshotParticipate`) · `crates/cad-projection/src/projection_structural/mod.rs` (`BRepHandle` + `EntityCadMap`) · `crates/cad-projection/src/projection_geometry/mod.rs` (`ProjectedMesh` + `project()`) · `crates/cad-projection/src/projection_cache/mod.rs` (`ProjectionCache` + dirty bits) · `crates/cad-projection/src/plugin_adapter.rs` (Tier-2 plugin canary) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. Pairs naturally with `CAD_CORE_MODEL.md` — this doc is the "ECS-side projection of the operator graph", that one is the "operator graph itself". The two are co-evolving substrates: every change to one touches the other.

## 1. The bridge problem

`cad-core::OperatorGraph` operates in its own universe — `NodeId`s, `Tessellation`s, `CheckpointId`s. ECS operates in another — `EntityId`s, `Component`s, `World`. Editor users see entities (the things they click on, drag, transform) and want to manipulate the parametric history behind them (the operator graph that produced those meshes).

`cad-projection` is the bridge layer per [PLAN.md §1.5.4.5](../../plans/PLAN.md). The bridge has three responsibilities:

1. **Bidirectional mapping** between `EntityId` and `cad_core::NodeId` — the [`EntityCadMap`].
2. **Mesh projection** of `cad_core::Tessellation` into an ECS-side `ProjectedMesh` carrying provenance metadata.
3. **Dirty-tracking + invalidation** so re-projection runs only when the cad graph advances.

Per PLAN §1.5.4.5, `cad-projection` is the **only** Tier-2 crate allowed to import `cad-core`. It is the user-facing API for editor-ui consumers; everything ECS-side that cares about CAD goes through this crate.

## 2. Module split per `projection-modules` lint

The crate splits into six modules per PLAN §1.5.4.5 to prevent god-bridge accumulation. The `projection-modules` architecture lint enforces a layering rule: `projection_structural` MUST NOT import from `projection_runtime` or `projection_editor`. Importing from `projection_geometry` and `projection_cache` is permitted.

| Module | Status | Owns |
|---|---|---|
| `projection_structural` | Implemented | `BRepHandle` ECS component; `EntityCadMap` bidirectional map; `EntityCadMapError`. |
| `projection_geometry` | Implemented | `ProjectedMesh` payload; `ProjectedMeshId`; `CheckpointTag` proxy; the free `project()` function; `ProjectionError`. |
| `projection_cache` | Implemented | `ProjectionCache` — per-entity mesh storage, dirty bits, head-tracking, `CacheStats`. |
| `projection_semantic` | Stub | Future home for material-slot bindings, selection-set membership. |
| `projection_runtime` | Stub | Future home for collision proxies, render-queue feeders. |
| `projection_editor` | Stub | Future home for gizmo bindings, picking surfaces. |

Per PLAN §0.6 freeze policy + §1.5.4.5 ("adding a 7th category requires ADR"), the 6-way split is conserved by leaving the un-implemented modules as documented stubs rather than collapsing them. Future dispatches fill them in as concrete use cases arrive.

The top-level `crate` orchestrator (`lib.rs`) owns: `EntityCadMap`, `ProjectionCache`, a private `cad_core::TessellationCache`, plus the `tick()` entry-point that drives them in concert. The `plugin_adapter` module is the Tier-2 plugin shim per PLAN §10.4 dogfood (see §10).

## 3. `BRepHandle` — ECS component

Lives at `crates/cad-projection/src/projection_structural/mod.rs`. Boundary-representation handle — the ECS-side identity of an entity that has a cad-graph projection.

```rust
pub struct BRepHandle {
    pub mesh_id: Option<ProjectedMeshId>,
    pub last_projected_checkpoint: Option<CheckpointTag>,
}

impl Component for BRepHandle {}
impl SnapshotComponent for BRepHandle {}
```

Both fields are `Option` because a freshly inserted handle has not been projected yet — the next `CadProjection::tick` call fills them in.

### SSoT refactor 2026-05-08 (Pairing-6 closure)

Per the 2026-05-08 `BRepHandle` SSoT refactor (Pairing-6 closure surfaced in HANDOFF.md), the handle DOES NOT carry a `cad_node: NodeId` field. The cad-node FK is owned **exclusively** by [`EntityCadMap`], which is now the **single source of truth** for entity↔cad-node mappings. Consumers look up the node at access time via `CadProjection::node_for(entity)`.

The pre-2026-05-08 design carried `cad_node` on both the handle AND the map — every call site had to be careful to keep them in sync, and a class of "handle says X, map says Y" drift bugs was structurally possible. The refactor eliminates an entire class of drift bugs by keeping the FK in exactly one place. `BRepHandle::new()` no longer takes a `NodeId`; entity creation goes through `CadProjection::spawn_brep_entity` which writes the map entry alongside the handle.

`BRepHandle` impls `Component` and `SnapshotComponent` so its bookkeeping fields round-trip through PIE world-bytes (the `World::serialize_snapshot` layer) on top of the participant payload. The `mesh_id` / `last_projected_checkpoint` fields are stable metadata; they re-derive on the next post-restore tick.

## 4. `EntityCadMap` — bidirectional mapping

Lives at `crates/cad-projection/src/projection_structural/mod.rs`. The single source of truth for entity↔cad-node mappings post-Pairing-6.

```rust
pub struct EntityCadMap {
    entity_to_cad: BTreeMap<EntityId, NodeId>,
    cad_to_entity: BTreeMap<NodeId, EntityId>,
}
```

Both forward and reverse directions are mutated atomically by `insert(entity, node) -> Result<(), EntityCadMapError>`: either both entries land or neither does. `BTreeMap` for deterministic iteration matching workspace convention (PLAN §1.6.8) — important for snapshot byte-stability.

### `EntityCadMapError`

```rust
pub enum EntityCadMapError {
    DuplicateEntity { entity: EntityId, existing_node: NodeId },
    DuplicateNode { node: NodeId, existing_entity: EntityId },
    NotFound,
}
```

`DuplicateEntity` fires when caller tries to bind `entity` to a different node than its current binding. `DuplicateNode` fires when caller tries to bind `node` to a different entity than its current binding. Re-inserting an identical `(entity, node)` pair is idempotent — a no-op success.

Removal via `remove_entity(entity) -> Option<NodeId>` and `remove_node(node) -> Option<EntityId>`; both update both directions atomically with `debug_assert!`-checked sync invariants.

### `EntityIdProxy` + manual serde bridge

`rge_kernel_ecs::EntityId` does NOT enable `ulid/serde` in its upstream crate, so the type itself has no `Serialize` / `Deserialize` impl. `EntityCadMap` bridges via a private serde-transparent newtype:

```rust
#[derive(Serialize, Deserialize)]
struct EntityIdProxy(ulid::Ulid);
```

The `cad-projection` crate enables `ulid/serde` in its own `Cargo.toml`, so the `Ulid` value DOES round-trip. `EntityCadMap` implements `Serialize` / `Deserialize` manually, encoding the wire format as `BTreeMap<EntityIdProxy, NodeId>` (the reverse direction is rebuilt at deserialization time from the forward map, so wire format stays compact).

## 5. `ProjectedMesh` — ECS-side mesh

Lives at `crates/cad-projection/src/projection_geometry/mod.rs`. The ECS-friendly payload re-stamped from `cad_core::Tessellation` with provenance metadata:

```rust
pub struct ProjectedMesh {
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub source_node: NodeId,
    pub source_checkpoint: CheckpointTag,
}
```

`source_node` + `source_checkpoint` together answer "is this mesh stale?" without dipping back into `cad-core`. Stored behind `Arc` inside `ProjectionCache` so multiple readers share allocation cheaply.

For Phase 7.3 the implementation **copies** position and index buffers out of the `Arc<Tessellation>` returned by `cad-core::OperatorGraph::evaluate`. This is correctness-first; a future optimization can have `ProjectedMesh` borrow from the underlying `Arc<Tessellation>` directly. Tracked as a deferred non-goal in the dispatch spec.

### `ProjectedMeshId`

```rust
pub struct ProjectedMeshId(pub u64);
```

Stable identifier allocated monotonically by `ProjectionCache::insert_mesh`. Across PIE round-trips the id sequence is reset on the receiving side; the next post-restore tick re-projects every dirty entity and assigns fresh ids (the `ParticipantPayload` deliberately does NOT serialize `next_mesh_id`).

### `CheckpointTag` proxy

`cad_core::CheckpointId` is `Copy + PartialEq + Eq + Hash` but does NOT derive `Serialize` / `Deserialize`. `ProjectedMesh` and the `ParticipantPayload` need to serialize provenance, so this layer-boundary proxy is used:

```rust
pub struct CheckpointTag(pub u64);

impl From<CheckpointId> for CheckpointTag { /* ... */ }
impl From<CheckpointTag> for CheckpointId { /* ... */ }
```

Conversion is loss-free in both directions.

### `project` — free function

```rust
pub fn project(
    cad: &CadGraph,
    node: NodeId,
    cache: &mut TessellationCache,
    tolerance: Tolerance,
) -> Result<Arc<ProjectedMesh>, ProjectionError>;
```

Pre-checks `node` is in the graph; calls `cad.graph().evaluate(node, cache, tolerance)` (per `CAD_CORE_MODEL.md` §4); copies the resulting `Tessellation` into a `ProjectedMesh` stamped with the node and current head; wraps in `Arc` and returns. The `cache` parameter is the `cad-core` `TessellationCache` — the projection layer does NOT own it (the orchestrator threads its own private one through `tick`).

`ProjectionError` variants: `Eval(EvalError)` (wraps cad-core), `Tolerance(ToleranceError)`, `NoBRepHandle { entity }`, `NodeNotInGraph(NodeId)`, `EntityCadMap(EntityCadMapError)`.

## 6. `ProjectionCache` — dirty-tracking and invalidation

Lives at `crates/cad-projection/src/projection_cache/mod.rs`. Per-entity `Arc<ProjectedMesh>` storage + dirty bits + head-tracking glue:

```rust
pub struct ProjectionCache {
    last_seen_checkpoint: Option<CheckpointId>,
    next_mesh_id: u64,
    meshes: BTreeMap<ProjectedMeshId, Arc<ProjectedMesh>>,
    entity_meshes: BTreeMap<EntityId, ProjectedMeshId>,
    dirty: BTreeSet<EntityId>,
    stats: CacheStats,
}
```

`BTreeMap` / `BTreeSet` everywhere for deterministic iteration matching `kernel/ecs` (PLAN §1.6.8). `CacheStats` carries `hits` / `misses` / `reprojections` u64 counters so the editor can surface cache-effectiveness metrics.

### `observe_checkpoint(head, all_entities)` — the head-advance trigger

```rust
pub(crate) fn observe_checkpoint<I: IntoIterator<Item = EntityId>>(
    &mut self,
    head: CheckpointId,
    all_entities: I,
);
```

If `head != last_seen_checkpoint`, every entity in `all_entities` is marked dirty (head-advanced ⇒ everything dirty). `last_seen_checkpoint` is updated unconditionally. This is the Phase 7.3 invalidation strategy — coarse but correct. Per-node fine-grained dependency tracking ("which entities depend on which cad nodes") is a deferred future-dispatch concern.

### Bookkeeping invariants in `insert_mesh`

`insert_mesh(entity, mesh) -> ProjectedMeshId` mutates four pieces of state atomically:

1. The previous mesh entry, if any, is removed from `meshes`.
2. `next_mesh_id` is bumped (saturating — the cache won't panic on u64 overflow).
3. The new id is recorded in `entity_meshes` and the `Arc<ProjectedMesh>` in `meshes`.
4. The dirty bit for `entity` is cleared.
5. `stats.reprojections` and `stats.misses` are bumped.

## 7. `CadProjection` — top-level orchestrator

Lives at `crates/cad-projection/src/lib.rs`. The user-facing facade:

```rust
pub struct CadProjection {
    entity_cad_map: EntityCadMap,
    cache: ProjectionCache,
    tess_cache: TessellationCache,  // owned cad-core memoization across ticks
}
```

The `tess_cache` is intentionally owned by the projection layer rather than created per-tick: subtree results survive between projections, so re-evaluating a parameter-changed root benefits from cached upstream tessellations.

### `tick(world, cad, tolerance) -> Result<TickReport, ProjectionError>`

The single re-projection entry-point. Step-by-step (per the lib-level module-doc):

1. **Observe head.** Calls `cache.observe_checkpoint(cad.head(), all_entities)`. If the head advanced, every known entity is marked dirty.
2. **Re-project dirty entities.** For each entity in the snapshotted dirty set, look up its bound cad node and call `projection_geometry::project`; insert the resulting `Arc<ProjectedMesh>` into the cache; update the entity's `BRepHandle` component in `world` with the fresh `mesh_id` and `last_projected_checkpoint`.
3. **Clear the dirty set.** `cache.clear_dirty()` after the loop.

An early-failed re-projection does NOT roll back earlier successes within the same tick — they remain valid; only the failing entity is left in its previous state.

### Accessors

- `node_for(entity) -> Option<NodeId>` — forward lookup through the map.
- `entity_for(node) -> Option<EntityId>` — reverse lookup through the map.
- `projected_mesh(entity) -> Option<&Arc<ProjectedMesh>>` — borrow the cached mesh.
- `spawn_brep_entity(world, node) -> Result<EntityId, ProjectionError>` — ECS spawn + map insert + dirty-mark in one atomic step (rolls back the spawn if the map insert fails).
- `despawn_brep_entity(world, entity) -> bool` — tear down all three: world entity, map entry, cache entry.

### `remap_entity(entity, new_node)` — atomic transition

Pre-2026-05-08 the idiom was `handle.cad_node = new_node` (a direct field write on the component); post-Pairing-6 the FK lives only in the map and the rebinding goes through:

```rust
pub fn remap_entity(
    &mut self,
    entity: EntityId,
    new_node: NodeId,
) -> Result<(), EntityCadMapError>;
```

Two-phase pre-validation: the entity must already be registered (`NotFound` else); `new_node` must be unbound or already bound to `entity` (`DuplicateNode` else). If the entity is already bound to `new_node`, the call is a no-op success but the entity is marked dirty so the next tick re-projects it. Otherwise the swap removes the old binding and inserts the new one — both pre-validations pass means the swap is infallible — then marks the entity dirty.

### `validate_handles(&CadGraph) -> Vec<(EntityId, NodeId)>`

```rust
pub fn validate_handles(&self, cad: &CadGraph) -> Vec<(EntityId, NodeId)>;
```

Iterates `EntityCadMap` and returns `(entity, node)` pairs where `cad.graph().node(node).is_none()` — orphan handles whose cad-node references no longer resolve. An empty `Vec` means every entry references a live cad-graph node.

This is the **CRITICAL #1 closure** post-restore guard (per the design constraint flagged in HANDOFF.md / ADR-098 hint). Callers SHOULD invoke `validate_handles` after restoring a `CadProjection` from PIE, with the cad-graph that was restored alongside. Orphan handles indicate a divergent-state PIE payload — the cad-graph and projection were captured at different times, or the cad-graph wasn't co-captured. The orchestrator decides recovery: log a diagnostic, mark entities for re-projection, or error out. Without this guard, post-restore ticks on a missing cad node fail with `ProjectionError::NodeNotInGraph` rather than silently producing stale meshes.

## 8. `SnapshotParticipate` impl — the `cad-projection.brep-handles` participant

Lives at the bottom of `crates/cad-projection/src/lib.rs`. Per PLAN §13.2 (all stateful Tier-2 has `SnapshotParticipate`).

```rust
const PARTICIPANT_ID: &str = "cad-projection.brep-handles";

#[derive(Serialize, Deserialize)]
struct ParticipantPayload {
    entity_cad_map: EntityCadMap,
    last_seen_checkpoint: Option<CheckpointTag>,
}

impl SnapshotParticipate for CadProjection {
    fn participant_id(&self) -> ParticipantId { ParticipantId::new(PARTICIPANT_ID) }
    fn capture(&self) -> Result<Vec<u8>, ParticipateError> { /* postcard */ }
    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError> { /* postcard */ }
}
```

### Wire format — postcard

Capture and restore use **postcard** for compact entity↔mesh-id round-trips. This is the workspace default per `kernel/ecs/src/participate.rs` SnapshotParticipate trait module-doc; `cad-core.cad-graph` is the documented exception that uses RON because postcard rejects `OperatorNode`'s `#[serde(tag = "kind")]` enum encoding (see `CAD_CORE_MODEL.md` §5).

### What's captured vs not

The payload carries `EntityCadMap` (so entity↔node mappings round-trip) and `last_seen_checkpoint` (so a tick after restore on an unchanged graph correctly skips re-projection). It deliberately does **NOT** carry:

- `Arc<ProjectedMesh>` data — meshes re-derive on the next tick.
- `next_mesh_id` — the receiving side starts at 0; fresh ids are assigned on re-projection.
- `tess_cache` contents — `cad-core` re-memoizes on re-evaluation.

`restore()` clean-slates `cache` and `tess_cache`, replaces `entity_cad_map` from the payload, and marks every known entity dirty so the next tick re-projects everything. The captured `last_seen_checkpoint` is intentionally NOT restored — letting the next tick observe the current head guarantees re-projection regardless of whether the head matches the captured one.

### Co-restore convention

Per PLAN §13.2 cross-architecture coherence, callers SHOULD restore `cad-core.cad-graph` AND `cad-projection.brep-handles` in the same `PieSnapshot::restore` call. After restoring, `CadProjection::validate_handles(&cad_graph)` is the post-restore guard (§7) that detects orphan references from a divergent-state payload. See `PIE_SNAPSHOT.md` §10 for the broader divergent-restore tolerance discussion.

## 9. `CadProjectionPlugin` — the first Tier-2 dogfood canary

Lives at `crates/cad-projection/src/plugin_adapter.rs`. Per PLAN §10.4 dogfood rule, every stateful Tier-2 subsystem implements the `Plugin` trait through a thin adapter shim.

`CadProjectionPlugin` is the **first canary** — the substrate validation that the type-erased `PluginContext` registry can carry real subsystem resources end-to-end. The adapter wraps a `CadProjection`, takes `World` + `CadGraph` + optional `Tolerance` from the context on `tick`, drives `self.projection.tick(...)`, and puts everything back. It follows **Pattern A — straight-line tick** per `PLUGIN_HOST_PATTERNS.md` §3, with idempotent failure put-back on missing resources (`ContractViolation`) and `RuntimeFault` mapping for projection failures.

The plugin is documented at length in `PLUGIN_API.md` and `PLUGIN_HOST_PATTERNS.md`; this doc is the cad-side reference. Stable ID: `rge-cad-projection.brep-handles-plugin`.

## 10. Failure class — snapshot-recoverable

Per PLAN §1.13, `cad-projection`'s lib.rs declares `//! Failure class: snapshot-recoverable` and every sub-module inherits the class. `ProjectionError::Eval` wraps `cad_core::EvalError` (snapshot-recoverable per `CAD_CORE_MODEL.md` §11); `EntityCadMapError` failures are caller-recoverable bookkeeping errors; `NodeNotInGraph` is caller-recoverable (typically by calling `validate_handles` and reacting to orphans).

The `architecture-lints` `failure-class` lint enforces the declaration; `cad-projection` does not appear in the failure-class exemptions table.

## 11. References

- **PLAN.md §1.5.4.5** — cad-projection ECS view layer; the 6-module split rule.
- **PLAN.md §10.4** — dogfood rule (`CadProjectionPlugin`).
- **PLAN.md §13.2** — `SnapshotParticipate` quality gate.
- **PLAN.md §1.13** — failure-class taxonomy.
- **PLAN.md §1.6.8** — determinism modes (BTreeMap convention).
- **`CAD_CORE_MODEL.md`** — sibling §18 doc; the operator graph + `CadGraph` this layer projects.
- **`PIE_SNAPSHOT.md`** — sibling §18 doc; the `SnapshotParticipate` substrate.
- **`PLUGIN_API.md`** / **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 docs; the canary lives at the intersection of those two.
- **CRITICAL #1 audit closure** (HANDOFF.md, 2026-05-08) — `validate_handles` post-restore guard.
- **MEDIUM #4 audit closure** (HANDOFF.md, 2026-05-08) — `BRepHandle` SSoT refactor / `cad_node` field drop / `EntityCadMap` as authoritative owner.
- **ADR-098** — topology lineage substrate (referenced by the SSoT refactor design).
- **ADR-114** — `PluginContext` owned-handoff design (the canary's substrate).
- **`crates/cad-projection/src/lib.rs`** — top-level orchestrator + `SnapshotParticipate`.
- **`crates/cad-projection/src/projection_structural/mod.rs`** — `BRepHandle` + `EntityCadMap`.
- **`crates/cad-projection/src/projection_geometry/mod.rs`** — `ProjectedMesh` + `project()`.
- **`crates/cad-projection/src/projection_cache/mod.rs`** — `ProjectionCache` + dirty bits.
- **`crates/cad-projection/src/plugin_adapter.rs`** — the Tier-2 plugin canary.
