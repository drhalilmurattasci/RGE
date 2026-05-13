//! `RoundFilletOp` constructor + helpers for `LoftOp` upstream
//! (sub-δ.revisit — Loft as thin upstream layer atop the general-
//! dihedral substrate landed at sub-β.γ).
//!
//! Per ADR-119 D5 (substrate parallelism, not sharing) + the
//! sub-δ.revisit green-light direction, this module mirrors the
//! shape of the chamfer side's
//! [`crate::operators::fillet::loft`] module but stays byte-distinct
//! AND uses **true incident-side-triangle plane normals** (not
//! chamfer's Extrude-style XY-only approximation) so the round-
//! fillet cylinder is geometrically tangent to the actual Loft side
//! surface at every cap-perimeter / vertical-seam edge.
//!
//! # Edge eligibility (sub-δ.revisit scope)
//!
//! `LoftOp`'s [`crate::topology::BRepEdgeProvider`] impl emits `3 * N`
//! edges for a profile pair of `N` vertices each (`profile_a.len()
//! == profile_b.len() == N`; enforced by `LoftOp::evaluate`), in
//! three classes:
//!
//! * Indices `[0, N)` — bottom-perimeter (`Bottom ∩ Side(i)`):
//!   straight 2-endpoint edges at `z = 0` on the bottom ring.
//!   **Accepted**.
//! * Indices `[N, 2N)` — top-perimeter (`Top ∩ Side(i)`): straight
//!   2-endpoint edges at `z = length` on the top ring. **Accepted**.
//! * Indices `[2N, 3N)` — vertical-seam (`Side(i) ∩ Side((i+1)%N)`):
//!   straight 2-endpoint edges from `bot_{(i+1)%N}` to
//!   `top_{(i+1)%N}`, generally diagonal in 3D when `profile_a ≠
//!   profile_b`. **Accepted**.
//!
//! All 3N edges accept under the sub-β.γ general-dihedral machinery.
//! Rejection cases are GENUINELY DEGENERATE:
//!
//! * `profile_a.len() != profile_b.len()` (defensive — `LoftOp::evaluate`
//!   already rejects, so unreachable for any LoftOp whose tessellation
//!   exists; reject early via [`RoundFilletError::UnsupportedEdgeGeometry`]
//!   anyway so callers get an early-fail signal).
//! * Zero-magnitude triangle plane normal (collinear corners). For
//!   valid Loft (Polygon2D rejects coincident adjacent points;
//!   length > 0), this is unreachable but defensively checked.
//!
//! Near-degenerate dihedrals (face_a_inward ≈ ±face_b_inward) are
//! NOT pre-empted at construction; sub-β.γ's evaluate-time guard
//! catches them via [`OpError::InvalidParameter`].
//!
//! # Triangle-incidence convention (v0, load-bearing)
//!
//! Each Loft `Side(i)` quad is split into 2 triangles by the
//! diagonal `bot_i → top_{i+1}` (`loft.rs::evaluate` L320-340):
//!
//! ```text
//! Tri 1(i) = (bot_i, bot_{i+1}, top_{i+1})    = (BL, BR, TR)
//! Tri 2(i) = (bot_i, top_{i+1}, top_i)         = (BL, TR, TL)
//! ```
//!
//! For non-uniform Lofts (`profile_a ≠ profile_b`), the two
//! triangles **generally have different plane normals** — the quad
//! is non-planar. We resolve the ambiguity by picking the triangle
//! INCIDENT to the selected edge:
//!
//! | Edge class | Side(i)'s incident triangle |
//! |---|---|
//! | Bottom-perimeter `i` | Tri 1 (contains both `bot_i` + `bot_{i+1}`) |
//! | Top-perimeter `i` | Tri 2 (contains both `top_i` + `top_{i+1}`) |
//! | Vertical-seam `i` | Side(i): Tri 1 (contains `bot_{i+1}` + `top_{i+1}`); Side((i+1)%N): Tri 2 (contains `bot_{i+1}` + `top_{i+1}`) |
//!
//! This is a **v0 local convention** — pinned by inline comments
//! and tests in this module, NOT by an ADR addendum (the convention
//! is local to Loft and doesn't generalize to other operators).
//!
//! # Inward-direction algorithm (orientation-robust)
//!
//! Given face A's outward normal `n_A`, face B's outward normal
//! `n_B`, and edge tangent `e_t` (oriented vertex_a → vertex_b):
//!
//! ```text
//! candidate = normalize(cross(e_t, n_A))   // perpendicular to both → in face A's plane
//! if dot(candidate, -n_B) < 0:
//!     face_a_inward = -candidate
//! else:
//!     face_a_inward = candidate
//! ```
//!
//! Symmetric for face B. The sign-check works for any non-degenerate
//! dihedral because `dot(candidate, -n_B) = dot(candidate,
//! proj_{plane_A}(-n_B))` (the `n_A`-component of `n_B` projects
//! away since `candidate ⊥ n_A` by construction).
//!
//! Reduces to sub-β Extrude's formula byte-for-byte when `profile_b
//! == profile_a` (Extrude special case): `N_tri1 = (dy_a·length,
//! -dx_a·length, 0)` matches Extrude's side-normal formula exactly,
//! and the sign-check resolves identically.
//!
//! # Substrate posture
//!
//! `RoundFilletOp` struct + `RoundFilletSpec` fields +
//! `RoundFilletError` enum + `RoundFilletUpstream` trait + sub-β.γ
//! general-dihedral `evaluate` body + sub-α/β/γ per-upstream impls
//! + resolver arms ALL byte-identical to sub-β.γ `cae3a84`. This
//! module adds ONLY the `RoundFilletUpstream` impl for `LoftOp`
//! and the public `RoundFilletOp::new_for_loft` constructor (thin
//! delegate to `from_upstream`). Chamfer's `fillet::loft` is D6
//! byte-identical to its sub-α landing.

use super::{
    RoundFilletError, RoundFilletOp, RoundFilletSpec, RoundFilletSpecKind, RoundFilletUpstream,
};
use crate::operators::LoftOp;
use crate::tessellation::TopologyFaceId;
use crate::topology::{BRepEdgeId, BRepOwnerId};

impl RoundFilletUpstream for LoftOp {
    fn resolve_round_spec(
        &self,
        canonical_index: usize,
    ) -> Result<RoundFilletSpecKind, &'static str> {
        let n_a = self.profile_a.len();
        let n_b = self.profile_b.len();
        // Defensive: LoftOp::evaluate rejects mismatched profile
        // lengths, so any LoftOp whose tessellation exists has
        // n_a == n_b. Mirroring chamfer's `fillet/loft.rs:L59` +
        // `BRepEdgeProvider` impl's `n_a.min(n_b)` pattern: reject
        // here too (early-fail signal) instead of trusting an
        // evaluate-time check that won't run if the caller only
        // queries `brep_edge_ids` + this trait without calling
        // `evaluate`.
        if n_a != n_b {
            return Err("loft profile_a.len() must equal profile_b.len() (LoftOp::evaluate enforces; rejecting at spec resolution as defense)");
        }
        let n = n_a;
        let n_u32 = u32::try_from(n).unwrap_or(u32::MAX);

        // Three-class dispatch over BRepEdgeProvider's canonical
        // emission order (`loft.rs::impl BRepEdgeProvider for
        // LoftOp`):
        //   [0..N)   bottom-perimeter — Bottom ∩ Side(i)
        //   [N..2N)  top-perimeter    — Top ∩ Side(i)
        //   [2N..3N) vertical-seam    — Side(i) ∩ Side((i+1)%N)
        if canonical_index < n {
            // Bottom-perimeter edge i.
            //
            // vertex_a = bot_i = i; vertex_b = bot_{i+1} = (i+1)%N.
            // face_a = Bottom (TopologyFaceId(0); outward normal
            //          n_Bottom = (0, 0, -1) by flat XY plane).
            // face_b = Side(i) (TopologyFaceId(2 + i)). Per v0
            //          triangle-incidence convention: the triangle
            //          INCIDENT to this bottom-perimeter edge is
            //          Side(i)'s Tri 1 = (bot_i, bot_{i+1}, top_{i+1}).
            let i = canonical_index;
            let (vertex_a, vertex_b) = loft_bottom_perimeter_vertex_pair(i, n_u32);
            let n_face_a = [0.0, 0.0, -1.0];
            let n_face_b = loft_side_tri1_outward_normal(self, i)?;
            let edge_tangent = loft_bottom_edge_tangent(self, i)?;
            let (face_a_inward, face_b_inward) =
                solve_inward_directions(edge_tangent, n_face_a, n_face_b);
            Ok(RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(0),
                face_b_id: TopologyFaceId(2 + i as u64),
                face_a_inward,
                face_b_inward,
            }))
        } else if canonical_index < 2 * n {
            // Top-perimeter edge i (local index = canonical - N).
            //
            // vertex_a = top_i = N + i; vertex_b = top_{i+1} = N + (i+1)%N.
            // face_a = Top (TopologyFaceId(1); outward normal
            //          n_Top = (0, 0, +1)).
            // face_b = Side(i). Per v0 triangle-incidence
            //          convention: the triangle INCIDENT to this
            //          top-perimeter edge is Side(i)'s Tri 2 =
            //          (bot_i, top_{i+1}, top_i).
            let local = canonical_index - n;
            let (vertex_a, vertex_b) = loft_top_perimeter_vertex_pair(local, n_u32);
            let n_face_a = [0.0, 0.0, 1.0];
            let n_face_b = loft_side_tri2_outward_normal(self, local)?;
            let edge_tangent = loft_top_edge_tangent(self, local)?;
            let (face_a_inward, face_b_inward) =
                solve_inward_directions(edge_tangent, n_face_a, n_face_b);
            Ok(RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(1),
                face_b_id: TopologyFaceId(2 + local as u64),
                face_a_inward,
                face_b_inward,
            }))
        } else if canonical_index < 3 * n {
            // Vertical-seam edge i.
            //
            // vertex_a = bot_{(i+1)%N}; vertex_b = top_{(i+1)%N}.
            // The edge runs from a bottom-ring vertex to its paired
            // top-ring vertex; generally diagonal in 3D when
            // profile_b ≠ profile_a translated/rotated relative
            // to profile_a.
            //
            // face_a = Side(local) (TopologyFaceId(2 + local)).
            //          Per v0 convention: Side(local)'s Tri 1 =
            //          (bot_local, bot_{local+1}, top_{local+1})
            //          is incident — contains BOTH vertex_a and
            //          vertex_b.
            // face_b = Side((local+1)%N) (TopologyFaceId(2 +
            //          (local+1)%N)). Per v0 convention:
            //          Side((local+1)%N)'s Tri 2 =
            //          (bot_{(local+1)%N}, top_{(local+2)%N},
            //          top_{(local+1)%N}) is incident — contains
            //          BOTH vertex_a and vertex_b.
            let local = canonical_index - 2 * n;
            let (vertex_a, vertex_b) = loft_vertical_seam_vertex_pair(local, n_u32);
            let n_face_a = loft_side_tri1_outward_normal(self, local)?;
            let n_face_b = loft_side_tri2_outward_normal(self, (local + 1) % n)?;
            let edge_tangent = loft_vertical_seam_edge_tangent(self, local)?;
            let (face_a_inward, face_b_inward) =
                solve_inward_directions(edge_tangent, n_face_a, n_face_b);
            Ok(RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a,
                vertex_b,
                face_a_id: TopologyFaceId(2 + local as u64),
                face_b_id: TopologyFaceId(2 + ((local + 1) % n) as u64),
                face_a_inward,
                face_b_inward,
            }))
        } else {
            // Defensive: from_upstream's caller-side filter
            // restricts canonical_index to the upstream's
            // brep_edge_ids length (exactly 3N for any LoftOp).
            // Unreachable in production paths.
            Err("loft canonical edge index out of range (must be < 3N)")
        }
    }
}

impl RoundFilletOp {
    /// Construct a [`RoundFilletOp`] validated against the upstream
    /// `LoftOp`. All 3N edges (bottom-perimeter + top-perimeter +
    /// vertical-seam) are accepted under the sub-β.γ general-
    /// dihedral machinery; rejection at construction is only for
    /// genuinely degenerate cases (mismatched profile lengths,
    /// zero-magnitude side-triangle normals).
    ///
    /// Mirrors [`RoundFilletOp::new`] (Cuboid) +
    /// [`RoundFilletOp::new_for_extrude`] (Extrude cap-perimeter) +
    /// [`RoundFilletOp::new_for_revolve`] (Revolve cap-side) but
    /// is the FIRST upstream constructor that lifts the 90° dihedral
    /// restriction — Loft side surfaces tilt for `profile_a ≠
    /// profile_b`, and the sub-β.γ general-dihedral evaluate body
    /// (`RoundFilletOp::evaluate` post-sub-β.γ) handles the
    /// arbitrary dihedral correctly.
    ///
    /// # Errors
    ///
    /// * [`RoundFilletError::InvalidRadius`] if `radius` is non-finite
    ///   or `<= 0`.
    /// * [`RoundFilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`RoundFilletError::EdgeNotInUpstream`] if any edge ID does
    ///   not appear in `upstream.brep_edge_ids(owner)`.
    /// * [`RoundFilletError::UnsupportedEdgeGeometry`] if the
    ///   upstream has mismatched profile lengths (defensive) or a
    ///   degenerate side triangle (collinear corners; unreachable
    ///   for valid LoftOp inputs).
    pub fn new_for_loft(
        upstream: &LoftOp,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, RoundFilletError> {
        Self::from_upstream(upstream, owner, edges, radius)
    }
}

// ---------------------------------------------------------------------------
// Vertex-pair helpers — derived from loft.rs::evaluate's 2N-vertex layout.
//
// Per ADR-119 D5 these are duplicated from `fillet::loft` (chamfer)
// rather than shared. Byte-identical formulas; intentional duplication
// so any future Loft winding evolution stays unilateral.
// ---------------------------------------------------------------------------

fn loft_bottom_perimeter_vertex_pair(i: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let i_u32 = i as u32;
    let next = ((i + 1) % n) as u32;
    (i_u32, next)
}

fn loft_top_perimeter_vertex_pair(i: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let i_u32 = i as u32;
    let next = ((i + 1) % n) as u32;
    (profile_count + i_u32, profile_count + next)
}

/// Vertical-seam edge `local` runs from `bot_{(local+1)%N}` to
/// `top_{(local+1)%N}`. Mirrors `loft.rs::impl BRepEdgeProvider
/// for LoftOp`'s vertical-seam adjacency: `Side(local) ∩
/// Side((local+1)%N)` shares profile-vertex `(local+1)%N`, so the
/// seam is the vertical (well, possibly-diagonal) edge spanning
/// that shared vertex between bottom and top rings.
fn loft_vertical_seam_vertex_pair(local: usize, profile_count: u32) -> (u32, u32) {
    let n = profile_count as usize;
    let seam_vertex = ((local + 1) % n) as u32;
    (seam_vertex, profile_count + seam_vertex)
}

// ---------------------------------------------------------------------------
// Edge tangent helpers — unit vectors along each edge class.
//
// Bottom and top perimeter edges run in the XY plane (z=0 or
// z=length). Vertical-seam edges generally have a non-zero XY
// component when profile_b ≠ profile_a translated/rotated
// relative to profile_a — the seam diagonally bridges
// (xa, ya, 0) → (xb, yb, length).
// ---------------------------------------------------------------------------

fn loft_bottom_edge_tangent(upstream: &LoftOp, i: usize) -> Result<[f32; 3], &'static str> {
    let pts = upstream.profile_a.points();
    let n = pts.len();
    if n < 3 {
        return Err("loft profile_a degenerate (n < 3); cannot construct edge tangent");
    }
    let p_i = pts[i % n];
    let p_next = pts[(i + 1) % n];
    let dx = p_next[0] - p_i[0];
    let dy = p_next[1] - p_i[1];
    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        // Polygon2D::new rejects coincident adjacent points so
        // this is defensive; unreachable for valid LoftOp.
        return Err("loft profile_a edge zero-length (degenerate); cannot construct edge tangent");
    }
    let inv = 1.0 / mag;
    Ok([dx * inv, dy * inv, 0.0])
}

fn loft_top_edge_tangent(upstream: &LoftOp, i: usize) -> Result<[f32; 3], &'static str> {
    let pts = upstream.profile_b.points();
    let n = pts.len();
    if n < 3 {
        return Err("loft profile_b degenerate (n < 3); cannot construct edge tangent");
    }
    let p_i = pts[i % n];
    let p_next = pts[(i + 1) % n];
    let dx = p_next[0] - p_i[0];
    let dy = p_next[1] - p_i[1];
    let mag = (dx * dx + dy * dy).sqrt();
    if mag < 1e-9 {
        return Err("loft profile_b edge zero-length (degenerate); cannot construct edge tangent");
    }
    let inv = 1.0 / mag;
    Ok([dx * inv, dy * inv, 0.0])
}

fn loft_vertical_seam_edge_tangent(
    upstream: &LoftOp,
    local: usize,
) -> Result<[f32; 3], &'static str> {
    let pa = upstream.profile_a.points();
    let pb = upstream.profile_b.points();
    let n = pa.len();
    if n < 3 || pb.len() < 3 {
        return Err("loft profile degenerate (n < 3); cannot construct vertical-seam tangent");
    }
    let seam = (local + 1) % n;
    let p_bot = pa[seam];
    let p_top = pb[seam];
    let length = upstream.length;
    let dx = p_top[0] - p_bot[0];
    let dy = p_top[1] - p_bot[1];
    // dz component is `length` (top ring sits at z = length, bottom at z = 0).
    let dz = length;
    let mag = (dx * dx + dy * dy + dz * dz).sqrt();
    if mag < 1e-9 {
        return Err("loft vertical-seam edge degenerate (zero-length); cannot construct tangent");
    }
    let inv = 1.0 / mag;
    Ok([dx * inv, dy * inv, dz * inv])
}

// ---------------------------------------------------------------------------
// Side-triangle outward normal helpers (v0 triangle-incidence convention).
//
// Side(i) is split into 2 triangles by the diagonal bot_i → top_{i+1}
// (loft.rs::evaluate L320-340):
//
//   Tri 1(i) = (bot_i, bot_{i+1}, top_{i+1})    = (BL, BR, TR)
//   Tri 2(i) = (bot_i, top_{i+1}, top_i)         = (BL, TR, TL)
//
// Plane normal is computed via cross product of two edge vectors
// from the same triangle vertex. CCW winding (as emitted by
// loft.rs::evaluate) places the OUTWARD normal in the direction
// of the cross product.
// ---------------------------------------------------------------------------

fn loft_side_tri1_outward_normal(upstream: &LoftOp, i: usize) -> Result<[f32; 3], &'static str> {
    let pa = upstream.profile_a.points();
    let pb = upstream.profile_b.points();
    let n = pa.len();
    if n < 3 || pb.len() < 3 {
        return Err("loft profile degenerate (n < 3); cannot construct side triangle normal");
    }
    let p_a_i = pa[i % n];
    let p_a_next = pa[(i + 1) % n];
    let p_b_next = pb[(i + 1) % n];
    let length = upstream.length;
    // BL = (p_a_i.x, p_a_i.y, 0); BR = (p_a_next.x, p_a_next.y, 0);
    // TR = (p_b_next.x, p_b_next.y, length).
    // Edge vectors from BL: (BR-BL) and (TR-BL).
    // (BR-BL) = (dx_a, dy_a, 0)
    // (TR-BL) = (ex, ey, length)
    let dx_a = p_a_next[0] - p_a_i[0];
    let dy_a = p_a_next[1] - p_a_i[1];
    let ex = p_b_next[0] - p_a_i[0];
    let ey = p_b_next[1] - p_a_i[1];
    // N = (BR-BL) × (TR-BL) = (dy_a·length, -dx_a·length, dx_a·ey - dy_a·ex).
    let n_x = dy_a * length;
    let n_y = -dx_a * length;
    let n_z = dx_a * ey - dy_a * ex;
    let mag = (n_x * n_x + n_y * n_y + n_z * n_z).sqrt();
    if mag < 1e-9 {
        return Err(
            "loft side triangle Tri1 zero-magnitude normal (degenerate; collinear corners)",
        );
    }
    let inv = 1.0 / mag;
    Ok([n_x * inv, n_y * inv, n_z * inv])
}

fn loft_side_tri2_outward_normal(upstream: &LoftOp, i: usize) -> Result<[f32; 3], &'static str> {
    let pa = upstream.profile_a.points();
    let pb = upstream.profile_b.points();
    let n = pa.len();
    if n < 3 || pb.len() < 3 {
        return Err("loft profile degenerate (n < 3); cannot construct side triangle normal");
    }
    let p_a_i = pa[i % n];
    let p_b_i = pb[i % n];
    let p_b_next = pb[(i + 1) % n];
    let length = upstream.length;
    // BL = (p_a_i.x, p_a_i.y, 0); TR = (p_b_next.x, p_b_next.y, length);
    // TL = (p_b_i.x, p_b_i.y, length).
    // Edge vectors from BL: (TR-BL) and (TL-BL).
    // (TR-BL) = (ex, ey, length)
    // (TL-BL) = (fx, fy, length)
    let ex = p_b_next[0] - p_a_i[0];
    let ey = p_b_next[1] - p_a_i[1];
    let fx = p_b_i[0] - p_a_i[0];
    let fy = p_b_i[1] - p_a_i[1];
    // N = (TR-BL) × (TL-BL) = (length·(ey-fy), length·(fx-ex), ex·fy - ey·fx).
    // Note ey-fy = dy_b, fx-ex = -dx_b where (dx_b, dy_b) = pb_next - pb_i.
    let n_x = length * (ey - fy);
    let n_y = length * (fx - ex);
    let n_z = ex * fy - ey * fx;
    let mag = (n_x * n_x + n_y * n_y + n_z * n_z).sqrt();
    if mag < 1e-9 {
        return Err(
            "loft side triangle Tri2 zero-magnitude normal (degenerate; collinear corners)",
        );
    }
    let inv = 1.0 / mag;
    Ok([n_x * inv, n_y * inv, n_z * inv])
}

// ---------------------------------------------------------------------------
// Inward-direction algorithm (orientation-robust).
//
// Given face A's outward normal `n_A`, face B's outward normal `n_B`,
// and edge tangent `e_t`:
//
//   candidate = normalize(cross(e_t, n_A))
//                — perpendicular to both → in face A's tangent plane
//                — magnitude is sin(angle between e_t and n_A)
//                  (= 1 for unit input vectors that are orthogonal,
//                   which is always true for a face-bounded edge
//                   since the edge lies in the face plane).
//   sign-check: candidate should point TOWARD face A's interior,
//               i.e., AWAY from face B's exterior. Test
//               dot(candidate, -n_B) > 0.
//   if sign-check fails: face_a_inward = -candidate.
//   else:                face_a_inward = candidate.
//
// Works for any non-degenerate dihedral because `candidate` is in
// face A's plane by construction (⊥ to n_A), so the
// `n_A`-component of `-n_B` projects away from the dot product:
//   dot(candidate, -n_B) = dot(candidate, proj_{plane_A}(-n_B)).
//
// Reduces to sub-α/β/γ formulas byte-for-byte for the 90°
// dihedral case (verified algebraically in sub-β.γ green-light
// inspection):
//   - Cuboid (perpendicular faces): cross(e_t, n_A) lies along
//     -n_B direction; sign-check passes; matches `-n_B` exactly.
//   - Extrude bottom-perimeter (Bottom ⊥ Side): cross gives
//     `(-dy_a, dx_a, 0)/||·||`; matches sub-β's
//     `[-side_normal_x, -side_normal_y, 0]` byte-for-byte.
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
// Sub-δ.revisit unit tests — Loft full upstream layer atop sub-β.γ
// general-dihedral machinery. Includes the load-bearing non-uniform
// translated Loft case per user direction: "Make sure the tests
// include the non-uniform translated Loft case; that's the proof
// that this is no longer just the old 90 degree plateau wearing a
// clever hat."
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::{Operator, Polygon2D};
    use crate::tessellation::TopologyFaceId;
    use crate::topology::BRepEdgeProvider;

    fn owner() -> BRepOwnerId {
        BRepOwnerId::from_bytes([0xed; 16])
    }

    fn unit_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("ccw unit square")
    }

    /// `profile_a == profile_b == unit_square` → degenerate-Loft
    /// which is geometrically equivalent to Extrude. Used to verify
    /// sub-δ.revisit accepts AND reduces to sub-β Extrude behavior
    /// (regression sanity).
    fn identity_loft() -> LoftOp {
        LoftOp::new(unit_square(), unit_square(), 1.0).expect("identity loft")
    }

    /// **Load-bearing non-uniform Loft per user direction**:
    /// profile_a = unit square at origin; profile_b = unit square
    /// translated up by (0, 1). The two profiles have the SAME
    /// vertex count and CCW winding, but the top ring sits at a
    /// different XY position than the bottom — so the side surfaces
    /// TILT, and bottom-perimeter / top-perimeter / vertical-seam
    /// edges all have NON-90° dihedrals. This is the case that
    /// "proves this is no longer just the old 90 degree plateau".
    fn translated_loft() -> LoftOp {
        let translated_b = Polygon2D::new(vec![[0.0, 1.0], [1.0, 1.0], [1.0, 2.0], [0.0, 2.0]])
            .expect("Y-translated unit square");
        LoftOp::new(unit_square(), translated_b, 1.0).expect("translated loft")
    }

    /// Twisted Loft: profile_b is profile_a rotated 45° around
    /// origin. Exercises a different non-uniform-shape pattern
    /// from the translated case.
    fn twisted_loft() -> LoftOp {
        let cos_45 = std::f32::consts::FRAC_1_SQRT_2;
        let sin_45 = std::f32::consts::FRAC_1_SQRT_2;
        // Rotate unit_square vertices around origin by 45°. Note:
        // unit_square has corners at (0,0), (1,0), (1,1), (0,1) —
        // rotating these around origin gives a CCW polygon by
        // construction (Polygon2D::new accepts).
        let rotated_b = Polygon2D::new(vec![
            [0.0, 0.0],
            [cos_45, sin_45],
            [cos_45 - sin_45, sin_45 + cos_45],
            [-sin_45, cos_45],
        ])
        .expect("rotated unit square");
        LoftOp::new(unit_square(), rotated_b, 1.0).expect("twisted loft")
    }

    // -- Construction reject paths (standard invariants) ---------------------

    #[test]
    fn new_for_loft_rejects_zero_radius() {
        let loft = identity_loft();
        let edge = loft.brep_edge_ids(owner())[0];
        let err = RoundFilletOp::new_for_loft(&loft, owner(), vec![edge], 0.0).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == 0.0));
    }

    #[test]
    fn new_for_loft_rejects_negative_radius() {
        let loft = identity_loft();
        let edge = loft.brep_edge_ids(owner())[0];
        let err = RoundFilletOp::new_for_loft(&loft, owner(), vec![edge], -0.5).unwrap_err();
        assert!(matches!(err, RoundFilletError::InvalidRadius { radius } if radius == -0.5));
    }

    #[test]
    fn new_for_loft_rejects_non_finite_radius() {
        let loft = identity_loft();
        let edge = loft.brep_edge_ids(owner())[0];
        let err_nan =
            RoundFilletOp::new_for_loft(&loft, owner(), vec![edge], f32::NAN).unwrap_err();
        assert!(matches!(err_nan, RoundFilletError::InvalidRadius { .. }));
        let err_inf =
            RoundFilletOp::new_for_loft(&loft, owner(), vec![edge], f32::INFINITY).unwrap_err();
        assert!(matches!(err_inf, RoundFilletError::InvalidRadius { .. }));
    }

    #[test]
    fn new_for_loft_rejects_empty_edge_list() {
        let loft = identity_loft();
        let err = RoundFilletOp::new_for_loft(&loft, owner(), vec![], 0.1).unwrap_err();
        assert_eq!(err, RoundFilletError::EmptyEdgeSelection);
    }

    #[test]
    fn new_for_loft_rejects_unknown_edge_id() {
        let loft = identity_loft();
        let phantom = BRepEdgeId::from_bytes([0u8; 16]);
        let err = RoundFilletOp::new_for_loft(&loft, owner(), vec![phantom], 0.1).unwrap_err();
        assert!(matches!(err, RoundFilletError::EdgeNotInUpstream { edge } if edge == phantom));
    }

    /// Defensive rejection of mismatched profile lengths.
    /// LoftOp::evaluate already enforces n_a == n_b; this test
    /// verifies the resolver also rejects (early-fail signal for
    /// callers who construct a malformed LoftOp via pub-field
    /// mutation before calling new_for_loft).
    #[test]
    fn new_for_loft_rejects_mismatched_profile_lengths() {
        let triangle = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle");
        // Mutate after construction: LoftOp::new accepts any
        // profile_a/profile_b, only evaluate enforces same length.
        let mut loft = LoftOp::new(unit_square(), unit_square(), 1.0).expect("loft");
        loft.profile_b = triangle;
        // BRepEdgeProvider uses n_a.min(n_b) = 3, so 9 edges in the
        // upstream output. Pick the first one (which is fine from
        // BRepEdgeProvider's perspective) — the resolver should
        // reject it because n_a != n_b.
        let edges = loft.brep_edge_ids(owner());
        assert!(!edges.is_empty());
        let err = RoundFilletOp::new_for_loft(&loft, owner(), vec![edges[0]], 0.1).unwrap_err();
        assert!(matches!(
            err,
            RoundFilletError::UnsupportedEdgeGeometry { reason, .. }
            if reason.contains("profile_a.len() must equal profile_b.len()")
        ));
    }

    // -- Cap-perimeter + vertical-seam acceptance (full Loft upstream) -------

    /// Identity-profile Loft (profile_a == profile_b geometrically):
    /// equivalent to Extrude. All 3N edges should accept.
    #[test]
    fn new_for_loft_accepts_all_3n_edges_for_identity_profile_loft() {
        let loft = identity_loft();
        let edges = loft.brep_edge_ids(owner());
        assert_eq!(edges.len(), 12, "n=4 → 3*4=12 edges");
        let op = RoundFilletOp::new_for_loft(&loft, owner(), edges.clone(), 0.05)
            .expect("identity Loft full upstream layer");
        assert_eq!(op.edges().len(), 12);
    }

    /// **Load-bearing per user direction**: non-uniform translated
    /// Loft accepts ALL 3N edges (non-90° dihedrals on cap-perimeter
    /// AND vertical-seam). This proves sub-δ.revisit is doing real
    /// general-dihedral work, not the old 90° plateau pattern.
    #[test]
    fn new_for_loft_accepts_all_3n_edges_for_translated_loft() {
        let loft = translated_loft();
        let edges = loft.brep_edge_ids(owner());
        assert_eq!(edges.len(), 12);
        let op = RoundFilletOp::new_for_loft(&loft, owner(), edges.clone(), 0.05)
            .expect("translated Loft full upstream layer — proves NON-90° cap-side acceptance");
        assert_eq!(op.edges().len(), 12);
    }

    /// Twisted Loft (profile_b is profile_a rotated 45°): different
    /// non-uniform shape from translated case; still accepts all
    /// 3N edges.
    #[test]
    fn new_for_loft_accepts_all_3n_edges_for_twisted_loft() {
        let loft = twisted_loft();
        let edges = loft.brep_edge_ids(owner());
        assert_eq!(edges.len(), 12);
        let op = RoundFilletOp::new_for_loft(&loft, owner(), edges.clone(), 0.05)
            .expect("twisted Loft full upstream layer");
        assert_eq!(op.edges().len(), 12);
    }

    // -- Evaluation geometry (general-dihedral path proven end-to-end) -------

    /// **Load-bearing per user direction**: translated-Loft
    /// bottom-perimeter edge fillet runs `RoundFilletOp::evaluate`
    /// under the general-dihedral path with a NON-90° dihedral
    /// (45° between Bottom plane and Side(0)'s tilted plane —
    /// derived algebraically in the inspection-first report). Per-
    /// edge contribution matches sub-α/β/γ: +22v / +16t / +48i.
    /// This is the proof "this is no longer just the old 90 degree
    /// plateau wearing a clever hat".
    #[test]
    fn evaluate_translated_loft_bottom_perimeter_edge_produces_expected_counts_at_non_90_dihedral()
    {
        let loft = translated_loft();
        let edges = loft.brep_edge_ids(owner());
        // edges[0] is bottom-perimeter 0 between profile_a edges
        // (0,0)→(1,0); the incident Side(0)'s Tri 1 tilts toward
        // profile_b at (0,1)→(1,1) → 45° dihedral with Bottom.
        let op = RoundFilletOp::new_for_loft(&loft, owner(), vec![edges[0]], 0.1)
            .expect("translated Loft bottom-perimeter accept");
        let upstream = loft.evaluate(&[]).expect("translated loft tess");
        let out = op
            .evaluate(&[&upstream])
            .expect("evaluate general-dihedral");

        assert_eq!(out.vertex_count(), upstream.vertex_count() + 22);
        assert_eq!(out.triangle_count(), upstream.triangle_count() + 16);
        assert_eq!(out.indices.len(), upstream.indices.len() + 48);
    }

    /// Translated-Loft vertical-seam edge fillet — vertical seam
    /// runs from (1, 0, 0) on bottom to (1, 1, 1) on top, generally
    /// diagonal in 3D. Evaluate must accept under general-dihedral
    /// machinery with the same per-edge contribution.
    #[test]
    fn evaluate_translated_loft_vertical_seam_edge_produces_expected_counts() {
        let loft = translated_loft();
        let edges = loft.brep_edge_ids(owner());
        // edges[8] is vertical-seam 0 (canonical index 2*4 + 0 = 8).
        let op = RoundFilletOp::new_for_loft(&loft, owner(), vec![edges[8]], 0.1)
            .expect("translated Loft vertical-seam accept");
        let upstream = loft.evaluate(&[]).expect("loft tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate vertical-seam");

        assert_eq!(out.vertex_count(), upstream.vertex_count() + 22);
        assert_eq!(out.triangle_count(), upstream.triangle_count() + 16);
        assert_eq!(out.indices.len(), upstream.indices.len() + 48);
    }

    /// Identity-Loft (profile_a == profile_b) bottom-perimeter edge
    /// fillet — reduces to Extrude behavior. Same +22v/+16t/+48i.
    /// Regression sanity that the v0 triangle-incidence convention
    /// composes correctly with the sub-β.γ general-dihedral
    /// machinery in the perpendicular special case.
    #[test]
    fn evaluate_identity_loft_bottom_perimeter_edge_matches_extrude_behavior_at_90_dihedral() {
        let loft = identity_loft();
        let edges = loft.brep_edge_ids(owner());
        let op = RoundFilletOp::new_for_loft(&loft, owner(), vec![edges[0]], 0.1).expect("ok");
        let upstream = loft.evaluate(&[]).expect("loft tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert_eq!(out.vertex_count(), upstream.vertex_count() + 22);
        assert_eq!(out.triangle_count(), upstream.triangle_count() + 16);
        assert_eq!(out.indices.len(), upstream.indices.len() + 48);
    }

    // -- Resolver correctness ------------------------------------------------

    /// Resolved face IDs match canonical face-emission order for
    /// the translated (non-uniform) Loft:
    ///   bottom-perimeter i → (TopologyFaceId(0), TopologyFaceId(2+i))
    ///   top-perimeter i    → (TopologyFaceId(1), TopologyFaceId(2+i))
    ///   vertical-seam i    → (TopologyFaceId(2+i), TopologyFaceId(2+(i+1)%N))
    #[test]
    fn resolve_round_spec_face_ids_match_canonical_emission_order_for_translated_loft() {
        let loft = translated_loft();
        let n_u64 = loft.profile_a.len() as u64;

        // Bottom-perimeter 0 → Bottom ∩ Side(0).
        let spec = loft.resolve_round_spec(0).expect("bottom-perimeter 0");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(0));
        assert_eq!(spec.face_b_id, TopologyFaceId(2));

        // Bottom-perimeter 3 (last) → Bottom ∩ Side(3).
        let spec = loft.resolve_round_spec(3).expect("bottom-perimeter 3");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(0));
        assert_eq!(spec.face_b_id, TopologyFaceId(5));

        // Top-perimeter 0 (canonical 4) → Top ∩ Side(0).
        let spec = loft.resolve_round_spec(4).expect("top-perimeter 0");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(1));
        assert_eq!(spec.face_b_id, TopologyFaceId(2));

        // Top-perimeter 2 (canonical 6) → Top ∩ Side(2).
        let spec = loft.resolve_round_spec(6).expect("top-perimeter 2");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(1));
        assert_eq!(spec.face_b_id, TopologyFaceId(4));

        // Vertical-seam 0 (canonical 8) → Side(0) ∩ Side(1).
        let spec = loft.resolve_round_spec(8).expect("vertical-seam 0");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(2));
        assert_eq!(spec.face_b_id, TopologyFaceId(3));

        // Vertical-seam 3 (canonical 11) → Side(3) ∩ Side(0) (wraps).
        let spec = loft.resolve_round_spec(11).expect("vertical-seam 3");
        let spec = spec.expect_two_endpoint();
        assert_eq!(spec.face_a_id, TopologyFaceId(2 + 3));
        assert_eq!(spec.face_b_id, TopologyFaceId(2)); // wraps to Side(0)

        // Out-of-range rejects.
        let err = loft.resolve_round_spec(12).unwrap_err();
        assert!(err.contains("out of range"));

        let _ = n_u64; // silence unused-variable warning if compiled in isolation
    }

    /// Inward vectors are unit-length across all 3N edges for
    /// multiple Loft profile shapes. Pins the cross+sign-check
    /// algorithm's unit-length output invariant.
    ///
    /// Note: for non-uniform Loft, the inward vectors are NOT
    /// pairwise perpendicular in general (that's the whole point of
    /// general-dihedral support). We only assert unit length here;
    /// perpendicularity is the special case sub-α/β/γ enforced.
    #[test]
    fn resolve_round_spec_inward_vectors_unit_across_loft_variations() {
        let lofts = [
            ("identity", identity_loft()),
            ("translated", translated_loft()),
            ("twisted", twisted_loft()),
        ];

        for (label, loft) in lofts {
            let n = loft.profile_a.len();
            for idx in 0..(3 * n) {
                let spec = loft
                    .resolve_round_spec(idx)
                    .expect("loft full upstream resolves");
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
                    "{label} loft idx {idx}: face_a_inward not unit (len={len_a})"
                );
                assert!(
                    (len_b - 1.0).abs() < 1e-5,
                    "{label} loft idx {idx}: face_b_inward not unit (len={len_b})"
                );
            }
        }
    }

    /// 45° dihedral verification for the translated-Loft bottom-
    /// perimeter edge 0 (the algebraic example from sub-δ.revisit
    /// inspection report). Pins the load-bearing geometric claim
    /// that sub-δ.revisit produces specs with non-90° dihedrals
    /// — confirming the general-dihedral substrate is actually
    /// being exercised, not bypassed.
    #[test]
    fn resolve_round_spec_translated_loft_bottom_perimeter_0_yields_45_degree_dihedral() {
        let loft = translated_loft();
        let spec = loft.resolve_round_spec(0).expect("bottom-perimeter 0");
        let spec = spec.expect_two_endpoint();
        // a · b = cos(φ). For 45° interior dihedral: cos(45°) = 1/√2 ≈ 0.707.
        let dot_ab = spec.face_a_inward[0] * spec.face_b_inward[0]
            + spec.face_a_inward[1] * spec.face_b_inward[1]
            + spec.face_a_inward[2] * spec.face_b_inward[2];
        let expected_dot = std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (dot_ab - expected_dot).abs() < 1e-5,
            "translated Loft bottom-perimeter 0 dihedral: expected a·b ≈ {expected_dot} (45°), got {dot_ab}"
        );
    }

    // -- Face-strip localization (Loft-specific watch test) -----------------

    /// Load-bearing watch test: face-strip substitution for a
    /// translated-Loft bottom-perimeter edge must localize to the
    /// target Bottom + Side(0) triangles. Adjacent Side(3) and
    /// Side(1) faces share vertex_a/vertex_b at the bottom ring
    /// but are NOT in the target face set — their references must
    /// be preserved byte-identical. Mirrors sub-γ Revolve's
    /// `evaluate_face_strip_localization_no_leak_beyond_target_faces`
    /// test pattern, adapted for Loft.
    #[test]
    fn evaluate_face_strip_localization_no_leak_for_translated_loft_bottom_perimeter() {
        let loft = translated_loft();
        let edges = loft.brep_edge_ids(owner());
        let edge = edges[0]; // bottom-perimeter 0
        let op = RoundFilletOp::new_for_loft(&loft, owner(), vec![edge], 0.05).expect("ok");
        let upstream = loft.evaluate(&[]).expect("loft tess");
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        let labels = upstream.face_labels.as_ref().expect("Loft always labels");
        let face_a_id = TopologyFaceId(0); // Bottom
        let face_b_id = TopologyFaceId(2); // Side(0)
        let vertex_a = 0u32; // bottom-perimeter 0 vertex_a
        let vertex_b = 1u32; // bottom-perimeter 0 vertex_b

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
            "face-strip substitution LEAKED beyond target faces — {} slots changed unrelated to spec; \
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
