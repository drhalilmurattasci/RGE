//! `LoftOp` — bridge two 2D convex polygon profiles at different `+Z` heights
//! to produce a closed solid (arity 0).
//!
//! Failure class: snapshot-recoverable
//!
//! # Geometry
//!
//! [`LoftOp`] consumes two [`Polygon2D`] profiles (in the XY plane) and a
//! positive, finite `length`, producing a closed solid whose bottom cross
//! section is `profile_a` at `z = 0` and whose top cross section is
//! `profile_b` at `z = length`. Both profiles must have the same point count
//! (`n_a == n_b == n`); the v0 vertex-pairing strategy assumes one-to-one
//! correspondence between bottom-ring and top-ring vertices.
//!
//! For `n` points per profile the produced mesh has `2 * n` vertices and
//! `4 * n - 4` triangles — structurally identical to [`crate::ExtrudeOp`]
//! (Loft is Extrude with a distinct top ring).
//!
//! * The bottom ring sits at `z = 0` (lifted `profile_a`).
//! * The top ring sits at `z = length` (lifted `profile_b`).
//! * End caps are fan-triangulated from vertex 0 of each ring.
//! * Side walls are quad strips between the two rings, each split into two
//!   triangles via the diagonal that runs from `bot_i` to `top_{i+1}`.
//!
//! # Conventions
//!
//! * **Right-handed CCW winding** when viewed from outside the solid.
//! * **Outward normals** — the bottom face normal points in `-Z`, the top in
//!   `+Z`, and side-wall normals point away from the polygon interior.
//! * **Profile winding is winding-agnostic from the caller's perspective**:
//!   the algorithm reads each profile's signed area independently and reverses
//!   iteration order internally for any CW input, so the produced solid
//!   always has correct outward normals regardless of which winding the
//!   caller passed for either profile.
//!
//! # Restrictions (Phase 7 D-Loft v0)
//!
//! * Both profiles must be **strictly convex** (validated at `evaluate` time
//!   via [`Polygon2D::convexity`]). Concave profiles are rejected with
//!   [`OpError::InvalidParameter`]. Same restriction as [`crate::ExtrudeOp`]
//!   and lifted by the same future earcut dispatch.
//! * Both profiles must have the **same point count**. Distinct cross-section
//!   point counts require a vertex-resampling strategy (e.g. uniform-arc
//!   reparameterization) that is out of v0 scope.
//! * Loft direction is fixed to `+Z`. Arbitrary-axis lofting is achieved by
//!   chaining a downstream [`crate::TransformOp`].
//! * No profile-to-profile rotation alignment beyond identity-pairing —
//!   `profile_a[i]` always pairs with `profile_b[i]`. Twist / start-vertex
//!   offset is achieved by rotating one of the input vertex orderings before
//!   construction.
//!
//! # Capability surface (per ADR-104)
//!
//! * `boolean_robust_under_tolerance`: true (no boolean op).
//! * `deterministic_triangulation`: true (fan from vertex 0; no
//!   float-comparison-dependent triangulation choice).
//! * `t_junction_handling`: true (closed solid has none).
//! * `concave_input_supported`: **false** — fan-triangulation produces
//!   inverted cap triangles on concave profiles; rejected at evaluate time.
//! * `arity`: 0 (both profiles are parameters, not upstream inputs).
//! * `output_labeled_when_input_labeled`: false (no inputs).

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator, Polygon2D};
use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topology::{
    BRepEdgeId, BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider, LoftFaceTag,
};

// ---------------------------------------------------------------------------
// LoftOp
// ---------------------------------------------------------------------------

/// Bridge two [`Polygon2D`] profiles at different `+Z` heights to produce a
/// closed solid.
///
/// `length` must be finite and strictly positive. Both profile and length
/// invariants are re-checked at [`LoftOp::evaluate`] time so that intermediate
/// graph states (where a parameter may be momentarily corrupted while being
/// edited) don't poison construction.
///
/// The two profiles must have the same point count; v0 pairs `profile_a[i]`
/// with `profile_b[i]` for every `i` to build the side wall. Distinct point
/// counts require a vertex-resampling strategy that is out of v0 scope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoftOp {
    /// Bottom-ring profile (placed at `z = 0`).
    pub profile_a: Polygon2D,
    /// Top-ring profile (placed at `z = length`).
    pub profile_b: Polygon2D,
    /// Sweep distance from `profile_a` to `profile_b` along `+Z`. Must be
    /// finite and `> 0.0`.
    pub length: f32,
}

impl LoftOp {
    /// Build a [`LoftOp`] after validating `length`.
    ///
    /// Profile invariants (point-count parity, finiteness, convexity, signed
    /// area) are checked at [`LoftOp::evaluate`] time — see [`Operator`].
    ///
    /// # Errors
    ///
    /// * [`OpError::InvalidParameter`] if `length` is not finite or not
    ///   strictly positive.
    pub fn new(profile_a: Polygon2D, profile_b: Polygon2D, length: f32) -> Result<Self, OpError> {
        if !length.is_finite() || length <= 0.0 {
            return Err(OpError::InvalidParameter(format!(
                "LoftOp.length must be finite and > 0 (got {length})"
            )));
        }
        Ok(Self {
            profile_a,
            profile_b,
            length,
        })
    }
}

impl Operator for LoftOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Loft
    }

    fn arity(&self) -> usize {
        0
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"loft:");
        hasher.update(&self.length.to_le_bytes());
        // try_from is infallible at any plausible profile size, but using it
        // satisfies clippy::cast_possible_truncation. Fall back to u32::MAX
        // for the unreachable >4G-point case (Tessellation::new would have
        // rejected long before).
        let n_a = u32::try_from(self.profile_a.len()).unwrap_or(u32::MAX);
        hasher.update(&n_a.to_le_bytes());
        for [x, y] in self.profile_a.points() {
            hasher.update(&x.to_le_bytes());
            hasher.update(&y.to_le_bytes());
        }
        let n_b = u32::try_from(self.profile_b.len()).unwrap_or(u32::MAX);
        hasher.update(&n_b.to_le_bytes());
        for [x, y] in self.profile_b.points() {
            hasher.update(&x.to_le_bytes());
            hasher.update(&y.to_le_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if !inputs.is_empty() {
            return Err(OpError::WrongArity {
                expected: 0,
                got: inputs.len(),
            });
        }

        // Re-validate length defensively (the field is `pub` and may have
        // been mutated post-construction).
        if !self.length.is_finite() || self.length <= 0.0 {
            return Err(OpError::InvalidParameter(format!(
                "loft length must be finite > 0 (got {})",
                self.length
            )));
        }

        // Per-profile defensive re-validation. profile_a.points / profile_b.points
        // are private fields on Polygon2D, but the profiles themselves are
        // pub fields on LoftOp — caller could swap in a fresh Polygon2D
        // (whose construction checks pass) but a future change might add
        // unchecked mutation hooks.
        if self.profile_a.len() < 3 {
            return Err(OpError::InvalidParameter(format!(
                "loft profile_a needs >= 3 points (got {})",
                self.profile_a.len()
            )));
        }
        if self.profile_b.len() < 3 {
            return Err(OpError::InvalidParameter(format!(
                "loft profile_b needs >= 3 points (got {})",
                self.profile_b.len()
            )));
        }

        // Point-count parity gate. v0 pairs profile_a[i] with profile_b[i];
        // distinct counts have no canonical pairing without a resampling
        // strategy that is out of v0 scope.
        if self.profile_a.len() != self.profile_b.len() {
            return Err(OpError::InvalidParameter(format!(
                "loft profiles must have the same point count (profile_a={}, profile_b={})",
                self.profile_a.len(),
                self.profile_b.len()
            )));
        }

        for (i, [x, y]) in self.profile_a.points().iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "loft profile_a has non-finite coordinate at index {i}"
                )));
            }
        }
        for (i, [x, y]) in self.profile_b.points().iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "loft profile_b has non-finite coordinate at index {i}"
                )));
            }
        }

        // Convexity gate, per profile.
        match self.profile_a.convexity() {
            Some(true) => {}
            Some(false) => {
                return Err(OpError::InvalidParameter(
                    "loft profile_a must be strictly convex".to_string(),
                ));
            }
            None => {
                return Err(OpError::InvalidParameter(
                    "loft profile_a is degenerate (all points collinear)".to_string(),
                ));
            }
        }
        match self.profile_b.convexity() {
            Some(true) => {}
            Some(false) => {
                return Err(OpError::InvalidParameter(
                    "loft profile_b must be strictly convex".to_string(),
                ));
            }
            None => {
                return Err(OpError::InvalidParameter(
                    "loft profile_b is degenerate (all points collinear)".to_string(),
                ));
            }
        }

        // Per-profile winding correction: signed_area > 0 → CCW (canonical);
        // signed_area < 0 → CW (reverse iteration order); near-zero → reject.
        // Epsilon comparison rather than exact == 0.0 to defend against tiny
        // float-drift in the shoelace sum that would otherwise sneak through.
        let sa_a = self.profile_a.signed_area();
        if sa_a.abs() < 1e-12_f32 {
            return Err(OpError::InvalidParameter(
                "loft profile_a is degenerate (near-zero area)".to_string(),
            ));
        }
        let sa_b = self.profile_b.signed_area();
        if sa_b.abs() < 1e-12_f32 {
            return Err(OpError::InvalidParameter(
                "loft profile_b is degenerate (near-zero area)".to_string(),
            ));
        }

        let n = self.profile_a.len();
        let ordered_a: Vec<[f32; 2]> = if sa_a > 0.0 {
            self.profile_a.points().to_vec()
        } else {
            self.profile_a.points().iter().rev().copied().collect()
        };
        let ordered_b: Vec<[f32; 2]> = if sa_b > 0.0 {
            self.profile_b.points().to_vec()
        } else {
            self.profile_b.points().iter().rev().copied().collect()
        };

        // Build vertex buffer: bottom ring at z=0, then top ring at z=length.
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(2 * n);
        for [x, y] in &ordered_a {
            positions.push([*x, *y, 0.0]);
        }
        for [x, y] in &ordered_b {
            positions.push([*x, *y, self.length]);
        }

        let n_u32 = u32::try_from(n).map_err(|_| {
            OpError::InvalidParameter(format!("loft profile too large: {n} points"))
        })?;

        // Index buffer:
        //   caps  : 2 * (n - 2) triangles
        //   sides : 2 * n triangles
        //   total : 4n - 4
        let mut indices: Vec<u32> = Vec::with_capacity(3 * (4 * n - 4));

        // Bottom cap — outward normal -Z. The ordered ring is CCW when
        // viewed from +Z (signed_area > 0). For a -Z-facing triangle we want
        // CCW winding when viewed from -Z, i.e. the indices listed in 3D are
        // (0, i+1, i) — the reverse of the projected CCW ordering.
        for i in 1..(n_u32 - 1) {
            indices.push(0);
            indices.push(i + 1);
            indices.push(i);
        }

        // Top cap — outward normal +Z. The ordered ring is CCW from +Z, so
        // (n, n+i, n+i+1) is CCW when viewed from +Z = correct outward
        // facing.
        for i in 1..(n_u32 - 1) {
            indices.push(n_u32);
            indices.push(n_u32 + i);
            indices.push(n_u32 + i + 1);
        }

        // Side walls. For each polygon edge (i, i+1), generate the quad
        // (bottom_i, bottom_{i+1}, top_{i+1}, top_i). With CCW polygon
        // ordering on both rings the outward normal of each side face points
        // away from the bottom polygon's interior — consistent with the
        // identity-pairing v0 strategy.
        for i in 0..n_u32 {
            let i1 = (i + 1) % n_u32;
            let bot_i = i;
            let bot_i1 = i1;
            let top_i = n_u32 + i;
            let top_i1 = n_u32 + i1;

            // Triangle 1: (bot_i, bot_{i+1}, top_{i+1})
            indices.push(bot_i);
            indices.push(bot_i1);
            indices.push(top_i1);
            // Triangle 2: (bot_i, top_{i+1}, top_i)
            indices.push(bot_i);
            indices.push(top_i1);
            indices.push(top_i);
        }

        Tessellation::new(positions, indices).map_err(|e| {
            OpError::InvalidParameter(format!("LoftOp produced invalid tessellation: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// BRepProvider — sub-7.2-δ B-Rep face identity for LoftOp
// ---------------------------------------------------------------------------

/// Pair the `N + 2` sequential per-tessellation `TopologyFaceId`s with
/// rebuild-stable `BRepFaceId`s seeded from the caller-supplied
/// [`BRepOwnerId`].
///
/// `LoftOp` is the first operator with **two profile inputs**. v0 pairs
/// `profile_a[i]` with `profile_b[i]` for every `i` and emits faces in the
/// canonical order `Bottom cap → Top cap → Side(0..N-1)`, structurally
/// mirroring `ExtrudeOp::evaluate`:
///
/// * `TopologyFaceId(0)` → [`LoftFaceTag::Bottom`] (`-Z` cap, `profile_a`
///   lifted to `z = 0`)
/// * `TopologyFaceId(1)` → [`LoftFaceTag::Top`] (`+Z` cap, `profile_b`
///   lifted to `z = length`)
/// * `TopologyFaceId(2 + i)` → [`LoftFaceTag::Side`] for `i in 0..N`
///
/// Each `Side` carries BOTH `profile_a_count` AND `profile_b_count`
/// independently per the substrate-honesty guardrail — even though
/// [`LoftOp::evaluate`] enforces equal counts at runtime, the tag does
/// not depend on that validation rule. See [`LoftFaceTag`] for the full
/// stability contract.
impl BRepProvider for LoftOp {
    fn brep_face_ids(&self, owner: BRepOwnerId) -> Vec<(TopologyFaceId, BRepFaceId)> {
        // Mirrors the `n_u32` cast pattern in `evaluate` above (and the
        // `extrude.rs::structural_hash` precedent at L274). saturating to
        // `u32::MAX` for the unreachable >4G-point case is the same
        // pattern Extrude/Revolve use; `Tessellation::new` would have
        // rejected long before.
        let n_a = u32::try_from(self.profile_a.len()).unwrap_or(u32::MAX);
        let n_b = u32::try_from(self.profile_b.len()).unwrap_or(u32::MAX);
        // Defensive cap on side count: at construction the equal-count
        // validation in `evaluate` has run, so they must be equal at any
        // point where `evaluate` would succeed. Using `min` here defends
        // against a hypothetical mutation through the `pub profile_a` /
        // `pub profile_b` fields between construction and `brep_face_ids`
        // call — `Polygon2D` doesn't expose interior mutability today, but
        // the fields are publicly assignable. The cost is negligible and
        // removes a panic surface (mismatched counts can't drive the
        // index range out of bounds against either profile).
        let n = n_a.min(n_b);
        let total = (u64::from(n)).saturating_add(2);
        let mut ids: Vec<(TopologyFaceId, BRepFaceId)> = Vec::with_capacity(total as usize);
        ids.push((
            TopologyFaceId(0),
            BRepFaceId::for_loft_face(owner, LoftFaceTag::Bottom),
        ));
        ids.push((
            TopologyFaceId(1),
            BRepFaceId::for_loft_face(owner, LoftFaceTag::Top),
        ));
        for i in 0..n {
            ids.push((
                TopologyFaceId(2 + u64::from(i)),
                BRepFaceId::for_loft_face(
                    owner,
                    LoftFaceTag::Side {
                        edge_index: i,
                        profile_a_count: n_a,
                        profile_b_count: n_b,
                    },
                ),
            ));
        }
        ids
    }
}

// ---------------------------------------------------------------------------
// BRepEdgeProvider — sub-7.2-ζ.δ B-Rep edge identity for LoftOp
// ---------------------------------------------------------------------------

/// Mint the `3 * N` stable B-Rep edge identities for a lofted solid.
///
/// `LoftOp`'s edge topology is structurally identical to
/// [`crate::ExtrudeOp`]'s — both are closed prisms whose side walls
/// fan from a bottom ring to a top ring. For a profile pair of `N`
/// vertices each (`LoftOp::evaluate` enforces equal counts), the
/// solid has exactly `3 * N` edges:
///
/// * `N` bottom-perimeter edges — one per `Bottom ∩ Side(i)` adjacency
///   for `i in 0..N`. Bottom is `TopologyFaceId(0)` from the
///   [`BRepProvider`] impl above.
/// * `N` top-perimeter edges — one per `Top ∩ Side(i)` adjacency
///   for `i in 0..N`. Top is `TopologyFaceId(1)`.
/// * `N` vertical-seam edges — one per `Side(i) ∩ Side((i + 1) % N)`
///   adjacency, the seam between adjacent side walls running from
///   bottom to top. (For Loft v0 these are straight diagonals, not
///   parallel like Extrude, but topologically a single edge each.)
///
/// Edges are emitted in that order. Every edge uses
/// `local_ordinal = 0`.
///
/// The defensive `n_a.min(n_b)` cap mirrors the [`BRepProvider`] impl
/// above: `LoftOp::evaluate`'s equal-count gate ensures at any valid
/// call site the two profiles have the same length, but `profile_a` /
/// `profile_b` are publicly assignable, so taking the minimum
/// removes a panic surface if the two get out of sync between
/// construction and `brep_edge_ids` call.
impl BRepEdgeProvider for LoftOp {
    fn brep_edge_ids(&self, owner: BRepOwnerId) -> Vec<BRepEdgeId> {
        let face_ids: Vec<BRepFaceId> = self
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        // Face emission order (sub-7.2-δ) — see `impl BRepProvider for
        // LoftOp` above:
        //   TopologyFaceId(0) = Bottom
        //   TopologyFaceId(1) = Top
        //   TopologyFaceId(2 + i) = Side(i) for i in 0..N
        let n_a = u32::try_from(self.profile_a.len()).unwrap_or(u32::MAX);
        let n_b = u32::try_from(self.profile_b.len()).unwrap_or(u32::MAX);
        // Defensive — equal-count enforced by `LoftOp::evaluate`, but the
        // pub fields are independently mutable.
        let n = n_a.min(n_b);
        let total = (u64::from(n)).saturating_mul(3);
        let mut edges: Vec<BRepEdgeId> = Vec::with_capacity(total as usize);

        // Bottom perimeter — N edges.
        for i in 0..n {
            let side_idx = 2 + i as usize;
            edges.push(BRepEdgeId::for_face_pair(
                face_ids[0],
                face_ids[side_idx],
                0,
            ));
        }
        // Top perimeter — N edges.
        for i in 0..n {
            let side_idx = 2 + i as usize;
            edges.push(BRepEdgeId::for_face_pair(
                face_ids[1],
                face_ids[side_idx],
                0,
            ));
        }
        // Vertical seams — N edges.
        for i in 0..n {
            let next = (i + 1) % n;
            let a = 2 + i as usize;
            let b = 2 + next as usize;
            edges.push(BRepEdgeId::for_face_pair(face_ids[a], face_ids[b], 0));
        }
        edges
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ccw_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("ccw unit square")
    }

    fn ccw_square_scaled() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]])
            .expect("ccw 2-unit square")
    }

    fn cw_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]])
            .expect("cw unit square")
    }

    fn ccw_triangle() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle")
    }

    fn ccw_triangle_b() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [2.0, 0.0], [1.0, 2.0]]).expect("triangle b")
    }

    fn ccw_pentagon() -> Polygon2D {
        Polygon2D::new(vec![
            [1.0, 0.0],
            [0.309, 0.951],
            [-0.809, 0.588],
            [-0.809, -0.588],
            [0.309, -0.951],
        ])
        .expect("regular pentagon")
    }

    fn ccw_pentagon_scaled() -> Polygon2D {
        Polygon2D::new(vec![
            [2.0, 0.0],
            [0.618, 1.902],
            [-1.618, 1.176],
            [-1.618, -1.176],
            [0.618, -1.902],
        ])
        .expect("scaled pentagon")
    }

    fn concave_l_shape() -> Polygon2D {
        Polygon2D::new(vec![
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ])
        .expect("concave L-shape")
    }

    // -- LoftOp::new ---------------------------------------------------------

    #[test]
    fn loft_op_new_rejects_zero_length() {
        let err = LoftOp::new(ccw_square(), ccw_square_scaled(), 0.0).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    #[test]
    fn loft_op_new_rejects_negative_length() {
        let err = LoftOp::new(ccw_square(), ccw_square_scaled(), -1.0).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    #[test]
    fn loft_op_new_rejects_non_finite_length() {
        let err = LoftOp::new(ccw_square(), ccw_square_scaled(), f32::NAN).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
        let err = LoftOp::new(ccw_square(), ccw_square_scaled(), f32::INFINITY).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    // -- evaluate vertex / triangle counts -----------------------------------

    #[test]
    fn loft_square_to_square_yields_8_vertices_12_triangles() {
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 2.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // n=4 ⇒ 2n=8 vertices, 4n-4=12 triangles, 36 indices.
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn loft_triangle_to_triangle_yields_6_vertices_8_triangles() {
        let op = LoftOp::new(ccw_triangle(), ccw_triangle_b(), 1.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // n=3 ⇒ 2n=6 vertices, 4n-4=8 triangles, 24 indices.
        assert_eq!(mesh.vertex_count(), 6);
        assert_eq!(mesh.triangle_count(), 8);
        assert_eq!(mesh.indices.len(), 24);
    }

    #[test]
    fn loft_pentagon_to_pentagon_yields_10_vertices_16_triangles() {
        let op = LoftOp::new(ccw_pentagon(), ccw_pentagon_scaled(), 0.5).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // n=5 ⇒ 2n=10 vertices, 4n-4=16 triangles, 48 indices.
        assert_eq!(mesh.vertex_count(), 10);
        assert_eq!(mesh.triangle_count(), 16);
        assert_eq!(mesh.indices.len(), 48);
    }

    // -- evaluate rejection paths --------------------------------------------

    #[test]
    fn loft_rejects_mismatched_point_counts() {
        let op = LoftOp::new(ccw_square(), ccw_triangle(), 1.0).expect("op");
        let err = op.evaluate(&[]).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(
                    msg.contains("same point count") || msg.contains("point count"),
                    "msg = {msg}"
                );
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn loft_rejects_concave_profile_a() {
        let op = LoftOp::new(concave_l_shape(), concave_l_shape(), 1.0).expect("op");
        let err = op.evaluate(&[]).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("convex"), "msg = {msg}");
                assert!(
                    msg.contains("profile_a"),
                    "expected profile_a in msg: {msg}"
                );
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn loft_rejects_concave_profile_b() {
        // profile_a is the same shape as profile_b structurally, but we want
        // to confirm the error specifically names profile_b when profile_a
        // is convex and profile_b is concave. Use a 6-vertex convex hexagon
        // for profile_a so it passes convexity, paired with the 6-vertex
        // concave L-shape for profile_b.
        let convex_hex = Polygon2D::new(vec![
            [1.0, 0.0],
            [0.5, 0.866],
            [-0.5, 0.866],
            [-1.0, 0.0],
            [-0.5, -0.866],
            [0.5, -0.866],
        ])
        .expect("convex hex");
        let op = LoftOp::new(convex_hex, concave_l_shape(), 1.0).expect("op");
        let err = op.evaluate(&[]).unwrap_err();
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("convex"), "msg = {msg}");
                assert!(
                    msg.contains("profile_b"),
                    "expected profile_b in msg: {msg}"
                );
            }
            other => panic!("expected InvalidParameter, got {other:?}"),
        }
    }

    #[test]
    fn loft_cw_profile_auto_flipped() {
        // profile_a CW, profile_b CCW. Algorithm should detect the negative
        // signed area on profile_a, reverse iteration order, and still
        // produce the expected vertex/triangle counts.
        let op = LoftOp::new(cw_square(), ccw_square_scaled(), 1.0).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn loft_rejects_inputs_for_arity_0() {
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");
        let bogus = Tessellation::new(vec![[0.0_f32, 0.0, 0.0]], vec![]).expect("ok");
        let err = op.evaluate(&[&bogus]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 0,
                got: 1
            }
        ));
    }

    #[test]
    fn loft_post_construction_length_corruption_rejected() {
        // `length` is a pub field — a caller can flip it to bogus values
        // after construction. evaluate() must defensively re-check.
        let mut op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");
        op.length = -1.0;
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(err, OpError::InvalidParameter(_)));
    }

    // -- structural_hash -----------------------------------------------------

    #[test]
    fn loft_structural_hash_deterministic() {
        let a = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.5).expect("a");
        let b = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.5).expect("b");
        assert_eq!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn loft_structural_hash_changes_with_length() {
        let a = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.5).expect("a");
        let b = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.6).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn loft_structural_hash_changes_with_profile_a_perturbation() {
        let a = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("a");
        let perturbed = Polygon2D::new(vec![[0.0, 0.0], [1.0 + 1e-3, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("perturbed a");
        let b = LoftOp::new(perturbed, ccw_square_scaled(), 1.0).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn loft_structural_hash_changes_with_profile_b_perturbation() {
        let a = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("a");
        let perturbed = Polygon2D::new(vec![[0.0, 0.0], [2.0 + 1e-3, 0.0], [2.0, 2.0], [0.0, 2.0]])
            .expect("perturbed b");
        let b = LoftOp::new(ccw_square(), perturbed, 1.0).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    // -- op_kind / arity -----------------------------------------------------

    #[test]
    fn loft_op_kind_is_loft() {
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");
        assert_eq!(op.op_kind(), OpKind::Loft);
        assert_eq!(op.arity(), 0);
    }

    /// `LoftOp` is arity 0 and emits an unlabeled `Tessellation::new(...)`
    /// — so the trait-default [`Operator::output_is_labeled`] (which returns
    /// `false` on an empty `inputs_labeled` slice via `iter().any`) matches
    /// the actual `evaluate` semantics. No override needed.
    #[test]
    fn loft_output_is_labeled_returns_false() {
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");
        assert!(!op.output_is_labeled(&[]));
    }

    // -- z-coordinate placement ---------------------------------------------

    #[test]
    fn loft_top_ring_z_equals_length() {
        let length = 3.5_f32;
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), length).expect("op");
        let mesh = op.evaluate(&[]).expect("evaluate");
        // Bottom ring (positions 0..4) at z=0; top ring (4..8) at z=length.
        for v in &mesh.positions[..4] {
            assert!(v[2].abs() < f32::EPSILON, "bottom z must be 0: {v:?}");
        }
        for v in &mesh.positions[4..8] {
            assert!(
                (v[2] - length).abs() < f32::EPSILON,
                "top z must be {length}: {v:?}"
            );
        }
    }

    // -- BRepProvider impl (sub-7.2-δ) ---------------------------------------

    /// `BRepProvider::brep_face_ids` must return exactly `N + 2` pairs for
    /// two squares (N = 4) — 4 sides + Bottom cap + Top cap.
    #[test]
    fn brep_provider_returns_n_plus_2_pairs_for_squares() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");
        let pairs = op.brep_face_ids(owner);
        assert_eq!(pairs.len(), 6, "two squares (N=4) should yield N+2=6 pairs");
    }

    /// `BRepProvider::brep_face_ids` must return exactly `N + 2` pairs for
    /// two pentagons (N = 5) — 5 sides + Bottom cap + Top cap.
    #[test]
    fn brep_provider_returns_n_plus_2_pairs_for_pentagons() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let op = LoftOp::new(ccw_pentagon(), ccw_pentagon_scaled(), 1.0).expect("op");
        let pairs = op.brep_face_ids(owner);
        assert_eq!(
            pairs.len(),
            7,
            "two pentagons (N=5) should yield N+2=7 pairs"
        );
    }

    /// The returned `TopologyFaceId(0)` corresponds to `Bottom`,
    /// `TopologyFaceId(1)` to `Top`, and `TopologyFaceId(2..N+2)` to
    /// `Side(0..N-1)` in canonical emission order. This pins the
    /// `TopologyFaceId` ↔ `LoftFaceTag` mapping byte-for-byte.
    #[test]
    fn brep_provider_topology_face_ids_are_canonical_emission_order() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");
        let pairs = op.brep_face_ids(owner);

        // Bottom at index 0 with TopologyFaceId(0).
        assert_eq!(pairs[0].0 .0, 0);
        assert_eq!(
            pairs[0].1,
            BRepFaceId::for_loft_face(owner, LoftFaceTag::Bottom)
        );

        // Top at index 1 with TopologyFaceId(1).
        assert_eq!(pairs[1].0 .0, 1);
        assert_eq!(
            pairs[1].1,
            BRepFaceId::for_loft_face(owner, LoftFaceTag::Top)
        );

        // Sides at indices 2..6 with TopologyFaceId(2..6) and edge_index 0..4.
        for i in 0u32..4 {
            let idx = (2 + i) as usize;
            assert_eq!(pairs[idx].0 .0, 2 + u64::from(i));
            assert_eq!(
                pairs[idx].1,
                BRepFaceId::for_loft_face(
                    owner,
                    LoftFaceTag::Side {
                        edge_index: i,
                        profile_a_count: 4,
                        profile_b_count: 4,
                    },
                ),
                "side at index {idx} (edge_index {i}) does not match canonical mapping"
            );
        }
    }

    // -- BRepEdgeProvider impl (sub-7.2-ζ.δ) ---------------------------------

    /// `BRepEdgeProvider::brep_edge_ids` must return exactly `3 * N` edges
    /// for a `LoftOp` with two equal-count profiles of length `N`. For two
    /// squares (`N = 4`) this is 12 edges; for two pentagons (`N = 5`)
    /// this is 15 edges.
    #[test]
    fn brep_edge_provider_returns_expected_edge_count() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);

        let sq_to_sq = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("sq×sq");
        assert_eq!(
            sq_to_sq.brep_edge_ids(owner).len(),
            12,
            "two squares N=4 → 3*4=12"
        );

        let tri_to_tri = LoftOp::new(ccw_triangle(), ccw_triangle_b(), 1.0).expect("tri×tri");
        assert_eq!(
            tri_to_tri.brep_edge_ids(owner).len(),
            9,
            "two triangles N=3 → 3*3=9"
        );

        let pen_to_pen = LoftOp::new(ccw_pentagon(), ccw_pentagon_scaled(), 1.0).expect("pen×pen");
        assert_eq!(
            pen_to_pen.brep_edge_ids(owner).len(),
            15,
            "two pentagons N=5 → 3*5=15"
        );
    }

    /// Every `BRepEdgeId` minted by `LoftOp` uses `local_ordinal = 0`.
    #[test]
    fn brep_edge_ids_use_local_ordinal_zero() {
        let owner = BRepOwnerId::from_bytes([0x99u8; 16]);
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");
        let face_ids: Vec<BRepFaceId> = op
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let edges = op.brep_edge_ids(owner);

        // Edge 0: Bottom ∩ Side(0).
        assert_eq!(
            edges[0],
            BRepEdgeId::for_face_pair(face_ids[0], face_ids[2], 0),
            "edge 0 must be derived with local_ordinal = 0"
        );
        // Edge 4: Top ∩ Side(0).
        assert_eq!(
            edges[4],
            BRepEdgeId::for_face_pair(face_ids[1], face_ids[2], 0),
            "edge 4 must be derived with local_ordinal = 0"
        );
    }

    /// The 12 edges for a `LoftOp` of two squares align with the canonical
    /// adjacency table documented in the `impl BRepEdgeProvider for
    /// LoftOp` block. We verify three representative edges.
    #[test]
    fn brep_edge_ids_align_with_canonical_adjacency_table() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");
        let face_ids: Vec<BRepFaceId> = op
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let edges = op.brep_edge_ids(owner);

        // Bottom-perimeter, edge 0: Bottom ∩ Side(0).
        assert_eq!(
            edges[0],
            BRepEdgeId::for_face_pair(face_ids[0], face_ids[2], 0),
            "edge 0 must be Bottom ∩ Side(0)"
        );
        // Top-perimeter, edge 4 (= N): Top ∩ Side(0).
        assert_eq!(
            edges[4],
            BRepEdgeId::for_face_pair(face_ids[1], face_ids[2], 0),
            "edge 4 must be Top ∩ Side(0)"
        );
        // Vertical seam, edge 8 (= 2N): Side(0) ∩ Side(1).
        assert_eq!(
            edges[8],
            BRepEdgeId::for_face_pair(face_ids[2], face_ids[3], 0),
            "edge 8 must be Side(0) ∩ Side(1)"
        );
    }
}
