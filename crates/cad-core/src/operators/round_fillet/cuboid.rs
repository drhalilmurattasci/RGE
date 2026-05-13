//! `RoundFilletOp` constructor + helpers for `CuboidOp` upstream (sub-α).
//!
//! Per ADR-119 D5 (substrate parallelism, not sharing), this module
//! mirrors the shape of the chamfer side's
//! [`crate::operators::fillet::cuboid`] module but stays
//! byte-distinct: distinct types, distinct trait impl, distinct
//! constants. The shared knowledge — Cuboid's 8-corner layout, the
//! 12-edge canonical adjacency table, the discriminant-ordered
//! `CuboidFaceTag` layout — is duplicated here intentionally so any
//! future evolution of either operator stays unilateral.
//!
//! Fixed-topology consumer of [`crate::topology::BRepEdgeId`]. A cuboid
//! always has exactly 12 edges; this module pairs each
//! [`BRepEdgeId`] with the two endpoint corner indices in the upstream
//! Cuboid's 8-vertex layout, the per-face `TopologyFaceId`s for
//! face-strip-removal substitution, and the two in-plane inward
//! directions used by the rolled-cylinder geometry.

use super::{
    RoundFilletError, RoundFilletOp, RoundFilletSpec, RoundFilletSpecKind, RoundFilletUpstream,
};
use crate::operators::CuboidOp;
use crate::tessellation::TopologyFaceId;
use crate::topology::{BRepEdgeId, BRepOwnerId, CuboidFaceTag};

/// Canonical (CuboidFaceTag, CuboidFaceTag) pair table parallel to the
/// 12-edge order returned by `<CuboidOp as BRepEdgeProvider>::brep_edge_ids`.
///
/// Per ADR-119 D5, this table is duplicated (NOT shared) from the
/// chamfer side's `fillet::cuboid::CUBOID_EDGE_TAG_PAIRS`. Both
/// modules MUST stay in sync with the canonical order documented in
/// `<CuboidOp as BRepEdgeProvider>` — re-ordering one without the
/// other would silently desynchronize edge-resolution between chamfer
/// and round-fillet for the same `BRepEdgeId`.
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

/// Compute the per-edge [`RoundFilletSpec`] for canonical Cuboid edge
/// index `0..12`. Always returns `Ok` for in-range indices — a cuboid
/// has no circular-path edges; every Cuboid edge is a clean
/// 2-endpoint adjacency between two perpendicular axis-aligned faces.
///
/// Geometry of `face_a_inward` / `face_b_inward`:
///
/// For an axis-aligned cuboid edge between face A (outward normal
/// `n_a`) and face B (outward normal `n_b`), the edge direction is
/// perpendicular to BOTH normals (along the third axis). The
/// "in-face-A-plane, perpendicular-to-edge, pointing-into-face-A-
/// interior" direction is simply `-n_b` — moving in face A's plane
/// away from face B (which sits on the other side of the shared
/// edge). Symmetrically `face_b_inward = -n_a`.
///
/// This is identical to chamfer's per-vertex inward bisector when
/// added: `face_a_inward + face_b_inward = -(n_a + n_b)`, which
/// matches chamfer's `inward_direction = -(n_a + n_b)/2` up to a
/// factor of 2.
fn cuboid_resolve_round_spec(canonical_index: usize) -> Result<RoundFilletSpec, &'static str> {
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
    let n_a = cuboid_face_normal(tag_a);
    let n_b = cuboid_face_normal(tag_b);
    let face_a_inward = [-n_b[0], -n_b[1], -n_b[2]];
    let face_b_inward = [-n_a[0], -n_a[1], -n_a[2]];
    // TopologyFaceId is the per-tessellation sequential face index
    // matching the canonical face-emission order. CuboidFaceTag's
    // discriminant (NegZ=0, PosZ=1, NegY=2, PosY=3, NegX=4, PosX=5)
    // matches that order exactly by construction (see
    // CuboidOp::evaluate's face_labels build).
    let face_a_id = TopologyFaceId(u64::from(tag_a.discriminant()));
    let face_b_id = TopologyFaceId(u64::from(tag_b.discriminant()));
    Ok(RoundFilletSpec {
        vertex_a,
        vertex_b,
        face_a_id,
        face_b_id,
        face_a_inward,
        face_b_inward,
    })
}

impl RoundFilletUpstream for CuboidOp {
    fn resolve_round_spec(
        &self,
        canonical_index: usize,
    ) -> Result<RoundFilletSpecKind, &'static str> {
        // Sub-ζ Commit 1 wrap: the free function `cuboid_resolve_round_spec`
        // still returns `RoundFilletSpec` directly (so the cuboid.rs test
        // module's direct callers stay byte-identical). The trait impl
        // wraps in `::TwoEndpoint(...)` for the new enum-carrier return
        // type. Cuboid's 12 edges are all 90°-dihedral 2-endpoint cases;
        // never produces `::Path(...)`.
        cuboid_resolve_round_spec(canonical_index).map(RoundFilletSpecKind::TwoEndpoint)
    }
}

impl RoundFilletOp {
    /// Construct a [`RoundFilletOp`] validated against the upstream
    /// Cuboid.
    ///
    /// Each [`BRepEdgeId`] is resolved against
    /// `upstream.brep_edge_ids(owner)`; resolution failure produces
    /// [`RoundFilletError::EdgeNotInUpstream`]. Per-edge
    /// [`RoundFilletSpec`]s are computed at construction time using
    /// the upstream Cuboid's 8-corner layout, outward face normals,
    /// and the canonical face-emission order.
    ///
    /// # Errors
    ///
    /// * [`RoundFilletError::InvalidRadius`] if `radius` is non-finite
    ///   or `<= 0`.
    /// * [`RoundFilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`RoundFilletError::EdgeNotInUpstream`] if any edge ID does
    ///   not appear in `upstream.brep_edge_ids(owner)`.
    /// * [`RoundFilletError::UnsupportedEdgeGeometry`] never occurs
    ///   for Cuboid upstream (every Cuboid edge is supported by v0
    ///   rolled-cylinder geometry); reserved for future upstreams
    ///   with circular-path edges (ADR-119 D8 / sub-ζ).
    pub fn new(
        upstream: &CuboidOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, RoundFilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Cuboid helper tables — derived from cuboid.rs::evaluate's 8-corner layout.
//
// Per ADR-119 D5 these are duplicated from `fillet::cuboid` rather
// than shared. The byte-identical body is intentional: any
// future-evolution divergence (a hypothetical Cuboid winding change
// affecting one operator but not the other) MUST be expressible
// without rippling.
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
    // exactly once. Discriminant order: NegZ=0 < PosZ=1 < NegY=2
    // < PosY=3 < NegX=4 < PosX=5.
    let (lo, hi) = if tag_a.discriminant() <= tag_b.discriminant() {
        (tag_a, tag_b)
    } else {
        (tag_b, tag_a)
    };

    use CuboidFaceTag::{NegX, NegY, NegZ, PosX, PosY, PosZ};
    match (lo, hi) {
        // NegZ (bottom of box, -Z) intersects each of the 4 X/Y faces:
        // these are the 4 edges of the bottom face.
        (NegZ, NegY) => (0, 1),
        (NegZ, PosY) => (3, 2),
        (NegZ, NegX) => (0, 3),
        (NegZ, PosX) => (1, 2),

        // PosZ (top of box, +Z) intersects each of the 4 X/Y faces:
        // these are the 4 edges of the top face.
        (PosZ, NegY) => (4, 5),
        (PosZ, PosY) => (7, 6),
        (PosZ, NegX) => (4, 7),
        (PosZ, PosX) => (5, 6),

        // The 4 vertical edges (Y-axis face × X-axis face).
        (NegY, NegX) => (0, 4),
        (NegY, PosX) => (1, 5),
        (PosY, NegX) => (3, 7),
        (PosY, PosX) => (2, 6),

        // Same axis (NegZ ∩ NegZ or NegZ ∩ PosZ etc.) is not a real
        // cuboid edge. Validation in `RoundFilletOp::new` already
        // rejects these via the BRepEdgeProvider lookup; this arm is
        // a defensive fallback that returns the dummy pair (0, 0) so
        // a hypothetical invalid call produces degenerate output
        // rather than panicking. Unreachable in production paths.
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

    fn assert_pos_close(actual: [f32; 3], expected: [f32; 3], context: &str) {
        for axis in 0..3 {
            assert!(
                (actual[axis] - expected[axis]).abs() < 1e-5,
                "{context}: axis {axis} expected {}, got {}",
                expected[axis],
                actual[axis]
            );
        }
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
        a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    }

    #[test]
    fn new_rejects_zero_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err = RoundFilletOp::new(&cube, owner(), vec![edge], 0.0).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_rejects_negative_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err = RoundFilletOp::new(&cube, owner(), vec![edge], -1.0).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == -1.0));
    }

    #[test]
    fn new_rejects_non_finite_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let err_nan = RoundFilletOp::new(&cube, owner(), vec![edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, RoundFilletError::InvalidRadius { .. }));
        let err_inf = RoundFilletOp::new(&cube, owner(), vec![edge], f32::INFINITY).unwrap_err();
        assert!(matches!(err_inf, RoundFilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_rejects_empty_edge_list() {
        let cube = unit_cube();
        let err = RoundFilletOp::new(&cube, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, RoundFilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_rejects_unknown_edge_id() {
        let cube = unit_cube();
        // Synthesize an edge ID that doesn't match any of the 12
        // Cuboid edges under this owner.
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err = RoundFilletOp::new(&cube, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, RoundFilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    #[test]
    fn new_accepts_valid_single_edge() {
        let cube = unit_cube();
        let first_edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![first_edge], 0.1).expect("valid");
        assert_eq!(op.edges(), &[first_edge]);
        assert!((op.radius() - 0.1).abs() < f32::EPSILON);
        assert_eq!(op.owner(), owner());
    }

    #[test]
    fn new_accepts_all_12_edges() {
        let cube = unit_cube();
        let all_edges = cube.brep_edge_ids(owner());
        let op = RoundFilletOp::new(&cube, owner(), all_edges.clone(), 0.05).expect("12 edges");
        assert_eq!(op.edges().len(), 12);
        assert_eq!(op.edges(), &all_edges[..]);
    }

    /// Evaluate produces the expected vertex / triangle counts for a
    /// single-edge fillet on a unit cube.
    ///
    /// Per-edge geometry (ROUND_FILLET_SEGMENTS = 8):
    ///   - 4 inset vertices added
    ///   - 2 * (N+1) = 18 cylinder vertices added
    ///   - Total: 22 new vertices per edge
    ///   - 2 * N = 16 new cylinder triangles per edge
    ///   - Upstream face triangles are RE-INDEXED (not added); upstream
    ///     triangle count is preserved.
    ///
    /// Unit cube before fillet: 8 verts + 12 tris + 36 indices.
    /// After 1 fillet edge: 30 verts + 28 tris + 84 indices.
    #[test]
    fn evaluate_one_edge_produces_expected_vertex_and_triangle_counts() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), 30, "8 upstream + 22 per-edge");
        assert_eq!(out.triangle_count(), 28, "12 upstream + 16 per-edge");
        assert_eq!(out.indices.len(), 84, "36 upstream + 48 per-edge");
    }

    /// Multi-edge case where the two filleted edges share NO common
    /// corner — the success-criterion case per user direction.
    ///
    /// Edge 0 (`NegZ ∩ NegY`) endpoints: corners (0, 1).
    /// Edge 5 (`PosZ ∩ PosY`) endpoints: corners (7, 6).
    ///   {0, 1} ∩ {7, 6} = ∅  → non-corner-sharing.
    ///
    /// Each filleted edge contributes 22 new verts + 16 new tris.
    /// Result: 8 + 22 + 22 = 52 verts; 12 + 16 + 16 = 44 tris.
    #[test]
    fn evaluate_two_non_corner_sharing_edges_composes_cleanly() {
        let cube = unit_cube();
        let all_edges = cube.brep_edge_ids(owner());
        let op =
            RoundFilletOp::new(&cube, owner(), vec![all_edges[0], all_edges[5]], 0.1).expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), 52);
        assert_eq!(out.triangle_count(), 44);
        assert_eq!(out.indices.len(), 132);
    }

    /// Sub-ε: when two selected edges share a corner and a face, the
    /// shared face corner is clipped to the intersection of both
    /// offset-edge lines, and a nameless corner patch fills the
    /// cylinder-end boundary.
    #[test]
    fn evaluate_two_corner_sharing_edges_adds_corner_blend_patch() {
        let cube = unit_cube();
        let all_edges = cube.brep_edge_ids(owner());
        // Edge 0 (NegZ ∩ NegY) and edge 2 (NegZ ∩ NegX) share
        // corner 0 and face NegZ.
        let op =
            RoundFilletOp::new(&cube, owner(), vec![all_edges[0], all_edges[2]], 0.1).expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), 54);
        assert_eq!(out.triangle_count(), 62);
        assert_eq!(out.indices.len(), 186);

        // NegZ's two triangles both reference corner 0 in the upstream.
        // Sub-ε must replace both with ONE face-corner inset, not the
        // order-dependent per-edge inset from whichever edge ran first.
        let clipped_corner = out.indices[0];
        assert_eq!(out.indices[3], clipped_corner);
        assert_ne!(clipped_corner, 0);
        let pos = out.positions[clipped_corner as usize];
        assert!((pos[0] - -0.4).abs() < 1e-5, "x={}", pos[0]);
        assert!((pos[1] - -0.4).abs() < 1e-5, "y={}", pos[1]);
        assert!((pos[2] - -0.5).abs() < 1e-5, "z={}", pos[2]);

        let labels = out.face_labels.as_ref().expect("labeled");
        for label in labels.iter().skip(12 + 32) {
            assert_eq!(
                *label,
                TopologyFaceId::DEGENERATE,
                "corner-patch triangles remain nameless per ADR-119 D3"
            );
        }
    }

    /// Sub-ε regression: all three Cuboid edges incident to corner 0
    /// participate without order-dependent face substitution. Each
    /// incident face gets its own shared corner inset, and the final
    /// nameless fan has stable aggregate outward orientation.
    #[test]
    fn evaluate_three_corner_incident_edges_resolves_each_face_corner_and_winds_outward() {
        let cube = unit_cube();
        let all_edges = cube.brep_edge_ids(owner());
        // Corner 0 = NegZ ∩ NegY ∩ NegX. These three canonical edges
        // are all incident to it:
        //   0: NegZ ∩ NegY
        //   2: NegZ ∩ NegX
        //   8: NegY ∩ NegX
        let op = RoundFilletOp::new(
            &cube,
            owner(),
            vec![all_edges[0], all_edges[2], all_edges[8]],
            0.1,
        )
        .expect("three corner-incident edges");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), 78);
        assert_eq!(out.triangle_count(), 87);
        assert_eq!(out.indices.len(), 261);

        let labels = upstream.face_labels.as_ref().expect("labeled");
        for (face, expected) in [
            (TopologyFaceId(0), [-0.4, -0.4, -0.5]), // NegZ
            (TopologyFaceId(2), [-0.4, -0.5, -0.4]), // NegY
            (TopologyFaceId(4), [-0.5, -0.4, -0.4]), // NegX
        ] {
            let mut replacement_indices = Vec::new();
            for (tri_idx, label) in labels.iter().enumerate() {
                if *label != face {
                    continue;
                }
                for j in 0..3 {
                    let idx_pos = tri_idx * 3 + j;
                    if upstream.indices[idx_pos] == 0 {
                        replacement_indices.push(out.indices[idx_pos]);
                    }
                }
            }
            assert!(
                !replacement_indices.is_empty(),
                "face {face:?} should reference upstream corner 0"
            );
            let first = replacement_indices[0];
            assert!(
                replacement_indices.iter().all(|idx| *idx == first),
                "face {face:?} should use one shared face-corner inset, got {replacement_indices:?}"
            );
            assert_pos_close(
                out.positions[first as usize],
                expected,
                "shared face-corner inset",
            );
        }

        let out_labels = out.face_labels.as_ref().expect("labeled output");
        let first_corner_patch_tri = upstream.triangle_count() + 3 * 16;
        assert!(
            out_labels[first_corner_patch_tri..]
                .iter()
                .all(|label| *label == TopologyFaceId::DEGENERATE),
            "corner fan triangles remain nameless"
        );

        let mut orientation_sum = 0.0_f32;
        for tri_idx in first_corner_patch_tri..out.triangle_count() {
            let ia = out.indices[tri_idx * 3] as usize;
            let ib = out.indices[tri_idx * 3 + 1] as usize;
            let ic = out.indices[tri_idx * 3 + 2] as usize;
            let a = out.positions[ia];
            let b = out.positions[ib];
            let c = out.positions[ic];
            let normal = cross(sub(b, a), sub(c, a));
            let centroid = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let area_sq = dot(normal, normal);
            assert!(
                area_sq > 1e-12,
                "corner fan triangle {tri_idx} should not be zero-area"
            );
            orientation_sum += dot(normal, centroid);
        }
        assert!(
            orientation_sum > 0.0,
            "corner fan should have aggregate outward orientation for corner 0; sum={orientation_sum}"
        );
    }

    /// Output preserves labeled-ness from the upstream: Cuboid always
    /// emits labels, so RoundFilletOp's output is also labeled. The
    /// new cylinder triangles get `TopologyFaceId::DEGENERATE` per
    /// ADR-119 D3 (cap surfaces nameless in v0).
    #[test]
    fn evaluate_one_edge_preserves_labels_with_degenerate_caps() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert!(
            out.is_labeled(),
            "Cuboid upstream is labeled → output is labeled"
        );
        let labels = out.face_labels.as_ref().expect("labeled");
        assert_eq!(labels.len(), 28, "12 upstream + 16 new cylinder triangles");

        // The first 12 entries are upstream face labels (unchanged
        // — labels for re-indexed triangles stay the same; only the
        // index payload changed). The last 16 entries are the new
        // cylinder cap triangles → all DEGENERATE.
        for (i, label) in labels.iter().enumerate().skip(12) {
            assert_eq!(
                *label,
                TopologyFaceId::DEGENERATE,
                "cylinder cap triangle at index {i} must be DEGENERATE \
                 (nameless cap-face per ADR-119 D3)"
            );
        }
    }

    /// `cuboid_resolve_round_spec` returns a spec whose
    /// `vertex_a` / `vertex_b` match the canonical corner-index
    /// pairing for each of the 12 canonical edges.
    #[test]
    fn cuboid_resolve_round_spec_returns_correct_vertex_pair() {
        // Verify a representative subset of the 12 canonical edges,
        // selected to cover all 3 edge-orientation classes
        // (NegZ-perimeter, PosZ-perimeter, vertical).
        let cases = [
            // (canonical_index, expected (vertex_a, vertex_b))
            (0, (0, 1)),  // NegZ ∩ NegY
            (4, (4, 5)),  // PosZ ∩ NegY
            (8, (0, 4)),  // NegY ∩ NegX
            (11, (2, 6)), // PosY ∩ PosX
        ];
        for &(idx, expected) in &cases {
            let spec =
                cuboid_resolve_round_spec(idx).expect("in-range cuboid edge always resolves");
            assert_eq!(
                (spec.vertex_a, spec.vertex_b),
                expected,
                "edge {idx} vertex pair mismatch"
            );
        }
    }

    /// `cuboid_resolve_round_spec` returns face IDs whose discriminant
    /// matches the canonical `CuboidFaceTag` discriminant.
    #[test]
    fn cuboid_resolve_round_spec_returns_correct_face_ids() {
        // Edge 0 is NegZ ∩ NegY → face_a_id = TopologyFaceId(0) (NegZ),
        // face_b_id = TopologyFaceId(2) (NegY).
        let spec = cuboid_resolve_round_spec(0).expect("ok");
        assert_eq!(spec.face_a_id, TopologyFaceId(0));
        assert_eq!(spec.face_b_id, TopologyFaceId(2));

        // Edge 11 is PosY ∩ PosX → face_a_id = TopologyFaceId(3) (PosY),
        // face_b_id = TopologyFaceId(5) (PosX).
        let spec = cuboid_resolve_round_spec(11).expect("ok");
        assert_eq!(spec.face_a_id, TopologyFaceId(3));
        assert_eq!(spec.face_b_id, TopologyFaceId(5));
    }

    /// `cuboid_resolve_round_spec` rejects out-of-range indices
    /// defensively.
    #[test]
    fn cuboid_resolve_round_spec_rejects_out_of_range() {
        let err = cuboid_resolve_round_spec(12).unwrap_err();
        assert!(err.contains("out of range"));
    }

    /// The two inward vectors in a `RoundFilletSpec` are unit length
    /// and perpendicular to each other (Cuboid faces are axis-aligned
    /// and perpendicular, so the inward bisectors lie along distinct
    /// axes).
    #[test]
    fn cuboid_resolve_round_spec_inward_vectors_unit_and_perpendicular() {
        for idx in 0..CUBOID_EDGE_TAG_PAIRS.len() {
            let spec = cuboid_resolve_round_spec(idx).expect("ok");
            let len_a = (spec.face_a_inward[0] * spec.face_a_inward[0]
                + spec.face_a_inward[1] * spec.face_a_inward[1]
                + spec.face_a_inward[2] * spec.face_a_inward[2])
                .sqrt();
            let len_b = (spec.face_b_inward[0] * spec.face_b_inward[0]
                + spec.face_b_inward[1] * spec.face_b_inward[1]
                + spec.face_b_inward[2] * spec.face_b_inward[2])
                .sqrt();
            assert!(
                (len_a - 1.0).abs() < 1e-6,
                "face_a_inward at idx {idx} not unit"
            );
            assert!(
                (len_b - 1.0).abs() < 1e-6,
                "face_b_inward at idx {idx} not unit"
            );

            let dot = spec.face_a_inward[0] * spec.face_b_inward[0]
                + spec.face_a_inward[1] * spec.face_b_inward[1]
                + spec.face_a_inward[2] * spec.face_b_inward[2];
            assert!(
                dot.abs() < 1e-6,
                "inward vectors at edge idx {idx} not perpendicular (dot={dot})"
            );
        }
    }
}
