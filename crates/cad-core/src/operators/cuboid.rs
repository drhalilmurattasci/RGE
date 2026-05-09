//! `CuboidOp` — origin-centered axis-aligned box primitive (arity 0).
//!
//! Failure class: snapshot-recoverable
//!
//! Produces a closed 8-vertex / 12-triangle box centered at the origin with
//! the half-extents `(width/2, height/2, depth/2)`. Right-handed CCW winding.
//!
//! # Capability surface (per ADR-104)
//!
//! All defaults — closed-form generative primitive with no inputs:
//!
//! * `boolean_robust_under_tolerance`: true (no boolean op).
//! * `deterministic_triangulation`: true (single-pass float-multiply; bit-identical given identical extents).
//! * `t_junction_handling`: true (8-vertex cube has none).
//! * `concave_input_supported`: N/A (no profile input).
//! * `arity`: 0.
//! * `output_labeled_when_input_labeled`: false (no inputs ⇒ default `iter().any` returns false).

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topology::{
    BRepEdgeId, BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider, CuboidFaceTag,
};

/// Origin-centered axis-aligned cuboid primitive.
///
/// All three dimensions must be positive and finite — `evaluate` rejects
/// `0.0`, negatives, infinities, and NaN with [`OpError::InvalidParameter`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CuboidOp {
    /// Extent along the X axis (positive, finite).
    pub width: f32,
    /// Extent along the Y axis (positive, finite).
    pub height: f32,
    /// Extent along the Z axis (positive, finite).
    pub depth: f32,
}

impl Default for CuboidOp {
    /// Default unit cube: `1.0 x 1.0 x 1.0`.
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }
    }
}

impl CuboidOp {
    /// Validate that all three dimensions are finite and `> 0.0`.
    fn validate(&self) -> Result<(), OpError> {
        for (label, value) in [
            ("width", self.width),
            ("height", self.height),
            ("depth", self.depth),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(OpError::InvalidParameter(format!(
                    "CuboidOp.{label} must be finite and > 0 (got {value})"
                )));
            }
        }
        Ok(())
    }
}

impl Operator for CuboidOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Cuboid
    }

    fn arity(&self) -> usize {
        0
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"cuboid");
        hasher.update(&self.width.to_le_bytes());
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.depth.to_le_bytes());
        *hasher.finalize().as_bytes()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        self.validate()?;

        let hx = self.width * 0.5;
        let hy = self.height * 0.5;
        let hz = self.depth * 0.5;

        // 8 corner vertices. Indexing convention:
        //   0: (-x,-y,-z)  1: (+x,-y,-z)  2: (+x,+y,-z)  3: (-x,+y,-z)
        //   4: (-x,-y,+z)  5: (+x,-y,+z)  6: (+x,+y,+z)  7: (-x,+y,+z)
        let positions = vec![
            [-hx, -hy, -hz],
            [hx, -hy, -hz],
            [hx, hy, -hz],
            [-hx, hy, -hz],
            [-hx, -hy, hz],
            [hx, -hy, hz],
            [hx, hy, hz],
            [-hx, hy, hz],
        ];

        // 12 triangles, two per face. Right-handed CCW winding when viewed
        // from outside the box (along the outward face normal).
        #[rustfmt::skip]
        let indices = vec![
            // -Z face (back, normal -z): viewed from -z, CCW order is 0,3,2,1.
            0, 3, 2,  0, 2, 1,
            // +Z face (front, normal +z): viewed from +z, CCW is 4,5,6,7.
            4, 5, 6,  4, 6, 7,
            // -Y face (bottom, normal -y): viewed from -y, CCW is 0,1,5,4.
            0, 1, 5,  0, 5, 4,
            // +Y face (top, normal +y): viewed from +y, CCW is 3,7,6,2.
            3, 7, 6,  3, 6, 2,
            // -X face (left, normal -x): viewed from -x, CCW is 0,4,7,3.
            0, 4, 7,  0, 7, 3,
            // +X face (right, normal +x): viewed from +x, CCW is 1,2,6,5.
            1, 2, 6,  1, 6, 5,
        ];

        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("CuboidOp produced invalid tessellation: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// BRepProvider — v0 sub-7.2-α B-Rep face identity for CuboidOp
// ---------------------------------------------------------------------------

/// Pair the 6 sequential per-tessellation `TopologyFaceId`s with rebuild-stable
/// `BRepFaceId`s seeded from the caller-supplied [`BRepOwnerId`].
///
/// The mapping `TopologyFaceId(N) -> CuboidFaceTag` matches the canonical
/// face-emission order in [`Operator::evaluate`] above — `(-Z, +Z, -Y, +Y,
/// -X, +X)`. Each face occupies 2 triangles (6 indices) starting at
/// `TopologyFaceId(N)` for `N in 0..6`. (The current `Tessellation` substrate
/// stores triangles, not faces; the `TopologyFaceId(N)` here is the FACE-level
/// index into that emission order, which is the correct granularity for
/// downstream B-Rep consumers.)
impl BRepProvider for CuboidOp {
    fn brep_face_ids(&self, owner: BRepOwnerId) -> Vec<(TopologyFaceId, BRepFaceId)> {
        // Canonical face-emission order — DO NOT reorder. See `evaluate`.
        const TAGS: [CuboidFaceTag; 6] = [
            CuboidFaceTag::NegZ,
            CuboidFaceTag::PosZ,
            CuboidFaceTag::NegY,
            CuboidFaceTag::PosY,
            CuboidFaceTag::NegX,
            CuboidFaceTag::PosX,
        ];
        TAGS.iter()
            .enumerate()
            .map(|(idx, tag)| {
                (
                    TopologyFaceId(idx as u64),
                    BRepFaceId::for_cuboid_face(owner, *tag),
                )
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// BRepEdgeProvider — sub-7.2-ζ.α B-Rep edge identity for CuboidOp
// ---------------------------------------------------------------------------

/// Mint the 12 stable B-Rep edge identities for an axis-aligned cuboid.
///
/// A cuboid has 12 edges, each of which is the topological intersection
/// of exactly two non-opposite faces. The opposite-face pairs (NegZ/PosZ,
/// NegY/PosY, NegX/PosX) never share an edge; the remaining 12 face-pair
/// combinations all do.
///
/// `CuboidFaceTag` discriminant order (per `face_tag.rs`):
///
/// ```text
/// TopologyFaceId(0) = NegZ,  TopologyFaceId(1) = PosZ,
/// TopologyFaceId(2) = NegY,  TopologyFaceId(3) = PosY,
/// TopologyFaceId(4) = NegX,  TopologyFaceId(5) = PosX.
/// ```
///
/// Edge order is canonical (frozen here): all 4 edges incident to NegZ
/// (the bottom face), then all 4 edges incident to PosZ (the top face),
/// then the 4 vertical edges (Y-axis face × X-axis face pairs that
/// don't involve Z).
///
/// Every edge uses `local_ordinal = 0` because for a cuboid no two
/// non-opposite faces share more than one edge. The `local_ordinal`
/// slot on [`BRepEdgeId::for_face_pair`] is reserved for future
/// operators with multi-edge face pairs.
impl BRepEdgeProvider for CuboidOp {
    fn brep_edge_ids(&self, owner: BRepOwnerId) -> Vec<BRepEdgeId> {
        // Get our own face IDs first — edges derive from face pairs.
        let face_ids: Vec<BRepFaceId> = self
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, face_id)| face_id)
            .collect();
        debug_assert_eq!(face_ids.len(), 6, "Cuboid must produce 6 face IDs");

        // Indices below refer to `face_ids[i]`, mirroring the
        // CuboidFaceTag discriminant order in face_tag.rs:
        //   0 = NegZ, 1 = PosZ, 2 = NegY, 3 = PosY, 4 = NegX, 5 = PosX.
        const ADJACENCIES: [(usize, usize); 12] = [
            // Bottom-face (NegZ) perimeter — 4 edges
            (0, 2), // NegZ ∩ NegY
            (0, 3), // NegZ ∩ PosY
            (0, 4), // NegZ ∩ NegX
            (0, 5), // NegZ ∩ PosX
            // Top-face (PosZ) perimeter — 4 edges
            (1, 2), // PosZ ∩ NegY
            (1, 3), // PosZ ∩ PosY
            (1, 4), // PosZ ∩ NegX
            (1, 5), // PosZ ∩ PosX
            // Vertical edges (Y-axis face × X-axis face) — 4 edges
            (2, 4), // NegY ∩ NegX
            (2, 5), // NegY ∩ PosX
            (3, 4), // PosY ∩ NegX
            (3, 5), // PosY ∩ PosX
        ];

        ADJACENCIES
            .iter()
            .map(|&(a, b)| BRepEdgeId::for_face_pair(face_ids[a], face_ids[b], 0))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_returns_unit_cube() {
        let op = CuboidOp::default();
        assert!((op.width - 1.0).abs() < f32::EPSILON);
        assert!((op.height - 1.0).abs() < f32::EPSILON);
        assert!((op.depth - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn evaluate_produces_8_vertices_and_12_triangles() {
        let op = CuboidOp::default();
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        // Spot-check that vertices are within ±0.5 (half-extents of unit cube).
        for [x, y, z] in &mesh.positions {
            assert!(x.abs() <= 0.5 + 1e-6);
            assert!(y.abs() <= 0.5 + 1e-6);
            assert!(z.abs() <= 0.5 + 1e-6);
        }
    }

    #[test]
    fn structural_hash_is_deterministic() {
        let a = CuboidOp {
            width: 1.5,
            height: 2.0,
            depth: 0.75,
        };
        let b = CuboidOp {
            width: 1.5,
            height: 2.0,
            depth: 0.75,
        };
        let c = CuboidOp {
            width: 1.5,
            height: 2.0,
            depth: 0.76,
        };
        assert_eq!(a.structural_hash(), b.structural_hash());
        assert_ne!(a.structural_hash(), c.structural_hash());
    }

    #[test]
    fn negative_dimension_rejected() {
        let op = CuboidOp {
            width: -1.0,
            height: 1.0,
            depth: 1.0,
        };
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    /// `CuboidOp` is arity 0 and emits an unlabeled `Tessellation::new(...)`
    /// — so the trait-default [`Operator::output_is_labeled`] (which returns
    /// `false` on an empty `inputs_labeled` slice via `iter().any`) matches
    /// the actual `evaluate` semantics. No override needed.
    #[test]
    fn cuboid_output_is_labeled_returns_false() {
        let op = CuboidOp::default();
        assert!(!op.output_is_labeled(&[]));
    }

    /// `BRepProvider::brep_face_ids` returns exactly 6 pairs, one per cuboid
    /// face, in canonical emission order: `(TopologyFaceId(0), NegZ)` through
    /// `(TopologyFaceId(5), PosX)`.
    #[test]
    fn brep_face_ids_returns_six_pairs_in_canonical_order() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let op = CuboidOp::default();
        let pairs = op.brep_face_ids(owner);

        assert_eq!(pairs.len(), 6);

        // Canonical face-emission order from `evaluate`.
        let expected_tags = [
            CuboidFaceTag::NegZ,
            CuboidFaceTag::PosZ,
            CuboidFaceTag::NegY,
            CuboidFaceTag::PosY,
            CuboidFaceTag::NegX,
            CuboidFaceTag::PosX,
        ];

        for (idx, (face_id, brep_id)) in pairs.iter().enumerate() {
            assert_eq!(face_id.0, idx as u64);
            assert_eq!(
                *brep_id,
                BRepFaceId::for_cuboid_face(owner, expected_tags[idx]),
                "pair at index {idx} does not match canonical tag"
            );
        }
    }

    /// The 6 pairs returned by `brep_face_ids` must all be distinct
    /// (no two faces share a `BRepFaceId` under the same owner).
    #[test]
    fn brep_face_ids_are_pairwise_distinct() {
        let owner = BRepOwnerId::from_bytes([0xa5u8; 16]);
        let op = CuboidOp::default();
        let pairs = op.brep_face_ids(owner);

        for i in 0..pairs.len() {
            for j in (i + 1)..pairs.len() {
                assert_ne!(pairs[i].1, pairs[j].1);
            }
        }
    }

    /// `BRepEdgeProvider::brep_edge_ids` returns exactly 12 pairwise-
    /// distinct `BRepEdgeId`s, one per cuboid edge.
    #[test]
    fn brep_edge_provider_returns_12_unique_edges() {
        let owner = BRepOwnerId::from_bytes([0xa5u8; 16]);
        let op = CuboidOp::default();
        let edges = op.brep_edge_ids(owner);

        assert_eq!(edges.len(), 12, "Cuboid must produce 12 edges");
        for i in 0..edges.len() {
            for j in (i + 1)..edges.len() {
                assert_ne!(
                    edges[i], edges[j],
                    "edge {i} collides with edge {j} under the same owner"
                );
            }
        }
    }

    /// The 12 edges returned by `brep_edge_ids` must align with the
    /// canonical face-pair adjacency table documented in the
    /// `impl BRepEdgeProvider for CuboidOp` block. We verify three
    /// representative edges by re-constructing `BRepEdgeId::for_face_pair`
    /// directly from the underlying face IDs.
    #[test]
    fn brep_edge_ids_align_with_canonical_adjacency_table() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let op = CuboidOp::default();
        let face_ids: Vec<BRepFaceId> = op
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, f)| f)
            .collect();
        let edges = op.brep_edge_ids(owner);

        // Position 0: NegZ ∩ NegY (bottom-face perimeter, first edge).
        assert_eq!(
            edges[0],
            BRepEdgeId::for_face_pair(face_ids[0], face_ids[2], 0),
            "edge 0 must be NegZ ∩ NegY"
        );
        // Position 4: PosZ ∩ NegY (top-face perimeter, first edge).
        assert_eq!(
            edges[4],
            BRepEdgeId::for_face_pair(face_ids[1], face_ids[2], 0),
            "edge 4 must be PosZ ∩ NegY"
        );
        // Position 8: NegY ∩ NegX (vertical edge, first).
        assert_eq!(
            edges[8],
            BRepEdgeId::for_face_pair(face_ids[2], face_ids[4], 0),
            "edge 8 must be NegY ∩ NegX"
        );
    }
}
