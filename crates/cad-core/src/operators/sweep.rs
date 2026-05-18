// SPLIT-EXEMPTION: cohesive SweepOp substrate — operator implementation,
// `BRepProvider` impl (face identity), `BRepEdgeProvider` impl (edge
// identity), and the unit tests that pin both impls' canonical-emission
// orders to evaluate's geometry. Splitting would require duplicating the
// `Polygon2D` + `Polyline3D` + `SweepOp` + `n_u32` cast invariants across
// files and would force the BRep impls to consume the operator through a
// public shim, breaking the "the operator owns its identity recipe"
// contract. Per PLAN.md §1.3 Rule 3 (1212 lines vs 1000-line hard cap;
// growth from ISSUE-19 `BRepEdgeProvider` impl + canonical-order edge
// unit tests; matches extrude.rs::SPLIT-EXEMPTION precedent at 1113 lines).

//! `SweepOp` — sweep a 2D convex polygon along a 3D polyline path to produce
//! a closed solid (arity 0).
//!
//! Failure class: snapshot-recoverable
//!
//! # Geometry
//!
//! [`SweepOp`] consumes a [`Polygon2D`] profile (in the XY plane) and a
//! [`Polyline3D`] path, producing a closed solid whose cross-section at each
//! path vertex is the profile rigidly translated to that vertex.
//!
//! For `n` profile points and `m` path points the produced mesh has `n * m`
//! vertices and `2 * n * (m - 1) + 2 * (n - 2)` triangles — generalises
//! [`crate::ExtrudeOp`] (Sweep with `m == 2` and a `+Z`-aligned path
//! produces an extrude-equivalent solid: `2n + 2(n - 2) = 4n - 4` triangles).
//!
//! * Ring `k` sits at `path[k]`; profile vertex `i` becomes
//!   `(path[k].x + profile[i].x, path[k].y + profile[i].y, path[k].z)`.
//! * End caps are fan-triangulated from vertex 0 of the first and last rings.
//! * Side walls are quad strips between consecutive rings, each split into
//!   two triangles via the diagonal that runs from `bot_i` to `top_{i+1}`.
//!
//! # Conventions
//!
//! * **Right-handed CCW winding** when viewed from outside the solid.
//! * **Outward normals** — the first-ring cap normal points in `-Z`; the
//!   last-ring cap normal points in `+Z`; side-wall normals point away from
//!   the polygon interior.
//! * **Profile winding is winding-agnostic from the caller's perspective**:
//!   the algorithm reads the signed area and reverses iteration order
//!   internally if the caller supplied a CW polygon, so the produced solid
//!   always has correct outward normals.
//!
//! # Restrictions (Phase 7 D-Sweep v0)
//!
//! * **Monotonic-Z path required.** Every consecutive path segment must
//!   strictly increase Z (`path[k + 1].z > path[k].z`). Non-monotonic-Z
//!   paths produce overlapping rings or backwards-facing solids and are
//!   rejected at `evaluate` time. This is the principal v0 restriction;
//!   it pins cap-orientation correctness without requiring path-tangent
//!   computation.
//! * **Profile is rigidly translated, not rotated.** The profile remains
//!   in the XY plane at every ring; the path-tangent direction does NOT
//!   rotate the profile. Paths with X / Y drift produce sheared but valid
//!   side walls; paths that turn sharply produce visibly non-perpendicular
//!   cross-sections (a v0 limitation, lifted by future
//!   rotation-minimizing-frame work).
//! * **Profile must be strictly convex** (validated at `evaluate` time via
//!   [`Polygon2D::convexity`]). Concave profiles produce inverted cap
//!   triangles under fan triangulation; rejected. Same restriction as
//!   [`crate::ExtrudeOp`] / [`crate::LoftOp`]; lifted by the same future
//!   earcut dispatch.
//! * **Open paths only.** Closed-loop paths (where `path.first() ==
//!   path.last()`) are rejected by [`Polyline3D::new`] because the closing
//!   segment would have zero length. Closed-loop sweep (torus-like
//!   geometry) is out of v0 scope.
//! * **No path-tangent perpendicular orientation.** No Frenet frames; no
//!   rotation-minimizing frames; no twist control. Out of v0 scope.
//! * **No variable scale along path.** The profile is the same shape at
//!   every ring. Tapered sweep is achieved by chaining downstream
//!   [`crate::TransformOp`] applications or by future variable-scale-sweep
//!   work.
//!
//! # Capability surface (per ADR-104)
//!
//! * `boolean_robust_under_tolerance`: true (no boolean op).
//! * `deterministic_triangulation`: true (fan from vertex 0; no
//!   float-comparison-dependent triangulation choice).
//! * `t_junction_handling`: true (closed solid has none).
//! * `concave_input_supported`: **false** — fan-triangulation produces
//!   inverted cap triangles on concave profiles; rejected at evaluate time.
//! * `arity`: 0 (profile and path are parameters, not upstream inputs).
//! * `output_labeled_when_input_labeled`: false (no inputs).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::operators::{OpError, OpKind, Operator, Polygon2D};
use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topology::{
    BRepEdgeId, BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider, SweepFaceTag,
};

// ---------------------------------------------------------------------------
// Polyline3DError
// ---------------------------------------------------------------------------

/// Errors produced by [`Polyline3D::new`] for malformed input.
///
/// These are construction-time errors. Domain errors (non-monotonic Z, etc.)
/// surface from [`SweepOp::evaluate`] as [`OpError::InvalidParameter`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Polyline3DError {
    /// Fewer than the minimum 2 points were supplied.
    #[error("polyline needs >= 2 points (got {got})")]
    TooFewPoints {
        /// The deficient point count.
        got: usize,
    },
    /// A coordinate was NaN or infinite.
    #[error("polyline contains non-finite coordinate at index {index}")]
    NonFiniteCoordinate {
        /// Position of the offending point in the input slice.
        index: usize,
    },
    /// Two adjacent points coincide (zero-length segment).
    #[error("polyline has coincident adjacent points at index {index}")]
    DegenerateSegment {
        /// Position of the second point of the offending segment.
        index: usize,
    },
}

// ---------------------------------------------------------------------------
// Polyline3D
// ---------------------------------------------------------------------------

/// Open 3D polyline path used as a sweep trajectory.
///
/// Construction enforces:
///
/// * `points.len() >= 2` (a path needs at least a start and an end).
/// * Every coordinate is finite.
/// * No two adjacent points coincide (no zero-length segments).
///
/// **Closed-loop paths** (where `points.first() == points.last()`) are
/// rejected by the coincident-adjacent check on the last → first segment
/// (which is implicit only for [`Polygon2D`]; [`Polyline3D`] is open by
/// construction).
///
/// **Monotonic-Z** is *not* enforced at construction time so a path can be
/// built up incrementally before being attached to a [`SweepOp`]. The sweep
/// operator validates monotonic-Z at `evaluate` time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Polyline3D {
    points: Vec<[f32; 3]>,
}

impl Polyline3D {
    /// Build a [`Polyline3D`] after validating point count, finiteness, and
    /// adjacent-point distinctness.
    ///
    /// # Errors
    ///
    /// * [`Polyline3DError::TooFewPoints`] if `points.len() < 2`.
    /// * [`Polyline3DError::NonFiniteCoordinate`] if any coordinate is NaN /
    ///   infinite.
    /// * [`Polyline3DError::DegenerateSegment`] if two adjacent points
    ///   coincide (zero-length segment).
    pub fn new(points: Vec<[f32; 3]>) -> Result<Self, Polyline3DError> {
        if points.len() < 2 {
            return Err(Polyline3DError::TooFewPoints { got: points.len() });
        }
        for (i, [x, y, z]) in points.iter().enumerate() {
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                return Err(Polyline3DError::NonFiniteCoordinate { index: i });
            }
        }
        // Adjacent-point distinctness. An open polyline has `points.len() - 1`
        // segments; the closing segment that [`Polygon2D`] validates is NOT
        // implicit here.
        for i in 0..points.len() - 1 {
            let a = points[i];
            let b = points[i + 1];
            if a[0].to_bits() == b[0].to_bits()
                && a[1].to_bits() == b[1].to_bits()
                && a[2].to_bits() == b[2].to_bits()
            {
                return Err(Polyline3DError::DegenerateSegment { index: i + 1 });
            }
        }
        Ok(Self { points })
    }

    /// Borrow the underlying point slice.
    #[must_use]
    pub fn points(&self) -> &[[f32; 3]] {
        &self.points
    }

    /// Number of points in the polyline.
    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Always `false` — [`Polyline3D::new`] guarantees `points.len() >= 2`.
    /// Provided for clippy-len-zero clarity.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

// ---------------------------------------------------------------------------
// SweepOp
// ---------------------------------------------------------------------------

/// Sweep a [`Polygon2D`] profile along a [`Polyline3D`] path to produce a
/// closed solid.
///
/// `path` must have at least 2 points; `path` Z-coordinates must be strictly
/// monotonically increasing (validated at [`SweepOp::evaluate`] time).
/// Profile invariants (point count, finiteness, convexity, signed area) are
/// re-checked at `evaluate` time so that intermediate graph states (where a
/// parameter may be momentarily corrupted while being edited) don't poison
/// construction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SweepOp {
    /// 2D profile rigidly translated to each path vertex.
    pub profile: Polygon2D,
    /// 3D polyline path. Z-coordinates must be strictly monotonically
    /// increasing (validated at evaluate time).
    pub path: Polyline3D,
}

impl SweepOp {
    /// Build a [`SweepOp`].
    ///
    /// All construction-time validation has already been performed by
    /// [`Polygon2D::new`] / [`Polyline3D::new`]; domain checks
    /// (monotonic-Z, convexity) are deferred to [`SweepOp::evaluate`].
    #[must_use]
    pub fn new(profile: Polygon2D, path: Polyline3D) -> Self {
        Self { profile, path }
    }
}

impl Operator for SweepOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Sweep
    }

    fn arity(&self) -> usize {
        0
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"sweep:");
        // try_from is infallible at any plausible profile/path size, but
        // using it satisfies clippy::cast_possible_truncation. Fall back to
        // u32::MAX for the unreachable >4G-point case.
        let n_profile = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        hasher.update(&n_profile.to_le_bytes());
        for [x, y] in self.profile.points() {
            hasher.update(&x.to_le_bytes());
            hasher.update(&y.to_le_bytes());
        }
        let n_path = u32::try_from(self.path.len()).unwrap_or(u32::MAX);
        hasher.update(&n_path.to_le_bytes());
        for [x, y, z] in self.path.points() {
            hasher.update(&x.to_le_bytes());
            hasher.update(&y.to_le_bytes());
            hasher.update(&z.to_le_bytes());
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

        // Re-validate path defensively (the field is `pub` and may have been
        // mutated post-construction).
        if self.path.len() < 2 {
            return Err(OpError::InvalidParameter(format!(
                "sweep path needs >= 2 points (got {})",
                self.path.len()
            )));
        }
        for (i, [x, y, z]) in self.path.points().iter().enumerate() {
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "sweep path has non-finite coordinate at index {i}"
                )));
            }
        }

        // Monotonic-Z gate. The principal v0 restriction; ensures cap-
        // orientation correctness without path-tangent computation.
        for k in 0..self.path.len() - 1 {
            let z0 = self.path.points()[k][2];
            let z1 = self.path.points()[k + 1][2];
            if !(z1 > z0) {
                return Err(OpError::InvalidParameter(format!(
                    "sweep path must be strictly monotonic in Z (segment {k}: z0={z0}, z1={z1})"
                )));
            }
        }

        // Re-validate profile invariants.
        if self.profile.len() < 3 {
            return Err(OpError::InvalidParameter(format!(
                "sweep profile needs >= 3 points (got {})",
                self.profile.len()
            )));
        }
        for (i, [x, y]) in self.profile.points().iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "sweep profile has non-finite coordinate at index {i}"
                )));
            }
        }

        // Convexity gate.
        match self.profile.convexity() {
            Some(true) => {}
            Some(false) => {
                return Err(OpError::InvalidParameter(
                    "sweep profile must be strictly convex".to_string(),
                ));
            }
            None => {
                return Err(OpError::InvalidParameter(
                    "sweep profile is degenerate (all points collinear)".to_string(),
                ));
            }
        }

        // Winding correction: signed_area > 0 → CCW (canonical); < 0 → CW
        // (reverse iteration order); near-zero → reject.
        let signed_area = self.profile.signed_area();
        if signed_area.abs() < 1e-12_f32 {
            return Err(OpError::InvalidParameter(
                "sweep profile is degenerate (near-zero area)".to_string(),
            ));
        }

        let n = self.profile.len();
        let m = self.path.len();
        let ordered_profile: Vec<[f32; 2]> = if signed_area > 0.0 {
            self.profile.points().to_vec()
        } else {
            self.profile.points().iter().rev().copied().collect()
        };

        // Build vertex buffer: m rings of n vertices each. Ring k holds the
        // profile rigidly translated to path[k].
        let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * m);
        for [px, py, pz] in self.path.points() {
            for [x, y] in &ordered_profile {
                positions.push([px + x, py + y, *pz]);
            }
        }

        let n_u32 = u32::try_from(n).map_err(|_| {
            OpError::InvalidParameter(format!("sweep profile too large: {n} points"))
        })?;
        let m_u32 = u32::try_from(m)
            .map_err(|_| OpError::InvalidParameter(format!("sweep path too large: {m} points")))?;

        // Index buffer:
        //   first cap : n - 2 triangles  (-Z normal)
        //   last cap  : n - 2 triangles  (+Z normal)
        //   sides     : 2 * n * (m - 1) triangles
        //   total     : 2 * n * (m - 1) + 2 * (n - 2)
        let cap_tris = 2 * (n - 2);
        let side_tris = 2 * n * (m - 1);
        let mut indices: Vec<u32> = Vec::with_capacity(3 * (cap_tris + side_tris));

        // First cap (ring 0) — outward normal -Z. The ordered ring is CCW
        // when viewed from +Z; for a -Z-facing triangle we need CCW winding
        // when viewed from -Z, i.e. (0, i+1, i) — the reverse of projected
        // CCW.
        for i in 1..(n_u32 - 1) {
            indices.push(0);
            indices.push(i + 1);
            indices.push(i);
        }

        // Last cap (ring m-1) — outward normal +Z. The ordered ring is CCW
        // from +Z, so (offset, offset+i, offset+i+1) is CCW from +Z = correct
        // outward facing.
        let last_ring_offset = (m_u32 - 1) * n_u32;
        for i in 1..(n_u32 - 1) {
            indices.push(last_ring_offset);
            indices.push(last_ring_offset + i);
            indices.push(last_ring_offset + i + 1);
        }

        // Side walls. For each path segment [k, k+1] and each polygon edge
        // (i, i+1), generate the quad (bot_i, bot_{i+1}, top_{i+1}, top_i).
        // With CCW polygon ordering the outward normal of each side face
        // points away from the polygon interior.
        for k in 0..(m_u32 - 1) {
            let bot_offset = k * n_u32;
            let top_offset = (k + 1) * n_u32;
            for i in 0..n_u32 {
                let i1 = (i + 1) % n_u32;
                let bot_i = bot_offset + i;
                let bot_i1 = bot_offset + i1;
                let top_i = top_offset + i;
                let top_i1 = top_offset + i1;
                // Quad split via diagonal (bot_i, top_i1):
                indices.push(bot_i);
                indices.push(bot_i1);
                indices.push(top_i1);
                indices.push(bot_i);
                indices.push(top_i1);
                indices.push(top_i);
            }
        }

        // Per-triangle face labels in the canonical emission order that
        // mirrors the index-buffer construction above:
        //
        //   * First cap  — `n - 2` triangles, all `TopologyFaceId(0)`.
        //   * Last cap   — `n - 2` triangles, all `TopologyFaceId(1)`.
        //   * Side(k, i) — for emitted path segment `k in 0..m-1` and
        //     profile edge `i in 0..n`, the 2 triangles of that side quad
        //     are both labeled `TopologyFaceId(2 + (k * n + i))`.
        //
        // Total `face_labels.len() == 2 * (n - 2) + 2 * n * (m - 1)`,
        // matching `triangle_count`. This follows the `ExtrudeOp` /
        // `LoftOp` cap-`0` / cap-`1` / side substrate pattern, extended so
        // side identity advances by emitted path segment before profile
        // edge (segment-major, edge-major) across the `m - 1` segments.
        let mut face_labels: Vec<TopologyFaceId> = Vec::with_capacity(cap_tris + side_tris);
        // First cap: n-2 triangles all labeled TopologyFaceId(0).
        for _ in 0..(n - 2) {
            face_labels.push(TopologyFaceId(0));
        }
        // Last cap: n-2 triangles all labeled TopologyFaceId(1).
        for _ in 0..(n - 2) {
            face_labels.push(TopologyFaceId(1));
        }
        // Side walls: 2 triangles per side quad, in the same segment-major
        // / edge-major order the side index buffer was emitted.
        for k in 0..(m - 1) {
            for i in 0..n {
                let side_label = TopologyFaceId(2 + (k * n + i) as u64);
                face_labels.push(side_label);
                face_labels.push(side_label);
            }
        }

        Tessellation::with_labels(positions, indices, face_labels).map_err(|e| {
            OpError::InvalidParameter(format!("sweep produced invalid tessellation: {e}"))
        })
    }

    /// Override the default `inputs_labeled.iter().any(...)` because
    /// [`Self::evaluate`] ALWAYS emits a labeled `Tessellation` — irrespective
    /// of input labeling (`SweepOp` has arity 0, so the input slice is always
    /// empty anyway). The cache-key contract is "`output_is_labeled` MUST
    /// match the actual `evaluate` output's [`Tessellation::is_labeled`]";
    /// `evaluate` now emits canonical per-triangle `TopologyFaceId` labels,
    /// so this override returns `true` unconditionally to match.
    fn output_is_labeled(&self, _inputs_labeled: &[bool]) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// BRepProvider — Sweep face-identity slice B-Rep face identity for SweepOp
// ---------------------------------------------------------------------------

/// Pair the `2 + n * (m - 1)` sequential per-tessellation `TopologyFaceId`s
/// with rebuild-stable `BRepFaceId`s seeded from the caller-supplied
/// [`BRepOwnerId`].
///
/// The pair order exactly follows the canonical face-label contract of
/// [`SweepOp::evaluate`] — see the `face_labels` construction there:
///
/// * `TopologyFaceId(0)` → [`SweepFaceTag::FirstCap`] (ring 0, `-Z` cap)
/// * `TopologyFaceId(1)` → [`SweepFaceTag::LastCap`] (ring `m - 1`, `+Z`
///   cap)
/// * `TopologyFaceId(2 + segment_index * n + edge_index)` →
///   [`SweepFaceTag::Side`] for every emitted path segment
///   `segment_index in 0..(m - 1)` and profile edge `edge_index in 0..n`,
///   advancing in segment-major, profile-edge-major order.
///
/// This is a topology-only restatement of `evaluate`'s label order — it
/// does NOT change geometry, validation, `structural_hash`, triangle
/// emission, or the existing face-label ordering. The provider works from
/// the raw `profile` / `path` point counts (it does not call `evaluate`),
/// so it returns IDs even for a `SweepOp` whose `evaluate` would reject the
/// profile or path; this mirrors the [`crate::LoftOp`] provider precedent.
///
/// Each `Side` carries `profile_count` (`n`) AND `path_segment_count`
/// (`m - 1`), so a profile-count change OR a path-segment-count change
/// breaks side identity by construction. See [`SweepFaceTag`] for the full
/// stability contract.
impl BRepProvider for SweepOp {
    fn brep_face_ids(&self, owner: BRepOwnerId) -> Vec<(TopologyFaceId, BRepFaceId)> {
        // Mirror the `n_u32` / `m_u32` cast pattern in `evaluate` (and the
        // `structural_hash` precedent). Saturating to `u32::MAX` for the
        // unreachable >4G-point case matches Extrude / Revolve / Loft;
        // `Tessellation::new` would have rejected long before.
        let n = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        let m = u32::try_from(self.path.len()).unwrap_or(u32::MAX);
        // `path_segment_count = m - 1`. `Polyline3D::new` guarantees
        // `m >= 2`, but the `path` field is publicly assignable, so
        // `saturating_sub` defends a hypothetical degenerate post-mutation
        // state without introducing a panic surface.
        let path_segment_count = m.saturating_sub(1);
        let side_count = u64::from(n).saturating_mul(u64::from(path_segment_count));
        let total = side_count.saturating_add(2);
        let mut ids: Vec<(TopologyFaceId, BRepFaceId)> = Vec::with_capacity(total as usize);
        ids.push((
            TopologyFaceId(0),
            BRepFaceId::for_sweep_face(owner, SweepFaceTag::FirstCap),
        ));
        ids.push((
            TopologyFaceId(1),
            BRepFaceId::for_sweep_face(owner, SweepFaceTag::LastCap),
        ));
        for segment_index in 0..path_segment_count {
            for edge_index in 0..n {
                let ordinal = u64::from(segment_index) * u64::from(n) + u64::from(edge_index);
                ids.push((
                    TopologyFaceId(2 + ordinal),
                    BRepFaceId::for_sweep_face(
                        owner,
                        SweepFaceTag::Side {
                            segment_index,
                            edge_index,
                            profile_count: n,
                            path_segment_count,
                        },
                    ),
                ));
            }
        }
        ids
    }
}

// ---------------------------------------------------------------------------
// BRepEdgeProvider — Sweep edge identity derived from the face-ID substrate
// ---------------------------------------------------------------------------

/// Mint the `n * (2 * s + 1)` stable B-Rep edge identities for a swept
/// solid, where `n = profile.len()` and `s = path.len() - 1` is the
/// emitted path-segment count.
///
/// Every edge is derived purely from the canonical Sweep face IDs of the
/// [`BRepProvider`] impl above via [`BRepEdgeId::for_face_pair`] — an edge
/// IS the topological intersection of the two faces it bounds, so its
/// identity composes from those faces' IDs with no coordinate input. This
/// reuses the established Sweep face-ID substrate rather than duplicating
/// face-tag construction. Every edge uses `local_ordinal = 0` (a Sweep
/// face pair shares at most one edge).
///
/// The `brep_face_ids` emission order this method indexes into is:
///
/// * `face_ids[0]` — [`SweepFaceTag::FirstCap`].
/// * `face_ids[1]` — [`SweepFaceTag::LastCap`].
/// * `face_ids[2 + segment_index * n + edge_index]` —
///   [`SweepFaceTag::Side`], segment-major / profile-edge-major.
///
/// Edges are emitted in this canonical order:
///
/// 1. **First-cap perimeter** — `n` edges, `FirstCap ∩ Side(0, i)` for
///    `i in 0..n`.
/// 2. **Last-cap perimeter** — `n` edges, `LastCap ∩ Side(s - 1, i)` for
///    `i in 0..n`.
/// 3. **Segment side seams** — `n * s` edges, `Side(k, i) ∩
///    Side(k, (i + 1) % n)` for each path segment `k in 0..s`.
/// 4. **Interior ring edges** — `n * (s - 1)` edges, `Side(k, i) ∩
///    Side(k + 1, i)` for each adjacent segment pair `k in 0..s - 1`.
///
/// Mirroring the [`BRepProvider`] impl's defensive
/// `path_segment_count = m.saturating_sub(1)` posture: `path` is a `pub`
/// field, so a degenerate post-mutation state must not panic during edge
/// indexing. When `s == 0` this returns an empty edge list rather than
/// indexing nonexistent side faces.
impl BRepEdgeProvider for SweepOp {
    fn brep_edge_ids(&self, owner: BRepOwnerId) -> Vec<BRepEdgeId> {
        // Anchor every edge to the canonical Sweep face-ID substrate —
        // `brep_face_ids` already mirrors `SweepOp::evaluate`'s face-label
        // contract, so deriving edges from its output keeps face and edge
        // identity composed by construction with no duplicated face-tag
        // logic.
        let face_ids: Vec<BRepFaceId> = self
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let n = self.profile.len();
        // `s = path_segment_count = m - 1`. `Polyline3D::new` guarantees
        // `m >= 2`, but the `path` field is publicly assignable, so
        // `saturating_sub` defends a hypothetical degenerate post-mutation
        // state. `s == 0` would leave no side faces to index, so return an
        // empty edge list rather than indexing nonexistent faces.
        let s = self.path.len().saturating_sub(1);
        if s == 0 {
            return Vec::new();
        }
        // `Side(segment k, profile edge i)` lives at `face_ids[2 + k*n + i]`.
        let side = |k: usize, i: usize| -> BRepFaceId { face_ids[2 + k * n + i] };
        // Edge total: `n * (2 * s + 1)` — saturating only to keep the
        // capacity hint panic-free under the same degenerate-state posture.
        let total = n.saturating_mul(s.saturating_mul(2).saturating_add(1));
        let mut edges: Vec<BRepEdgeId> = Vec::with_capacity(total);

        // First-cap perimeter — `n` edges: FirstCap ∩ Side(0, i).
        for i in 0..n {
            edges.push(BRepEdgeId::for_face_pair(face_ids[0], side(0, i), 0));
        }
        // Last-cap perimeter — `n` edges: LastCap ∩ Side(s - 1, i).
        for i in 0..n {
            edges.push(BRepEdgeId::for_face_pair(face_ids[1], side(s - 1, i), 0));
        }
        // Segment side seams — `n * s` edges: Side(k, i) ∩ Side(k, i+1).
        for k in 0..s {
            for i in 0..n {
                let next = (i + 1) % n;
                edges.push(BRepEdgeId::for_face_pair(side(k, i), side(k, next), 0));
            }
        }
        // Interior ring edges — `n * (s - 1)` edges: Side(k, i) ∩
        // Side(k + 1, i) for each adjacent segment pair.
        for k in 0..s - 1 {
            for i in 0..n {
                edges.push(BRepEdgeId::for_face_pair(side(k, i), side(k + 1, i), 0));
            }
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

    fn unit_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("unit square profile")
    }

    fn unit_triangle() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("unit triangle profile")
    }

    fn z_path(zs: &[f32]) -> Polyline3D {
        Polyline3D::new(zs.iter().map(|z| [0.0, 0.0, *z]).collect()).expect("z-axis path")
    }

    // ----- Polyline3D construction -----

    #[test]
    fn polyline_new_rejects_too_few_points() {
        let err = Polyline3D::new(vec![[0.0, 0.0, 0.0]]).expect_err("too few");
        assert_eq!(err, Polyline3DError::TooFewPoints { got: 1 });
    }

    #[test]
    fn polyline_new_rejects_non_finite_coordinate() {
        let err =
            Polyline3D::new(vec![[0.0, 0.0, 0.0], [f32::NAN, 0.0, 1.0]]).expect_err("nan rejected");
        assert_eq!(err, Polyline3DError::NonFiniteCoordinate { index: 1 });
    }

    #[test]
    fn polyline_new_rejects_coincident_adjacent_points() {
        let err = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]]).expect_err("zero-length");
        assert_eq!(err, Polyline3DError::DegenerateSegment { index: 1 });
    }

    #[test]
    fn polyline_round_trip_via_points() {
        let pts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]];
        let p = Polyline3D::new(pts.clone()).expect("3-point z path");
        assert_eq!(p.points(), &pts[..]);
        assert_eq!(p.len(), 3);
        assert!(!p.is_empty());
    }

    // ----- SweepOp construction + arity -----

    #[test]
    fn sweep_op_new_accepts_valid_inputs() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert_eq!(op.op_kind(), OpKind::Sweep);
        assert_eq!(op.arity(), 0);
    }

    #[test]
    fn sweep_rejects_inputs_for_arity_0() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let dummy = Tessellation::new(vec![[0.0, 0.0, 0.0]], vec![]).expect("empty tess");
        let err = op.evaluate(&[&dummy]).expect_err("arity mismatch");
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 0,
                got: 1
            }
        ));
    }

    // ----- SweepOp validation -----

    #[test]
    fn sweep_rejects_concave_profile() {
        // Concave L-shape.
        let concave = Polygon2D::new(vec![
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 1.0],
            [1.0, 1.0],
            [1.0, 2.0],
            [0.0, 2.0],
        ])
        .expect("concave polygon constructs");
        let op = SweepOp::new(concave, z_path(&[0.0, 1.0]));
        let err = op.evaluate(&[]).expect_err("concave rejected");
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("convex"), "got: {msg}");
            }
            _ => panic!("unexpected: {err:?}"),
        }
    }

    #[test]
    fn sweep_rejects_non_monotonic_z_path() {
        // z goes 0 → 1 → 0.5 (drops in the second segment).
        let path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.5]])
            .expect("path constructs");
        let op = SweepOp::new(unit_square(), path);
        let err = op.evaluate(&[]).expect_err("non-monotonic Z rejected");
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("monotonic"), "got: {msg}");
            }
            _ => panic!("unexpected: {err:?}"),
        }
    }

    #[test]
    fn sweep_rejects_zero_z_segment() {
        // z stays the same across a segment.
        let path =
            Polyline3D::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]).expect("path constructs");
        let op = SweepOp::new(unit_square(), path);
        let err = op.evaluate(&[]).expect_err("zero-z segment rejected");
        match err {
            OpError::InvalidParameter(msg) => {
                assert!(msg.contains("monotonic"), "got: {msg}");
            }
            _ => panic!("unexpected: {err:?}"),
        }
    }

    // ----- SweepOp geometry -----

    #[test]
    fn sweep_square_along_2_point_z_path_yields_8_verts_12_tris() {
        // n=4, m=2 → 8 vertices, 4n-4 = 12 triangles, 36 indices.
        // Equivalent to ExtrudeOp(unit_square, length=1).
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn sweep_triangle_along_3_point_z_path_yields_9_verts_14_tris() {
        // n=3, m=3 → 9 vertices, 2*3*2 + 2*1 = 14 triangles, 42 indices.
        let op = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0, 2.0]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 9);
        assert_eq!(mesh.triangle_count(), 14);
        assert_eq!(mesh.indices.len(), 42);
    }

    #[test]
    fn sweep_square_along_4_point_z_path_yields_16_verts_28_tris() {
        // n=4, m=4 → 16 vertices, 2*4*3 + 2*2 = 28 triangles, 84 indices.
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0, 3.0]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 16);
        assert_eq!(mesh.triangle_count(), 28);
        assert_eq!(mesh.indices.len(), 84);
    }

    #[test]
    fn sweep_with_xy_drift_in_path_still_valid() {
        // Stair-step path: x changes between segments. Sheared but valid
        // sweep (rigid profile translation; monotonic-Z preserved). n=4,
        // m=3 → 12 vertices, 2*4*2 + 2*2 = 20 triangles, 60 indices.
        let path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 2.0]])
            .expect("stair-step path constructs");
        let op = SweepOp::new(unit_square(), path);
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert_eq!(mesh.vertex_count(), 12);
        assert_eq!(mesh.triangle_count(), 20);
    }

    #[test]
    fn sweep_cw_profile_auto_flipped() {
        // Same square but CW. Algorithm reads signed_area and reverses
        // iteration order so the output solid is identical to the CCW input.
        let cw_square = Polygon2D::new(vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]])
            .expect("cw square constructs");
        let op_cw = SweepOp::new(cw_square, z_path(&[0.0, 1.0]));
        let op_ccw = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let mesh_cw = op_cw.evaluate(&[]).expect("cw evaluate");
        let mesh_ccw = op_ccw.evaluate(&[]).expect("ccw evaluate");
        assert_eq!(mesh_cw.vertex_count(), mesh_ccw.vertex_count());
        assert_eq!(mesh_cw.triangle_count(), mesh_ccw.triangle_count());
    }

    #[test]
    fn sweep_top_ring_z_equals_last_path_z() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.5]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        // Last-ring offset = (m-1) * n = 2 * 4 = 8. Vertices 8..12 should be
        // at z=2.5.
        for v in &mesh.positions[8..12] {
            assert!((v[2] - 2.5).abs() < 1e-6, "expected z=2.5; got {}", v[2]);
        }
        // First-ring vertices 0..4 at z=0.
        for v in &mesh.positions[0..4] {
            assert!(v[2].abs() < 1e-6, "expected z=0; got {}", v[2]);
        }
    }

    // ----- structural_hash -----

    #[test]
    fn sweep_structural_hash_deterministic() {
        let op_a = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let op_b = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert_eq!(op_a.structural_hash(), op_b.structural_hash());
    }

    #[test]
    fn sweep_structural_hash_changes_with_path() {
        let op_a = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let op_b = SweepOp::new(unit_square(), z_path(&[0.0, 2.0]));
        assert_ne!(op_a.structural_hash(), op_b.structural_hash());
    }

    #[test]
    fn sweep_structural_hash_changes_with_profile() {
        let op_a = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let op_b = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0]));
        assert_ne!(op_a.structural_hash(), op_b.structural_hash());
    }

    #[test]
    fn sweep_structural_hash_changes_with_path_segment_count() {
        // Same start + end but different segment count: hash must differ.
        let op_a = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let op_b = SweepOp::new(unit_square(), z_path(&[0.0, 0.5, 1.0]));
        assert_ne!(op_a.structural_hash(), op_b.structural_hash());
    }

    #[test]
    fn sweep_op_kind_is_sweep() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert_eq!(op.op_kind(), OpKind::Sweep);
    }

    #[test]
    fn sweep_output_is_labeled_returns_true() {
        // `SweepOp::evaluate` ALWAYS emits a labeled `Tessellation`, so the
        // `output_is_labeled` override returns `true` unconditionally to
        // keep the cache-key contract consistent with `is_labeled()`.
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert!(op.output_is_labeled(&[]));
    }

    // ----- SweepOp face labels -----

    /// `SweepOp::evaluate` emits a labeled `Tessellation` for a 2-point
    /// path: first-cap triangles are `TopologyFaceId(0)`, last-cap
    /// triangles `TopologyFaceId(1)`, then side quads in profile-edge
    /// order labeled `TopologyFaceId(2 + i)`. For a square profile
    /// (`n = 4`, `m = 2`) the canonical order is two `0` triangles, two
    /// `1` triangles, then four side quads `2, 3, 4, 5` (2 triangles
    /// each), totalling 12 labels = `triangle_count`.
    #[test]
    fn sweep_emits_canonical_face_labels_for_2_point_path() {
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert!(mesh.is_labeled(), "sweep output is labeled");
        let labels = mesh.face_labels().expect("labeled");
        assert_eq!(
            labels.len(),
            mesh.triangle_count(),
            "one label per triangle"
        );
        assert_eq!(labels.len(), 12, "n=4, m=2 → 4n-4 = 12 triangles");

        // First cap: 2 triangles all TopologyFaceId(0).
        assert_eq!(labels[0], TopologyFaceId(0), "tri 0 is first cap");
        assert_eq!(labels[1], TopologyFaceId(0), "tri 1 is first cap");
        // Last cap: 2 triangles all TopologyFaceId(1).
        assert_eq!(labels[2], TopologyFaceId(1), "tri 2 is last cap");
        assert_eq!(labels[3], TopologyFaceId(1), "tri 3 is last cap");
        // Side quads: 2 triangles each, edge i → TopologyFaceId(2 + i).
        for i in 0..4u64 {
            let tri_a = 4 + (i as usize) * 2;
            let tri_b = tri_a + 1;
            assert_eq!(
                labels[tri_a],
                TopologyFaceId(2 + i),
                "side tri {tri_a} is face {}",
                2 + i
            );
            assert_eq!(
                labels[tri_b],
                TopologyFaceId(2 + i),
                "side tri {tri_b} is face {}",
                2 + i
            );
        }
    }

    /// For a multi-segment path the side labels advance by emitted path
    /// segment before profile edge (segment-major, edge-major). With a
    /// triangle profile (`n = 3`) and a 3-point path (`m = 3`, two
    /// segments), the side quads of segment 0 are `2, 3, 4` and of
    /// segment 1 are `5, 6, 7`, each with 2 triangles, following the
    /// `2 + (k * n + i)` rule. Caps remain `0` and `1`.
    #[test]
    fn sweep_emits_canonical_face_labels_for_multi_segment_path() {
        let op = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0, 2.0]));
        let mesh = op.evaluate(&[]).expect("evaluate");
        assert!(mesh.is_labeled(), "sweep output is labeled");
        let labels = mesh.face_labels().expect("labeled");
        assert_eq!(
            labels.len(),
            mesh.triangle_count(),
            "one label per triangle"
        );
        // n=3, m=3 → 2*(n-2) + 2*n*(m-1) = 2 + 12 = 14 triangles.
        assert_eq!(labels.len(), 14, "n=3, m=3 → 14 triangles");

        // First cap: n-2 = 1 triangle TopologyFaceId(0).
        assert_eq!(labels[0], TopologyFaceId(0), "tri 0 is first cap");
        // Last cap: n-2 = 1 triangle TopologyFaceId(1).
        assert_eq!(labels[1], TopologyFaceId(1), "tri 1 is last cap");

        // Side quads: segment-major, edge-major. For segment k and edge i,
        // label is TopologyFaceId(2 + (k * 3 + i)). Two triangles each.
        let n = 3u64;
        for k in 0..2u64 {
            for i in 0..n {
                let quad_ordinal = (k * n + i) as usize;
                let tri_a = 2 + quad_ordinal * 2;
                let tri_b = tri_a + 1;
                let expected = TopologyFaceId(2 + k * n + i);
                assert_eq!(labels[tri_a], expected, "segment {k} edge {i} tri {tri_a}");
                assert_eq!(labels[tri_b], expected, "segment {k} edge {i} tri {tri_b}");
            }
        }
        // Spot-check the segment-before-edge ordering explicitly: segment 0
        // is faces 2,3,4 and segment 1 is faces 5,6,7.
        assert_eq!(labels[2], TopologyFaceId(2), "seg0 edge0");
        assert_eq!(labels[4], TopologyFaceId(3), "seg0 edge1");
        assert_eq!(labels[6], TopologyFaceId(4), "seg0 edge2");
        assert_eq!(labels[8], TopologyFaceId(5), "seg1 edge0");
        assert_eq!(labels[10], TopologyFaceId(6), "seg1 edge1");
        assert_eq!(labels[12], TopologyFaceId(7), "seg1 edge2");
    }

    // ----- SweepOp BRepProvider (Sweep face-identity slice) -----

    /// `brep_face_ids` returns exactly `2 + n * (m - 1)` pairs for a square
    /// profile (`n = 4`) over a 2-point path (`m = 2`) and a triangle
    /// profile (`n = 3`) over a 3-point path (`m = 3`).
    #[test]
    fn sweep_brep_face_ids_count_is_2_plus_n_times_segments() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        // n=4, m=2 → 2 + 4 * 1 = 6.
        let square_2 = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert_eq!(square_2.brep_face_ids(owner).len(), 6);
        // n=3, m=3 → 2 + 3 * 2 = 8.
        let triangle_3 = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0, 2.0]));
        assert_eq!(triangle_3.brep_face_ids(owner).len(), 8);
        // n=4, m=4 → 2 + 4 * 3 = 14.
        let square_4 = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0, 3.0]));
        assert_eq!(square_4.brep_face_ids(owner).len(), 14);
    }

    /// `brep_face_ids` pair order exactly follows `SweepOp::evaluate`'s
    /// canonical face-label contract: `TopologyFaceId(0)` first cap,
    /// `TopologyFaceId(1)` last cap, then sides in segment-major,
    /// profile-edge-major order with the matching `SweepFaceTag`.
    #[test]
    fn sweep_brep_face_ids_follow_canonical_order() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        // Triangle over a 3-point path: n=3, m=3 → 2 segments.
        let op = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0, 2.0]));
        let pairs = op.brep_face_ids(owner);
        assert_eq!(pairs.len(), 8);

        // Caps.
        assert_eq!(pairs[0].0, TopologyFaceId(0));
        assert_eq!(
            pairs[0].1,
            BRepFaceId::for_sweep_face(owner, SweepFaceTag::FirstCap)
        );
        assert_eq!(pairs[1].0, TopologyFaceId(1));
        assert_eq!(
            pairs[1].1,
            BRepFaceId::for_sweep_face(owner, SweepFaceTag::LastCap)
        );

        // Sides: segment-major, profile-edge-major. n=3, path_segment_count=2.
        let n = 3u32;
        for segment_index in 0..2u32 {
            for edge_index in 0..n {
                let ordinal = u64::from(segment_index) * u64::from(n) + u64::from(edge_index);
                let pair = pairs[2 + ordinal as usize];
                assert_eq!(pair.0, TopologyFaceId(2 + ordinal));
                assert_eq!(
                    pair.1,
                    BRepFaceId::for_sweep_face(
                        owner,
                        SweepFaceTag::Side {
                            segment_index,
                            edge_index,
                            profile_count: n,
                            path_segment_count: 2,
                        },
                    )
                );
            }
        }
    }

    /// `brep_face_ids` is deterministic — repeated calls for the same
    /// `(SweepOp, owner)` return byte-identical IDs in byte-identical order.
    #[test]
    fn sweep_brep_face_ids_repeated_calls_byte_identical() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0]));
        assert_eq!(op.brep_face_ids(owner), op.brep_face_ids(owner));
    }

    // ----- SweepOp BRepEdgeProvider (Sweep edge-identity slice) -----

    /// `brep_edge_ids` returns exactly `n * (2 * s + 1)` edge IDs, where
    /// `n = profile.len()` and `s = path.len() - 1` is the emitted
    /// path-segment count.
    #[test]
    fn sweep_brep_edge_ids_count_is_n_times_2s_plus_1() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        // n=4, s=1 → 4 * (2 * 1 + 1) = 12.
        let square_2 = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        assert_eq!(square_2.brep_edge_ids(owner).len(), 12);
        // n=3, s=2 → 3 * (2 * 2 + 1) = 15.
        let triangle_3 = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0, 2.0]));
        assert_eq!(triangle_3.brep_edge_ids(owner).len(), 15);
        // n=4, s=3 → 4 * (2 * 3 + 1) = 28.
        let square_4 = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0, 3.0]));
        assert_eq!(square_4.brep_edge_ids(owner).len(), 28);
    }

    /// `brep_edge_ids` for a square over a 2-point path (`n = 4`,
    /// `s = 1`) emits the canonical order: first-cap perimeter, last-cap
    /// perimeter, then the single segment's side seams (no interior ring
    /// edges exist for a one-segment path). Expected face pairs are built
    /// from `brep_face_ids` itself so the edge provider stays anchored to
    /// the Sweep face-ID substrate.
    #[test]
    fn sweep_brep_edge_ids_follow_canonical_order_2_point_path() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        let faces: Vec<BRepFaceId> = op
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let edges = op.brep_edge_ids(owner);
        assert_eq!(edges.len(), 12);

        let n = 4usize;
        let side = |k: usize, i: usize| faces[2 + k * n + i];
        let mut expected: Vec<BRepEdgeId> = Vec::new();
        // First-cap perimeter: FirstCap ∩ Side(0, i).
        for i in 0..n {
            expected.push(BRepEdgeId::for_face_pair(faces[0], side(0, i), 0));
        }
        // Last-cap perimeter: LastCap ∩ Side(s - 1, i) — s - 1 == 0.
        for i in 0..n {
            expected.push(BRepEdgeId::for_face_pair(faces[1], side(0, i), 0));
        }
        // Segment side seams (k == 0): Side(0, i) ∩ Side(0, (i + 1) % n).
        for i in 0..n {
            expected.push(BRepEdgeId::for_face_pair(
                side(0, i),
                side(0, (i + 1) % n),
                0,
            ));
        }
        assert_eq!(edges, expected);
    }

    /// `brep_edge_ids` for a triangle over a 3-point path (`n = 3`,
    /// `s = 2`) emits 15 edges in canonical order, and pins one
    /// first-cap edge, one last-cap edge, one segment side seam from each
    /// of the two segments, and one interior ring edge — all built from
    /// `brep_face_ids` output.
    #[test]
    fn sweep_brep_edge_ids_follow_canonical_order_multi_segment_path() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        let op = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0, 2.0]));
        let faces: Vec<BRepFaceId> = op
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let edges = op.brep_edge_ids(owner);
        assert_eq!(edges.len(), 15);

        let n = 3usize;
        let s = 2usize;
        let side = |k: usize, i: usize| faces[2 + k * n + i];

        // Full expected vector in canonical order.
        let mut expected: Vec<BRepEdgeId> = Vec::new();
        for i in 0..n {
            expected.push(BRepEdgeId::for_face_pair(faces[0], side(0, i), 0));
        }
        for i in 0..n {
            expected.push(BRepEdgeId::for_face_pair(faces[1], side(s - 1, i), 0));
        }
        for k in 0..s {
            for i in 0..n {
                expected.push(BRepEdgeId::for_face_pair(
                    side(k, i),
                    side(k, (i + 1) % n),
                    0,
                ));
            }
        }
        for k in 0..s - 1 {
            for i in 0..n {
                expected.push(BRepEdgeId::for_face_pair(side(k, i), side(k + 1, i), 0));
            }
        }
        assert_eq!(edges, expected);

        // Spot-pin one edge of each class at its canonical offset.
        // First-cap edge 0 at offset 0.
        assert_eq!(
            edges[0],
            BRepEdgeId::for_face_pair(faces[0], side(0, 0), 0),
            "first-cap perimeter edge"
        );
        // Last-cap edge 0 at offset n.
        assert_eq!(
            edges[n],
            BRepEdgeId::for_face_pair(faces[1], side(1, 0), 0),
            "last-cap perimeter edge"
        );
        // Segment 0 side seam at offset 2n.
        assert_eq!(
            edges[2 * n],
            BRepEdgeId::for_face_pair(side(0, 0), side(0, 1), 0),
            "segment 0 side seam"
        );
        // Segment 1 side seam at offset 3n.
        assert_eq!(
            edges[3 * n],
            BRepEdgeId::for_face_pair(side(1, 0), side(1, 1), 0),
            "segment 1 side seam"
        );
        // Interior ring edge 0 at offset 2n + n * s.
        assert_eq!(
            edges[2 * n + n * s],
            BRepEdgeId::for_face_pair(side(0, 0), side(1, 0), 0),
            "interior ring edge"
        );
    }

    /// Rebuilding a Sweep with the same `profile_count` and
    /// `path_segment_count` but different profile/path coordinates yields
    /// byte-identical edge IDs for the same owner — Sweep edge identity
    /// is topology-derived from face IDs, never coordinate-derived.
    #[test]
    fn sweep_brep_edge_ids_stable_under_coordinate_change() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        // Baseline: unit square over a 2-point path (n=4, s=1).
        let base = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
        // Same topology (n=4, s=1) but different profile + path coords.
        let big_square = Polygon2D::new(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 5.0], [0.0, 5.0]])
            .expect("scaled square profile");
        let drifted_path =
            Polyline3D::new(vec![[2.0, -1.0, 0.5], [4.0, 7.0, 9.0]]).expect("drifted 2-point path");
        let rebuilt = SweepOp::new(big_square, drifted_path);
        assert_eq!(base.brep_edge_ids(owner), rebuilt.brep_edge_ids(owner));
    }

    /// `brep_edge_ids` is deterministic — repeated calls for the same
    /// `(SweepOp, owner)` return byte-identical IDs in byte-identical
    /// order.
    #[test]
    fn sweep_brep_edge_ids_repeated_calls_byte_identical() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        let op = SweepOp::new(unit_triangle(), z_path(&[0.0, 1.0, 2.0]));
        assert_eq!(op.brep_edge_ids(owner), op.brep_edge_ids(owner));
    }
}
