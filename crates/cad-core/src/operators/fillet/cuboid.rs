//! `FilletOp` constructor + helpers for `CuboidOp` upstream (sub-α).
//!
//! Fixed-topology consumer of [`crate::topology::BRepEdgeId`]. A cuboid
//! always has exactly 12 edges; this module pairs each
//! [`BRepEdgeId`] with the two endpoint corner indices in the upstream
//! Cuboid's 8-vertex layout and the inward bisector direction derived
//! from the two adjacent face outward normals.
//!
//! Sub-γ refactor: per-edge resolution lives behind the
//! [`super::FilletUpstream`] trait so [`FilletOp::from_upstream`] can
//! drive the shared validation pipeline. [`FilletOp::new`] is now a
//! thin delegate.

use super::{ChamferSpec, FilletError, FilletOp, FilletUpstream};
use crate::operators::CuboidOp;
use crate::topology::{BRepEdgeId, BRepOwnerId, CuboidFaceTag};

/// Canonical (CuboidFaceTag, CuboidFaceTag) pair table parallel to
/// the 12-edge order returned by `<CuboidOp as BRepEdgeProvider>::brep_edge_ids`.
///
/// See `cuboid.rs::impl BRepEdgeProvider for CuboidOp` for the canonical
/// adjacency table — this constant mirrors the same `(face_a_tag, face_b_tag)`
/// pairs in the same order:
///
/// ```text
/// 0..3  : NegZ ∩ {NegY, PosY, NegX, PosX}
/// 4..7  : PosZ ∩ {NegY, PosY, NegX, PosX}
/// 8..11 : {NegY, PosY} × {NegX, PosX}
/// ```
const CUBOID_EDGE_TAG_PAIRS: [(CuboidFaceTag, CuboidFaceTag); 12] = [
    // Bottom-face (NegZ) perimeter — 4 edges
    (CuboidFaceTag::NegZ, CuboidFaceTag::NegY),
    (CuboidFaceTag::NegZ, CuboidFaceTag::PosY),
    (CuboidFaceTag::NegZ, CuboidFaceTag::NegX),
    (CuboidFaceTag::NegZ, CuboidFaceTag::PosX),
    // Top-face (PosZ) perimeter — 4 edges
    (CuboidFaceTag::PosZ, CuboidFaceTag::NegY),
    (CuboidFaceTag::PosZ, CuboidFaceTag::PosY),
    (CuboidFaceTag::PosZ, CuboidFaceTag::NegX),
    (CuboidFaceTag::PosZ, CuboidFaceTag::PosX),
    // Vertical edges (Y-axis face × X-axis face) — 4 edges
    (CuboidFaceTag::NegY, CuboidFaceTag::NegX),
    (CuboidFaceTag::NegY, CuboidFaceTag::PosX),
    (CuboidFaceTag::PosY, CuboidFaceTag::NegX),
    (CuboidFaceTag::PosY, CuboidFaceTag::PosX),
];

/// Compute the per-edge [`ChamferSpec`] for canonical Cuboid edge
/// index `0..12`. Always returns `Ok` for in-range indices — a cuboid
/// has no circular-path edges, every edge is a clean 2-endpoint
/// adjacency between two adjacent faces.
fn cuboid_resolve_chamfer_spec(canonical_index: usize) -> Result<ChamferSpec, &'static str> {
    if canonical_index >= CUBOID_EDGE_TAG_PAIRS.len() {
        // Defensive: from_upstream's caller-side filter already
        // restricts canonical_index to the upstream's brep_edge_ids
        // length, which is exactly 12 for any CuboidOp. This arm is
        // unreachable in production paths but keeps the public
        // signature total.
        return Err("cuboid canonical edge index out of range (must be < 12)");
    }
    let (tag_a, tag_b) = CUBOID_EDGE_TAG_PAIRS[canonical_index];
    let (vertex_a, vertex_b) = cuboid_edge_corner_indices(tag_a, tag_b);
    // Inward bisector = average of the two adjacent face outward
    // normals, negated. Magnitude is half the sum (matches sub-α's
    // evaluation-time formula bit-for-bit).
    let n_a = cuboid_face_normal(tag_a);
    let n_b = cuboid_face_normal(tag_b);
    let inward_direction = [
        -(n_a[0] + n_b[0]) / 2.0,
        -(n_a[1] + n_b[1]) / 2.0,
        -(n_a[2] + n_b[2]) / 2.0,
    ];
    Ok(ChamferSpec {
        vertex_a,
        vertex_b,
        inward_direction,
    })
}

impl FilletUpstream for CuboidOp {
    fn resolve_chamfer_spec(&self, canonical_index: usize) -> Result<ChamferSpec, &'static str> {
        cuboid_resolve_chamfer_spec(canonical_index)
    }
}

impl FilletOp {
    /// Construct a FilletOp validated against the upstream Cuboid.
    ///
    /// Each [`BRepEdgeId`] is resolved against
    /// `upstream.brep_edge_ids(owner)`; resolution failure produces
    /// [`FilletError::EdgeNotInUpstream`]. Per-edge [`ChamferSpec`]s
    /// are computed at construction time using the upstream Cuboid's
    /// 8-corner layout and outward face normals.
    ///
    /// # Errors
    ///
    /// * [`FilletError::InvalidRadius`] if `radius` is non-finite or
    ///   `<= 0`.
    /// * [`FilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`FilletError::EdgeNotInUpstream`] if any edge ID does not
    ///   appear in `upstream.brep_edge_ids(owner)`.
    pub fn new(
        upstream: &CuboidOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, FilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Cuboid helper tables — derived from cuboid.rs::evaluate's 8-corner layout.
// ---------------------------------------------------------------------------

/// Return the 2 vertex indices in the Cuboid's vertex array that form
/// the endpoints of the edge between `tag_a` and `tag_b`.
///
/// Cuboid corner indexing (per `cuboid.rs::evaluate`):
///
/// ```text
/// 0: (-x,-y,-z)  1: (+x,-y,-z)  2: (+x,+y,-z)  3: (-x,+y,-z)
/// 4: (-x,-y,+z)  5: (+x,-y,+z)  6: (+x,+y,+z)  7: (-x,+y,+z)
/// ```
///
/// Each edge spans 2 corners that differ in exactly ONE axis sign
/// (the axis perpendicular to BOTH faces' normals). Argument order
/// does not matter — the helper sorts the tag pair internally so
/// `f(a, b) == f(b, a)`.
fn cuboid_edge_corner_indices(tag_a: CuboidFaceTag, tag_b: CuboidFaceTag) -> (u32, u32) {
    // Sort by discriminant so the match handles each unordered pair
    // exactly once. The discriminant ordering is frozen at
    // NegZ=0 < PosZ=1 < NegY=2 < PosY=3 < NegX=4 < PosX=5 per
    // face_tag.rs (sub-7.2-α).
    let (lo, hi) = if tag_a.discriminant() <= tag_b.discriminant() {
        (tag_a, tag_b)
    } else {
        (tag_b, tag_a)
    };

    use CuboidFaceTag::{NegX, NegY, NegZ, PosX, PosY, PosZ};
    match (lo, hi) {
        // NegZ (bottom of box, -Z) intersects each of the 4 X/Y faces:
        // these are the 4 edges of the bottom face.
        (NegZ, NegY) => (0, 1), // -Z ∩ -Y → (-,-,-) and (+,-,-)
        (NegZ, PosY) => (3, 2), // -Z ∩ +Y → (-,+,-) and (+,+,-)
        (NegZ, NegX) => (0, 3), // -Z ∩ -X → (-,-,-) and (-,+,-)
        (NegZ, PosX) => (1, 2), // -Z ∩ +X → (+,-,-) and (+,+,-)

        // PosZ (top of box, +Z) intersects each of the 4 X/Y faces:
        // these are the 4 edges of the top face.
        (PosZ, NegY) => (4, 5), // +Z ∩ -Y → (-,-,+) and (+,-,+)
        (PosZ, PosY) => (7, 6), // +Z ∩ +Y → (-,+,+) and (+,+,+)
        (PosZ, NegX) => (4, 7), // +Z ∩ -X → (-,-,+) and (-,+,+)
        (PosZ, PosX) => (5, 6), // +Z ∩ +X → (+,-,+) and (+,+,+)

        // The 4 vertical edges (Y-axis face × X-axis face).
        (NegY, NegX) => (0, 4), // -Y ∩ -X → (-,-,-) and (-,-,+)
        (NegY, PosX) => (1, 5), // -Y ∩ +X → (+,-,-) and (+,-,+)
        (PosY, NegX) => (3, 7), // +Y ∩ -X → (-,+,-) and (-,+,+)
        (PosY, PosX) => (2, 6), // +Y ∩ +X → (+,+,-) and (+,+,+)

        // Same axis (e.g. NegZ ∩ NegZ or NegZ ∩ PosZ): not a real
        // cuboid edge. The validation in FilletOp::new should have
        // already rejected these via the BRepEdgeProvider lookup —
        // this arm is a defensive fallback that returns the dummy
        // pair (0, 0) so the caller's geometry produces a degenerate
        // (zero-area) chamfer triangle rather than panicking. In
        // practice `cuboid_edge_corner_indices` is only called for
        // (tag_a, tag_b) pairs we already validated come from
        // `CUBOID_EDGE_TAG_PAIRS`, so this arm is unreachable in
        // production paths.
        _ => (0, 0),
    }
}

/// Outward-pointing unit normal for the given Cuboid face tag.
fn cuboid_face_normal(tag: CuboidFaceTag) -> [f32; 3] {
    match tag {
        CuboidFaceTag::NegX => [-1.0, 0.0, 0.0],
        CuboidFaceTag::PosX => [1.0, 0.0, 0.0],
        CuboidFaceTag::NegY => [0.0, -1.0, 0.0],
        CuboidFaceTag::PosY => [0.0, 1.0, 0.0],
        CuboidFaceTag::NegZ => [0.0, 0.0, -1.0],
        CuboidFaceTag::PosZ => [0.0, 0.0, 1.0],
    }
}

// ---------------------------------------------------------------------------
// Sub-α unit tests — Cuboid constructor + helper-table correctness.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::Operator;
    use crate::topology::BRepEdgeProvider;

    fn unit_cube() -> CuboidOp {
        CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }
    }

    fn owner() -> BRepOwnerId {
        BRepOwnerId::from_bytes([0xed; 16])
    }

    #[test]
    fn new_rejects_zero_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err = FilletOp::new(&cube, owner(), vec![edge], 0.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_rejects_negative_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err = FilletOp::new(&cube, owner(), vec![edge], -1.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == -1.0));
    }

    #[test]
    fn new_rejects_non_finite_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err_nan = FilletOp::new(&cube, owner(), vec![edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, FilletError::InvalidRadius { .. }));
        let err_inf = FilletOp::new(&cube, owner(), vec![edge], f32::INFINITY).unwrap_err();
        assert!(matches!(err_inf, FilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_rejects_empty_edge_list() {
        let cube = unit_cube();
        let err = FilletOp::new(&cube, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, FilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_rejects_unknown_edge_id() {
        let cube = unit_cube();
        // Synthesize an edge ID with bytes that don't match any
        // valid Cuboid edge under this owner.
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err = FilletOp::new(&cube, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, FilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    #[test]
    fn new_accepts_valid_single_edge() {
        let cube = unit_cube();
        let first_edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![first_edge], 0.1).expect("valid");
        assert_eq!(op.edges(), &[first_edge]);
        assert!((op.radius() - 0.1).abs() < f32::EPSILON);
        assert_eq!(op.owner(), owner());
    }

    #[test]
    fn new_accepts_all_12_edges() {
        let cube = unit_cube();
        let all_edges = cube.brep_edge_ids(owner());
        let op = FilletOp::new(&cube, owner(), all_edges.clone(), 0.05).expect("12 edges");
        assert_eq!(op.edges().len(), 12);
        assert_eq!(op.edges(), &all_edges[..]);
    }

    #[test]
    fn evaluate_one_edge_adds_2_vertices_and_2_triangles() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        // Cuboid: 8 verts + 36 indices (12 triangles).
        // After 1 fillet: +2 verts + 6 indices (2 triangles).
        assert_eq!(out.vertex_count(), 10);
        assert_eq!(out.indices.len(), 42);
        assert_eq!(out.triangle_count(), 14);
    }

    #[test]
    fn evaluate_three_edges_adds_6_vertices_and_6_triangles() {
        let cube = unit_cube();
        let all_edges = cube.brep_edge_ids(owner());
        // Use 3 non-adjacent edges (the 3 edges of corner 0 would
        // share corner 0; the spec says the substrate-validation
        // tests don't exercise that case — pick edges that don't all
        // meet at a single corner).
        let op = FilletOp::new(
            &cube,
            owner(),
            vec![all_edges[0], all_edges[5], all_edges[11]],
            0.1,
        )
        .expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        // 8 + 6 = 14 verts; 36 + 18 = 54 indices.
        assert_eq!(out.vertex_count(), 14);
        assert_eq!(out.indices.len(), 54);
        assert_eq!(out.triangle_count(), 18);
    }

    #[test]
    fn cuboid_edge_corner_indices_arg_order_independent() {
        // f(a, b) == f(b, a) for every valid pair.
        for &(a, b) in &CUBOID_EDGE_TAG_PAIRS {
            let (i_ab, j_ab) = cuboid_edge_corner_indices(a, b);
            let (i_ba, j_ba) = cuboid_edge_corner_indices(b, a);
            assert_eq!(
                (i_ab, j_ab),
                (i_ba, j_ba),
                "corner-indices helper must be order-independent for ({a:?}, {b:?})"
            );
        }
    }

    #[test]
    fn cuboid_edge_corner_indices_all_pairs_in_bounds() {
        // All 12 canonical pairs return corner indices in [0, 8).
        for &(a, b) in &CUBOID_EDGE_TAG_PAIRS {
            let (i, j) = cuboid_edge_corner_indices(a, b);
            assert!(
                i < 8 && j < 8,
                "corner indices ({i}, {j}) for ({a:?}, {b:?}) out of cuboid 8-corner bounds"
            );
            assert_ne!(i, j, "edge endpoints must differ for ({a:?}, {b:?})");
        }
    }
}
