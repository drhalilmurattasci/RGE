//! `FilletOp` constructor + helpers for `ExtrudeOp` upstream (sub-β).
//!
//! Variable-topology consumer of [`crate::topology::BRepEdgeId`]. An
//! Extrude with profile of `N` vertices yields exactly `3 * N` edges
//! (`N` bottom-perimeter, `N` top-perimeter, `N` vertical-seam) per
//! the [`crate::topology::BRepEdgeProvider`] impl on `ExtrudeOp`. This
//! module pairs each [`BRepEdgeId`] with the two endpoint corner
//! indices in the extrude's `2N`-vertex layout and the inward bisector
//! direction derived from the two adjacent face outward normals.
//!
//! Sub-γ refactor: per-edge resolution lives behind the
//! [`super::FilletUpstream`] trait so [`FilletOp::from_upstream`] can
//! drive the shared validation pipeline. [`FilletOp::new_for_extrude`]
//! is now a thin delegate.

use super::{ChamferSpec, FilletError, FilletOp, FilletUpstream};
use crate::operators::ExtrudeOp;
use crate::topology::{BRepEdgeId, BRepOwnerId};

impl FilletUpstream for ExtrudeOp {
    fn resolve_chamfer_spec(&self, canonical_index: usize) -> Result<ChamferSpec, &'static str> {
        let n = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        Ok(extrude_chamfer_spec(canonical_index, n, self))
    }
}

impl FilletOp {
    /// Construct a FilletOp validated against the upstream Extrude.
    ///
    /// Mirrors [`FilletOp::new`] (Cuboid) but resolves edges against
    /// `upstream.brep_edge_ids(owner)` (the
    /// [`crate::topology::BRepEdgeProvider`] impl on [`ExtrudeOp`],
    /// emitting `3 * N` edges for an `N`-vertex profile in the
    /// canonical order
    /// `[Bottom-perimeter | Top-perimeter | Vertical-seams]`) and
    /// computes chamfer offsets from profile-edge geometry.
    ///
    /// # Errors
    ///
    /// * [`FilletError::InvalidRadius`] if `radius` is non-finite or
    ///   `<= 0`.
    /// * [`FilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`FilletError::EdgeNotInUpstream`] if any edge ID does not
    ///   appear in `upstream.brep_edge_ids(owner)`.
    pub fn new_for_extrude(
        upstream: &ExtrudeOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, FilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Extrude helpers — derived from extrude.rs::evaluate's 2N-vertex layout.
// ---------------------------------------------------------------------------

/// Map a canonical Extrude edge index `0..3N` to the `(vertex_a, vertex_b)`
/// pair in the upstream tessellation's vertex array.
///
/// `ExtrudeOp` vertex layout (per `extrude.rs::evaluate`):
///
/// * `positions[0..N]` — bottom ring at `z = 0`, in the (CCW-corrected)
///   profile order.
/// * `positions[N..2N]` — top ring at `z = length`, same order.
///
/// `BRepEdgeProvider for ExtrudeOp` emission order:
///
/// * indices `0..N` — bottom-perimeter edges; edge `i` connects
///   `bottom_ring[i]` and `bottom_ring[(i+1) % N]`.
/// * indices `N..2N` — top-perimeter edges; edge `i` (local) connects
///   `top_ring[i]` and `top_ring[(i+1) % N]`.
/// * indices `2N..3N` — vertical-seam edges; seam `i` (local) is the
///   shared edge between `Side(i)` and `Side((i + 1) % N)`. With
///   `Side(i)` covering the profile edge `(profile[i] → profile[i+1])`,
///   the shared boundary between consecutive sides is the vertical
///   spanning `bottom_ring[(i+1) % N] → top_ring[(i+1) % N]`.
fn extrude_edge_vertex_pair(canonical_index: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    if canonical_index < n {
        // Bottom-perimeter edge i.
        let i = canonical_index;
        let i_u32 = i as u32;
        let next = ((i + 1) % n) as u32;
        (i_u32, next)
    } else if canonical_index < 2 * n {
        // Top-perimeter edge i (local index = canonical_index - N).
        let local = canonical_index - n;
        let i = local as u32;
        let next = ((local + 1) % n) as u32;
        (profile_count + i, profile_count + next)
    } else {
        // Vertical seam i (local index = canonical_index - 2N).
        // Connects bottom_ring[(i+1) % N] with top_ring[(i+1) % N].
        let local = canonical_index - 2 * n;
        let v = ((local + 1) % n) as u32;
        (v, profile_count + v)
    }
}

/// Outward normal of Extrude's side face `i`, in the XY plane.
///
/// `Side(i)` corresponds to the profile edge from `profile[i]` to
/// `profile[(i+1) % N]`. For a CCW-wound profile (signed_area > 0),
/// the outward normal is obtained by rotating the edge vector
/// `(dx, dy)` by `-90°`, i.e. `(dy, -dx)`. Returns the zero vector
/// for a degenerate (zero-length) edge — `Polygon2D::new` rejects
/// these at construction so this is a defensive fallback.
fn extrude_side_outward_normal(i: usize, upstream: &ExtrudeOp) -> [f32; 3] {
    let n = upstream.profile.len();
    let p_i = upstream.profile.points()[i];
    let p_next = upstream.profile.points()[(i + 1) % n];
    let dx = p_next[0] - p_i[0];
    let dy = p_next[1] - p_i[1];
    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        return [0.0, 0.0, 0.0];
    }
    [dy / mag, -dx / mag, 0.0]
}

/// Compute the inward chamfer-offset direction for an Extrude edge.
///
/// Magnitude is the un-normalized half-bisector length (matches the
/// magnitude convention sub-α used for Cuboid: `-(normal_a + normal_b) / 2`).
/// At evaluation time, this is multiplied by `radius` to produce the
/// actual chamfer offset.
///
/// `canonical_index` is in the same `0..3N` space as
/// [`extrude_edge_vertex_pair`]:
///
/// * `0..N` (bottom-perimeter): bisector of `Bottom = (0, 0, -1)` and
///   `Side(i)` outward normals.
/// * `N..2N` (top-perimeter): bisector of `Top = (0, 0, 1)` and
///   `Side(i)` outward normals.
/// * `2N..3N` (vertical-seam): bisector of `Side(i)` and `Side((i+1) % N)`
///   outward normals (both lie in the XY plane).
fn extrude_chamfer_inward_direction(
    canonical_index: usize,
    profile_count: u32,
    upstream: &ExtrudeOp,
) -> [f32; 3] {
    let n = profile_count as usize;
    if canonical_index < n {
        // Bottom-perimeter edge i: adjacent faces are Bottom (-Z) and Side(i).
        let i = canonical_index;
        let side_normal = extrude_side_outward_normal(i, upstream);
        // -(side + bottom) / 2 = -(side_xy + (0,0,-1)) / 2.
        [
            -side_normal[0] / 2.0,
            -side_normal[1] / 2.0,
            0.5, // -((-1) + 0) / 2 = 0.5; side has no Z component
        ]
    } else if canonical_index < 2 * n {
        // Top-perimeter edge i: adjacent faces are Top (+Z) and Side(i).
        let local = canonical_index - n;
        let side_normal = extrude_side_outward_normal(local, upstream);
        [
            -side_normal[0] / 2.0,
            -side_normal[1] / 2.0,
            -0.5, // -((1) + 0) / 2 = -0.5
        ]
    } else {
        // Vertical-seam edge i: adjacent faces are Side(i) and Side((i+1) % N).
        let local = canonical_index - 2 * n;
        let normal_i = extrude_side_outward_normal(local, upstream);
        let normal_next = extrude_side_outward_normal((local + 1) % n, upstream);
        [
            -(normal_i[0] + normal_next[0]) / 2.0,
            -(normal_i[1] + normal_next[1]) / 2.0,
            0.0, // both side normals lie in XY plane
        ]
    }
}

/// Build a [`ChamferSpec`] for one canonical Extrude edge index.
fn extrude_chamfer_spec(
    canonical_index: usize,
    profile_count: u32,
    upstream: &ExtrudeOp,
) -> ChamferSpec {
    let (vertex_a, vertex_b) = extrude_edge_vertex_pair(canonical_index, profile_count);
    let inward_direction =
        extrude_chamfer_inward_direction(canonical_index, profile_count, upstream);
    ChamferSpec {
        vertex_a,
        vertex_b,
        inward_direction,
    }
}

// ---------------------------------------------------------------------------
// Sub-β unit tests — Extrude constructor + helper-table correctness.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::{Operator, Polygon2D};
    use crate::topology::BRepEdgeProvider;

    fn owner() -> BRepOwnerId {
        BRepOwnerId::from_bytes([0xed; 16])
    }

    fn unit_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("ccw unit square")
    }

    fn small_pentagon() -> Polygon2D {
        Polygon2D::new(vec![
            [1.0, 0.0],
            [0.309, 0.951],
            [-0.809, 0.588],
            [-0.809, -0.588],
            [0.309, -0.951],
        ])
        .expect("ccw regular pentagon")
    }

    #[test]
    fn new_for_extrude_rejects_zero_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err = FilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_for_extrude_rejects_negative_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err = FilletOp::new_for_extrude(&extrude, owner(), vec![edge], -1.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == -1.0));
    }

    #[test]
    fn new_for_extrude_rejects_non_finite_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err_nan =
            FilletOp::new_for_extrude(&extrude, owner(), vec![edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, FilletError::InvalidRadius { .. }));
        let err_inf =
            FilletOp::new_for_extrude(&extrude, owner(), vec![edge], f32::INFINITY).unwrap_err();
        assert!(matches!(err_inf, FilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_for_extrude_rejects_empty_edge_list() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let err = FilletOp::new_for_extrude(&extrude, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, FilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_for_extrude_rejects_unknown_edge_id() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err = FilletOp::new_for_extrude(&extrude, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, FilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    #[test]
    fn new_for_extrude_accepts_single_bottom_perimeter_edge() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edges = extrude.brep_edge_ids(owner());
        // Bottom-perimeter edges occupy indices 0..N=4. Edge[0] is
        // Bottom ∩ Side(0).
        let op = FilletOp::new_for_extrude(&extrude, owner(), vec![edges[0]], 0.1).expect("ok");
        assert_eq!(op.edges(), &[edges[0]]);
        assert!((op.radius() - 0.1).abs() < f32::EPSILON);
        assert_eq!(op.owner(), owner());
    }

    #[test]
    fn new_for_extrude_accepts_all_3n_edges() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        assert_eq!(all_edges.len(), 12); // 3 * 4
        let op = FilletOp::new_for_extrude(&extrude, owner(), all_edges.clone(), 0.05)
            .expect("12 edges");
        assert_eq!(op.edges().len(), 12);
        assert_eq!(op.edges(), &all_edges[..]);
    }

    /// Confirm the canonical edge → vertex-pair mapping for a
    /// 4-vertex profile against extrude.rs::evaluate's vertex layout.
    /// For N=4:
    /// * Bottom-perimeter `0` connects `bottom_ring[0]=0` and
    ///   `bottom_ring[1]=1` → `(0, 1)`.
    /// * Top-perimeter `0` (canonical index 4) connects
    ///   `top_ring[0]=4` and `top_ring[1]=5` → `(4, 5)`.
    /// * Vertical-seam `0` (canonical index 8) connects
    ///   `bottom_ring[1]=1` and `top_ring[1]=5` → `(1, 5)`.
    #[test]
    fn extrude_edge_vertex_pair_table_correctness() {
        // Bottom perimeter (canonical_index 0..N).
        assert_eq!(extrude_edge_vertex_pair(0, 4), (0, 1));
        assert_eq!(extrude_edge_vertex_pair(1, 4), (1, 2));
        assert_eq!(extrude_edge_vertex_pair(2, 4), (2, 3));
        assert_eq!(extrude_edge_vertex_pair(3, 4), (3, 0));
        // Top perimeter (canonical_index N..2N).
        assert_eq!(extrude_edge_vertex_pair(4, 4), (4, 5));
        assert_eq!(extrude_edge_vertex_pair(5, 4), (5, 6));
        assert_eq!(extrude_edge_vertex_pair(6, 4), (6, 7));
        assert_eq!(extrude_edge_vertex_pair(7, 4), (7, 4));
        // Vertical seams (canonical_index 2N..3N).
        // Side(0) ∩ Side(1) is the boundary between profile edge 0
        // (p0 → p1) and profile edge 1 (p1 → p2) — they share p1, so
        // the vertical seam runs from bottom_ring[1] to top_ring[1].
        assert_eq!(extrude_edge_vertex_pair(8, 4), (1, 5));
        assert_eq!(extrude_edge_vertex_pair(9, 4), (2, 6));
        assert_eq!(extrude_edge_vertex_pair(10, 4), (3, 7));
        assert_eq!(extrude_edge_vertex_pair(11, 4), (0, 4));
    }

    #[test]
    fn evaluate_one_extrude_edge_adds_2_vertices_and_2_triangles() {
        let extrude = ExtrudeOp::new(small_pentagon(), 1.5).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let op = FilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.05).expect("ok");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        // Pentagon extrude: 2N=10 verts, 4N-4=16 triangles (48 indices).
        // After 1 fillet: +2 verts + 6 indices (2 triangles).
        assert_eq!(out.positions.len(), upstream.positions.len() + 2);
        assert_eq!(out.indices.len(), upstream.indices.len() + 6);
    }

    #[test]
    fn evaluate_three_extrude_edges_linear_growth() {
        let extrude = ExtrudeOp::new(small_pentagon(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        // Pick 3 non-adjacent canonical edges: one bottom, one top,
        // one vertical-seam (indices 0, 5, 11).
        let op = FilletOp::new_for_extrude(
            &extrude,
            owner(),
            vec![all_edges[0], all_edges[5], all_edges[11]],
            0.05,
        )
        .expect("ok");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        // 3 fillets: +6 verts + 18 indices.
        assert_eq!(out.positions.len(), upstream.positions.len() + 6);
        assert_eq!(out.indices.len(), upstream.indices.len() + 18);
    }
}
