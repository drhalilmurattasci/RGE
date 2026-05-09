//! `FilletOp` constructor + helpers for `RevolveOp` upstream (sub-γ).
//!
//! Mode-driven topology consumer of [`crate::topology::BRepEdgeId`].
//! `RevolveOp` is the only direct provider whose **edge count depends
//! on the mode** — Full revolution emits `n` side-side adjacencies
//! only; Partial revolution emits `n` side-side + `n` start-cap-side +
//! `n` end-cap-side, `3n` total.
//!
//! See [`super`] for the FilletOp shape and [`super::FilletUpstream`]
//! trait. This file implements the trait for [`RevolveOp`] and exposes
//! [`FilletOp::new_for_revolve`] as the public constructor.
//!
//! # Geometry support matrix (v0)
//!
//! | Mode | Canonical index range | Edge type | Support |
//! |---|---|---|---|
//! | Full | `0..n` | Side-side adjacencies | **Unsupported** (circular path) |
//! | Partial | `0..n` | Side-side adjacencies | **Unsupported** (circular path) |
//! | Partial | `n..2n` | Start-cap-side | **Supported** (2-endpoint) |
//! | Partial | `2n..3n` | End-cap-side | **Supported** (2-endpoint) |
//!
//! Full revolution has zero chamferable edges in v0 because every
//! `Side(i) ∩ Side((i+1) % n)` edge is a swept circular path with
//! `segments` vertices in the tessellation, not a clean 2-endpoint
//! edge. Partial mode supports `2n` of `3n` edges (the cap-side
//! adjacencies). Side-side edges in either mode return
//! [`super::FilletError::UnsupportedEdgeGeometry`] at construction
//! time.
//!
//! Validation always works for any valid Revolve edge — the
//! [`crate::topology::BRepEdgeProvider`] lookup is upstream-agnostic
//! — but evaluation rejects circular-path edges at construction time.
//! A constructed FilletOp can never be in a state where evaluation
//! will fail on unsupported geometry.
//!
//! Consumers can determine ahead of time whether their selection is
//! chamferable by inspecting [`RevolveOp::is_full_revolution`] and
//! the canonical edge index range — but in practice, the
//! construction-time error with the offending edge ID is the canonical
//! signal.

use super::{ChamferSpec, FilletError, FilletOp, FilletUpstream};
use crate::operators::RevolveOp;
use crate::topology::{BRepEdgeId, BRepOwnerId};

impl FilletUpstream for RevolveOp {
    fn resolve_chamfer_spec(&self, canonical_index: usize) -> Result<ChamferSpec, &'static str> {
        let n = self.profile.len();
        if self.is_full_revolution() {
            // Full mode: only side-side adjacencies exist; all
            // circular paths through `segments` vertices.
            return Err(
                "revolve full-mode side-side edges are circular paths; not chamferable in v0",
            );
        }
        // Partial mode: canonical index ranges are
        //   `0..n`   — side-side (unsupported, circular paths)
        //   `n..2n`  — start-cap-side (supported)
        //   `2n..3n` — end-cap-side (supported)
        if canonical_index < n {
            return Err(
                "revolve partial side-side edges are circular paths; only cap-side edges chamferable in v0",
            );
        }

        // Cap-side edge — supported.
        let segments = self.segments() as usize;
        let local = canonical_index - n;
        let is_end_cap = local >= n;
        let cap_local = if is_end_cap { local - n } else { local };
        // Partial mode has rings `0..=segments`. Cap-side edges live
        // at ring 0 (start cap) or ring `segments` (end cap).
        let ring = if is_end_cap { segments } else { 0 };

        // Vertex pair: ring * n + cap_local, ring * n + (cap_local + 1) % n
        let n_u32 = u32::try_from(n).unwrap_or(u32::MAX);
        let ring_offset = u32::try_from(ring)
            .unwrap_or(u32::MAX)
            .saturating_mul(n_u32);
        let vertex_a = ring_offset.saturating_add(u32::try_from(cap_local).unwrap_or(u32::MAX));
        let vertex_b =
            ring_offset.saturating_add(u32::try_from((cap_local + 1) % n).unwrap_or(u32::MAX));

        let inward_direction = revolve_chamfer_inward_direction(self, ring, cap_local, n);

        Ok(ChamferSpec {
            vertex_a,
            vertex_b,
            inward_direction,
        })
    }
}

/// Compute the inward chamfer-offset direction for a Revolve cap-side
/// edge.
///
/// Uses a **centroid-based** approach for substrate honesty: the
/// chamfer pushes the offset vertex toward the operator's tessellation
/// centroid (Y-axis at the edge midpoint's Y-coordinate, since
/// RevolveOp is rotationally symmetric around the Y-axis), producing
/// geometry that visually points into the swept volume's radial
/// interior. The magnitude is normalized to the sub-α half-bisector
/// convention (~0.707) so the structural delta (vertex/index counts)
/// is consistent with Cuboid + Extrude chamfers.
///
/// Cap-side edge endpoints in the upstream's local coordinate frame:
///
/// * `vertex_a = profile[cap_local]` rotated to ring's angle
/// * `vertex_b = profile[(cap_local + 1) % n]` rotated to ring's angle
///
/// For partial mode: ring 0 is at theta=0 (start cap, the profile in
/// the XY plane), ring `segments` is at theta=`self.angle()` (end cap).
fn revolve_chamfer_inward_direction(
    upstream: &RevolveOp,
    ring: usize,
    cap_local: usize,
    n: usize,
) -> [f32; 3] {
    let segments = upstream.segments() as usize;
    // Cap-side edges live at ring 0 (start cap, theta=0) or ring
    // `segments` (end cap, theta=angle). Defensive fallback for any
    // other value computes a proportional theta — should not be
    // reachable since the trait impl gates on Partial mode + cap
    // index range.
    #[allow(
        clippy::cast_precision_loss,
        reason = "segments / ring bounded by UI knob; precision loss in usize→f32 angle math is well below tessellation tolerance"
    )]
    let theta = if ring == 0 {
        0.0_f32
    } else if ring == segments {
        upstream.angle()
    } else {
        (ring as f32 / segments as f32) * upstream.angle()
    };
    let cos_t = theta.cos();
    let sin_t = theta.sin();

    let p_a = upstream.profile.points()[cap_local];
    let p_b = upstream.profile.points()[(cap_local + 1) % n];
    let pos_a = [p_a[0] * cos_t, p_a[1], p_a[0] * sin_t];
    let pos_b = [p_b[0] * cos_t, p_b[1], p_b[0] * sin_t];

    // Edge midpoint in 3D.
    let mid = [
        (pos_a[0] + pos_b[0]) / 2.0,
        (pos_a[1] + pos_b[1]) / 2.0,
        (pos_a[2] + pos_b[2]) / 2.0,
    ];
    // Centroid is approximately on the Y-axis at the profile's mean
    // Y. Use origin-Y projection of the midpoint as the inward
    // target — close enough for the chamfer-approximation purpose.
    let target = [0.0, mid[1], 0.0];
    let raw_dir = [target[0] - mid[0], target[1] - mid[1], target[2] - mid[2]];
    let mag = (raw_dir[0].powi(2) + raw_dir[1].powi(2) + raw_dir[2].powi(2)).sqrt();
    if mag < 1e-9 {
        // Edge is on the rotation axis — degenerate. Use zero offset
        // (chamfer collapses; consumer will see zero-volume cap
        // triangles but no panic).
        return [0.0, 0.0, 0.0];
    }
    let scale = 0.707_f32 / mag; // match sub-α magnitude convention
    [raw_dir[0] * scale, raw_dir[1] * scale, raw_dir[2] * scale]
}

impl FilletOp {
    /// Sub-γ public API — Revolve constructor.
    ///
    /// Validates each [`BRepEdgeId`] against the upstream's
    /// [`crate::topology::BRepEdgeProvider`] (returns
    /// [`FilletError::EdgeNotInUpstream`] for unknown IDs). For known
    /// IDs, attempts to resolve geometry; returns
    /// [`FilletError::UnsupportedEdgeGeometry`] for circular-path
    /// edges (all Full-mode edges and Partial-mode side-side edges).
    ///
    /// # Errors
    ///
    /// See [`FilletError`]. Notably, callers can encounter
    /// [`FilletError::UnsupportedEdgeGeometry`] even with a valid edge
    /// ID — pre-screen selections using [`RevolveOp::is_full_revolution`]
    /// and the canonical edge index range if needed.
    pub fn new_for_revolve(
        upstream: &RevolveOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, FilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Sub-γ unit tests — Revolve constructor + geometry-support matrix.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, PI};

    use super::*;
    use crate::operators::{Operator, Polygon2D};
    use crate::topology::BRepEdgeProvider;

    fn owner() -> BRepOwnerId {
        BRepOwnerId::from_bytes([0xed; 16])
    }

    /// Square profile in +X half-plane (Revolve requires `x >= 0`).
    fn ring_profile() -> Polygon2D {
        Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("ring")
    }

    /// 5-vertex profile in +X half-plane.
    fn pentagon_ring_profile() -> Polygon2D {
        Polygon2D::new(vec![
            [1.0, 0.0],
            [2.0, 0.5],
            [2.5, 1.5],
            [1.5, 2.0],
            [1.0, 1.0],
        ])
        .expect("pentagon ring")
    }

    #[test]
    fn new_for_revolve_rejects_zero_radius() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let cap_edge = revolve.brep_edge_ids(owner())[4]; // first start-cap-side
        let err = FilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], 0.0).unwrap_err();
        assert!(matches!(err, FilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_for_revolve_rejects_negative_radius() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let cap_edge = revolve.brep_edge_ids(owner())[4];
        let err = FilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], -0.1).unwrap_err();
        assert!(
            matches!(err, FilletError::InvalidRadius { radius } if (radius - -0.1).abs() < 1e-6)
        );
    }

    #[test]
    fn new_for_revolve_rejects_non_finite_radius() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let cap_edge = revolve.brep_edge_ids(owner())[4];
        let err_nan =
            FilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, FilletError::InvalidRadius { .. }));
        let err_inf = FilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], f32::INFINITY)
            .unwrap_err();
        assert!(matches!(err_inf, FilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_for_revolve_rejects_empty_edge_list() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let err = FilletOp::new_for_revolve(&revolve, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, FilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_for_revolve_rejects_unknown_edge_id() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err = FilletOp::new_for_revolve(&revolve, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, FilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    /// Full revolution has zero chamferable edges in v0 — every
    /// edge in `revolve.brep_edge_ids(owner)` produces
    /// `UnsupportedEdgeGeometry`.
    #[test]
    fn new_for_revolve_full_mode_all_edges_unsupported() {
        let revolve = RevolveOp::new(ring_profile(), 8).expect("full");
        let edges = revolve.brep_edge_ids(owner());
        assert_eq!(edges.len(), 4); // n=4 for full mode
        for &edge in &edges {
            let err = FilletOp::new_for_revolve(&revolve, owner(), vec![edge], 0.05).unwrap_err();
            assert!(
                matches!(err, FilletError::UnsupportedEdgeGeometry { edge: e, .. } if e == edge),
                "expected UnsupportedEdgeGeometry for full-mode edge {edge:?}, got {err:?}"
            );
        }
    }

    /// Partial revolution side-side edges (canonical 0..n) reject
    /// with `UnsupportedEdgeGeometry`.
    #[test]
    fn new_for_revolve_partial_mode_side_side_unsupported() {
        let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n: usize = 4;
        assert_eq!(edges.len(), 3 * n); // 3*n for partial
        for canonical in 0..n {
            let edge = edges[canonical];
            let err = FilletOp::new_for_revolve(&revolve, owner(), vec![edge], 0.05).unwrap_err();
            assert!(
                matches!(err, FilletError::UnsupportedEdgeGeometry { edge: e, .. } if e == edge),
                "expected UnsupportedEdgeGeometry for partial side-side {canonical}, got {err:?}"
            );
        }
    }

    /// Partial revolution cap-side edges (canonical n..3n) succeed.
    #[test]
    fn new_for_revolve_partial_mode_cap_side_supported() {
        let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n: usize = 4;
        // n..2n: start-cap-side; 2n..3n: end-cap-side.
        for canonical in n..(3 * n) {
            let edge = edges[canonical];
            let op =
                FilletOp::new_for_revolve(&revolve, owner(), vec![edge], 0.05).expect("cap-side");
            assert_eq!(op.edges(), &[edge]);
        }
    }

    /// Mixed selection: a side-side edge in the list rejects the
    /// whole construction with `UnsupportedEdgeGeometry` carrying the
    /// offending side-side edge ID.
    #[test]
    fn new_for_revolve_partial_mode_mixed_selection_rejects_first_unsupported() {
        let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n = 4usize;
        let cap_side = edges[n]; // start-cap-side
        let side_side = edges[0]; // side-side (unsupported)
        let result = FilletOp::new_for_revolve(&revolve, owner(), vec![cap_side, side_side], 0.05);
        assert!(matches!(
            result,
            Err(FilletError::UnsupportedEdgeGeometry { edge, .. }) if edge == side_side
        ));
    }

    /// Single cap-side edge fillet: pentagon profile, partial 90°,
    /// upstream tessellation gains exactly +2 verts + 6 indices.
    #[test]
    fn evaluate_partial_cap_side_edge_adds_2_vertices_and_2_triangles() {
        let revolve = RevolveOp::partial(pentagon_ring_profile(), 8, FRAC_PI_2).expect("rev");
        let edges = revolve.brep_edge_ids(owner());
        let n: usize = 5;
        // First start-cap-side edge.
        let cap_edge = edges[n];
        let op = FilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], 0.05).expect("ok");
        let upstream = revolve.evaluate(&[]).expect("rev tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        assert_eq!(out.positions.len(), upstream.positions.len() + 2);
        assert_eq!(out.indices.len(), upstream.indices.len() + 6);
    }

    /// Smoke: non-degenerate cap-side inward direction has
    /// magnitude ~0.707 (sub-α convention).
    #[test]
    fn revolve_chamfer_spec_centroid_geometry_smoke() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        // Start-cap edge between profile[0]=(1,0) and profile[1]=(2,0)
        // at ring 0 (theta=0). Midpoint ≈ (1.5, 0, 0). Target = (0,0,0).
        // raw_dir ≈ (-1.5, 0, 0); magnitude 1.5.
        let n: usize = 4;
        let dir = revolve_chamfer_inward_direction(&revolve, 0, 0, n);
        let mag = (dir[0].powi(2) + dir[1].powi(2) + dir[2].powi(2)).sqrt();
        assert!(
            (mag - 0.707).abs() < 1e-3,
            "inward direction magnitude must be ~0.707 (got {mag})"
        );
        // Direction must point in the -X half-space (toward Y-axis
        // from a +X-side profile vertex).
        assert!(
            dir[0] < 0.0,
            "inward direction X must be negative (got {})",
            dir[0]
        );
    }
}
