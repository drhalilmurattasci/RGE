// SPLIT-EXEMPTION: cohesive Extrude round-fillet substrate —
// `RoundFilletUpstream` impl for `ExtrudeOp` (sub-β cap-perimeter +
// sub-β.γ-extend vertical-seam lift) + per-edge helpers
// (`extrude_bottom_perimeter_vertex_pair` / `_top_perimeter_*` /
// `_vertical_seam_*` / `extrude_side_outward_normal`) + vector
// primitives + `orient_inward` + `solve_inward_directions`
// (duplicated from `round_fillet/loft.rs` per ADR-119 D5
// substrate-parallelism) + `RoundFilletOp::new_for_extrude`
// constructor + the unit tests that pin sub-β cap-perimeter
// invariants AND sub-β.γ-extend vertical-seam invariants (square
// 90° / pentagon 108° / triangle 60° dihedrals, full-Extrude-
// closure acceptance, face-strip localization no-leak watch).
// Splitting would force the test module to consume `pub(crate)
// RoundFilletSpec` through a public shim, breaking the
// "the operator owns its identity recipe" contract that
// `round_fillet/mod.rs::SPLIT-EXEMPTION` + `extrude.rs::SPLIT-EXEMPTION`
// + `loft.rs::SPLIT-EXEMPTION` cite at the same line-cap boundary.
// Per PLAN.md §1.3 Rule 3 (1146 lines vs 1000-line hard cap; growth
// from sub-β.γ-extend vertical-seam dispatch + 9 new pinning tests
// + 2 reject-test flips for the boundary-shift bookkeeping).
//
//! `RoundFilletOp` constructor + helpers for `ExtrudeOp` upstream
//! (sub-β cap-perimeter + sub-β.γ-extend vertical-seam lift).
//!
//! Per ADR-119 D5 (substrate parallelism, not sharing) + the sub-β
//! green-light direction (cap-perimeter only at sub-β landing) + the
//! sub-β.γ-extend green-light direction (vertical-seam lifted now
//! that general-dihedral cylinder math landed at sub-β.γ `cae3a84`
//! and was empirically validated on a real non-90° upstream at
//! sub-δ.revisit `cf64962`), this module mirrors the shape of the
//! chamfer side's [`crate::operators::fillet::extrude`] module but
//! stays byte-distinct.
//!
//! # Edge eligibility (sub-β.γ-extend scope — full Extrude coverage)
//!
//! `ExtrudeOp`'s [`crate::topology::BRepEdgeProvider`] impl emits `3 * N`
//! edges for a profile of `N` vertices, in three classes:
//!
//! * Indices `[0, N)` — bottom-perimeter (`Bottom ∩ Side(i)`):
//!   straight 2-endpoint edges at `z = 0`. **Accepted** (sub-β; 90°
//!   dihedrals by construction).
//! * Indices `[N, 2N)` — top-perimeter (`Top ∩ Side(i)`): straight
//!   2-endpoint edges at `z = length`. **Accepted** (sub-β; 90°
//!   dihedrals by construction).
//! * Indices `[2N, 3N)` — vertical-seam (`Side(i) ∩ Side((i + 1) % N)`):
//!   straight 2-endpoint edges along `+Z` (Extrude has `profile_b ==
//!   profile_a` positionally so the seam is purely vertical with
//!   `e_t = (0, 0, 1)` exactly). **Accepted** (sub-β.γ-extend; uses
//!   sub-β.γ's general-dihedral machinery — dihedral equals the
//!   profile's interior polygon angle at the shared vertex, e.g.
//!   90° for square, 60° for equilateral triangle, 108° for regular
//!   pentagon).
//!
//! All `3 * N` edges accept under the sub-β.γ general-dihedral
//! machinery. The vertical-seam case is geometrically SIMPLER than
//! sub-δ.revisit Loft's vertical-seam — Extrude sides are planar
//! (`profile_b == profile_a` positionally → no triangle-incidence
//! ambiguity), and the edge tangent is purely vertical (not
//! diagonal-in-3D as Loft's can be).
//!
//! Rejection cases (genuinely degenerate, defensive):
//!
//! * Zero-length profile edge (`Polygon2D::new` already rejects
//!   coincident adjacent points; unreachable for valid Extrude).
//! * Zero-magnitude side outward normal (same Polygon2D rejection).
//!
//! Near-degenerate dihedrals (face_a_inward ≈ ±face_b_inward) are
//! NOT pre-empted here; sub-β.γ's evaluate-time `DIHEDRAL_EPSILON_SQ`
//! guard catches them via [`OpError::InvalidParameter`]. In practice
//! no convex polygon has interior angles outside `(0°, 180°)`
//! exclusive, and `ExtrudeOp::evaluate` enforces strict convexity,
//! so the guard never fires for valid Extrude inputs.
//!
//! # Substrate posture
//!
//! `RoundFilletOp` (struct, evaluate body — sub-β.γ general-dihedral
//! at `cae3a84`, error enum, spec, trait) stays **byte-identical to
//! sub-δ.revisit `cf64962`**. This module's sub-β.γ-extend dispatch
//! lifts the previous vertical-seam rejection to acceptance using
//! the same orient_inward / solve_inward_directions pattern proven
//! by sub-δ.revisit Loft. The orient_inward / solve_inward_directions
//! helpers + 4 vector primitives are duplicated here from
//! `round_fillet/loft.rs` per ADR-119 D5 (substrate parallelism, not
//! sharing) — same byte-identical formulas, intentional duplication
//! so any future Extrude-specific or Loft-specific evolution stays
//! unilateral.
//!
//! Chamfer's `fillet::extrude::FilletOp::new_for_extrude` (D6 byte-
//! identical) is parallel substrate, not shared.

use super::{
    RoundFilletError, RoundFilletOp, RoundFilletSpec, RoundFilletSpecKind, RoundFilletUpstream,
};
use crate::operators::ExtrudeOp;
use crate::tessellation::TopologyFaceId;
use crate::topology::{BRepEdgeId, BRepOwnerId};

impl RoundFilletUpstream for ExtrudeOp {
    fn resolve_round_spec(
        &self,
        canonical_index: usize,
    ) -> Result<RoundFilletSpecKind, &'static str> {
        let n = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        let n_usize = n as usize;

        // Three edge-class dispatch over BRepEdgeProvider's canonical
        // emission order:
        //   [0..N)   bottom-perimeter — Bottom ∩ Side(i)
        //   [N..2N)  top-perimeter    — Top ∩ Side(i)
        //   [2N..3N) vertical-seam    — Side(i) ∩ Side((i+1)%N)  — REJECTED
        if canonical_index < n_usize {
            // Bottom-perimeter edge i.
            let i = canonical_index;
            let (vertex_a, vertex_b) = extrude_bottom_perimeter_vertex_pair(i, n);
            let side_normal = extrude_side_outward_normal(i, self);
            // face_a = Bottom (TopologyFaceId(0), normal -Z).
            // face_b = Side(i) (TopologyFaceId(2 + i), normal in XY).
            //
            // face_a_inward: in Bottom's plane (z=0), perpendicular to
            // edge (which runs along the profile edge in XY),
            // pointing INTO Bottom's interior (= away from Side(i)).
            // Cap × side dihedrals are perpendicular so this is just
            // `-side_normal` projected to XY (which equals
            // `-side_normal` itself since side_normal has no Z).
            //
            // face_b_inward: in Side(i)'s plane, perpendicular to edge,
            // pointing INTO Side(i)'s interior (= toward Top from
            // Bottom). Side's plane is z-extruded so the
            // perpendicular-to-edge direction in-plane is ±Z. From
            // Bottom edge, "into Side" means going UP toward Top:
            // face_b_inward = (0, 0, 1).
            let face_a_inward = [-side_normal[0], -side_normal[1], 0.0];
            let face_b_inward = [0.0, 0.0, 1.0];
            Ok(RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(0),
                face_b_id: TopologyFaceId(2 + i as u64),
                face_a_inward,
                face_b_inward,
            }))
        } else if canonical_index < 2 * n_usize {
            // Top-perimeter edge i (local index = canonical_index - N).
            let local = canonical_index - n_usize;
            let (vertex_a, vertex_b) = extrude_top_perimeter_vertex_pair(local, n);
            let side_normal = extrude_side_outward_normal(local, self);
            // face_a = Top (TopologyFaceId(1), normal +Z).
            // face_b = Side(local).
            //
            // face_a_inward: in Top's plane, away from Side =
            // -side_normal (no Z component).
            // face_b_inward: in Side(i)'s plane, perpendicular to
            // edge, pointing INTO Side(i) interior (= DOWN toward
            // Bottom from Top): (0, 0, -1).
            let face_a_inward = [-side_normal[0], -side_normal[1], 0.0];
            let face_b_inward = [0.0, 0.0, -1.0];
            Ok(RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(1),
                face_b_id: TopologyFaceId(2 + local as u64),
                face_a_inward,
                face_b_inward,
            }))
        } else if canonical_index < 3 * n_usize {
            // Vertical-seam edge (sub-β.γ-extend lift).
            //
            // local index `i ∈ [0, N)`: this edge sits at the shared
            // profile-vertex `(i+1)%N`, between Side(i) and
            // Side((i+1)%N).
            //
            // vertex_a = bot_{(i+1)%N} = (i+1)%N
            //   (positions[0..N] is the bottom ring at z=0).
            // vertex_b = top_{(i+1)%N} = N + (i+1)%N
            //   (positions[N..2N] is the top ring at z=length).
            // Edge tangent `e_t = (0, 0, 1)` exactly — since Extrude
            // has profile_b == profile_a positionally, bot_{(i+1)%N}
            // and top_{(i+1)%N} have IDENTICAL XY coordinates and
            // differ only in Z. No diagonal-in-3D case (cleaner
            // than sub-δ.revisit Loft, where vertical-seam tangent
            // generally has XY drift).
            //
            // face_a = Side(local) (TopologyFaceId(2 + local));
            //         outward normal n_face_a = extrude_side_outward_normal(local).
            // face_b = Side((local+1)%N) (TopologyFaceId(2 + (local+1)%N));
            //         outward normal n_face_b = extrude_side_outward_normal((local+1)%N).
            //
            // Dihedral between Side(local) and Side((local+1)%N) =
            // interior polygon angle at vertex (local+1)%N:
            //   - Square: 90° (regression case — same as sub-α/β
            //     90° plateau; cylinder math reduces to sub-α
            //     byte-for-byte at φ=90°).
            //   - Equilateral triangle: 60° (acute).
            //   - Regular pentagon: 108° (obtuse).
            //   - General convex polygon: any φ ∈ (0°, 180°)
            //     exclusive (ExtrudeOp::evaluate's convexity gate
            //     guarantees this).
            //
            // The orient_inward sign-check resolves face orientation
            // robustly (same algorithm as sub-δ.revisit Loft). All
            // face-strip substitution semantics from sub-β.γ remain
            // byte-identical — only the inset POSITIONS shift per
            // the dihedral angle.
            let local = canonical_index - 2 * n_usize;
            let (vertex_a, vertex_b) = extrude_vertical_seam_vertex_pair(local, n);
            let n_face_a = extrude_side_outward_normal(local, self);
            let n_face_b = extrude_side_outward_normal((local + 1) % n_usize, self);
            // Defensive: extrude_side_outward_normal returns the zero
            // vector for degenerate (zero-length) profile edges.
            // Polygon2D::new rejects coincident adjacent points so
            // this is unreachable for valid Extrude inputs.
            let n_face_a_mag_sq =
                n_face_a[0] * n_face_a[0] + n_face_a[1] * n_face_a[1] + n_face_a[2] * n_face_a[2];
            let n_face_b_mag_sq =
                n_face_b[0] * n_face_b[0] + n_face_b[1] * n_face_b[1] + n_face_b[2] * n_face_b[2];
            if n_face_a_mag_sq < 1e-12 || n_face_b_mag_sq < 1e-12 {
                return Err("extrude side face zero-magnitude outward normal (degenerate; profile edge zero-length)");
            }
            let edge_tangent = [0.0, 0.0, 1.0];
            let (face_a_inward, face_b_inward) =
                solve_inward_directions(edge_tangent, n_face_a, n_face_b);
            Ok(RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(2 + local as u64),
                face_b_id: TopologyFaceId(2 + ((local + 1) % n_usize) as u64),
                face_a_inward,
                face_b_inward,
            }))
        } else {
            // Defensive: from_upstream's caller-side filter already
            // restricts canonical_index to the upstream's
            // brep_edge_ids length (exactly 3N for any ExtrudeOp).
            // Unreachable in production paths.
            Err("extrude canonical edge index out of range (must be < 3N)")
        }
    }
}

impl RoundFilletOp {
    /// Construct a [`RoundFilletOp`] validated against the upstream
    /// `ExtrudeOp`, with edge selection restricted to cap-perimeter
    /// edges (bottom-perimeter + top-perimeter; `2 * N` edges total).
    ///
    /// Mirrors [`RoundFilletOp::new`] (Cuboid) but resolves edges
    /// against `upstream.brep_edge_ids(owner)` (the
    /// [`crate::topology::BRepEdgeProvider`] impl on
    /// [`crate::operators::ExtrudeOp`], emitting `3 * N` edges in the
    /// canonical order `[Bottom-perimeter | Top-perimeter |
    /// Vertical-seams]`). Vertical-seam edges are intentionally
    /// rejected via [`RoundFilletError::UnsupportedEdgeGeometry`] per
    /// sub-β scope (ADR-119 D7 — sub-β.γ generalizes to arbitrary
    /// dihedrals).
    ///
    /// # Errors
    ///
    /// * [`RoundFilletError::InvalidRadius`] if `radius` is non-finite
    ///   or `<= 0`.
    /// * [`RoundFilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`RoundFilletError::EdgeNotInUpstream`] if any edge ID does
    ///   not appear in `upstream.brep_edge_ids(owner)`.
    /// * [`RoundFilletError::UnsupportedEdgeGeometry`] if any edge ID
    ///   corresponds to a vertical-seam edge (canonical index `>=
    ///   2N`). Sub-β.γ lifts this restriction.
    pub fn new_for_extrude(
        upstream: &ExtrudeOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, RoundFilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Extrude helpers — derived from extrude.rs::evaluate's 2N-vertex layout.
//
// Per ADR-119 D5 these are duplicated from `fillet::extrude` (chamfer)
// rather than shared. The byte-identical formulas for vertex-pair
// mapping and side outward-normal are intentional; future-evolution
// divergence (a hypothetical Extrude winding change affecting one
// operator but not the other) MUST be expressible without rippling.
// ---------------------------------------------------------------------------

/// Map a bottom-perimeter local index `i ∈ [0, N)` to the
/// `(vertex_a, vertex_b)` pair in the upstream Extrude's vertex array.
///
/// `ExtrudeOp` bottom ring occupies `positions[0..N]` in
/// (CCW-corrected) profile order; bottom-perimeter edge `i` connects
/// `bottom_ring[i]` and `bottom_ring[(i + 1) % N]`.
fn extrude_bottom_perimeter_vertex_pair(i: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let i_u32 = i as u32;
    let next = ((i + 1) % n) as u32;
    (i_u32, next)
}

/// Map a top-perimeter local index `i ∈ [0, N)` to the
/// `(vertex_a, vertex_b)` pair. Top ring occupies `positions[N..2N]`.
fn extrude_top_perimeter_vertex_pair(i: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let i_u32 = i as u32;
    let next = ((i + 1) % n) as u32;
    (profile_count + i_u32, profile_count + next)
}

/// Outward normal of Extrude's side face `i`, in the XY plane.
///
/// `Side(i)` corresponds to the profile edge from `profile[i]` to
/// `profile[(i + 1) % N]`. For a CCW-wound profile (signed_area > 0),
/// the outward normal is obtained by rotating the edge vector
/// `(dx, dy)` by `-90°`, i.e. `(dy, -dx)`. Returns the zero vector
/// for a degenerate (zero-length) edge — `Polygon2D::new` rejects
/// these at construction so this is a defensive fallback.
///
/// **CCW-profile convention**: matches `extrude.rs`'s side-wall
/// outward-normal direction for canonical (CCW or CCW-corrected)
/// profile order. CW profiles surface the CW caveat documented in
/// `extrude.rs` — sub-β coverage uses CCW profiles only.
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

/// Map a vertical-seam local index `i ∈ [0, N)` to the
/// `(vertex_a, vertex_b)` pair in the upstream Extrude's vertex
/// array. Vertical-seam edge `i` is the seam between Side(i) and
/// Side((i+1)%N) at the shared profile-vertex `(i+1)%N`; it runs
/// from `bot_{(i+1)%N}` (positions[(i+1)%N], at z=0) to
/// `top_{(i+1)%N}` (positions[N + (i+1)%N], at z=length).
///
/// Mirrors the BRepEdgeProvider's vertical-seam vertex pairing
/// for `ExtrudeOp` + sub-δ.revisit Loft's
/// `loft_vertical_seam_vertex_pair` (parallel substrate per ADR-119
/// D5, NOT shared).
fn extrude_vertical_seam_vertex_pair(i: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let seam_vertex = ((i + 1) % n) as u32;
    (seam_vertex, profile_count + seam_vertex)
}

// ---------------------------------------------------------------------------
// Vector primitives + orientation-robust inward-direction algorithm
// (sub-β.γ-extend).
//
// Per ADR-119 D5 these are DUPLICATED from sub-δ.revisit Loft's
// `round_fillet/loft.rs` rather than shared. Byte-identical formulas;
// intentional duplication so any future Extrude-specific or
// Loft-specific evolution stays unilateral.
//
// Algorithm: For face A's outward normal `n_A`, face B's outward
// normal `n_B`, and edge tangent `e_t`:
//
//   candidate = normalize(cross(e_t, n_A))   // in face A's plane (⊥ n_A)
//   sign-check: dot(candidate, -n_B) — flip if negative
//   face_a_inward = oriented candidate
//
// Symmetric for face B. Works for any non-degenerate dihedral
// because `candidate ⊥ n_A` makes the `n_A`-component of `n_B`
// project away from the dot test (algebraically: `dot(candidate,
// -n_B) = dot(candidate, proj_{plane_A}(-n_B))`).
//
// For Extrude vertical-seam at a square profile vertex (90°
// interior angle):
//   n_A = (0, -1, 0)   Side(0) outward normal (south wall)
//   n_B = (1, 0, 0)    Side(1) outward normal (east wall)
//   e_t = (0, 0, 1)    vertical edge
//   candidate_a = cross((0,0,1), (0,-1,0)) = (1, 0, 0)
//   dot(candidate_a, -n_B) = dot((1,0,0), (-1,0,0)) = -1 < 0 → flip
//   face_a_inward = (-1, 0, 0)  ✓ in Side(0) plane, perpendicular
//                                to edge, points toward profile
//                                interior (away from Side(1)).
//   a · b = 0 → 90° dihedral matches square's interior angle.
// ---------------------------------------------------------------------------

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn neg3(v: [f32; 3]) -> [f32; 3] {
    [-v[0], -v[1], -v[2]]
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let mag = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if mag < 1e-9 {
        // Defensive: callers ensure non-degenerate input. The
        // sub-β.γ evaluate-time guard catches degenerate dihedrals
        // downstream if a zero vector slips through here.
        return [0.0, 0.0, 0.0];
    }
    let inv = 1.0 / mag;
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

fn orient_inward(edge_tangent: [f32; 3], n_own_face: [f32; 3], n_other_face: [f32; 3]) -> [f32; 3] {
    let candidate = normalize3(cross3(edge_tangent, n_own_face));
    let sign_check = dot3(candidate, neg3(n_other_face));
    if sign_check < 0.0 {
        neg3(candidate)
    } else {
        candidate
    }
}

fn solve_inward_directions(
    edge_tangent: [f32; 3],
    n_face_a: [f32; 3],
    n_face_b: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    let a = orient_inward(edge_tangent, n_face_a, n_face_b);
    let b = orient_inward(edge_tangent, n_face_b, n_face_a);
    (a, b)
}

// ---------------------------------------------------------------------------
// Sub-β unit tests — Extrude constructor + cap-perimeter acceptance
// + vertical-seam rejection + profile-size scaling.
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

    // -- Construction reject paths (mirrors chamfer + sub-α discipline) -----

    #[test]
    fn new_for_extrude_rejects_zero_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.0).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_for_extrude_rejects_negative_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], -1.0).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == -1.0));
    }

    #[test]
    fn new_for_extrude_rejects_non_finite_radius() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let err_nan =
            RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, RoundFilletError::InvalidRadius { .. }));
        let err_inf = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], f32::INFINITY)
            .unwrap_err();
        assert!(matches!(err_inf, RoundFilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_for_extrude_rejects_empty_edge_list() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let err = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, RoundFilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_for_extrude_rejects_unknown_edge_id() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err =
            RoundFilletOp::new_for_extrude(&extrude, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, RoundFilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    // -- Sub-β.γ-extend: vertical-seam acceptance (boundary flip) -----------
    //
    // Pre-sub-β.γ-extend (sub-β landing): vertical-seam edges
    // REJECTED with `UnsupportedEdgeGeometry`. Two tests pinned that
    // rejection. Sub-β.γ-extend LIFTS the rejection — the
    // general-dihedral cylinder math landed at sub-β.γ + Loft sub-
    // δ.revisit proved the orient_inward / solve_inward_directions
    // pattern works for non-90° accept-path edges, so the seam
    // edges now resolve to specs honoring the polygon's interior
    // angle at each shared vertex. The two pre-sub-β.γ-extend
    // rejection tests are FLIPPED to acceptance tests below
    // (deliberate boundary-shift bookkeeping — the rejection IS
    // what sub-β.γ-extend lifts).

    /// All 4 vertical-seam edges of a square Extrude (canonical
    /// indices 8..12) ACCEPT under sub-β.γ-extend. Each yields a
    /// 90° interior dihedral matching the square's polygon angles.
    /// Replaces the pre-sub-β.γ-extend
    /// `new_for_extrude_rejects_vertical_seam_edges_with_unsupported_edge_geometry`
    /// test (which asserted the inverse — the rejection that's now
    /// lifted).
    #[test]
    fn new_for_extrude_accepts_vertical_seam_edges_after_general_dihedral_lift() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        assert_eq!(all_edges.len(), 12, "square N=4 → 3*4=12 edges");

        // For a square profile (N=4), vertical-seam edges are
        // canonical indices 8, 9, 10, 11. Each must accept under
        // sub-β.γ-extend's general-dihedral path.
        for vs_idx in 8..12 {
            let edge = all_edges[vs_idx];
            let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.1)
                .expect("vertical-seam edge accepts under sub-β.γ-extend lift");
            assert_eq!(op.edges(), &[edge]);
        }
    }

    /// Mixed selection (1 cap-perimeter + 1 vertical-seam) now
    /// ACCEPTS — both edge classes are supported under sub-β.γ-extend.
    /// Replaces the pre-sub-β.γ-extend
    /// `new_for_extrude_rejects_mixed_selection_with_vertical_seam`
    /// test.
    #[test]
    fn new_for_extrude_accepts_mixed_selection_with_vertical_seam() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        let op = RoundFilletOp::new_for_extrude(
            &extrude,
            owner(),
            vec![all_edges[0], all_edges[8]], // bottom-perimeter + vertical-seam
            0.1,
        )
        .expect("mixed cap-perimeter + vertical-seam selection accepts");
        assert_eq!(op.edges().len(), 2);
    }

    // -- Cap-perimeter acceptance --------------------------------------------

    #[test]
    fn new_for_extrude_accepts_single_bottom_perimeter_edge() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edges = extrude.brep_edge_ids(owner());
        // Bottom-perimeter edges occupy indices 0..N=4. Edge[0] is
        // Bottom ∩ Side(0).
        let op =
            RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edges[0]], 0.1).expect("ok");
        assert_eq!(op.edges(), &[edges[0]]);
        assert!((op.radius() - 0.1).abs() < f32::EPSILON);
        assert_eq!(op.owner(), owner());
    }

    #[test]
    fn new_for_extrude_accepts_single_top_perimeter_edge() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edges = extrude.brep_edge_ids(owner());
        // Top-perimeter edges occupy indices N..2N = 4..8. Edge[4] is
        // Top ∩ Side(0).
        let op =
            RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edges[4]], 0.1).expect("ok");
        assert_eq!(op.edges(), &[edges[4]]);
    }

    #[test]
    fn new_for_extrude_accepts_all_cap_perimeter_edges() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edges = extrude.brep_edge_ids(owner());
        // All 8 cap-perimeter edges (indices 0..8).
        let cap_edges: Vec<_> = edges[..8].to_vec();
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), cap_edges.clone(), 0.05)
            .expect("8 cap-perimeter");
        assert_eq!(op.edges().len(), 8);
        assert_eq!(op.edges(), &cap_edges[..]);
    }

    // -- Evaluation geometry --------------------------------------------------

    /// Bottom-perimeter cap × side fillet on a unit-square extrude:
    /// upstream = 8 verts / 12 tris / 36 indices; per-edge addition =
    /// 4 inset + 2*(N+1)=18 cylinder = 22 verts, 2*N=16 cylinder tris;
    /// upstream-triangle indices substituted (not added). Total:
    /// 8 + 22 = 30 verts; 12 + 16 = 28 tris; 36 + 48 = 84 indices.
    #[test]
    fn evaluate_one_bottom_perimeter_edge_produces_expected_counts() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.1).expect("ok");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), 30, "8 upstream + 22 per-edge");
        assert_eq!(out.triangle_count(), 28, "12 upstream + 16 per-edge");
        assert_eq!(out.indices.len(), 84);
    }

    /// Top-perimeter cap × side fillet: same per-edge math as bottom-
    /// perimeter (mirror across z=length/2 plane).
    #[test]
    fn evaluate_one_top_perimeter_edge_produces_expected_counts() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        // Edge[4] is the first top-perimeter edge.
        let edge = extrude.brep_edge_ids(owner())[4];
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.1).expect("ok");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), 30);
        assert_eq!(out.triangle_count(), 28);
        assert_eq!(out.indices.len(), 84);
    }

    /// Output preserves labeled-ness from the upstream + cylinder
    /// triangles get `TopologyFaceId::DEGENERATE` (sub-α + ADR-119 D3).
    /// The Bottom cap fan triangles' labels stay `TopologyFaceId(0)`
    /// after vertex-substitution; the Side(0) wall triangles' labels
    /// stay `TopologyFaceId(2)`; the new 16 cylinder triangles all
    /// emit `DEGENERATE`.
    #[test]
    fn evaluate_preserves_labels_with_degenerate_caps_on_extrude() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let edge = extrude.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.1).expect("ok");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert!(out.is_labeled(), "ExtrudeOp upstream is labeled");
        let labels = out.face_labels.as_ref().expect("labeled");
        assert_eq!(labels.len(), 28, "12 upstream + 16 cylinder");

        // First 12 entries are upstream-face labels (vertex indices
        // changed inside the triangles, but the face IDs themselves
        // are unchanged). The trailing 16 are cylinder DEGENERATE.
        for (i, label) in labels.iter().enumerate().skip(12) {
            assert_eq!(
                *label,
                TopologyFaceId::DEGENERATE,
                "cylinder triangle {i} must be DEGENERATE"
            );
        }
    }

    // -- Profile-size scaling -------------------------------------------------

    /// User guardrail: "Tests should prove ... profile-size scaling."
    /// Same one-cap-perimeter-edge fillet on three profile sizes
    /// (triangle / square / pentagon) — per-edge geometry contribution
    /// is constant (22 verts / 16 tris) regardless of profile shape;
    /// upstream baseline grows linearly with N. Proves that the
    /// RoundFilletUpstream impl is profile-size-agnostic for cap-
    /// perimeter edges.
    #[test]
    fn evaluate_pentagon_profile_scales_with_profile_size() {
        let triangle = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle");

        // (profile, expected upstream vert count = 2N, expected
        // upstream tri count = 4N - 4)
        let cases: Vec<(Polygon2D, usize, usize)> = vec![
            (triangle, 6, 8),           // N=3
            (unit_square(), 8, 12),     // N=4
            (small_pentagon(), 10, 16), // N=5
        ];

        for (profile, upstream_verts, upstream_tris) in cases {
            let extrude = ExtrudeOp::new(profile.clone(), 1.0).expect("ext");
            let upstream = extrude.evaluate(&[]).expect("ext tess");
            assert_eq!(upstream.vertex_count(), upstream_verts);
            assert_eq!(upstream.triangle_count(), upstream_tris);

            let edge = extrude.brep_edge_ids(owner())[0];
            let op =
                RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.05).expect("ok");
            let out = op.evaluate(&[&upstream]).expect("evaluate");

            // +22 verts and +16 tris regardless of profile size.
            assert_eq!(
                out.vertex_count(),
                upstream_verts + 22,
                "profile N={} should add 22 verts per cap-perimeter edge",
                profile.len()
            );
            assert_eq!(
                out.triangle_count(),
                upstream_tris + 16,
                "profile N={} should add 16 tris per cap-perimeter edge",
                profile.len()
            );
        }
    }

    /// Profile-size scaling at the resolver level: number of cap-
    /// perimeter edges = `2 * N`. Verify the supported-edge band
    /// boundary at N=3, 4, 5 by accepting all `2N` cap-perimeter edges
    /// for each profile.
    #[test]
    fn new_for_extrude_accepts_all_cap_perimeter_edges_across_profile_sizes() {
        let triangle = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle");

        for (profile, n) in [
            (triangle, 3usize),
            (unit_square(), 4),
            (small_pentagon(), 5),
        ] {
            let extrude = ExtrudeOp::new(profile, 1.0).expect("ext");
            let all_edges = extrude.brep_edge_ids(owner());
            assert_eq!(all_edges.len(), 3 * n);
            // First 2N edges are cap-perimeter (bottom + top).
            let cap_edges: Vec<_> = all_edges[..2 * n].to_vec();
            let op = RoundFilletOp::new_for_extrude(&extrude, owner(), cap_edges, 0.05)
                .expect("all cap-perimeter");
            assert_eq!(op.edges().len(), 2 * n);
        }
    }

    // -- Helper-table correctness --------------------------------------------

    /// `extrude_bottom_perimeter_vertex_pair` + `_top_perimeter_*` match
    /// the canonical vertex layout of `extrude.rs::evaluate`. Verifies
    /// the duplicate-but-parallel substrate per ADR-119 D5 stays in
    /// sync with the upstream tessellation positionally.
    #[test]
    fn extrude_vertex_pair_helpers_match_extrude_evaluate_layout() {
        // Square (N=4).
        // Bottom: edges 0..4 connect (0,1), (1,2), (2,3), (3,0).
        assert_eq!(extrude_bottom_perimeter_vertex_pair(0, 4), (0, 1));
        assert_eq!(extrude_bottom_perimeter_vertex_pair(1, 4), (1, 2));
        assert_eq!(extrude_bottom_perimeter_vertex_pair(2, 4), (2, 3));
        assert_eq!(extrude_bottom_perimeter_vertex_pair(3, 4), (3, 0));
        // Top: edges 0..4 (local) connect (4,5), (5,6), (6,7), (7,4).
        assert_eq!(extrude_top_perimeter_vertex_pair(0, 4), (4, 5));
        assert_eq!(extrude_top_perimeter_vertex_pair(1, 4), (5, 6));
        assert_eq!(extrude_top_perimeter_vertex_pair(2, 4), (6, 7));
        assert_eq!(extrude_top_perimeter_vertex_pair(3, 4), (7, 4));

        // Triangle (N=3).
        assert_eq!(extrude_bottom_perimeter_vertex_pair(0, 3), (0, 1));
        assert_eq!(extrude_top_perimeter_vertex_pair(2, 3), (5, 3));

        // Pentagon (N=5).
        assert_eq!(extrude_bottom_perimeter_vertex_pair(4, 5), (4, 0));
        assert_eq!(extrude_top_perimeter_vertex_pair(0, 5), (5, 6));
    }

    /// `extrude_side_outward_normal` for the unit square: Side(0)
    /// covers profile edge (0,0)→(1,0), runs along +X; outward normal
    /// is -Y = (0, -1, 0). Side(1) covers (1,0)→(1,1), outward = +X =
    /// (1, 0, 0). Pins the CCW-convention math.
    #[test]
    fn extrude_side_outward_normal_unit_square_directions() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let n0 = extrude_side_outward_normal(0, &extrude);
        let n1 = extrude_side_outward_normal(1, &extrude);
        let n2 = extrude_side_outward_normal(2, &extrude);
        let n3 = extrude_side_outward_normal(3, &extrude);

        let close = |a: [f32; 3], b: [f32; 3]| -> bool {
            (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6 && (a[2] - b[2]).abs() < 1e-6
        };
        assert!(
            close(n0, [0.0, -1.0, 0.0]),
            "Side(0) outward = -Y, got {n0:?}"
        );
        assert!(
            close(n1, [1.0, 0.0, 0.0]),
            "Side(1) outward = +X, got {n1:?}"
        );
        assert!(
            close(n2, [0.0, 1.0, 0.0]),
            "Side(2) outward = +Y, got {n2:?}"
        );
        assert!(
            close(n3, [-1.0, 0.0, 0.0]),
            "Side(3) outward = -X, got {n3:?}"
        );
    }

    /// Resolver returns spec with the right face IDs for cap-perimeter
    /// edges. Bottom-perimeter edge i → face_a_id = TopologyFaceId(0)
    /// (Bottom), face_b_id = TopologyFaceId(2 + i) (Side(i)).
    /// Top-perimeter edge (local i) → face_a_id = TopologyFaceId(1)
    /// (Top), face_b_id = TopologyFaceId(2 + i).
    #[test]
    fn resolve_round_spec_face_ids_match_canonical_emission_order() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");

        // Bottom-perimeter edge 0 → Bottom ∩ Side(0).
        let spec = extrude.resolve_round_spec(0).expect("bottom-perimeter 0");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(0));
        assert_eq!(spec.face_b_id, TopologyFaceId(2));

        // Bottom-perimeter edge 2 → Bottom ∩ Side(2).
        let spec = extrude.resolve_round_spec(2).expect("bottom-perimeter 2");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(0));
        assert_eq!(spec.face_b_id, TopologyFaceId(4));

        // Top-perimeter edge (local 0, canonical 4) → Top ∩ Side(0).
        let spec = extrude.resolve_round_spec(4).expect("top-perimeter 0");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(1));
        assert_eq!(spec.face_b_id, TopologyFaceId(2));

        // Top-perimeter edge (local 3, canonical 7) → Top ∩ Side(3).
        let spec = extrude.resolve_round_spec(7).expect("top-perimeter 3");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(1));
        assert_eq!(spec.face_b_id, TopologyFaceId(5));

        // Vertical-seam canonical 8 (local 0) — accepts under
        // sub-β.γ-extend; returns spec for Side(0) ∩ Side(1).
        // (Pre-sub-β.γ-extend this branch asserted Err containing
        // "vertical-seam"; the rejection is lifted, so we now
        // assert the acceptance path's face-ID mapping. The
        // dedicated `resolve_round_spec_vertical_seam_face_ids_*`
        // test below exercises the full vertical-seam face-ID
        // coverage; this trailing assertion stays in-place for
        // cross-class continuity of the canonical emission order
        // check.)
        let spec = extrude
            .resolve_round_spec(8)
            .expect("vertical-seam 0 accepts post-sub-β.γ-extend");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(2)); // Side(0)
        assert_eq!(spec.face_b_id, TopologyFaceId(3)); // Side(1)
    }

    /// Resolver returns unit-length, perpendicular inward vectors for
    /// cap-perimeter edges across multiple profile sizes — confirms
    /// the 90° dihedral invariant the sub-α `RoundFilletOp::evaluate`
    /// body assumes geometrically.
    #[test]
    fn resolve_round_spec_inward_vectors_unit_and_perpendicular_for_cap_perimeter() {
        for profile in [unit_square(), small_pentagon()] {
            let extrude = ExtrudeOp::new(profile.clone(), 1.0).expect("ext");
            let n = profile.len();
            // Cap-perimeter canonical indices: 0..2N.
            for idx in 0..2 * n {
                let spec = extrude
                    .resolve_round_spec(idx)
                    .expect("cap-perimeter always resolves");
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
                    (len_a - 1.0).abs() < 1e-6,
                    "face_a_inward at idx {idx} (N={n}) not unit (len={len_a})"
                );
                assert!(
                    (len_b - 1.0).abs() < 1e-6,
                    "face_b_inward at idx {idx} (N={n}) not unit (len={len_b})"
                );

                let dot = spec.face_a_inward[0] * spec.face_b_inward[0]
                    + spec.face_a_inward[1] * spec.face_b_inward[1]
                    + spec.face_a_inward[2] * spec.face_b_inward[2];
                assert!(
                    dot.abs() < 1e-6,
                    "inward vectors at cap-perimeter idx {idx} (N={n}) not perpendicular (dot={dot})"
                );
            }
        }
    }

    // -- Sub-β.γ-extend: vertical-seam coverage + non-90° dihedrals ---------
    //
    // After the sub-β.γ general-dihedral cylinder math landed at
    // sub-β.γ `cae3a84` and Loft sub-δ.revisit `cf64962` empirically
    // validated it on non-90° accept-path edges, Extrude vertical-seam
    // becomes a thin upstream lift over `[2N, 3N)`. The interior
    // dihedral at each vertical-seam equals the profile's interior
    // polygon angle at the shared vertex: 90° for square, 60° for
    // equilateral triangle, 108° for regular pentagon, etc.
    //
    // Per ADR-119 D7 chapter shape, sub-β.γ-extend "completes Extrude
    // coverage" — all `3 * N` Extrude edges now accept (matching
    // chamfer's permissive accept-all-3N scope, with the round-fillet
    // additional honesty of true triangle-plane normals rather than
    // chamfer's Extrude-style XY-only approximation).

    fn ccw_triangle() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("ccw triangle")
    }

    /// All `3 * N` edges accept for a square Extrude (N=4 → 12 edges
    /// total: 4 bottom-perimeter + 4 top-perimeter + 4 vertical-seam).
    /// Proves the full Extrude upstream is now closed at sub-β.γ-extend.
    #[test]
    fn new_for_extrude_accepts_all_3n_edges_for_square_profile() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        assert_eq!(all_edges.len(), 12);
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), all_edges.clone(), 0.05)
            .expect("full Extrude upstream closure at sub-β.γ-extend");
        assert_eq!(op.edges().len(), 12);
    }

    /// All `3 * N` edges accept for a pentagon Extrude (N=5 → 15
    /// edges total). Variation across profile shapes; pentagon
    /// vertical-seam edges have 108° (non-90°) interior dihedrals,
    /// exercising the general-dihedral path end-to-end.
    #[test]
    fn new_for_extrude_accepts_all_3n_edges_for_pentagon_profile() {
        let extrude = ExtrudeOp::new(small_pentagon(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        assert_eq!(all_edges.len(), 15, "pentagon N=5 → 3*5=15 edges");
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), all_edges.clone(), 0.05)
            .expect("pentagon Extrude full upstream closure");
        assert_eq!(op.edges().len(), 15);
    }

    /// Evaluate counts for a square vertical-seam edge fillet: same
    /// per-edge contribution as cap-perimeter edges (+22v / +16t /
    /// +48i — the contribution is dihedral-agnostic, only the
    /// inset POSITIONS shift per dihedral angle).
    #[test]
    fn evaluate_one_vertical_seam_edge_produces_expected_counts_on_square() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        let vs_edge = all_edges[8]; // first vertical-seam (canonical 2N+0)
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![vs_edge], 0.1)
            .expect("vertical-seam accept");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate vertical-seam");

        assert_eq!(out.vertex_count(), 30, "8 upstream + 22 per-edge");
        assert_eq!(out.triangle_count(), 28, "12 upstream + 16 per-edge");
        assert_eq!(out.indices.len(), 84);
    }

    /// Sub-ε cross-upstream proof: a cap-perimeter edge and an
    /// incident vertical seam share the bottom-ring corner on Side(0).
    /// The evaluator must add corner-blend geometry instead of leaving
    /// the pre-sub-ε order-dependent corner gap.
    #[test]
    fn evaluate_bottom_perimeter_plus_vertical_seam_adds_corner_patch() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        let op = RoundFilletOp::new_for_extrude(
            &extrude,
            owner(),
            vec![all_edges[0], all_edges[8]],
            0.1,
        )
        .expect("corner-sharing mixed selection accepts");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert!(
            out.vertex_count() > upstream.vertex_count() + 44,
            "corner blend should add vertices beyond two independent edge cylinders"
        );
        assert!(
            out.triangle_count() > upstream.triangle_count() + 32,
            "corner blend should add nameless patch triangles"
        );

        // Side(0)'s first triangle is (bot_0, bot_1, top_1); bot_1
        // is the shared corner. It should be substituted once to the
        // resolved face-corner inset.
        let side0_first_triangle = 2 * (unit_square().len() - 2);
        let shared_slot = side0_first_triangle * 3 + 1;
        assert_ne!(out.indices[shared_slot], upstream.indices[shared_slot]);

        let labels = out.face_labels.as_ref().expect("labeled");
        assert!(
            labels
                .iter()
                .skip(upstream.triangle_count() + 32)
                .all(|label| *label == TopologyFaceId::DEGENERATE),
            "corner patch triangles are nameless"
        );
    }

    /// Evaluate counts for a pentagon vertical-seam edge fillet
    /// (108° dihedral, NON-90°). Same per-edge contribution as the
    /// square case — the general-dihedral arc subtends `π − φ`
    /// radians (72° for pentagon vs 90° for square) but is
    /// subdivided by the same `ROUND_FILLET_SEGMENTS = 8`, yielding
    /// the same +22v/+16t/+48i count.
    #[test]
    fn evaluate_one_vertical_seam_edge_produces_expected_counts_on_pentagon() {
        let extrude = ExtrudeOp::new(small_pentagon(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        // Pentagon vertical-seams are canonical indices 10..15 (2N..3N for N=5).
        let vs_edge = all_edges[10];
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![vs_edge], 0.05)
            .expect("pentagon vertical-seam accept (108° dihedral)");
        let upstream = extrude.evaluate(&[]).expect("ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate vertical-seam");

        // Pentagon Extrude: 2N=10 verts, 4N-4=16 tris, 48 indices.
        // After 1 fillet: +22 verts, +16 tris, +48 indices.
        assert_eq!(out.vertex_count(), 32);
        assert_eq!(out.triangle_count(), 32);
        assert_eq!(out.indices.len(), 96);
    }

    /// `resolve_round_spec` for vertical-seam edges returns specs
    /// with the right face IDs: face_a = Side(i), face_b =
    /// Side((i+1)%N). Pins the canonical face emission order from
    /// `extrude.rs::evaluate` (Bottom=0, Top=1, Side(i)=2+i).
    #[test]
    fn resolve_round_spec_vertical_seam_face_ids_match_canonical_emission_order() {
        // Square (N=4): vertical-seam local indices 0..4 correspond
        // to canonical indices 8..12.
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");

        // Canonical 8 (local 0) → Side(0) ∩ Side(1).
        let spec = extrude.resolve_round_spec(8).expect("vertical-seam 0");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(2));
        assert_eq!(spec.face_b_id, TopologyFaceId(3));

        // Canonical 9 (local 1) → Side(1) ∩ Side(2).
        let spec = extrude.resolve_round_spec(9).expect("vertical-seam 1");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(3));
        assert_eq!(spec.face_b_id, TopologyFaceId(4));

        // Canonical 11 (local 3, wraps) → Side(3) ∩ Side(0).
        let spec = extrude.resolve_round_spec(11).expect("vertical-seam 3");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(5));
        assert_eq!(spec.face_b_id, TopologyFaceId(2)); // wraps to Side(0)
    }

    /// **Load-bearing dihedral verification — square Extrude vertical-
    /// seam yields 90° dihedral** (`a · b ≈ 0`). Pins the regression
    /// invariant that the general-dihedral path lands on the right
    /// answer at the 90° special case (matches sub-α/β/γ at the
    /// regression boundary).
    #[test]
    fn resolve_round_spec_square_vertical_seam_yields_90_degree_dihedral() {
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("ext");
        for canonical in 8..12 {
            let spec = extrude
                .resolve_round_spec(canonical)
                .expect("square vertical-seam accepts");
            let spec = spec.expect_two_endpoint();
            let dot_ab = spec.face_a_inward[0] * spec.face_b_inward[0]
                + spec.face_a_inward[1] * spec.face_b_inward[1]
                + spec.face_a_inward[2] * spec.face_b_inward[2];
            assert!(
                dot_ab.abs() < 1e-5,
                "square vertical-seam canonical {canonical}: expected 90° dihedral (a·b≈0), got a·b={dot_ab}"
            );
        }
    }

    /// **Load-bearing dihedral verification — pentagon Extrude
    /// vertical-seam yields 108° interior dihedral** (`a · b ≈
    /// cos(108°) ≈ -0.309`). Proves the general-dihedral path
    /// runs end-to-end on a real non-90° Extrude case, not just
    /// synthetic specs from sub-β.γ.
    #[test]
    fn resolve_round_spec_pentagon_vertical_seam_yields_108_degree_dihedral() {
        let extrude = ExtrudeOp::new(small_pentagon(), 1.0).expect("ext");
        // Regular pentagon interior angle = 108°; cos(108°) ≈ -0.309.
        let expected = (108.0_f32).to_radians().cos();
        for canonical in 10..15 {
            let spec = extrude
                .resolve_round_spec(canonical)
                .expect("pentagon vertical-seam accepts");
            let spec = spec.expect_two_endpoint();
            let dot_ab = spec.face_a_inward[0] * spec.face_b_inward[0]
                + spec.face_a_inward[1] * spec.face_b_inward[1]
                + spec.face_a_inward[2] * spec.face_b_inward[2];
            assert!(
                (dot_ab - expected).abs() < 1e-3,
                "pentagon vertical-seam canonical {canonical}: expected a·b≈{expected} (108° interior), got a·b={dot_ab}"
            );
        }
    }

    /// Inward vectors are unit-length for vertical-seam edges across
    /// multiple profile shapes (square / triangle / pentagon).
    /// Verifies the orient_inward / solve_inward_directions output
    /// is well-formed regardless of dihedral angle (90° square /
    /// 60° triangle / 108° pentagon).
    ///
    /// Unlike cap-perimeter (where inward vectors are perpendicular
    /// by construction), vertical-seam inward vectors are
    /// pairwise-perpendicular ONLY for 90° dihedrals (= rectangular
    /// profiles). For non-90° polygons they share the same dihedral
    /// angle the profile's interior angle at the shared vertex.
    #[test]
    fn resolve_round_spec_inward_vectors_unit_for_vertical_seam_across_profile_shapes() {
        for (label, profile) in [
            ("square", unit_square()),
            ("triangle", ccw_triangle()),
            ("pentagon", small_pentagon()),
        ] {
            let extrude = ExtrudeOp::new(profile.clone(), 1.0).expect("ext");
            let n = profile.len();
            // Vertical-seam canonical range: [2N, 3N).
            for idx in (2 * n)..(3 * n) {
                let spec = extrude
                    .resolve_round_spec(idx)
                    .expect("vertical-seam accept");
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
                    "{label} extrude vertical-seam idx {idx}: face_a_inward not unit (len={len_a})"
                );
                assert!(
                    (len_b - 1.0).abs() < 1e-5,
                    "{label} extrude vertical-seam idx {idx}: face_b_inward not unit (len={len_b})"
                );
            }
        }
    }

    /// **Face-strip localization no-leak watch test for Extrude
    /// vertical-seam** (sub-β.γ-extend's load-bearing invariant per
    /// the user-flagged halt condition "halt only if ... the
    /// face-strip no-leak watch fails"). Scans every upstream-
    /// triangle index slot post-substitution: target slots (label =
    /// face_a/face_b AND original index = vertex_a/vertex_b) MUST
    /// flip; all OTHER slots MUST be byte-identical.
    ///
    /// For pentagon Extrude vertical-seam 0:
    ///   - vertex_a = bot_1, vertex_b = top_1
    ///   - face_a = Side(0), face_b = Side(1)
    ///   - Side(0)'s 2*segments triangles: only the quad at the
    ///     seam ring contains vertex_a + vertex_b
    ///   - Side(1)'s 2*segments triangles: only the quad at the
    ///     seam ring contains vertex_a + vertex_b
    ///   - Adjacent Side(4) shares profile-vertex 0 (bot_0+top_0,
    ///     NOT the seam vertices) — unmodified
    ///   - Adjacent Side(2) shares profile-vertex 2 (bot_2+top_2)
    ///     — unmodified
    ///   - Bottom/Top caps share bot_1/top_1 in their fans —
    ///     unmodified (NOT in target face set)
    #[test]
    fn evaluate_face_strip_localization_no_leak_for_pentagon_extrude_vertical_seam() {
        let extrude = ExtrudeOp::new(small_pentagon(), 1.0).expect("ext");
        let all_edges = extrude.brep_edge_ids(owner());
        let n = 5usize;
        let edge = all_edges[2 * n]; // vertical-seam canonical 10 (local 0)
        let op = RoundFilletOp::new_for_extrude(&extrude, owner(), vec![edge], 0.05).expect("ok");
        let upstream = extrude.evaluate(&[]).expect("pentagon ext tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        let labels = upstream
            .face_labels
            .as_ref()
            .expect("Extrude always labels");
        let face_a_id = TopologyFaceId(2); // Side(0)
        let face_b_id = TopologyFaceId(3); // Side(1)
        let vertex_a = 1u32; // bot_1
        let vertex_b = (n + 1) as u32; // top_1 = N + 1 = 6

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
                    assert_ne!(
                        modified, original,
                        "target slot tri={tri_idx} pos={j} label={label:?} \
                         vertex={original} was NOT substituted"
                    );
                    substituted_target_slots += 1;
                } else if modified != original {
                    leaked_non_target_slots.push((idx_pos, *label, original, modified));
                }
            }
        }

        assert!(
            leaked_non_target_slots.is_empty(),
            "face-strip substitution LEAKED beyond target Side(0)+Side(1) faces — {} slots changed unrelated to spec; \
             samples: {:?}",
            leaked_non_target_slots.len(),
            leaked_non_target_slots.iter().take(5).collect::<Vec<_>>()
        );
        assert!(
            substituted_target_slots > 0,
            "no target slot got substituted — spec mismatch"
        );
    }
}
