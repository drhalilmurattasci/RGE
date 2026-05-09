//! `cad_projection::projection_cache` — memoized [`ProjectedMesh`] storage,
//! per-entity dirty bits, and head-tracking glue.
//!
//! Failure class: snapshot-recoverable
//!
//! # Purpose
//!
//! `cad-core` already memoizes operator-graph evaluation through its
//! `TessellationCache`. This module memoizes the **projection-side** of the
//! pipeline:
//!
//! * The `Arc<ProjectedMesh>` for each ECS entity (so consumers don't pay the
//!   tessellation copy cost more than once).
//! * The dirty-set for "which entities need re-projection on the next tick".
//! * The last-seen `cad-core` checkpoint — when this differs from
//!   `CadGraph::head()` we mark every known entity dirty (head-advanced ⇒
//!   everything dirty). Per-node fine-grained dependency tracking is a
//!   future-dispatch concern.
//!
//! # Design notes
//!
//! * `BTreeMap` is used everywhere for deterministic iteration; `HashMap`
//!   would be marginally faster but determinism matters for snapshot byte
//!   stability and is cheap at the entity counts the editor sees.
//! * The cache does NOT own the `cad-core` `TessellationCache` — that is
//!   threaded into [`crate::CadProjection::tick`] from the lib-level
//!   orchestrator.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rge_cad_core::CheckpointId;
use rge_kernel_ecs::EntityId;
use serde::{Deserialize, Serialize};

use crate::projection_geometry::{ProjectedMesh, ProjectedMeshId};

// ---------------------------------------------------------------------------
// CacheStats
// ---------------------------------------------------------------------------

/// Lightweight counters for cache effectiveness telemetry.
///
/// `hits` = re-projection avoided because the entity was clean.
/// `misses` = re-projection ran (entity was dirty or unknown).
/// `reprojections` = total re-project calls actually performed; equal to
/// `misses` so far but separated so future per-node-dirty bookkeeping can
/// distinguish them.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of cache hits (re-projection avoided).
    pub hits: u64,
    /// Number of cache misses (re-projection performed).
    pub misses: u64,
    /// Number of re-projections actually executed.
    pub reprojections: u64,
}

// ---------------------------------------------------------------------------
// ProjectionCache
// ---------------------------------------------------------------------------

/// Per-entity projected-mesh storage with dirty-bit and head-tracking
/// bookkeeping.
///
/// Owned by [`crate::CadProjection`].
#[derive(Debug, Default)]
pub struct ProjectionCache {
    /// Last `cad-core` checkpoint we observed. When `tick` sees a different
    /// head, every known entity is marked dirty.
    last_seen_checkpoint: Option<CheckpointId>,
    /// Monotonic id counter for [`ProjectedMeshId`] allocation.
    next_mesh_id: u64,
    /// All projected meshes currently held by the cache, keyed by their id.
    meshes: BTreeMap<ProjectedMeshId, Arc<ProjectedMesh>>,
    /// Each known entity's currently-bound mesh id.
    entity_meshes: BTreeMap<EntityId, ProjectedMeshId>,
    /// Entities whose mesh needs re-projecting on the next tick.
    dirty: BTreeSet<EntityId>,
    /// Hit/miss/reprojection counters.
    stats: CacheStats,
}

impl ProjectionCache {
    /// Construct an empty cache (no entries, no checkpoint observed).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The last `cad-core` checkpoint observed by this cache, if any.
    #[must_use]
    pub fn last_seen_checkpoint(&self) -> Option<CheckpointId> {
        self.last_seen_checkpoint
    }

    /// Look up the projected mesh currently bound to `entity`, if any.
    #[must_use]
    pub fn mesh_for(&self, entity: EntityId) -> Option<&Arc<ProjectedMesh>> {
        let mesh_id = self.entity_meshes.get(&entity)?;
        self.meshes.get(mesh_id)
    }

    /// Look up the [`ProjectedMeshId`] currently bound to `entity`, if any.
    #[must_use]
    pub fn mesh_id_for(&self, entity: EntityId) -> Option<ProjectedMeshId> {
        self.entity_meshes.get(&entity).copied()
    }

    /// Mark `entity` dirty so the next tick re-projects it.
    pub fn mark_dirty(&mut self, entity: EntityId) {
        self.dirty.insert(entity);
    }

    /// Mark every entity in `entities` dirty.
    pub fn mark_all_dirty<I: IntoIterator<Item = EntityId>>(&mut self, entities: I) {
        for e in entities {
            self.dirty.insert(e);
        }
    }

    /// Borrow the dirty-entity set.
    #[must_use]
    pub fn dirty_entities(&self) -> &BTreeSet<EntityId> {
        &self.dirty
    }

    /// Snapshot of the cache statistics.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// Insert (or replace) the cached mesh for `entity`. Returns the new
    /// [`ProjectedMeshId`]. The bookkeeping consequences:
    ///
    /// * The previous mesh entry, if any, is removed from `meshes`.
    /// * The dirty bit for `entity` is cleared.
    /// * `stats.reprojections` is incremented.
    /// * `stats.misses` is incremented.
    pub(crate) fn insert_mesh(
        &mut self,
        entity: EntityId,
        mesh: Arc<ProjectedMesh>,
    ) -> ProjectedMeshId {
        if let Some(prev_id) = self.entity_meshes.get(&entity).copied() {
            self.meshes.remove(&prev_id);
        }
        let id = ProjectedMeshId(self.next_mesh_id);
        self.next_mesh_id = self.next_mesh_id.saturating_add(1);
        self.meshes.insert(id, mesh);
        self.entity_meshes.insert(entity, id);
        self.dirty.remove(&entity);
        self.stats.reprojections = self.stats.reprojections.saturating_add(1);
        self.stats.misses = self.stats.misses.saturating_add(1);
        id
    }

    /// Notify the cache of the cad graph's current head checkpoint.
    ///
    /// If `head` differs from `last_seen_checkpoint`, every entity in
    /// `all_entities` is marked dirty (head-advanced ⇒ everything dirty).
    /// `last_seen_checkpoint` is updated unconditionally.
    pub(crate) fn observe_checkpoint<I: IntoIterator<Item = EntityId>>(
        &mut self,
        head: CheckpointId,
        all_entities: I,
    ) {
        let head_changed = self.last_seen_checkpoint != Some(head);
        if head_changed {
            for e in all_entities {
                self.dirty.insert(e);
            }
            self.last_seen_checkpoint = Some(head);
        }
    }

    /// Record a cache hit (re-projection avoided because the entity was
    /// clean). Used by [`crate::CadProjection::tick`] for telemetry.
    pub(crate) fn record_hit(&mut self) {
        self.stats.hits = self.stats.hits.saturating_add(1);
    }

    /// Forget which entities are currently dirty without dropping any cached
    /// meshes. Called by `tick` after dispatching the dirty work for a frame.
    pub(crate) fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Drop the entry for `entity` from the cache (does NOT touch any
    /// `EntityCadMap` or world component — those belong to the caller).
    pub(crate) fn forget_entity(&mut self, entity: EntityId) {
        if let Some(prev_id) = self.entity_meshes.remove(&entity) {
            self.meshes.remove(&prev_id);
        }
        self.dirty.remove(&entity);
    }

    /// Drop all cached meshes. `stats` is preserved; `last_seen_checkpoint`
    /// is preserved. `dirty` is preserved (caller may want to re-project on
    /// the next tick).
    pub fn clear_meshes(&mut self) {
        self.meshes.clear();
        self.entity_meshes.clear();
    }

    /// Reset hit/miss/reprojection counters.
    pub fn reset_stats(&mut self) {
        self.stats = CacheStats::default();
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_graph_foundation::NodeId;

    use super::*;
    use crate::projection_geometry::CheckpointTag;

    fn dummy_mesh() -> Arc<ProjectedMesh> {
        Arc::new(ProjectedMesh {
            positions: vec![[0.0, 0.0, 0.0]],
            indices: vec![],
            source_node: NodeId::from_raw(1),
            source_checkpoint: CheckpointTag(0),
            face_labels: None,
        })
    }

    #[test]
    fn new_cache_has_no_checkpoint() {
        let cache = ProjectionCache::new();
        assert_eq!(cache.last_seen_checkpoint(), None);
        assert_eq!(cache.dirty_entities().len(), 0);
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.reprojections, 0);
    }

    #[test]
    fn mesh_for_unknown_entity_is_none() {
        let cache = ProjectionCache::new();
        let e = EntityId::new();
        assert!(cache.mesh_for(e).is_none());
        assert!(cache.mesh_id_for(e).is_none());
    }

    #[test]
    fn mark_dirty_records_entity() {
        let mut cache = ProjectionCache::new();
        let e1 = EntityId::new();
        let e2 = EntityId::new();
        cache.mark_dirty(e1);
        assert_eq!(cache.dirty_entities().len(), 1);
        cache.mark_all_dirty([e1, e2]);
        assert_eq!(cache.dirty_entities().len(), 2);
        assert!(cache.dirty_entities().contains(&e1));
        assert!(cache.dirty_entities().contains(&e2));
    }

    #[test]
    fn observe_checkpoint_marks_all_entities_dirty_on_advance() {
        let mut cache = ProjectionCache::new();
        let e1 = EntityId::new();
        let e2 = EntityId::new();
        // Initial observation populates dirty set with all known entities.
        cache.observe_checkpoint(CheckpointId(1), [e1, e2]);
        assert_eq!(cache.dirty_entities().len(), 2);
        assert_eq!(cache.last_seen_checkpoint(), Some(CheckpointId(1)));

        // Clear and observe a new head; entities re-marked dirty.
        cache.clear_dirty();
        assert_eq!(cache.dirty_entities().len(), 0);
        cache.observe_checkpoint(CheckpointId(2), [e1, e2]);
        assert_eq!(cache.dirty_entities().len(), 2);
    }

    #[test]
    fn observe_checkpoint_idempotent_when_head_unchanged() {
        let mut cache = ProjectionCache::new();
        let e = EntityId::new();
        cache.observe_checkpoint(CheckpointId(5), [e]);
        cache.clear_dirty();
        // Re-observing the same head should NOT re-mark dirty.
        cache.observe_checkpoint(CheckpointId(5), [e]);
        assert_eq!(cache.dirty_entities().len(), 0);
        assert_eq!(cache.last_seen_checkpoint(), Some(CheckpointId(5)));
    }

    #[test]
    fn clear_meshes_drops_arcs_but_keeps_stats() {
        let mut cache = ProjectionCache::new();
        let e = EntityId::new();
        cache.insert_mesh(e, dummy_mesh());
        let pre = cache.stats();
        assert!(cache.mesh_for(e).is_some());
        cache.clear_meshes();
        assert!(cache.mesh_for(e).is_none());
        let post = cache.stats();
        assert_eq!(pre.misses, post.misses);
        assert_eq!(pre.reprojections, post.reprojections);
    }

    #[test]
    fn insert_mesh_clears_dirty_for_entity() {
        let mut cache = ProjectionCache::new();
        let e = EntityId::new();
        cache.mark_dirty(e);
        assert!(cache.dirty_entities().contains(&e));
        let _id = cache.insert_mesh(e, dummy_mesh());
        assert!(!cache.dirty_entities().contains(&e));
        assert!(cache.mesh_for(e).is_some());
    }
}
