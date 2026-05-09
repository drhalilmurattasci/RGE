//! `cad_projection::projection_geometry` — `ProjectedMesh` payload + the free
//! [`project`] function that ferries `cad-core::Tessellation` into the
//! ECS-side shape.
//!
//! Failure class: snapshot-recoverable
//!
//! # Purpose
//!
//! `cad-core` produces a `Tessellation` (positions + indices) inside its own
//! universe. This module re-stamps that data into a [`ProjectedMesh`] that
//! carries provenance metadata (the source `NodeId` + the `CheckpointId` at
//! which projection happened) so downstream consumers can tell "did the cad
//! state advance since I last looked at this entity's mesh?" without dipping
//! back into `cad-core`.
//!
//! # Tradeoff
//!
//! For Phase 7.3, [`project`] **copies** position and index buffers out of the
//! `Arc<Tessellation>` `cad-core` returns. This is correctness-first; a
//! future optimization can have `ProjectedMesh` borrow from the
//! `Arc<Tessellation>` directly and avoid the copy. Tracked: deferred per
//! the dispatch's "non-goal" list.

use std::sync::Arc;

use rge_cad_core::{
    CadGraph, CheckpointId, EvalError, TessellationCache, Tolerance, ToleranceError, TopologyFaceId,
};
use rge_kernel_ecs::EntityId;
use rge_kernel_graph_foundation::NodeId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::projection_structural::EntityCadMapError;

// ---------------------------------------------------------------------------
// CheckpointTag — serializable proxy for cad_core::CheckpointId
// ---------------------------------------------------------------------------

/// Serializable proxy wrapping `cad_core::CheckpointId`'s inner `u64`.
///
/// `cad_core::CheckpointId` is `Copy + PartialEq + Eq + Hash` but does not
/// derive `Serialize` / `Deserialize`. [`ProjectedMesh`] needs to carry
/// provenance (the head at projection time) and serialize, so this proxy is
/// used at the projection-layer boundary. Conversion is loss-free in both
/// directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CheckpointTag(pub u64);

impl From<CheckpointId> for CheckpointTag {
    fn from(id: CheckpointId) -> Self {
        Self(id.0)
    }
}

impl From<CheckpointTag> for CheckpointId {
    fn from(tag: CheckpointTag) -> Self {
        CheckpointId(tag.0)
    }
}

// ---------------------------------------------------------------------------
// ProjectedMeshId
// ---------------------------------------------------------------------------

/// Stable identifier for a [`ProjectedMesh`] within a `CadProjection`.
///
/// Allocated monotonically by [`crate::projection_cache::ProjectionCache`].
/// Across PIE round-trips the id sequence is reset on the receiving side; the
/// next tick re-projects every dirty entity and assigns fresh ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProjectedMeshId(pub u64);

// ---------------------------------------------------------------------------
// ProjectedMesh
// ---------------------------------------------------------------------------

/// ECS-side rendering / collision-friendly view of a tessellated `cad-core`
/// node.
///
/// Stored behind an [`Arc`] inside [`crate::projection_cache::ProjectionCache`]
/// so multiple readers can hold the same allocation cheaply. The mesh is
/// stamped with the [`NodeId`] it was projected from and the
/// [`CheckpointTag`] at the time of projection — together, they answer "is
/// this mesh stale?".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectedMesh {
    /// Per-vertex positions in object space, `[x, y, z]` order.
    pub positions: Vec<[f32; 3]>,
    /// Triangle indices, three per triangle, into `positions`.
    pub indices: Vec<u32>,
    /// The `cad-core` node this mesh was projected from.
    pub source_node: NodeId,
    /// Checkpoint at which the projection ran.
    pub source_checkpoint: CheckpointTag,
    /// Per-triangle face labels carried through from the upstream
    /// `Tessellation::face_labels`. `None` if the upstream Tessellation
    /// was unlabeled (e.g. Fillet output, or any operator other than
    /// Cuboid pre-D-projection-α). Preserves the
    /// `TopologyFaceId(u64)` sequential identity; lazy resolution to
    /// stable `BRepFaceId` happens via the resolver, NOT pre-resolved
    /// here. See [`crate::CadProjection::brep_face_id_for_triangle`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_labels: Option<Vec<TopologyFaceId>>,
}

impl ProjectedMesh {
    /// Number of vertices.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

// ---------------------------------------------------------------------------
// ProjectionError
// ---------------------------------------------------------------------------

/// Errors produced by the projection layer.
#[derive(Debug, Error)]
pub enum ProjectionError {
    /// `cad-core::OperatorGraph::evaluate` failed.
    #[error("cad-core evaluation failed: {0}")]
    Eval(#[from] EvalError),
    /// Caller supplied an invalid tolerance.
    #[error("invalid tolerance: {0}")]
    Tolerance(#[from] ToleranceError),
    /// An entity expected to carry a `BRepHandle` did not.
    #[error("entity {entity} has no BRepHandle")]
    NoBRepHandle {
        /// The entity that was missing its `BRepHandle`.
        entity: EntityId,
    },
    /// A cad node referenced by the projection layer is not in the cad
    /// graph.
    #[error("node {0} not in cad-core graph")]
    NodeNotInGraph(NodeId),
    /// The `EntityCadMap` invariant was violated upstream.
    #[error("structural mapping inconsistent: {0}")]
    EntityCadMap(#[from] EntityCadMapError),
}

// ---------------------------------------------------------------------------
// project()
// ---------------------------------------------------------------------------

/// Evaluate `node` in `cad`'s operator graph, then project the resulting
/// `cad_core::Tessellation` into a [`ProjectedMesh`] stamped with the source
/// node and the cad graph's current head checkpoint.
///
/// `cache` is the `cad-core` tessellation cache — the projection layer does
/// NOT own it (the cache is `cad-core` substrate; the projection layer owns
/// its own [`crate::projection_cache::ProjectionCache`] which stores the
/// resulting [`Arc<ProjectedMesh>`] keyed by `EntityId`).
///
/// # Errors
///
/// * [`ProjectionError::NodeNotInGraph`] if `node` is absent from the
///   operator graph.
/// * [`ProjectionError::Eval`] wrapping any `cad-core` evaluation error
///   (cycles, port mismatch, operator failure, …).
pub fn project(
    cad: &CadGraph,
    node: NodeId,
    cache: &mut TessellationCache,
    tolerance: Tolerance,
) -> Result<Arc<ProjectedMesh>, ProjectionError> {
    if cad.graph().node(node).is_none() {
        return Err(ProjectionError::NodeNotInGraph(node));
    }
    let tess = cad.graph().evaluate(node, cache, tolerance)?;
    let head: CheckpointTag = cad.head().into();
    let mesh = ProjectedMesh {
        positions: tess.positions.clone(),
        indices: tess.indices.clone(),
        source_node: node,
        source_checkpoint: head,
        face_labels: tess.face_labels.clone(),
    };
    Ok(Arc::new(mesh))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_cad_core::{CuboidOp, OperatorNode};

    use super::*;

    fn tol() -> Tolerance {
        Tolerance::new(0.001).expect("tol")
    }

    fn build_cuboid_graph(w: f32, h: f32, d: f32) -> (CadGraph, NodeId) {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: w,
                height: h,
                depth: d,
            }))
            .expect("add");
        cad.graph_mut().expect("mut2").set_root(node).expect("root");
        cad.commit("test cuboid").expect("commit");
        (cad, node)
    }

    /// Projecting a Cuboid yields 8 vertices and 12 triangles (36 indices).
    #[test]
    fn project_cuboid_yields_8_vertices_36_indices() {
        let (cad, node) = build_cuboid_graph(1.0, 1.0, 1.0);
        let mut cache = TessellationCache::new();
        let mesh = project(&cad, node, &mut cache, tol()).expect("project");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.indices.len(), 36);
        assert_eq!(mesh.triangle_count(), 12);
    }

    /// Projecting a node that is not in the graph yields `NodeNotInGraph`.
    #[test]
    fn project_unknown_node_errors_node_not_in_graph() {
        let (cad, _real_node) = build_cuboid_graph(1.0, 1.0, 1.0);
        let bogus = NodeId::from_raw(0xdead_beef);
        let mut cache = TessellationCache::new();
        let err = project(&cad, bogus, &mut cache, tol()).unwrap_err();
        assert!(matches!(err, ProjectionError::NodeNotInGraph(n) if n == bogus));
    }

    /// After commit, `mesh.source_checkpoint` matches `cad.head()`.
    #[test]
    fn project_records_source_checkpoint_after_commit() {
        let (cad, node) = build_cuboid_graph(2.0, 2.0, 2.0);
        let head_tag: CheckpointTag = cad.head().into();
        let mut cache = TessellationCache::new();
        let mesh = project(&cad, node, &mut cache, tol()).expect("project");
        assert_eq!(mesh.source_checkpoint, head_tag);
        assert_eq!(mesh.source_node, node);
    }

    /// Triangle count derives from `indices.len() / 3` and vertex count from
    /// `positions.len()` consistently.
    #[test]
    fn mesh_vertex_and_triangle_counts_match_indices_len() {
        let (cad, node) = build_cuboid_graph(1.5, 1.5, 1.5);
        let mut cache = TessellationCache::new();
        let mesh = project(&cad, node, &mut cache, tol()).expect("project");
        assert_eq!(mesh.vertex_count(), mesh.positions.len());
        assert_eq!(mesh.triangle_count(), mesh.indices.len() / 3);
        assert!(mesh.triangle_count() * 3 == mesh.indices.len());
    }

    /// A freshly-constructed `ProjectedMesh` literal with `face_labels: None`
    /// is the additive baseline — pre-D-projection-α consumers (which never
    /// touched `face_labels`) keep working unchanged.
    #[test]
    fn projected_mesh_face_labels_default_none() {
        let mesh = ProjectedMesh {
            positions: vec![[0.0, 0.0, 0.0]],
            indices: vec![],
            source_node: NodeId::from_raw(1),
            source_checkpoint: CheckpointTag(0),
            face_labels: None,
        };
        assert!(mesh.face_labels.is_none());
    }

    /// `project()` propagates the `Tessellation::face_labels` from the
    /// upstream `cad-core` evaluation into `ProjectedMesh.face_labels`. For a
    /// Cuboid root, the upstream emits 12 labels (2 triangles per face, in
    /// the canonical `NegZ → PosZ → NegY → PosY → NegX → PosX` order
    /// matching `impl BRepProvider for CuboidOp`). See D-projection-α.
    #[test]
    fn project_propagates_cuboid_face_labels() {
        use rge_cad_core::TopologyFaceId;
        let (cad, node) = build_cuboid_graph(1.0, 1.0, 1.0);
        let mut cache = TessellationCache::new();
        let mesh = project(&cad, node, &mut cache, tol()).expect("project");
        let labels = mesh.face_labels.as_ref().expect("face_labels propagated");
        assert_eq!(labels.len(), 12);
        // 2 triangles per face — canonical order `(0,0,1,1,2,2,3,3,4,4,5,5)`.
        for face_idx in 0..6u64 {
            let tri_a = (face_idx as usize) * 2;
            let tri_b = tri_a + 1;
            assert_eq!(labels[tri_a], TopologyFaceId(face_idx));
            assert_eq!(labels[tri_b], TopologyFaceId(face_idx));
        }
    }
}
