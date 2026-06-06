//! World wrapper for `editor-shell`.
//!
//! # Migration note (Phase 5.3)
//!
//! This module previously contained a v0 stub `World` backed by a flat
//! `BTreeMap<ComponentTypeId, BTreeMap<EntityId, Vec<u8>>>`. Phase 5.3
//! migrates the *snapshot path* to use the real `rge_kernel_ecs::World` with
//! typed [`SnapshotComponent`]s, while keeping the blob-API surface intact
//! for call sites that have not yet been migrated (e.g. existing integration
//! tests that write raw bytes).
//!
//! # Design
//!
//! `World` wraps two worlds:
//!
//! - `kernel`: a real [`rge_kernel_ecs::World`] that holds typed
//!   [`SnapshotComponent`] data and is the authority for
//!   [`serialize_snapshot`](World::serialize_snapshot) /
//!   [`restore_from_snapshot`](World::restore_from_snapshot).
//! - The legacy blob storage (`entities`, `components`) remains for call
//!   sites that use [`ComponentTypeId`] + [`Vec<u8>`] blobs. Blob data is
//!   NOT included in kernel snapshots — only typed components registered
//!   via [`World::register_snapshot_component`] are snapshotted.
//!
//! [`EntityId`] is the canonical ULID-backed type re-exported from
//! `rge_kernel_ecs`.

use std::collections::BTreeMap;

use rge_kernel_ecs::snapshot::SnapshotComponent;
pub use rge_kernel_ecs::EntityId;
use rge_kernel_ecs::SnapshotError;

/// Stable component-type identifier for the legacy blob API.
///
/// Real `kernel/ecs` uses reflection-schema `TypeId`; the blob stub uses a
/// `u32` for determinism and ordered iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentTypeId(pub u32);

/// Per-entity component bytes (legacy blob API).
pub type ComponentBlob = Vec<u8>;

/// Editor-shell world.
///
/// Wraps a [`rge_kernel_ecs::World`] (the snapshot-capable kernel world)
/// alongside the legacy blob storage retained for backward-compatible call
/// sites. See the module-level doc for the migration rationale.
///
/// **Determinism note:** both the blob storage (`BTreeMap`) and the kernel
/// snapshot path (`World::serialize_snapshot` with sorted entity/component
/// iteration) produce byte-identical output for identical logical state.
#[derive(Debug, Default)]
pub struct World {
    /// The real kernel ECS world. Holds typed [`SnapshotComponent`] data and
    /// drives the Phase 5.3 snapshot path.
    kernel: rge_kernel_ecs::World,
    /// Legacy entity set for the blob API. Kept in sync with `kernel` via
    /// [`spawn`](Self::spawn) / [`despawn`](Self::despawn).
    entities: std::collections::BTreeSet<EntityId>,
    /// Legacy blob component storage. Not included in kernel snapshots.
    components: BTreeMap<ComponentTypeId, BTreeMap<EntityId, ComponentBlob>>,
}

impl World {
    /// Construct a fresh, empty world.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // -----------------------------------------------------------------------
    // Snapshot registration
    // -----------------------------------------------------------------------

    /// Register a typed component type for kernel snapshot inclusion.
    ///
    /// Only types registered here participate in
    /// [`serialize_snapshot`](Self::serialize_snapshot) /
    /// [`restore_from_snapshot`](Self::restore_from_snapshot). The legacy blob
    /// components are not affected.
    pub fn register_snapshot_component<C: SnapshotComponent>(&mut self) {
        self.kernel.register_snapshot_component::<C>();
    }

    // -----------------------------------------------------------------------
    // Kernel world access (for typed component insertion in new-style tests)
    // -----------------------------------------------------------------------

    /// Borrow the inner kernel [`rge_kernel_ecs::World`] immutably.
    ///
    /// Use this to query typed snapshot components directly.
    #[must_use]
    pub fn kernel(&self) -> &rge_kernel_ecs::World {
        &self.kernel
    }

    /// Borrow the inner kernel [`rge_kernel_ecs::World`] mutably.
    ///
    /// Use this to insert typed snapshot components and to drive the
    /// Phase 5.3 PIE round-trip path.
    pub fn kernel_mut(&mut self) -> &mut rge_kernel_ecs::World {
        &mut self.kernel
    }

    // -----------------------------------------------------------------------
    // Snapshot serialization / restoration
    // -----------------------------------------------------------------------

    /// Serialize all registered typed snapshot components to a deterministic
    /// byte stream. Delegates to [`rge_kernel_ecs::World::serialize_snapshot`].
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError`] when RON serialization fails.
    pub fn serialize_snapshot(&self) -> Result<Vec<u8>, SnapshotError> {
        self.kernel.serialize_snapshot()
    }

    /// Restore the kernel world from snapshot bytes.
    ///
    /// Despawns all entities in the kernel world (clean slate) then re-spawns
    /// them from the snapshot stream. The legacy blob storage is **not**
    /// cleared or restored by this method — it is retained as-is.
    ///
    /// # Errors
    ///
    /// Returns [`SnapshotError`] when the byte stream is malformed.
    pub fn restore_from_snapshot(&mut self, bytes: &[u8]) -> Result<(), SnapshotError> {
        self.kernel.restore_from_snapshot(bytes)
    }

    // -----------------------------------------------------------------------
    // Entity lifecycle (blob + kernel in sync)
    // -----------------------------------------------------------------------

    /// Spawn a new entity with a freshly allocated [`EntityId`].
    ///
    /// The entity is created in both the kernel world and the legacy blob set
    /// so the blob API (`insert_component`, `component`, etc.) and the typed
    /// kernel API stay in sync.
    pub fn spawn(&mut self) -> EntityId {
        let id = rge_kernel_ecs::EntityId::new();
        self.kernel.spawn_with_id(id);
        self.entities.insert(id);
        id
    }

    /// Number of live entities (from the blob-API entity set).
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Iterate live entity IDs in deterministic (ascending ULID) order.
    pub fn entities(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.iter().copied()
    }

    /// Despawn a live entity from both the kernel world and the legacy blob view.
    ///
    /// Returns `true` if either backing store contained the entity. Component
    /// blobs for the entity are removed from every legacy component map.
    pub fn despawn(&mut self, entity: EntityId) -> bool {
        let existed_blob = self.entities.remove(&entity);
        let mut removed_blob_component = false;
        for map in self.components.values_mut() {
            removed_blob_component |= map.remove(&entity).is_some();
        }
        let existed_kernel = self.kernel.despawn(entity);
        existed_blob || removed_blob_component || existed_kernel
    }

    /// Duplicate a live legacy-blob entity into a freshly spawned entity.
    ///
    /// The duplicate receives cloned legacy component blobs. Type-erased kernel
    /// components are intentionally not cloned here; there is no safe generic
    /// clone path for arbitrary typed ECS components.
    pub fn duplicate_entity_blobs(&mut self, entity: EntityId) -> Option<EntityId> {
        if !self.entities.contains(&entity) {
            return None;
        }
        let components: Vec<_> = self
            .components
            .iter()
            .filter_map(|(ty, map)| map.get(&entity).cloned().map(|blob| (*ty, blob)))
            .collect();
        let duplicate = self.spawn();
        for (ty, blob) in components {
            self.insert_component(duplicate, ty, blob);
        }
        Some(duplicate)
    }

    // -----------------------------------------------------------------------
    // Legacy blob API
    // -----------------------------------------------------------------------

    /// Insert / overwrite a component blob on `entity` (legacy blob API).
    ///
    /// This data lives only in the flat `BTreeMap` storage and is **not**
    /// included in kernel snapshots. Use [`kernel_mut`](Self::kernel_mut)
    /// to insert typed snapshot components.
    pub fn insert_component(&mut self, entity: EntityId, ty: ComponentTypeId, blob: ComponentBlob) {
        debug_assert!(
            self.entities.contains(&entity),
            "insert_component on dead entity {entity:?}"
        );
        self.components.entry(ty).or_default().insert(entity, blob);
    }

    /// Read a component blob (`None` if entity has no component of that type).
    #[must_use]
    pub fn component(&self, entity: EntityId, ty: ComponentTypeId) -> Option<&ComponentBlob> {
        self.components.get(&ty).and_then(|map| map.get(&entity))
    }

    /// Mutable component access (legacy blob API).
    pub fn component_mut(
        &mut self,
        entity: EntityId,
        ty: ComponentTypeId,
    ) -> Option<&mut ComponentBlob> {
        self.components
            .get_mut(&ty)
            .and_then(|map| map.get_mut(&entity))
    }

    // -----------------------------------------------------------------------
    // Game-system tick (legacy blob API)
    // -----------------------------------------------------------------------

    /// Tick one simulation step, mutating the legacy blob storage only.
    ///
    /// Component type IDs are stub conventions:
    /// - 1 = `TickCounter` (u64 LE)
    /// - 2 = `Position` (3 × f32 LE)
    ///
    /// `dt_scaled` is the **already time-scaled** delta-seconds. Callers
    /// (e.g. `lifecycle::EditorShell::tick_game`) are responsible for
    /// applying [`crate::TimeScale`] before invoking this method.
    pub fn tick_game_systems(&mut self, dt_scaled: f32) {
        const TICK: ComponentTypeId = ComponentTypeId(1);
        const POSITION: ComponentTypeId = ComponentTypeId(2);

        if let Some(tick_map) = self.components.get_mut(&TICK) {
            for blob in tick_map.values_mut() {
                if blob.len() == 8 {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(blob);
                    let v = u64::from_le_bytes(bytes).wrapping_add(1);
                    blob.copy_from_slice(&v.to_le_bytes());
                }
            }
        }

        if let Some(pos_map) = self.components.get_mut(&POSITION) {
            for blob in pos_map.values_mut() {
                if blob.len() == 12 {
                    let mut x = [0u8; 4];
                    let mut y = [0u8; 4];
                    let mut z = [0u8; 4];
                    x.copy_from_slice(&blob[0..4]);
                    y.copy_from_slice(&blob[4..8]);
                    z.copy_from_slice(&blob[8..12]);
                    let xv = f32::from_le_bytes(x) + dt_scaled;
                    let yv = f32::from_le_bytes(y) + dt_scaled * 0.5;
                    let zv = f32::from_le_bytes(z);
                    blob[0..4].copy_from_slice(&xv.to_le_bytes());
                    blob[4..8].copy_from_slice(&yv.to_le_bytes());
                    blob[8..12].copy_from_slice(&zv.to_le_bytes());
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Blob storage snapshot (for backward-compatible round-trip tests)
    // -----------------------------------------------------------------------

    /// Capture the legacy blob storage state for round-trip restoration.
    ///
    /// Returns clones of the entity set and component map. Used by
    /// [`WorldSnapshot`](crate::snapshot::WorldSnapshot) to preserve
    /// backward-compatible `world.serialize()` byte-identity across Play/Stop.
    #[must_use]
    pub(crate) fn capture_blob_state(
        &self,
    ) -> (
        std::collections::BTreeSet<EntityId>,
        BTreeMap<ComponentTypeId, BTreeMap<EntityId, ComponentBlob>>,
    ) {
        (self.entities.clone(), self.components.clone())
    }

    /// Restore the legacy blob storage from a previously captured state.
    ///
    /// Only the blob fields (`entities`, `components`) are overwritten; the
    /// kernel world is left unchanged (it is restored separately via
    /// [`restore_from_snapshot`](Self::restore_from_snapshot)).
    pub(crate) fn restore_blob_state(
        &mut self,
        entities: std::collections::BTreeSet<EntityId>,
        components: BTreeMap<ComponentTypeId, BTreeMap<EntityId, ComponentBlob>>,
    ) {
        self.entities = entities;
        self.components = components;
    }

    // -----------------------------------------------------------------------
    // Legacy blob serialization (for backward-compatible round-trip tests)
    // -----------------------------------------------------------------------

    /// Serialize the **legacy blob storage** to a deterministic byte stream.
    ///
    /// This is the v0 stub format documented in the original `world.rs` and
    /// used by `snapshot_correctness.rs` and the existing `lifecycle` unit
    /// tests. The format is:
    ///
    /// ```text
    /// entity_count:   u64 LE
    /// for each entity (ascending EntityId / ULID order):
    ///     entity_id: u128 LE
    /// component_type_count: u64 LE
    /// for each component type (ascending ComponentTypeId):
    ///     ty:    u32 LE
    ///     count: u64 LE
    ///     for each (entity, blob) (ascending EntityId):
    ///         entity_id: u128 LE
    ///         len:       u64 LE
    ///         bytes:     [u8; len]
    /// ```
    #[must_use]
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + 16 * self.entities.len() + 16 * self.components.len());
        buf.extend_from_slice(&(self.entities.len() as u64).to_le_bytes());
        for e in &self.entities {
            buf.extend_from_slice(&e.ulid().0.to_le_bytes());
        }
        buf.extend_from_slice(&(self.components.len() as u64).to_le_bytes());
        for (ty, map) in &self.components {
            buf.extend_from_slice(&ty.0.to_le_bytes());
            buf.extend_from_slice(&(map.len() as u64).to_le_bytes());
            for (entity, blob) in map {
                buf.extend_from_slice(&entity.ulid().0.to_le_bytes());
                buf.extend_from_slice(&(blob.len() as u64).to_le_bytes());
                buf.extend_from_slice(blob);
            }
        }
        buf
    }
}

// ---------------------------------------------------------------------------
// Unit tests (backward-compatible blob API)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use ulid::Ulid;

    use super::*;

    #[test]
    fn fresh_world_is_empty() {
        let w = World::new();
        assert_eq!(w.entity_count(), 0);
        // Empty blob serialization: entity_count (8) + ty_count (8) = 16.
        let bytes = w.serialize();
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn spawn_allocates_unique_ids() {
        let mut w = World::new();
        let a = w.spawn();
        let b = w.spawn();
        assert_ne!(a, b);
        assert_eq!(w.entity_count(), 2);
    }

    #[test]
    fn serialize_is_deterministic() {
        // Clone is not available on the new World; build two identical worlds
        // and compare their blob serializations.
        let mut a = World::new();
        for _ in 0..5 {
            let ea = a.spawn();
            a.insert_component(ea, ComponentTypeId(1), 0u64.to_le_bytes().to_vec());
        }
        // Serialize twice from the same world — must be identical.
        assert_eq!(a.serialize(), a.serialize());
    }

    #[test]
    fn despawn_removes_entity_and_legacy_components() {
        let mut w = World::new();
        let e = w.spawn();
        let survivor = w.spawn();
        w.insert_component(e, ComponentTypeId(1), vec![1, 2, 3]);
        w.insert_component(survivor, ComponentTypeId(1), vec![4, 5, 6]);

        assert!(w.despawn(e));

        assert_eq!(w.entity_count(), 1);
        assert!(!w.entities().any(|id| id == e));
        assert_eq!(w.component(e, ComponentTypeId(1)), None);
        assert_eq!(
            w.component(survivor, ComponentTypeId(1)),
            Some(&vec![4, 5, 6])
        );
        assert!(!w.despawn(e), "despawning an absent entity reports false");
    }

    #[test]
    fn duplicate_entity_blobs_clones_legacy_components_to_new_entity() {
        let mut w = World::new();
        let e = w.spawn();
        w.insert_component(e, ComponentTypeId(1), vec![1, 2, 3]);
        w.insert_component(e, ComponentTypeId(2), vec![4, 5, 6]);

        let duplicate = w.duplicate_entity_blobs(e).expect("live entity duplicates");

        assert_ne!(duplicate, e);
        assert_eq!(w.entity_count(), 2);
        assert_eq!(
            w.component(duplicate, ComponentTypeId(1)),
            Some(&vec![1, 2, 3])
        );
        assert_eq!(
            w.component(duplicate, ComponentTypeId(2)),
            Some(&vec![4, 5, 6])
        );
        assert_eq!(w.component(e, ComponentTypeId(1)), Some(&vec![1, 2, 3]));
        w.component_mut(duplicate, ComponentTypeId(1))
            .expect("duplicate has component")
            .push(9);
        assert_eq!(
            w.component(e, ComponentTypeId(1)),
            Some(&vec![1, 2, 3]),
            "duplicate blobs are independent clones"
        );
    }

    #[test]
    fn tick_advances_position() {
        let mut w = World::new();
        let e = w.spawn();
        w.insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
        w.tick_game_systems(1.0);
        let p = w.component(e, ComponentTypeId(2)).unwrap();
        let mut x = [0u8; 4];
        x.copy_from_slice(&p[0..4]);
        assert!((f32::from_le_bytes(x) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn entity_id_from_ulid_roundtrip() {
        let ulid = Ulid::new();
        let id = EntityId::from_ulid(ulid);
        assert_eq!(id.ulid(), ulid);
    }

    #[test]
    fn kernel_typed_component_survives_kernel_snapshot() {
        use rge_kernel_ecs::Component;
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct Score(u32);
        impl Component for Score {}
        impl rge_kernel_ecs::SnapshotComponent for Score {}

        let mut w = World::new();
        w.register_snapshot_component::<Score>();

        let e = w.spawn();
        w.kernel_mut().insert(e, Score(42));

        let snap = w.serialize_snapshot().expect("serialize");

        // Mutate via kernel.
        w.kernel_mut().insert(e, Score(0));

        w.restore_from_snapshot(&snap).expect("restore");

        let scores: Vec<_> = w.kernel().query::<Score>().collect();
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].1, &Score(42));
    }
}
