//! `RoundFilletOp` constructor + helpers for `RevolveOp` upstream (sub-γ).
//!
//! Per ADR-119 D5 (substrate parallelism, not sharing) + the sub-γ
//! green-light direction ("Revolve cap-side only; full-mode rejects
//! all edges; partial side-side rejects; partial start/end cap-side
//! accepts"), this module mirrors the shape of the chamfer side's
//! [`crate::operators::fillet::revolve`] module but stays
//! byte-distinct AND restricts edge eligibility to the 90°-dihedral
//! cap-side classes.
//!
//! # Edge eligibility (sub-γ scope)
//!
//! `RevolveOp`'s [`crate::topology::BRepEdgeProvider`] impl emits a
//! mode-dependent edge set:
//!
//! * **Full** (`angle == 2π`): `n` edges, all `Side(i) ∩ Side((i+1)%n)`
//!   side-side adjacencies. Each is a **closed circular path** swept
//!   through `segments` vertices — not a clean 2-endpoint edge.
//!   **REJECTED unconditionally.**
//! * **Partial** (`angle < 2π`): `3 * n` edges in three classes:
//!   - `[0, N)` side-side (axial seams) — **1/k-circular arc** with
//!     `segments + 1` vertices; **REJECTED** (circular path).
//!   - `[N, 2N)` start-cap-side (`StartCap ∩ Side(i)`) — straight
//!     two-endpoint edges at ring 0; **SUPPORTED**.
//!   - `[2N, 3N)` end-cap-side (`EndCap ∩ Side(i)`) — straight
//!     two-endpoint edges at ring `segments`; **SUPPORTED**.
//!
//! Sub-γ accepts the `2 * N` cap-side edges in partial mode. Both
//! cap-side classes have **90° dihedrals by construction** (proven
//! algebraically — see [`Self::resolve_round_spec`] inline comments),
//! which fits sub-α's cylinder math (`axis_center = pos + r·(a + b)`,
//! quarter-arc θ ∈ [0, π/2]) without any change to
//! `RoundFilletOp::evaluate`'s body. Side-side edges (circular / arc
//! paths) are unsupported until sub-ζ (general circular-path).
//!
//! # Substrate posture
//!
//! `RoundFilletOp` (struct, evaluate body, error enum, spec, trait,
//! resolver arms) stays **byte-identical to sub-α + sub-β** (`c5c590a`
//! → `7087589` → `d147ba4`). This module adds ONLY the
//! `RoundFilletUpstream` impl for `RevolveOp` and the public
//! `RoundFilletOp::new_for_revolve` constructor (thin delegate to
//! `from_upstream`). Chamfer's `fillet::revolve::FilletOp::new_for_revolve`
//! (D6 byte-identical) is parallel substrate, not shared.
//!
//! # Face-strip substitution localization (load-bearing per user)
//!
//! `RoundFilletOp::evaluate` substitutes `vertex_a` / `vertex_b`
//! indices ONLY in triangles labeled `face_a_id` or `face_b_id`. For
//! cap-side edges this localizes to (a) the cap's fan-triangulated
//! face (StartCap or EndCap), and (b) Side(i)'s ring-0 quad (for
//! start-cap-side) or ring-`segments` quad (for end-cap-side).
//! Side(i)'s remaining `2*(segments - 1)` triangles span other rings
//! whose vertex indices `≥ n` (start-cap-side) or `< segments*n`
//! (end-cap-side) — they never match `vertex_a` or `vertex_b`, so
//! substitution naturally skips them. Adjacent `Side((i±1)%n)` faces
//! that contain the same shared profile-vertex at the cap ring are
//! NOT in the target face set, so substitution does not modify them
//! — producing the v0 "corner gap" visual imperfection pattern
//! identical to sub-α (Cuboid) and sub-β (Extrude cap-perimeter).
//!
//! The `evaluate_face_strip_localization_no_leak_beyond_target_faces`
//! unit test pins this invariant empirically by scanning every
//! upstream-triangle slot post-substitution.
//!
//! # CCW-profile convention
//!
//! Following the sub-β + chamfer-revolve precedent, this module
//! computes inward directions from `RevolveOp.profile.points()`
//! directly (NOT from the CCW-corrected `ordered` ring used by
//! `partial_path.rs::evaluate_partial`). Inward direction is correct
//! for CCW profiles; CW profiles inherit the same caveat documented
//! at `extrude.rs:437` and elsewhere — sub-γ coverage is CCW-only.
//! Geometric mismatch on CW profiles is the same shape as chamfer's;
//! CW-aware Revolve handling is deferred to the broader CW-aware-
//! projection dispatch.

use super::{
    RoundFilletError, RoundFilletOp, RoundFilletSpec, RoundFilletSpecKind, RoundFilletUpstream,
};
use crate::operators::RevolveOp;
use crate::tessellation::TopologyFaceId;
use crate::topology::{BRepEdgeId, BRepOwnerId};

impl RoundFilletUpstream for RevolveOp {
    fn resolve_round_spec(
        &self,
        canonical_index: usize,
    ) -> Result<RoundFilletSpecKind, &'static str> {
        let n = self.profile.len();

        // Full mode: all edges are side-side circular paths.
        if self.is_full_revolution() {
            return Err(
                "revolve full-mode side-side edges are circular paths; not round-fillet-able in sub-γ",
            );
        }

        // Partial mode three-class dispatch over BRepEdgeProvider's
        // canonical emission order (mode-driven, see
        // `revolve/mod.rs::impl BRepEdgeProvider for RevolveOp`):
        //   [0..N)   side-side (axial seam) — 1/k-arc, REJECTED
        //   [N..2N)  start-cap-side — straight 2-endpoint, SUPPORTED
        //   [2N..3N) end-cap-side   — straight 2-endpoint, SUPPORTED
        if canonical_index < n {
            return Err(
                "revolve partial side-side edges are circular arcs; only cap-side edges round-fillet-able in sub-γ",
            );
        }

        // Cap-side dispatch.
        let n_u32 = u32::try_from(n).unwrap_or(u32::MAX);
        let segments = self.segments();
        let angle = self.angle();

        if canonical_index < 2 * n {
            // Start-cap-side edge (canonical index = N + local).
            let local = canonical_index - n;
            let (vertex_a, vertex_b) = revolve_start_cap_side_vertex_pair(local, n_u32);
            let (dx, dy) = revolve_profile_edge_xy(local, self);
            let edge_len = (dx * dx + dy * dy).sqrt();
            if edge_len < 1e-9 {
                // Polygon2D::new rejects coincident points so this is
                // defensive — should be unreachable.
                return Err("revolve profile edge degenerate (zero-length); cannot construct inward direction");
            }
            // face_a = StartCap (TopologyFaceId(n), outward normal -Z).
            // face_b = Side(local) (TopologyFaceId(local)).
            //
            // face_a_inward: in StartCap plane (z=0, XY plane),
            // perpendicular to the cap-side edge (which runs along
            // (dx, dy, 0)), pointing INTO StartCap interior. For a
            // CCW profile in XY, the "left of edge" direction in the
            // cap plane is (-dy, dx, 0)/||(dx,dy)|| — the standard
            // 90°-CCW rotation of the edge tangent. This direction
            // points into the profile polygon's interior (= cap
            // interior).
            //
            // face_b_inward: in Side(local)'s tangent plane at θ=0
            // (spanned by edge direction (dx, dy, 0) and swept tangent
            // (0, 0, 1)), perpendicular to edge, pointing INTO Side
            // interior (= toward θ > 0). The swept tangent +Z is
            // already perpendicular to the edge (which lies in XY) and
            // is unit-length; it IS face_b_inward.
            //
            // Perpendicularity verification: face_a_inward · face_b_inward
            // = (-dy, dx, 0)/||·|| · (0, 0, 1) = 0. ✓
            // Unit-length: both vectors normalized by construction. ✓
            let inv_edge_len = 1.0 / edge_len;
            let face_a_inward = [-dy * inv_edge_len, dx * inv_edge_len, 0.0];
            let face_b_inward = [0.0, 0.0, 1.0];

            return Ok(RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(u64::from(n_u32)),
                face_b_id: TopologyFaceId(local as u64),
                face_a_inward,
                face_b_inward,
            }));
        }

        if canonical_index < 3 * n {
            // End-cap-side edge (canonical index = 2N + local).
            let local = canonical_index - 2 * n;
            let (vertex_a, vertex_b) = revolve_end_cap_side_vertex_pair(local, n_u32, segments);
            let (dx, dy) = revolve_profile_edge_xy(local, self);
            let edge_len = (dx * dx + dy * dy).sqrt();
            if edge_len < 1e-9 {
                return Err("revolve profile edge degenerate (zero-length); cannot construct inward direction");
            }
            // face_a = EndCap (TopologyFaceId(n + 1), outward normal
            // +tangent at θ=angle = (-sin(angle), 0, cos(angle))).
            // face_b = Side(local) (TopologyFaceId(local)).
            //
            // face_a_inward: in EndCap plane (spanned by rotated +X
            // basis (cos(angle), 0, sin(angle)) and +Y basis
            // (0, 1, 0) — the profile rotated to θ=angle),
            // perpendicular to the cap-side edge (which runs along
            // rotated edge direction (dx·cos(angle), dy, dx·sin(angle))),
            // pointing INTO EndCap interior. The "left of edge" in
            // (x_rot, y_rot) coordinates is (-dy, dx) — in 3D this is
            // -dy · x_rot + dx · y_rot
            // = -dy·(cos(angle), 0, sin(angle)) + dx·(0, 1, 0)
            // = (-dy·cos(angle), dx, -dy·sin(angle)), normalized.
            //
            // face_b_inward: in Side(local)'s tangent plane at
            // θ=angle, perpendicular to edge, pointing INTO Side
            // interior (= toward θ < angle). The -swept-tangent at
            // θ=angle is (sin(angle), 0, -cos(angle)) — perpendicular
            // to the edge (which has zero component in the
            // -swept-tangent direction) and unit-length.
            //
            // Perpendicularity verification: face_a_inward · face_b_inward
            // = (-dy·cos(angle))·sin(angle)/||·|| + dx·0/||·||
            //   + (-dy·sin(angle))·(-cos(angle))/||·||
            // = dy/||·|| · (-cos(angle)·sin(angle) + sin(angle)·cos(angle))
            // = 0. ✓
            // Unit-length: face_a_inward = √(dy²·(cos² + sin²) + dx²)
            //                            / ||(dx, dy)||
            //              = √(dx² + dy²) / ||(dx, dy)|| = 1. ✓
            //              face_b_inward = √(sin² + cos²) = 1. ✓
            let inv_edge_len = 1.0 / edge_len;
            let cos_a = angle.cos();
            let sin_a = angle.sin();
            let face_a_inward = [
                -dy * cos_a * inv_edge_len,
                dx * inv_edge_len,
                -dy * sin_a * inv_edge_len,
            ];
            let face_b_inward = [sin_a, 0.0, -cos_a];

            return Ok(RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(u64::from(n_u32) + 1),
                face_b_id: TopologyFaceId(local as u64),
                face_a_inward,
                face_b_inward,
            }));
        }

        // Defensive: from_upstream's caller-side filter already
        // restricts canonical_index to the upstream's brep_edge_ids
        // length (exactly 3N for partial mode). Unreachable in
        // production paths.
        Err("revolve canonical edge index out of range (must be < 3N for partial mode)")
    }
}

impl RoundFilletOp {
    /// Construct a [`RoundFilletOp`] validated against the upstream
    /// `RevolveOp`, with edge selection restricted to **partial-mode
    /// cap-side edges** (start-cap-side + end-cap-side; `2 * N` edges
    /// total for an N-vertex profile).
    ///
    /// Mirrors [`RoundFilletOp::new`] (Cuboid) +
    /// [`RoundFilletOp::new_for_extrude`] (Extrude cap-perimeter) but
    /// applies a stricter geometry filter:
    ///
    /// * **Full-mode** edges all reject with
    ///   [`RoundFilletError::UnsupportedEdgeGeometry`] (closed
    ///   circular paths; sub-ζ scope).
    /// * **Partial-mode side-side** edges (canonical `[0, N)`) reject
    ///   with [`RoundFilletError::UnsupportedEdgeGeometry`] (1/k
    ///   circular arc; sub-ζ scope).
    /// * **Partial-mode cap-side** edges (canonical `[N, 3N)`) are
    ///   accepted — straight two-endpoint edges with 90° dihedrals
    ///   by construction; fit sub-α's cylinder math.
    ///
    /// # Errors
    ///
    /// * [`RoundFilletError::InvalidRadius`] if `radius` is non-finite
    ///   or `<= 0`.
    /// * [`RoundFilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`RoundFilletError::EdgeNotInUpstream`] if any edge ID does
    ///   not appear in `upstream.brep_edge_ids(owner)`.
    /// * [`RoundFilletError::UnsupportedEdgeGeometry`] if any edge ID
    ///   corresponds to a full-mode side-side edge or a partial-mode
    ///   side-side edge (canonical `[0, N)`). Sub-ζ lifts this
    ///   restriction.
    pub fn new_for_revolve(
        upstream: &RevolveOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, RoundFilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Revolve helpers — derived from revolve/partial_path.rs's
// `n * (segments + 1)` vertex layout (ring r occupies indices
// `r * n .. (r + 1) * n`).
//
// Per ADR-119 D5 these are duplicated from `fillet::revolve` (chamfer)
// rather than shared. The byte-identical formulas for vertex-pair
// mapping are intentional; future-evolution divergence (a
// hypothetical Revolve winding change affecting one operator but not
// the other) MUST be expressible without rippling.
// ---------------------------------------------------------------------------

/// Map a start-cap-side local index `i ∈ [0, N)` to the
/// `(vertex_a, vertex_b)` pair in the upstream Revolve's vertex array.
///
/// Start cap occupies ring 0 (positions `0..N`); cap-side edge `i`
/// connects `bottom_ring[i]` and `bottom_ring[(i + 1) % N]`.
fn revolve_start_cap_side_vertex_pair(i: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let i_u32 = i as u32;
    let next = ((i + 1) % n) as u32;
    (i_u32, next)
}

/// Map an end-cap-side local index `i ∈ [0, N)` to the
/// `(vertex_a, vertex_b)` pair in the upstream Revolve's vertex array.
///
/// End cap occupies ring `segments` (positions `segments*N ..
/// (segments + 1)*N`); cap-side edge `i` connects
/// `end_ring[i]` and `end_ring[(i + 1) % N]`.
fn revolve_end_cap_side_vertex_pair(i: usize, profile_count: u32, segments: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let ring_offset = segments.saturating_mul(profile_count);
    let i_u32 = i as u32;
    let next = ((i + 1) % n) as u32;
    (
        ring_offset.saturating_add(i_u32),
        ring_offset.saturating_add(next),
    )
}

/// Return the raw (un-normalized) XY-plane edge vector
/// `(dx, dy) = profile[(i+1) % N] - profile[i]` from the upstream's
/// stored profile points.
///
/// **CCW-profile convention**: returns the edge in stored-profile
/// order. For CCW profiles (signed_area > 0) this matches the
/// tessellation's `ordered` ring exactly. For CW profiles the
/// tessellation reverses to `ordered = profile.points().rev()`, so
/// the returned `(dx, dy)` is reversed relative to the tessellation's
/// actual ring-0 edge — same caveat as chamfer's
/// `fillet/revolve.rs` and `fillet/extrude.rs`, and same caveat
/// documented at `extrude.rs:437`. Sub-γ coverage is CCW-only.
fn revolve_profile_edge_xy(i: usize, upstream: &RevolveOp) -> (f32, f32) {
    let n = upstream.profile.len();
    let p_i = upstream.profile.points()[i];
    let p_next = upstream.profile.points()[(i + 1) % n];
    (p_next[0] - p_i[0], p_next[1] - p_i[1])
}

// ---------------------------------------------------------------------------
// Sub-γ unit tests — Revolve constructor + cap-side acceptance + side-side
// rejection + full-mode rejection + face-strip localization invariant.
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

    /// 4-vertex profile in +X half-plane.
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

    // -- Construction reject paths -------------------------------------------

    #[test]
    fn new_for_revolve_rejects_zero_radius() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let cap_edge = revolve.brep_edge_ids(owner())[4]; // first start-cap-side
        let err =
            RoundFilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], 0.0).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_for_revolve_rejects_negative_radius() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let cap_edge = revolve.brep_edge_ids(owner())[4];
        let err =
            RoundFilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], -0.1).unwrap_err();
        assert!(
            matches!(err, RoundFilletError::InvalidRadius { radius } if (radius - -0.1).abs() < 1e-6)
        );
    }

    #[test]
    fn new_for_revolve_rejects_non_finite_radius() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let cap_edge = revolve.brep_edge_ids(owner())[4];
        let err_nan = RoundFilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], f32::NAN)
            .unwrap_err();
        assert!(matches!(err_nan, RoundFilletError::InvalidRadius { .. }));
        let err_inf =
            RoundFilletOp::new_for_revolve(&revolve, owner(), vec![cap_edge], f32::INFINITY)
                .unwrap_err();
        assert!(matches!(err_inf, RoundFilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_for_revolve_rejects_empty_edge_list() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let err = RoundFilletOp::new_for_revolve(&revolve, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, RoundFilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_for_revolve_rejects_unknown_edge_id() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("rev");
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err =
            RoundFilletOp::new_for_revolve(&revolve, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, RoundFilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    // -- Mode + edge-class rejection ----------------------------------------

    /// Full revolution has ZERO round-fillet-able edges in sub-γ —
    /// every edge in `brep_edge_ids` is a closed circular path.
    /// Mirrors chamfer's `new_for_revolve_full_mode_all_edges_unsupported`.
    #[test]
    fn new_for_revolve_full_mode_all_edges_unsupported() {
        let revolve = RevolveOp::new(ring_profile(), 8).expect("full");
        let edges = revolve.brep_edge_ids(owner());
        assert_eq!(edges.len(), 4, "full mode: n=4 side-side edges only");
        for &edge in &edges {
            let err =
                RoundFilletOp::new_for_revolve(&revolve, owner(), vec![edge], 0.05).unwrap_err();
            match err {
                RoundFilletError::UnsupportedEdgeGeometry { edge: e, reason } => {
                    assert_eq!(e, edge);
                    assert!(
                        reason.contains("full-mode") || reason.contains("circular"),
                        "expected reason to mention full-mode / circular, got: {reason}"
                    );
                }
                other => {
                    panic!("expected UnsupportedEdgeGeometry for full-mode edge, got {other:?}")
                }
            }
        }
    }

    /// Partial revolution side-side edges (canonical `[0, N)`) reject
    /// with `UnsupportedEdgeGeometry` — they're 1/k-circular arcs.
    #[test]
    fn new_for_revolve_partial_mode_side_side_unsupported() {
        let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n: usize = 4;
        assert_eq!(edges.len(), 3 * n);
        for canonical in 0..n {
            let edge = edges[canonical];
            let err =
                RoundFilletOp::new_for_revolve(&revolve, owner(), vec![edge], 0.05).unwrap_err();
            assert!(
                matches!(err, RoundFilletError::UnsupportedEdgeGeometry { edge: e, .. } if e == edge),
                "partial side-side canonical {canonical} must reject; got {err:?}"
            );
        }
    }

    /// Mixed selection (cap-side + side-side) rejects the whole
    /// construction at the first unsupported edge in the loop order.
    #[test]
    fn new_for_revolve_partial_mode_mixed_selection_rejects_first_unsupported() {
        let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n = 4usize;
        let cap_side = edges[n]; // first start-cap-side
        let side_side = edges[0]; // first side-side (unsupported)
        let result =
            RoundFilletOp::new_for_revolve(&revolve, owner(), vec![cap_side, side_side], 0.05);
        assert!(matches!(
            result,
            Err(RoundFilletError::UnsupportedEdgeGeometry { edge, .. }) if edge == side_side
        ));
    }

    // -- Cap-side acceptance -------------------------------------------------

    #[test]
    fn new_for_revolve_accepts_single_start_cap_side_edge() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n = 4usize;
        // edges[N..2N] are start-cap-side.
        let op =
            RoundFilletOp::new_for_revolve(&revolve, owner(), vec![edges[n]], 0.05).expect("ok");
        assert_eq!(op.edges(), &[edges[n]]);
    }

    #[test]
    fn new_for_revolve_accepts_single_end_cap_side_edge() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n = 4usize;
        // edges[2N..3N] are end-cap-side.
        let op = RoundFilletOp::new_for_revolve(&revolve, owner(), vec![edges[2 * n]], 0.05)
            .expect("ok");
        assert_eq!(op.edges(), &[edges[2 * n]]);
    }

    #[test]
    fn new_for_revolve_accepts_all_2n_cap_side_edges() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n = 4usize;
        // Cap-side edges occupy [N, 3N).
        let cap_side: Vec<_> = edges[n..3 * n].to_vec();
        assert_eq!(cap_side.len(), 2 * n);
        let op = RoundFilletOp::new_for_revolve(&revolve, owner(), cap_side.clone(), 0.03)
            .expect("8 cap-side");
        assert_eq!(op.edges().len(), 2 * n);
        assert_eq!(op.edges(), &cap_side[..]);
    }

    // -- Evaluation geometry -------------------------------------------------

    /// Start-cap-side fillet on a partial Revolve: per-edge
    /// contribution matches sub-α/β (+22 verts / +16 tris / +48
    /// indices). Upstream baseline depends on (n, segments).
    #[test]
    fn evaluate_one_start_cap_side_edge_produces_expected_counts() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n = 4usize;
        let edge = edges[n]; // first start-cap-side
        let op = RoundFilletOp::new_for_revolve(&revolve, owner(), vec![edge], 0.05).expect("ok");
        let upstream = revolve.evaluate(&[]).expect("rev tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        // Per-edge contribution: +22 verts (4 inset + 18 cylinder),
        // +16 tris (16 cylinder), +48 indices.
        assert_eq!(out.vertex_count(), upstream.vertex_count() + 22);
        assert_eq!(out.triangle_count(), upstream.triangle_count() + 16);
        assert_eq!(out.indices.len(), upstream.indices.len() + 48);
    }

    /// End-cap-side fillet: same +22v/+16t/+48i contribution
    /// (geometry is mirror-symmetric across the rotation axis).
    #[test]
    fn evaluate_one_end_cap_side_edge_produces_expected_counts() {
        let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n = 4usize;
        let edge = edges[2 * n]; // first end-cap-side
        let op = RoundFilletOp::new_for_revolve(&revolve, owner(), vec![edge], 0.05).expect("ok");
        let upstream = revolve.evaluate(&[]).expect("rev tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), upstream.vertex_count() + 22);
        assert_eq!(out.triangle_count(), upstream.triangle_count() + 16);
        assert_eq!(out.indices.len(), upstream.indices.len() + 48);
    }

    /// Profile-size + segments + angle scaling: cap-side count = 2N
    /// per partial Revolve regardless of (segments, angle). Per-edge
    /// fillet contribution stays constant +22v/+16t/+48i. Pins the
    /// `RoundFilletUpstream` impl as agnostic to profile shape and
    /// rotational parameters.
    #[test]
    fn cap_side_count_and_per_edge_growth_independent_of_segments_and_angle() {
        let cases: Vec<(u32, f32, usize)> = vec![
            // (segments, angle, expected_n)
            (3, FRAC_PI_2, 4),
            (8, PI, 4),
            (16, FRAC_PI_2, 5),
        ];
        for &(segments, angle, expected_n) in &cases {
            let profile = if expected_n == 4 {
                ring_profile()
            } else {
                pentagon_ring_profile()
            };
            let revolve = RevolveOp::partial(profile, segments, angle).expect("partial");
            let edges = revolve.brep_edge_ids(owner());
            // 3N total in partial; cap-side is [N, 3N) = 2N.
            assert_eq!(edges.len(), 3 * expected_n);
            let cap_side: Vec<_> = edges[expected_n..3 * expected_n].to_vec();
            assert_eq!(cap_side.len(), 2 * expected_n);

            // Single cap-side fillet — per-edge contribution constant.
            let op = RoundFilletOp::new_for_revolve(&revolve, owner(), vec![cap_side[0]], 0.05)
                .expect("ok");
            let upstream = revolve.evaluate(&[]).expect("tess");
            let out = op.evaluate(&[&upstream]).expect("evaluate");
            assert_eq!(
                out.vertex_count() - upstream.vertex_count(),
                22,
                "case (segments={segments}, angle={angle}, n={expected_n}) per-edge verts"
            );
            assert_eq!(
                out.triangle_count() - upstream.triangle_count(),
                16,
                "case (segments={segments}, angle={angle}, n={expected_n}) per-edge tris"
            );
        }
    }

    // -- Face-strip localization (LOAD-BEARING; user-flagged watch) ----------

    /// **Load-bearing per user direction**: face-strip substitution
    /// must localize to the target cap fan + the target Side(i)'s
    /// ring-0 (or ring-`segments`) quad. Adjacent `Side((i±1)%n)`
    /// faces share the cap-side edge's endpoint vertices via their
    /// ring-0 quad's BL/BR corners but are NOT in the substitution
    /// target set — their index references must remain BYTE-IDENTICAL
    /// to the upstream after substitution. Same invariant for the
    /// EndCap fan when filleting a start-cap-side edge, and for the
    /// StartCap fan when filleting an end-cap-side edge.
    ///
    /// Test scans every upstream-triangle index slot post-substitution:
    /// flipped slots must be (a) referencing vertex_a or vertex_b AND
    /// (b) belonging to a triangle labeled face_a_id or face_b_id.
    /// Any other flipped slot = leak = bug.
    #[test]
    fn evaluate_face_strip_localization_no_leak_beyond_target_faces() {
        // Pentagon profile (n=5) with segments=6, angle=π/2 — gives
        // enough non-trivial topology to expose any leak:
        //   2*5*6 = 60 side triangles + 2*(5-2) = 6 cap triangles
        //   = 66 upstream triangles total.
        //   3N = 15 edges; cap-side = 10.
        //
        // Fillet a single start-cap-side edge i=0:
        //   face_a_id = TopologyFaceId(5) (StartCap)
        //   face_b_id = TopologyFaceId(0) (Side(0))
        //   vertex_a = 0 (ring 0)
        //   vertex_b = 1 (ring 0)
        //
        // Adjacent Side faces that reference vertex_a / vertex_b at
        // ring 0:
        //   Side(4) ring-0 quad's BR = (4+1) % 5 = 0 = vertex_a
        //   Side(1) ring-0 quad's BL = 1 = vertex_b
        // These references MUST NOT change.
        let revolve = RevolveOp::partial(pentagon_ring_profile(), 6, FRAC_PI_2).expect("partial");
        let edges = revolve.brep_edge_ids(owner());
        let n = 5usize;
        let edge = edges[n]; // start-cap-side, local 0
        let op = RoundFilletOp::new_for_revolve(&revolve, owner(), vec![edge], 0.05).expect("ok");
        let upstream = revolve.evaluate(&[]).expect("tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        let labels = upstream
            .face_labels
            .as_ref()
            .expect("Revolve always labels");
        let face_a_id = TopologyFaceId(n as u64); // StartCap = 5
        let face_b_id = TopologyFaceId(0); // Side(0)
        let vertex_a = 0u32;
        let vertex_b = 1u32;

        // Iterate every upstream-triangle slot and classify:
        //   - target triangle (label is face_a_id or face_b_id) AND target vertex (vertex_a or _b)
        //     → MUST be flipped (substituted to an inset index)
        //   - everything else → MUST be byte-identical
        let mut substituted_target_slots = 0usize;
        let mut leaked_non_target_slots: Vec<(usize, TopologyFaceId, u32, u32)> = Vec::new();
        for (tri_idx, label) in labels.iter().enumerate() {
            let is_target_face = *label == face_a_id || *label == face_b_id;
            for j in 0..3 {
                let idx_pos = tri_idx * 3 + j;
                let original = upstream.indices[idx_pos];
                let modified = out.indices[idx_pos];
                let is_target_vertex = original == vertex_a || original == vertex_b;

                if is_target_face && is_target_vertex {
                    // Should have been substituted.
                    assert_ne!(
                        modified, original,
                        "target slot tri={tri_idx} pos={j} label={label:?} \
                         vertex={original} was NOT substituted"
                    );
                    substituted_target_slots += 1;
                } else {
                    // Should be unchanged.
                    if modified != original {
                        leaked_non_target_slots.push((idx_pos, *label, original, modified));
                    }
                }
            }
        }

        assert!(
            leaked_non_target_slots.is_empty(),
            "face-strip substitution LEAKED beyond target faces — {} slots changed unrelated to spec; \
             samples: {:?}",
            leaked_non_target_slots.len(),
            leaked_non_target_slots.iter().take(5).collect::<Vec<_>>()
        );
        assert!(
            substituted_target_slots > 0,
            "no target slot got substituted — face_a_id / face_b_id / vertex spec mismatch"
        );

        // Also verify: triangles labeled NEITHER face_a nor face_b
        // that REFERENCE vertex_a or vertex_b (e.g., adjacent Side(4)
        // and Side(1)) have their references preserved exactly. This
        // is the v0 "corner gap" pattern — preserved across cap-side
        // fillets just like Cuboid/Extrude precedents.
        let mut adjacent_side_references_preserved = false;
        for (tri_idx, label) in labels.iter().enumerate() {
            // Side(4) is TopologyFaceId(4); Side(1) is TopologyFaceId(1).
            let is_adjacent_side = *label == TopologyFaceId(4) || *label == TopologyFaceId(1);
            if !is_adjacent_side {
                continue;
            }
            for j in 0..3 {
                let idx_pos = tri_idx * 3 + j;
                let original = upstream.indices[idx_pos];
                if original == vertex_a || original == vertex_b {
                    assert_eq!(
                        out.indices[idx_pos], original,
                        "adjacent-Side reference at tri={tri_idx} pos={j} label={label:?} \
                         vertex={original} MUST be preserved (v0 corner-gap pattern)"
                    );
                    adjacent_side_references_preserved = true;
                }
            }
        }
        // For pentagon n=5, Side(4) has BR=0=vertex_a at ring 0, and
        // Side(1) has BL=1=vertex_b at ring 0 — so at least one such
        // reference must exist and be preserved.
        assert!(
            adjacent_side_references_preserved,
            "expected at least one adjacent-Side reference to vertex_a/_b but found none — \
             test fixture broken"
        );
    }

    // -- Helper-table + resolver correctness ---------------------------------

    /// Vertex-pair helpers match `partial_path.rs::evaluate_partial`'s
    /// `n * (segments + 1)` layout — ring `r` occupies indices
    /// `r * n .. (r + 1) * n`.
    #[test]
    fn vertex_pair_helpers_match_partial_path_layout() {
        // n=4, segments=8 → ring 0 = positions 0..4; ring 8 = 32..36.
        assert_eq!(revolve_start_cap_side_vertex_pair(0, 4), (0, 1));
        assert_eq!(revolve_start_cap_side_vertex_pair(3, 4), (3, 0)); // wrap
        assert_eq!(revolve_end_cap_side_vertex_pair(0, 4, 8), (32, 33));
        assert_eq!(revolve_end_cap_side_vertex_pair(3, 4, 8), (35, 32)); // wrap

        // n=5, segments=6 → ring 0 = 0..5; ring 6 = 30..35.
        assert_eq!(revolve_start_cap_side_vertex_pair(2, 5), (2, 3));
        assert_eq!(revolve_end_cap_side_vertex_pair(4, 5, 6), (34, 30)); // wrap
    }

    /// `resolve_round_spec` returns specs with the right face IDs:
    /// start-cap-side `i` → (StartCap, Side(i)); end-cap-side `i`
    /// → (EndCap, Side(i)).
    #[test]
    fn resolve_round_spec_face_ids_match_canonical_emission_order() {
        let revolve = RevolveOp::partial(pentagon_ring_profile(), 6, FRAC_PI_2).expect("partial");
        let n: u64 = 5;

        // Start-cap-side canonical 5 (local 0) → StartCap ∩ Side(0).
        let spec = revolve.resolve_round_spec(5).expect("start-cap-side 0");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(n)); // StartCap = 5
        assert_eq!(spec.face_b_id, TopologyFaceId(0)); // Side(0)

        // Start-cap-side canonical 8 (local 3) → StartCap ∩ Side(3).
        let spec = revolve.resolve_round_spec(8).expect("start-cap-side 3");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(n));
        assert_eq!(spec.face_b_id, TopologyFaceId(3));

        // End-cap-side canonical 10 (local 0) → EndCap ∩ Side(0).
        let spec = revolve.resolve_round_spec(10).expect("end-cap-side 0");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(n + 1)); // EndCap = 6
        assert_eq!(spec.face_b_id, TopologyFaceId(0));

        // End-cap-side canonical 14 (local 4) → EndCap ∩ Side(4).
        let spec = revolve.resolve_round_spec(14).expect("end-cap-side 4");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(n + 1));
        assert_eq!(spec.face_b_id, TopologyFaceId(4));

        // Partial side-side canonical 2 → Err.
        let err = revolve.resolve_round_spec(2).unwrap_err();
        assert!(err.contains("side-side") || err.contains("circular"));
    }

    /// Inward vectors are unit-length and pairwise perpendicular for
    /// EVERY cap-side edge across multiple (profile, segments, angle)
    /// tuples — the 90° dihedral invariant the sub-α evaluate body
    /// requires geometrically.
    #[test]
    fn resolve_round_spec_inward_vectors_unit_and_perpendicular_for_cap_side() {
        let cases: Vec<(Polygon2D, u32, f32)> = vec![
            (ring_profile(), 4, FRAC_PI_2),
            (ring_profile(), 8, PI),
            (pentagon_ring_profile(), 6, FRAC_PI_2),
            (pentagon_ring_profile(), 12, 1.234_f32), // arbitrary non-π/2 angle
        ];

        for (profile, segments, angle) in cases {
            let n = profile.len();
            let revolve = RevolveOp::partial(profile.clone(), segments, angle).expect("partial");

            // Cap-side canonical range: [n, 3n).
            for idx in n..(3 * n) {
                let spec = revolve
                    .resolve_round_spec(idx)
                    .expect("cap-side always resolves in partial mode");
                let spec = spec.expect_two_endpoint();
                let len_a = (spec.face_a_inward[0] * spec.face_a_inward[0]
                    + spec.face_a_inward[1] * spec.face_a_inward[1]
                    + spec.face_a_inward[2] * spec.face_a_inward[2])
                    .sqrt();
                let len_b = (spec.face_b_inward[0] * spec.face_b_inward[0]
                    + spec.face_b_inward[1] * spec.face_b_inward[1]
                    + spec.face_b_inward[2] * spec.face_b_inward[2])
                    .sqrt();
                assert!(
                    (len_a - 1.0).abs() < 1e-5,
                    "face_a_inward not unit at idx {idx} (n={n}, segments={segments}, angle={angle}): len={len_a}"
                );
                assert!(
                    (len_b - 1.0).abs() < 1e-5,
                    "face_b_inward not unit at idx {idx} (n={n}, segments={segments}, angle={angle}): len={len_b}"
                );

                let dot = spec.face_a_inward[0] * spec.face_b_inward[0]
                    + spec.face_a_inward[1] * spec.face_b_inward[1]
                    + spec.face_a_inward[2] * spec.face_b_inward[2];
                assert!(
                    dot.abs() < 1e-5,
                    "inward vectors not perpendicular at idx {idx} (n={n}, segments={segments}, angle={angle}): dot={dot}"
                );
            }

            // Side-side canonical [0, n) returns Err — substrate
            // rejects circular paths.
            for idx in 0..n {
                assert!(
                    revolve.resolve_round_spec(idx).is_err(),
                    "side-side idx {idx} (n={n}) must reject"
                );
            }
        }
    }
}
