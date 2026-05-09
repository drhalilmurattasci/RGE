//! `rge-cad-projection` — ECS view layer for `cad-core`.
//!
//! Failure class: snapshot-recoverable
//!
//! Per [PLAN.md §1.5.4.5](../../plans/PLAN.md). Internal split into 6 modules
//! to prevent god-bridge accumulation. CI rule (`projection-modules` lint):
//! `projection_structural` cannot import `projection_runtime` or
//! `projection_editor`.
//!
//! # The 6 modules
//!
//! Of the six split modules, this dispatch implements four:
//!
//! * [`projection_structural`] — `BRepHandle` ECS component +
//!   `EntityCadMap` bidirectional mapping.
//! * [`projection_geometry`] — `ProjectedMesh` payload + `project()`.
//! * [`projection_cache`] — `ProjectionCache` (per-entity mesh storage,
//!   dirty bits, head-tracking).
//! * [`crate`] (this top-level orchestrator) — owns an `EntityCadMap` + a
//!   `ProjectionCache` + a `cad_core::TessellationCache`. Drives `tick()` to
//!   re-project dirty entities. Implements
//!   [`rge_kernel_ecs::SnapshotParticipate`] so the projection's bookkeeping
//!   rides through PIE snapshots.
//!
//! Three modules remain stubs and will be filled in by future dispatches as
//! concrete use cases arrive:
//!
//! * [`projection_semantic`] — material slots, selection sets.
//! * [`projection_runtime`] — collision proxies, render queue feeders.
//! * [`projection_editor`] — gizmos, picking.
//!
//! Per PLAN §0.6 freeze policy + §1.5.4.5 ("adding a 7th category requires
//! ADR"), the 6-way split is conserved by leaving the un-implemented modules
//! as stubs rather than collapsing them.
//!
//! # Tick contract
//!
//! [`CadProjection::tick`] is the single entry-point that synchronises ECS
//! `BRepHandle` components with the current state of a `cad_core::CadGraph`.
//! On each call:
//!
//! 1. The cache observes `cad.head()`. If the head advanced, every known
//!    entity is marked dirty (head-advanced ⇒ everything dirty — sufficient
//!    for Phase 7.3 per the dispatch spec; finer-grained per-node dependency
//!    tracking is deferred).
//! 2. Each dirty entity is re-projected via `projection_geometry::project`,
//!    its `BRepHandle` updated with the new `mesh_id` /
//!    `last_projected_checkpoint`, and the cache populated.
//! 3. The dirty set is cleared.
//!
//! The return value [`TickReport`] reports how many entities were re-projected
//! and the head we settled at.

#![forbid(unsafe_code)]

use std::sync::Arc;

use rge_cad_core::{
    brep_face_ids_for_node, BRepFaceId, CadGraph, CheckpointId, OperatorGraph, TessellationCache,
    Tolerance,
};
use rge_kernel_ecs::{EntityId, ParticipantId, ParticipateError, SnapshotParticipate, World};
use rge_kernel_graph_foundation::NodeId;
use serde::{Deserialize, Serialize};

pub mod plugin_adapter;
pub mod projection_cache;
pub mod projection_editor;
pub mod projection_geometry;
pub mod projection_runtime;
pub mod projection_semantic;
pub mod projection_structural;

pub use plugin_adapter::{CadProjectionPlugin, CAD_PROJECTION_PLUGIN_ID};
pub use projection_cache::{CacheStats, ProjectionCache};
pub use projection_geometry::{
    project, CheckpointTag, ProjectedMesh, ProjectedMeshId, ProjectionError,
};
pub use projection_structural::{BRepHandle, EntityCadMap, EntityCadMapError};

// ---------------------------------------------------------------------------
// TickReport
// ---------------------------------------------------------------------------

/// Outcome summary returned by [`CadProjection::tick`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickReport {
    /// Number of entities whose mesh was re-projected this tick.
    pub entities_reprojected: usize,
    /// Number of cache hits (entities skipped because they were not dirty).
    pub cache_hits: usize,
    /// Number of cache misses (entities re-projected).
    pub cache_misses: usize,
    /// Cad-core head observed at the start of this tick.
    pub head_advanced_to: CheckpointId,
}

// ---------------------------------------------------------------------------
// CadProjection — top-level orchestrator
// ---------------------------------------------------------------------------

/// CAD ↔ ECS bridge facade.
///
/// Owns the [`EntityCadMap`], the [`ProjectionCache`], and a private
/// [`TessellationCache`] threaded into `cad_core::OperatorGraph::evaluate`.
/// Per PLAN §1.5.4.5 the projection is the only Tier-2 crate allowed to
/// import `cad-core` — this facade is the user-facing API.
#[derive(Debug)]
pub struct CadProjection {
    entity_cad_map: EntityCadMap,
    cache: ProjectionCache,
    /// Owned `cad-core` tessellation cache. The projection layer holds it
    /// across ticks so subtree results survive between projections.
    tess_cache: TessellationCache,
}

impl Default for CadProjection {
    fn default() -> Self {
        Self::new()
    }
}

impl CadProjection {
    /// Construct an empty projection.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entity_cad_map: EntityCadMap::new(),
            cache: ProjectionCache::new(),
            tess_cache: TessellationCache::new(),
        }
    }

    /// Look up the entity bound to `node`, if any.
    #[must_use]
    pub fn entity_for(&self, node: NodeId) -> Option<EntityId> {
        self.entity_cad_map.entity_for(node)
    }

    /// Look up the cad node bound to `entity`, if any.
    #[must_use]
    pub fn node_for(&self, entity: EntityId) -> Option<NodeId> {
        self.entity_cad_map.node_for(entity)
    }

    /// Look up the projected mesh currently bound to `entity`, if any.
    #[must_use]
    pub fn projected_mesh(&self, entity: EntityId) -> Option<&Arc<ProjectedMesh>> {
        self.cache.mesh_for(entity)
    }

    /// Lazily resolve the stable [`BRepFaceId`] for a triangle in an
    /// entity's projected mesh.
    ///
    /// Returns `None` if any of the following hold:
    ///
    /// * `entity` has no [`BRepHandle`] component, OR
    /// * the entity's `BRepHandle.brep_owner` is `None`, OR
    /// * the entity has no projected mesh in the cache, OR
    /// * the projected mesh has no `face_labels` (the upstream
    ///   `Tessellation` was unlabeled — e.g. `FilletOp` output, or any
    ///   operator other than `CuboidOp` as of D-projection-α), OR
    /// * `triangle_idx` is out of bounds for the projected mesh, OR
    /// * the resolver cannot resolve the source node's face IDs (e.g. the
    ///   source operator is `TopologyChangingOperator` from the resolver's
    ///   perspective — `FilletOp`, `BooleanOp`, `SweepOp`).
    ///
    /// Resolution is **lazy**: each call invokes
    /// [`rge_cad_core::brep_face_ids_for_node`] and matches the projected
    /// mesh's per-triangle `TopologyFaceId` against the resolver's
    /// `Vec<(TopologyFaceId, BRepFaceId)>` mapping. The owner-seeded
    /// contract from D-7.2-α is preserved — no `BRepFaceId` is baked into
    /// `ProjectedMesh` storage.
    ///
    /// # Substrate posture
    ///
    /// This is the first cad-projection consumer of B-Rep face identity.
    /// For Cuboid roots, the answer is `Some(stable_brep_face_id)` for
    /// every triangle. For Cuboid → Fillet roots, the answer is `None`
    /// for every triangle — Fillet emits an unlabeled output AND the
    /// resolver classifies Fillet as a topology-changing operator.
    /// That double-`None` is the visible substrate-pressure on the
    /// `FILLET_OUTPUT_IDENTITY.md` parked design note (NOT an answer to
    /// it; the parked question stays parked).
    #[must_use]
    pub fn brep_face_id_for_triangle(
        &self,
        entity: EntityId,
        triangle_idx: usize,
        world: &World,
        graph: &OperatorGraph,
    ) -> Option<BRepFaceId> {
        let entity_ref = world.entity(entity)?;
        let handle = entity_ref.get::<BRepHandle>()?;
        let owner = handle.brep_owner?;
        let mesh = self.projected_mesh(entity)?;
        let face_labels = mesh.face_labels.as_ref()?;
        let topology_face_id = *face_labels.get(triangle_idx)?;
        let pairs = brep_face_ids_for_node(graph, mesh.source_node, owner).ok()?;
        pairs
            .into_iter()
            .find(|(t, _)| *t == topology_face_id)
            .map(|(_, brep_id)| brep_id)
    }

    /// Verify every [`EntityCadMap`] entry's [`NodeId`] is present in the
    /// supplied cad-graph. Returns orphan `(entity, node)` pairs that no
    /// longer resolve. An empty `Vec` means every entry in the projection's
    /// single source of truth (the [`EntityCadMap`]) references a live
    /// cad-graph node.
    ///
    /// **Convention**: callers SHOULD invoke this after restoring a
    /// [`CadProjection`] from PIE, with the cad-graph that was restored
    /// alongside. Orphan handles indicate a divergent-state PIE payload —
    /// the cad-graph and projection were captured at different times, or
    /// the cad-graph was not captured at all (PLAN §13.2 cad-graph
    /// `SnapshotParticipate` participant should always be co-restored).
    /// The orchestrator decides recovery: log a diagnostic, mark entities
    /// for re-projection, or error out.
    ///
    /// Symmetric counterpart of `cad-core`'s
    /// [`SnapshotParticipate`][rge_cad_core::CadGraph] impl — this method
    /// is the post-restore handle-validation guard that closes the
    /// silent-inconsistency window where the projection's cad-node
    /// references could orphan after divergent restore. Post-2026-05-08
    /// (`BRepHandle` `SSoT` refactor / Pairing-6 closure), the orphan check
    /// reads from the [`EntityCadMap`], which is now the only place the
    /// cad-node FK is stored.
    #[must_use]
    pub fn validate_handles(&self, cad: &CadGraph) -> Vec<(EntityId, NodeId)> {
        let mut orphans = Vec::new();
        for (entity, node) in self.entity_cad_map.iter() {
            if cad.graph().node(node).is_none() {
                orphans.push((entity, node));
            }
        }
        orphans
    }

    /// Remap an existing entity to a different cad-node.
    ///
    /// Used when a committed cad-graph mutation re-creates a node (for
    /// example, a parameter change produces a new content-derived [`NodeId`]).
    /// The function atomically updates the entity's binding in the
    /// [`EntityCadMap`] to `new_node`, then marks the entity dirty so the
    /// next [`Self::tick`] re-projects it.
    ///
    /// Pre-validates that `new_node` is unbound (or already bound to
    /// `entity`) so the map is never partially mutated on error.
    ///
    /// # Errors
    ///
    /// * [`EntityCadMapError::NotFound`] if `entity` was not registered in
    ///   the projection.
    /// * [`EntityCadMapError::DuplicateNode`] if `new_node` is already bound
    ///   to a different entity. The map is unchanged on this error.
    pub fn remap_entity(
        &mut self,
        entity: EntityId,
        new_node: NodeId,
    ) -> Result<(), EntityCadMapError> {
        // Pre-validate the entity is registered. Bail without mutating.
        if self.entity_cad_map.node_for(entity).is_none() {
            return Err(EntityCadMapError::NotFound);
        }
        // Pre-validate `new_node` is either unbound or already bound to
        // `entity` (which makes this remap a no-op). Bail without mutating
        // when it's bound to some OTHER entity.
        if let Some(existing_entity) = self.entity_cad_map.entity_for(new_node) {
            if existing_entity != entity {
                return Err(EntityCadMapError::DuplicateNode {
                    node: new_node,
                    existing_entity,
                });
            }
            // Same entity already bound to new_node — no-op except marking
            // dirty so the caller's expectation of "tick will re-project"
            // still holds.
            self.cache.mark_dirty(entity);
            return Ok(());
        }
        // Pre-checks passed. The two-step swap is now infallible.
        let removed = self.entity_cad_map.remove_entity(entity);
        debug_assert!(
            removed.is_some(),
            "pre-validated entity must still be registered",
        );
        // Both the forward slot for `entity` (just removed) and the reverse
        // slot for `new_node` (pre-validated as empty) are vacant; insert
        // cannot fail.
        let insert_result = self.entity_cad_map.insert(entity, new_node);
        debug_assert!(
            insert_result.is_ok(),
            "post-validation insert is invariantly successful",
        );
        let _ = insert_result;
        // Mark dirty so the next tick re-projects the entity at its new node.
        self.cache.mark_dirty(entity);
        Ok(())
    }

    /// Spawn a new ECS entity carrying a fresh [`BRepHandle`], register the
    /// `(entity, node)` binding in the [`EntityCadMap`] (the single source of
    /// truth for the cad-node FK post-2026-05-08 `SSoT` refactor), and mark
    /// the new entity dirty so the next [`tick`](Self::tick) projects its
    /// mesh.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectionError::EntityCadMap`] when the cad node is already
    /// bound to a different entity (we surface the upstream error through
    /// `From`).
    pub fn spawn_brep_entity(
        &mut self,
        world: &mut World,
        node: NodeId,
    ) -> Result<EntityId, ProjectionError> {
        // Post-2026-05-08 SSoT refactor: the BRepHandle does NOT carry the
        // cad-node FK any more — that lives exclusively in
        // `entity_cad_map`. The handle stores only projection bookkeeping
        // (mesh_id + last_projected_checkpoint).
        let entity = world.spawn_with(BRepHandle::new());
        match self.entity_cad_map.insert(entity, node) {
            Ok(()) => {
                self.cache.mark_dirty(entity);
                Ok(entity)
            }
            Err(e) => {
                // Roll back the spawn so we don't leak an orphan entity.
                world.despawn(entity);
                Err(e.into())
            }
        }
    }

    /// Despawn `entity`, drop its mesh from the cache, and clear the
    /// `EntityCadMap` entry. Returns `true` if the entity existed.
    pub fn despawn_brep_entity(&mut self, world: &mut World, entity: EntityId) -> bool {
        let existed = world.despawn(entity);
        self.entity_cad_map.remove_entity(entity);
        self.cache.forget_entity(entity);
        existed
    }

    /// Re-project every dirty entity, advancing per-entity `BRepHandle`
    /// metadata (`mesh_id` / `last_projected_checkpoint`) on success.
    ///
    /// Step-by-step:
    /// 1. Observe `cad.head()` and, if the head advanced, mark all known
    ///    entities dirty.
    /// 2. For each dirty entity, look up its bound cad node and project it.
    /// 3. Update the entity's `BRepHandle` component in `world` with the
    ///    fresh `mesh_id` and `last_projected_checkpoint`.
    /// 4. Clear the dirty set; record telemetry.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectionError`] on the first failed re-projection. Earlier
    /// successes within this tick are NOT rolled back — they remain valid;
    /// only the failing entity is left in its previous state.
    pub fn tick(
        &mut self,
        world: &mut World,
        cad: &CadGraph,
        tolerance: Tolerance,
    ) -> Result<TickReport, ProjectionError> {
        let head = cad.head();

        // Capture the entity list once so observe_checkpoint sees the same
        // set the rest of the tick will operate on.
        let known_entities: Vec<EntityId> = self.entity_cad_map.iter().map(|(e, _)| e).collect();
        self.cache
            .observe_checkpoint(head, known_entities.iter().copied());

        // Snapshot dirty set before iterating since insert_mesh mutates it.
        let dirty: Vec<EntityId> = self.cache.dirty_entities().iter().copied().collect();

        // Count clean entities as cache hits so the telemetry matches the
        // narrative "cache hit = re-projection avoided".
        let clean_count = known_entities.len().saturating_sub(dirty.len());
        for _ in 0..clean_count {
            self.cache.record_hit();
        }

        let mut reprojected = 0usize;
        for entity in &dirty {
            let Some(node) = self.entity_cad_map.node_for(*entity) else {
                // The entity was in `dirty` but no longer in the map (e.g.
                // it was despawned between ticks). Skip silently.
                continue;
            };
            let mesh = projection_geometry::project(cad, node, &mut self.tess_cache, tolerance)?;
            let mesh_id = self.cache.insert_mesh(*entity, mesh);

            // Update the BRepHandle component in the world.
            if let Some(mut em) = world.entity_mut(*entity) {
                if let Some(mut handle) = em.get_mut::<BRepHandle>() {
                    handle.mesh_id = Some(mesh_id);
                    handle.last_projected_checkpoint = Some(CheckpointTag::from(head));
                }
            }
            reprojected += 1;
        }
        self.cache.clear_dirty();

        let stats = self.cache.stats();
        Ok(TickReport {
            entities_reprojected: reprojected,
            cache_hits: usize::try_from(stats.hits).unwrap_or(usize::MAX),
            cache_misses: usize::try_from(stats.misses).unwrap_or(usize::MAX),
            head_advanced_to: head,
        })
    }
}

// ---------------------------------------------------------------------------
// SnapshotParticipate
// ---------------------------------------------------------------------------

/// Stable participant id for [`CadProjection`] in PIE snapshots.
const PARTICIPANT_ID: &str = "cad-projection.brep-handles";

/// Wire-format payload captured / restored by
/// [`CadProjection`]'s [`SnapshotParticipate`] impl.
///
/// Carries:
///
/// * The full [`EntityCadMap`] (so entity↔node mappings round-trip).
/// * The last cad-core checkpoint observed by the cache (so a tick after
///   restore on an unchanged graph correctly skips re-projection).
///
/// `ProjectedMesh` `Arc`s are NOT included — they re-derive on the next
/// tick. `next_mesh_id` is also not included; the receiving side starts at 0
/// and the tick re-projects all entities, generating fresh mesh ids.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParticipantPayload {
    entity_cad_map: EntityCadMap,
    last_seen_checkpoint: Option<CheckpointTag>,
}

/// `SnapshotParticipate` impl — captures `EntityCadMap` + last-seen checkpoint
/// so PIE round-trips preserve the entity↔node bridge across save/load.
///
/// **Co-restore convention** (PLAN §13.2 cross-architecture coherence): when
/// PIE-restoring `CadProjection`, the caller SHOULD also restore the matching
/// `cad-core.cad-graph` participant in the same `PieSnapshot::restore` call.
/// Otherwise call [`CadProjection::validate_handles`] with whatever cad-graph
/// is available afterwards to detect orphan `BRepHandle.cad_node` references
/// (a divergent-state PIE payload). Without this guard, post-restore ticks on
/// a missing cad node fail with `ProjectionError::NodeNotInGraph` rather than
/// silently producing stale meshes — but the empty-orphans contract is the
/// preferred invariant and the orchestrator is responsible for upholding it.
impl SnapshotParticipate for CadProjection {
    fn participant_id(&self) -> ParticipantId {
        ParticipantId::new(PARTICIPANT_ID)
    }

    fn capture(&self) -> Result<Vec<u8>, ParticipateError> {
        let payload = ParticipantPayload {
            entity_cad_map: self.entity_cad_map.clone(),
            last_seen_checkpoint: self.cache.last_seen_checkpoint().map(CheckpointTag::from),
        };
        postcard::to_allocvec(&payload).map_err(|e| ParticipateError::Custom(e.to_string()))
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError> {
        let payload: ParticipantPayload =
            postcard::from_bytes(bytes).map_err(|e| ParticipateError::Custom(e.to_string()))?;

        // Clean slate the cache; it re-derives on the next tick.
        self.cache = ProjectionCache::new();
        self.tess_cache = TessellationCache::new();
        self.entity_cad_map = payload.entity_cad_map;

        // Re-mark every known entity dirty so the next tick re-projects
        // everything. Note: we do NOT call observe_checkpoint here — we
        // leave last_seen_checkpoint at None so the next tick observes a
        // head change unconditionally and re-marks everything (in addition
        // to what we mark below). Either way, every known entity ends up
        // in `dirty` before the first post-restore re-projection runs.
        let entities: Vec<EntityId> = self.entity_cad_map.iter().map(|(e, _)| e).collect();
        self.cache.mark_all_dirty(entities);

        // We don't restore last_seen_checkpoint — letting the next tick
        // observe the current head guarantees re-projection regardless of
        // whether the head matches the captured one.
        let _ = payload.last_seen_checkpoint;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unit tests — top-level orchestrator
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_cad_core::{CuboidOp, OperatorNode};
    use rge_kernel_ecs::World;

    use super::*;

    fn tol() -> Tolerance {
        Tolerance::new(0.001).expect("tol")
    }

    /// Build a [`CadGraph`] with a single Cuboid(`w`,`h`,`d`) committed; return
    /// the graph + the new node id.
    fn cuboid_graph(w: f32, h: f32, d: f32) -> (CadGraph, NodeId) {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let n = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: w,
                height: h,
                depth: d,
            }))
            .expect("add");
        cad.graph_mut().expect("mut2").set_root(n).expect("root");
        cad.commit("first cuboid").expect("commit");
        (cad, n)
    }

    /// Test 1 — `spawn_brep_entity` inserts the [`BRepHandle`] and records
    /// the bidirectional mapping.
    #[test]
    fn spawn_brep_entity_inserts_handle_and_records_mapping() {
        let mut world = World::new();
        world.register_snapshot_component::<BRepHandle>();
        let mut projection = CadProjection::new();
        let (_cad, node) = cuboid_graph(1.0, 1.0, 1.0);

        let entity = projection
            .spawn_brep_entity(&mut world, node)
            .expect("spawn");

        // World has the BRepHandle.
        let er = world.entity(entity).expect("entity");
        let handle = er.get::<BRepHandle>().expect("handle");
        // Post-2026-05-08 SSoT refactor: the cad-node FK lives only in the
        // EntityCadMap — the handle does not store it.
        assert_eq!(handle.mesh_id, None);
        assert_eq!(handle.last_projected_checkpoint, None);

        // The single source of truth (EntityCadMap) records both directions.
        assert_eq!(projection.node_for(entity), Some(node));
        assert_eq!(projection.entity_for(node), Some(entity));
    }

    /// Test 2 — after a commit + tick, the entity's projected mesh has 8
    /// vertices and 12 triangles.
    #[test]
    fn tick_after_commit_projects_mesh() {
        let mut world = World::new();
        world.register_snapshot_component::<BRepHandle>();
        let mut projection = CadProjection::new();
        let (cad, node) = cuboid_graph(1.0, 1.0, 1.0);
        let entity = projection
            .spawn_brep_entity(&mut world, node)
            .expect("spawn");

        let report = projection.tick(&mut world, &cad, tol()).expect("tick");
        assert_eq!(report.entities_reprojected, 1);
        assert_eq!(report.head_advanced_to, cad.head());

        let mesh = projection.projected_mesh(entity).expect("mesh");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);

        // BRepHandle was updated with mesh_id + last_projected_checkpoint.
        let er = world.entity(entity).expect("entity");
        let handle = er.get::<BRepHandle>().expect("handle");
        assert!(handle.mesh_id.is_some());
        assert_eq!(
            handle.last_projected_checkpoint,
            Some(CheckpointTag::from(cad.head()))
        );
    }

    /// Test 3 — a second tick with the head unchanged is a no-op (no
    /// re-projections).
    #[test]
    fn tick_no_op_when_head_unchanged() {
        let mut world = World::new();
        world.register_snapshot_component::<BRepHandle>();
        let mut projection = CadProjection::new();
        let (cad, node) = cuboid_graph(1.0, 1.0, 1.0);
        let _entity = projection
            .spawn_brep_entity(&mut world, node)
            .expect("spawn");

        let r1 = projection.tick(&mut world, &cad, tol()).expect("tick1");
        assert_eq!(r1.entities_reprojected, 1);
        let r2 = projection.tick(&mut world, &cad, tol()).expect("tick2");
        assert_eq!(
            r2.entities_reprojected, 0,
            "head unchanged → no re-projection"
        );
    }

    /// Test 4 — despawn clears the world entity AND the projection mapping.
    #[test]
    fn despawn_brep_entity_clears_mapping_and_world_component() {
        let mut world = World::new();
        world.register_snapshot_component::<BRepHandle>();
        let mut projection = CadProjection::new();
        let (cad, node) = cuboid_graph(1.0, 1.0, 1.0);
        let entity = projection
            .spawn_brep_entity(&mut world, node)
            .expect("spawn");
        let _ = projection.tick(&mut world, &cad, tol()).expect("tick");
        assert!(projection.projected_mesh(entity).is_some());

        let removed = projection.despawn_brep_entity(&mut world, entity);
        assert!(removed);
        assert!(world.entity(entity).is_none());
        assert_eq!(projection.node_for(entity), None);
        assert_eq!(projection.entity_for(node), None);
        assert!(projection.projected_mesh(entity).is_none());
    }
}
