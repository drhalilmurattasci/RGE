//! `cad_projection::projection_structural` — `BRepHandle` ECS component +
//! `EntityCadMap` bidirectional `EntityId` ↔ `NodeId` mapping.
//!
//! Failure class: snapshot-recoverable
//!
//! # Purpose
//!
//! This module owns the **structural** half of the CAD ↔ ECS bridge: who-is-who.
//! It does NOT own geometry (that's [`crate::projection_geometry`]) and it does
//! NOT own caching / dirty tracking (that's [`crate::projection_cache`]).
//!
//! Per [PLAN.md §1.5.4.5](../../../plans/PLAN.md), `projection_structural` MUST
//! NOT import from `projection_runtime` or `projection_editor`. The
//! `projection-modules` lint enforces this. Importing from
//! `projection_geometry` and `projection_cache` is permitted.
//!
//! # Components
//!
//! * [`BRepHandle`] — ECS component carrying the most recently projected
//!   mesh id and the checkpoint at which that mesh was projected. Per the
//!   2026-05-08 `BRepHandle` `SSoT` refactor (Pairing-6 closure), the
//!   cad-node FK is owned exclusively by [`EntityCadMap`] — the handle no
//!   longer stores a `cad_node` field. Consumers look up the node at access
//!   time via [`crate::CadProjection::node_for`].
//! * [`EntityCadMap`] — owned by [`crate::CadProjection`]; an
//!   atomic-bidirectional `BTreeMap` (`EntityId` ↔ `NodeId`). The single
//!   source of truth for entity ↔ cad-node mappings.
//! * [`EntityCadMapError`] — duplicate / not-found errors raised by
//!   [`EntityCadMap`].
//!
//! Note: the [`CheckpointTag`] proxy lives in [`crate::projection_geometry`]
//! because [`crate::projection_geometry::ProjectedMesh`] also needs it; the
//! two types share the same provenance metadata.

use std::collections::BTreeMap;

use rge_cad_core::BRepOwnerId;
use rge_kernel_ecs::{Component, EntityId, SnapshotComponent};
use rge_kernel_graph_foundation::NodeId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::projection_geometry::{CheckpointTag, ProjectedMeshId};

// ---------------------------------------------------------------------------
// EntityIdProxy — serialization bridge for EntityId
// ---------------------------------------------------------------------------
//
// `rge-kernel-ecs` does not enable `ulid`'s optional `serde` feature, so
// `EntityId` itself has no `Serialize` / `Deserialize` impl. We bridge the
// gap by serialising via the `Ulid` value (which DOES implement serde when
// the `ulid/serde` feature is enabled in this crate's Cargo.toml).

/// Internal serde-transparent newtype for round-tripping `EntityId` through
/// its underlying ulid `u128`.
///
/// Used by [`EntityCadMap`]'s manual `Serialize`/`Deserialize` impls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct EntityIdProxy(ulid::Ulid);

impl From<EntityId> for EntityIdProxy {
    fn from(id: EntityId) -> Self {
        Self(id.ulid())
    }
}

impl From<EntityIdProxy> for EntityId {
    fn from(p: EntityIdProxy) -> Self {
        EntityId::from_ulid(p.0)
    }
}

// ---------------------------------------------------------------------------
// BRepHandle
// ---------------------------------------------------------------------------

/// ECS component carrying projection bookkeeping for a B-Rep entity.
///
/// Stores the most recently projected mesh id and the checkpoint at which
/// the projection was last performed. Both are `Option` because a freshly
/// inserted handle has not been projected yet — the next
/// [`crate::CadProjection::tick`] call fills them in.
///
/// **Single source of truth for the cad-node FK** (post-2026-05-08 `SSoT`
/// refactor, Pairing-6 closure): the entity ↔ cad-node mapping lives
/// **only** in [`EntityCadMap`], owned by [`crate::CadProjection`]. The
/// handle deliberately does not store a `cad_node` field; consumers look up
/// the node via [`crate::CadProjection::node_for`] at access time. Removing
/// the duplicated FK eliminates an entire class of drift bugs (handle
/// pointing at one node while the map points at another) that were possible
/// before this refactor.
///
/// `BRepHandle` implements [`SnapshotComponent`] so its bookkeeping fields
/// round-trip through PIE snapshots. The `mesh_id` /
/// `last_projected_checkpoint` fields are stable metadata; they re-derive on
/// the next tick after restore.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BRepHandle {
    /// Most recently projected mesh id, if any.
    pub mesh_id: Option<ProjectedMeshId>,
    /// Checkpoint at which `mesh_id` was projected, if any.
    pub last_projected_checkpoint: Option<CheckpointTag>,
    /// Owner-seed for stable [`rge_cad_core::BRepFaceId`] /
    /// [`rge_cad_core::BRepEdgeId`] derivation. `None` means the entity has
    /// no associated B-Rep identity space (the legacy default for
    /// pre-D-projection-α handles). When `Some`, used by
    /// [`crate::CadProjection::brep_face_id_for_triangle`] to resolve
    /// `TopologyFaceId` → `BRepFaceId` lazily through the resolver.
    ///
    /// `#[serde(default)]` keeps PIE snapshots round-trip-compatible with
    /// pre-this-dispatch serialised handles: the deserialiser fills the
    /// missing field with `Option::default()` (i.e. `None`).
    #[serde(default)]
    pub brep_owner: Option<BRepOwnerId>,
}

impl BRepHandle {
    /// Construct a fresh `BRepHandle`. No mesh has been projected yet — the
    /// next tick will fill in `mesh_id` and `last_projected_checkpoint`.
    ///
    /// **Note (post-2026-05-08 `BRepHandle` `SSoT` refactor)**: this
    /// constructor no longer takes a [`NodeId`]. The cad-node FK is owned
    /// exclusively by [`EntityCadMap`] (single source of truth per Pairing-6
    /// closure). Use [`crate::CadProjection::spawn_brep_entity`] to spawn a
    /// `BRepHandle` entity together with its corresponding map entry; or
    /// insert via free fns if you manage the map separately.
    ///
    /// `brep_owner` defaults to `None`. Use [`Self::with_brep_owner`] for
    /// builder-style attachment of a B-Rep identity space.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mesh_id: None,
            last_projected_checkpoint: None,
            brep_owner: None,
        }
    }

    /// Set the B-Rep owner seed. Returns `self` for builder-style chaining.
    ///
    /// The owner-seed is the substrate input that lifts per-tessellation
    /// `TopologyFaceId` (sequential, per-rebuild) into stable
    /// [`rge_cad_core::BRepFaceId`] (rebuild-stable, owner-seeded). Setting
    /// the owner does NOT mutate the entity's projected mesh; it only
    /// records the owner so [`crate::CadProjection::brep_face_id_for_triangle`]
    /// can resolve identities at query time.
    #[must_use]
    pub fn with_brep_owner(mut self, owner: BRepOwnerId) -> Self {
        self.brep_owner = Some(owner);
        self
    }
}

impl Default for BRepHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for BRepHandle {}
impl SnapshotComponent for BRepHandle {}

// ---------------------------------------------------------------------------
// EntityCadMapError
// ---------------------------------------------------------------------------

/// Errors raised by [`EntityCadMap`] when its bidirectional invariant would be
/// violated, or when an entry is not found.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EntityCadMapError {
    /// Caller attempted to insert a mapping whose `entity` already exists in
    /// the map (currently bound to a different cad node).
    #[error("EntityCadMap: entity {entity} already mapped to a different node ({existing_node})")]
    DuplicateEntity {
        /// The entity that is already present.
        entity: EntityId,
        /// The node it is already bound to.
        existing_node: NodeId,
    },
    /// Caller attempted to insert a mapping whose `node` already exists in the
    /// map (currently bound to a different entity).
    #[error("EntityCadMap: node {node} already mapped to a different entity ({existing_entity})")]
    DuplicateNode {
        /// The node that is already present.
        node: NodeId,
        /// The entity it is already bound to.
        existing_entity: EntityId,
    },
    /// Lookup target not present in the map.
    #[error("EntityCadMap: key not found")]
    NotFound,
}

// ---------------------------------------------------------------------------
// EntityCadMap
// ---------------------------------------------------------------------------

/// Bidirectional mapping between ECS entity ids and `cad-core` node ids.
///
/// Both forward and reverse maps are mutated atomically by [`Self::insert`]:
/// either both entries land or neither does, returning a [`EntityCadMapError`]
/// in the duplicate cases. The internal storage is [`BTreeMap`] so iteration
/// is deterministic — important for snapshot byte-stability.
///
/// `Serialize` / `Deserialize` are implemented manually because
/// `rge_kernel_ecs::EntityId` lacks a serde impl (`ulid/serde` is not enabled
/// upstream). The wire format is a single `BTreeMap<EntityIdProxy, NodeId>`
/// — the reverse direction is rebuilt at deserialization time.
#[derive(Clone, Debug, Default)]
pub struct EntityCadMap {
    entity_to_cad: BTreeMap<EntityId, NodeId>,
    cad_to_entity: BTreeMap<NodeId, EntityId>,
}

impl Serialize for EntityCadMap {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let proxied: BTreeMap<EntityIdProxy, NodeId> = self
            .entity_to_cad
            .iter()
            .map(|(e, n)| (EntityIdProxy::from(*e), *n))
            .collect();
        proxied.serialize(s)
    }
}

impl<'de> Deserialize<'de> for EntityCadMap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let proxied = BTreeMap::<EntityIdProxy, NodeId>::deserialize(d)?;
        let mut entity_to_cad: BTreeMap<EntityId, NodeId> = BTreeMap::new();
        let mut cad_to_entity: BTreeMap<NodeId, EntityId> = BTreeMap::new();
        for (proxy, node) in proxied {
            let entity = EntityId::from(proxy);
            entity_to_cad.insert(entity, node);
            cad_to_entity.insert(node, entity);
        }
        Ok(Self {
            entity_to_cad,
            cad_to_entity,
        })
    }
}

impl EntityCadMap {
    /// Construct an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an `(entity, node)` pair atomically.
    ///
    /// # Errors
    ///
    /// * [`EntityCadMapError::DuplicateEntity`] when `entity` is already
    ///   mapped to some other node.
    /// * [`EntityCadMapError::DuplicateNode`] when `node` is already mapped
    ///   to some other entity.
    ///
    /// Re-inserting an identical `(entity, node)` pair is a no-op and
    /// succeeds.
    pub fn insert(&mut self, entity: EntityId, node: NodeId) -> Result<(), EntityCadMapError> {
        if let Some(existing_node) = self.entity_to_cad.get(&entity).copied() {
            if existing_node == node {
                debug_assert_eq!(self.cad_to_entity.get(&node).copied(), Some(entity));
                return Ok(());
            }
            return Err(EntityCadMapError::DuplicateEntity {
                entity,
                existing_node,
            });
        }
        if let Some(existing_entity) = self.cad_to_entity.get(&node).copied() {
            return Err(EntityCadMapError::DuplicateNode {
                node,
                existing_entity,
            });
        }
        self.entity_to_cad.insert(entity, node);
        self.cad_to_entity.insert(node, entity);
        Ok(())
    }

    /// Remove the entry for `entity`, returning its previously-bound node id
    /// (or `None` if `entity` was not present).
    pub fn remove_entity(&mut self, entity: EntityId) -> Option<NodeId> {
        let node = self.entity_to_cad.remove(&entity)?;
        let removed = self.cad_to_entity.remove(&node);
        debug_assert_eq!(
            removed,
            Some(entity),
            "EntityCadMap reverse-direction lost sync"
        );
        Some(node)
    }

    /// Remove the entry for `node`, returning its previously-bound entity id
    /// (or `None` if `node` was not present).
    pub fn remove_node(&mut self, node: NodeId) -> Option<EntityId> {
        let entity = self.cad_to_entity.remove(&node)?;
        let removed = self.entity_to_cad.remove(&entity);
        debug_assert_eq!(
            removed,
            Some(node),
            "EntityCadMap forward-direction lost sync"
        );
        Some(entity)
    }

    /// Look up the entity bound to `node`, if any.
    #[must_use]
    pub fn entity_for(&self, node: NodeId) -> Option<EntityId> {
        self.cad_to_entity.get(&node).copied()
    }

    /// Look up the node bound to `entity`, if any.
    #[must_use]
    pub fn node_for(&self, entity: EntityId) -> Option<NodeId> {
        self.entity_to_cad.get(&entity).copied()
    }

    /// Iterate over all `(entity, node)` pairs, sorted by `EntityId`.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, NodeId)> + '_ {
        self.entity_to_cad.iter().map(|(e, n)| (*e, *n))
    }

    /// Number of `(entity, node)` pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entity_to_cad.len()
    }

    /// Whether the map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entity_to_cad.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn n(raw: u128) -> NodeId {
        NodeId::from_raw(raw)
    }

    #[test]
    fn empty_map_returns_none_both_ways() {
        let map = EntityCadMap::new();
        let ent = EntityId::new();
        assert_eq!(map.entity_for(n(1)), None);
        assert_eq!(map.node_for(ent), None);
        assert_eq!(map.len(), 0);
        assert!(map.is_empty());
    }

    #[test]
    fn insert_then_lookup_both_ways() {
        let mut map = EntityCadMap::new();
        let e = EntityId::new();
        let nd = n(0xabcd);
        map.insert(e, nd).expect("insert");
        assert_eq!(map.entity_for(nd), Some(e));
        assert_eq!(map.node_for(e), Some(nd));
        assert_eq!(map.len(), 1);
        assert!(!map.is_empty());
    }

    #[test]
    fn duplicate_entity_rejected() {
        let mut map = EntityCadMap::new();
        let e = EntityId::new();
        let n1 = n(1);
        let n2 = n(2);
        map.insert(e, n1).expect("first");
        let err = map.insert(e, n2).unwrap_err();
        assert!(matches!(
            err,
            EntityCadMapError::DuplicateEntity { entity, existing_node }
            if entity == e && existing_node == n1
        ));
        // Reverse direction still consistent: n1 maps to e, n2 unmapped.
        assert_eq!(map.entity_for(n1), Some(e));
        assert_eq!(map.entity_for(n2), None);
    }

    #[test]
    fn duplicate_node_rejected() {
        let mut map = EntityCadMap::new();
        let e1 = EntityId::new();
        let e2 = EntityId::new();
        let nd = n(7);
        map.insert(e1, nd).expect("first");
        let err = map.insert(e2, nd).unwrap_err();
        assert!(matches!(
            err,
            EntityCadMapError::DuplicateNode { node, existing_entity }
            if node == nd && existing_entity == e1
        ));
        // Forward direction still consistent.
        assert_eq!(map.node_for(e1), Some(nd));
        assert_eq!(map.node_for(e2), None);
    }

    #[test]
    fn remove_entity_clears_both_directions() {
        let mut map = EntityCadMap::new();
        let e = EntityId::new();
        let nd = n(99);
        map.insert(e, nd).expect("ins");
        assert_eq!(map.remove_entity(e), Some(nd));
        assert_eq!(map.node_for(e), None);
        assert_eq!(map.entity_for(nd), None);
        assert_eq!(map.remove_entity(e), None, "removing again is None");
    }

    #[test]
    fn remove_node_clears_both_directions() {
        let mut map = EntityCadMap::new();
        let e = EntityId::new();
        let nd = n(123);
        map.insert(e, nd).expect("ins");
        assert_eq!(map.remove_node(nd), Some(e));
        assert_eq!(map.node_for(e), None);
        assert_eq!(map.entity_for(nd), None);
        assert_eq!(map.remove_node(nd), None, "removing again is None");
    }

    #[test]
    fn iter_is_sorted_by_entity_id() {
        let mut map = EntityCadMap::new();
        let mut entities: Vec<EntityId> = (0..5).map(|_| EntityId::new()).collect();
        for (i, e) in entities.iter().enumerate() {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "test fixture; i is bounded by `entities.len() == 5`"
            )]
            map.insert(*e, n((i as u128) + 1)).expect("ins");
        }
        // Sort our local list by EntityId so we can compare.
        entities.sort();
        let collected: Vec<EntityId> = map.iter().map(|(e, _)| e).collect();
        assert_eq!(collected, entities, "iter must be sorted by EntityId");
    }

    #[test]
    fn brep_handle_new_zero_state() {
        // Post-2026-05-08 SSoT refactor: BRepHandle::new() takes no NodeId.
        // The cad-node FK lives in EntityCadMap; the handle stores only
        // projection bookkeeping (mesh_id + last_projected_checkpoint).
        let h = BRepHandle::new();
        assert_eq!(h.mesh_id, None);
        assert_eq!(h.last_projected_checkpoint, None);
    }

    #[test]
    fn brep_handle_default_matches_new() {
        // Default impl is delegated to ::new(); they must produce identical
        // zero-state handles.
        assert_eq!(BRepHandle::default(), BRepHandle::new());
    }

    #[test]
    fn idempotent_reinsert_is_ok() {
        let mut map = EntityCadMap::new();
        let e = EntityId::new();
        let nd = n(5);
        map.insert(e, nd).expect("first");
        map.insert(e, nd).expect("identical re-insert is a no-op");
        assert_eq!(map.len(), 1);
    }

    // ---- D-projection-α additions: brep_owner field on BRepHandle --------

    /// `BRepHandle::new()` defaults `brep_owner: None` — the additive
    /// pre-D-projection-α baseline. Existing constructors / call sites are
    /// unaffected.
    #[test]
    fn brep_handle_default_has_no_owner() {
        let h = BRepHandle::new();
        assert_eq!(h.brep_owner, None);
    }

    /// `BRepHandle::with_brep_owner` sets the owner; chaining preserves the
    /// other fields' default state.
    #[test]
    fn brep_handle_with_brep_owner_sets_owner() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        let h = BRepHandle::new().with_brep_owner(owner);
        assert_eq!(h.brep_owner, Some(owner));
        // Other fields untouched.
        assert_eq!(h.mesh_id, None);
        assert_eq!(h.last_projected_checkpoint, None);
    }

    /// `BRepHandle` round-trips through `postcard` with the `brep_owner`
    /// preserved. Ensures PIE snapshot capture/restore (which uses postcard)
    /// preserves the owner seed.
    #[test]
    fn brep_handle_serde_round_trip_with_owner() {
        let owner = BRepOwnerId::from_bytes([0xab; 16]);
        let h = BRepHandle::new().with_brep_owner(owner);
        let bytes = postcard::to_allocvec(&h).expect("encode");
        let decoded: BRepHandle = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(decoded, h);
        assert_eq!(decoded.brep_owner, Some(owner));
    }

    /// Pre-D-projection-α serialised handles (no `brep_owner` field) must
    /// continue to deserialise — the new field is `#[serde(default)]` so
    /// missing-field deserialisation fills it with `None`. We can't test
    /// against a literal pre-this-dispatch byte stream (the encoding is
    /// versioned via postcard's wire format) but a fresh handle with
    /// `brep_owner: None` round-trips identically.
    #[test]
    fn brep_handle_no_owner_round_trips_as_none() {
        let h = BRepHandle::new();
        let bytes = postcard::to_allocvec(&h).expect("encode");
        let decoded: BRepHandle = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(decoded, h);
        assert_eq!(decoded.brep_owner, None);
    }
}
