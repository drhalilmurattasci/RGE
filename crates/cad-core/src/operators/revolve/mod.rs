//! Revolve operator: rotate a 2D profile around the Y-axis through a sweep
//! angle in `(0, 2π]`.
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! # Geometry
//!
//! The profile is a closed [`Polygon2D`] in the XY plane with all `x >= 0`
//! (lying on the +X side of the Y-axis). Revolving each point `(x, y)` around
//! the Y-axis through `θ` produces `(x·cos θ, y, x·sin θ)` — a circle of
//! radius `x` at height `y` in the XZ plane.
//!
//! # Output topology
//!
//! For a profile with `n` points and `segments` rotational steps:
//!
//! * **Full** (`angle == 2π`): `n*segments` verts, `2*n*segments` tris (no caps —
//!   index wrap closes the surface).
//! * **Partial** (`angle < 2π`): `n*(segments+1)` verts, `2*n*segments` side
//!   tris + `2*(n-2)` cap tris (fan-triangulated start+end caps; convex only).
//!
//! # Concave profiles
//!
//! Full revolution emits side walls only (no caps), so concave profiles
//! project correctly. Partial revolution requires fan-triangulated caps
//! (mirrors [`crate::operators::ExtrudeOp`]'s convexity restriction) — caps
//! validated against [`Polygon2D::convexity`] at evaluate time. Self-
//! intersecting profiles produce incorrect output but are not detected —
//! caller's responsibility.
//!
//! # Winding convention
//!
//! Profile is interpreted as CCW in the XY plane (signed area > 0). CW input
//! is auto-reversed internally so the algorithm always processes CCW. The
//! side-wall outward-facing normals point radially outward + along the
//! polygon-edge normal (correct for CCW input). For partial revolution, the
//! start cap (ring 0, θ=0) has outward normal in -Z (away from the swept
//! volume which extends into +Z half-space as θ increases from 0); the end
//! cap (ring `segments`, θ=angle) has outward normal in the +tangent
//! direction at the end angle.
//!
//! # Module layout
//!
//! * `full_path` — full-2π revolution algorithm (no caps; concave profiles
//!   accepted).
//! * `partial_path` — partial-revolution algorithm (fan-triangulated start /
//!   end caps; convexity required).
//!
//! # Capability surface (per ADR-104)
//!
//! * `boolean_robust_under_tolerance`: true (no boolean op).
//! * `deterministic_triangulation`: true (sin/cos sweep deterministic; no
//!   triangulation-choice indeterminism).
//! * `t_junction_handling`: true (closed surface; no T-junctions in side
//!   walls or fan-triangulated caps).
//! * `concave_input_supported`: **mode-dependent** — true for full revolution
//!   (no caps ⇒ no fan-triangulation constraint); **false** for partial
//!   revolution (fan-triangulated caps require strict convexity). Validated
//!   at evaluate time.
//! * `arity`: 0 (profile is a parameter, not an upstream input).
//! * `output_labeled_when_input_labeled`: false (no inputs).

mod full_path;
mod partial_path;
#[cfg(test)]
mod tests;

use std::f32::consts::PI;

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator, Polygon2D};
use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topology::{
    BRepEdgeId, BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider, RevolveFaceTag,
    RevolveMode,
};

// ---------------------------------------------------------------------------
// RevolveOp
// ---------------------------------------------------------------------------

/// Sweep a [`Polygon2D`] profile around the Y-axis through `angle` radians to
/// produce a surface of revolution.
///
/// `segments` is the number of rotational steps and must be `>= 3`. `angle`
/// must lie in `(0, 2π]` and is finite. The profile must lie entirely on the
/// +X side of the Y-axis (`all x >= 0`), validated at [`RevolveOp::evaluate`]
/// time. For full revolution (`angle == 2π`) concave profiles are accepted;
/// for partial revolution (`angle < 2π`) caps require a strictly convex
/// profile (same fan-triangulation constraint as
/// [`crate::operators::ExtrudeOp`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RevolveOp {
    /// 2D profile rotated around the Y-axis.
    pub profile: Polygon2D,
    /// Number of rotational steps. Must be `>= 3`.
    pub segments: u32,
    /// Sweep angle in radians, `(0, 2π]`. Defaults to `2π` (full revolution)
    /// for serde compatibility with pre-D-Partial-Revolve snapshots.
    #[serde(default = "default_angle_full_revolution")]
    pub angle: f32,
}

/// Serde default for [`RevolveOp::angle`] — `2π` (full revolution),
/// preserving legacy snapshot semantics.
fn default_angle_full_revolution() -> f32 {
    2.0 * PI
}

impl RevolveOp {
    /// Full-revolution constructor (`angle = 2π`). Backwards-compatible with
    /// pre-D-Partial-Revolve callers.
    ///
    /// # Errors
    ///
    /// * [`OpError::InvalidParameter`] if `segments < 3`.
    pub fn new(profile: Polygon2D, segments: u32) -> Result<Self, OpError> {
        Self::partial(profile, segments, 2.0 * PI)
    }

    /// Partial-revolution constructor. Validates `segments >= 3`,
    /// `angle ∈ (0, 2π]` and finite. The profile-shape validity (all
    /// `x >= 0`, `signed_area != 0`, plus convexity check when
    /// `angle < 2π`) is checked at [`RevolveOp::evaluate`] time.
    ///
    /// # Errors
    ///
    /// * [`OpError::InvalidParameter`] if `segments < 3`.
    /// * [`OpError::InvalidParameter`] if `angle` is not finite.
    /// * [`OpError::InvalidParameter`] if `angle <= 0` or `angle > 2π + 1e-5`.
    pub fn partial(profile: Polygon2D, segments: u32, angle: f32) -> Result<Self, OpError> {
        if segments < 3 {
            return Err(OpError::InvalidParameter(format!(
                "RevolveOp.segments must be >= 3 (got {segments})"
            )));
        }
        if !angle.is_finite() {
            return Err(OpError::InvalidParameter(format!(
                "RevolveOp.angle must be finite (got {angle})"
            )));
        }
        let two_pi = 2.0 * PI;
        if angle <= 0.0 || angle > two_pi + 1e-5 {
            return Err(OpError::InvalidParameter(format!(
                "RevolveOp.angle must be in (0, 2π] (got {angle})"
            )));
        }
        // Clamp to exactly 2π if within epsilon — protects the
        // full-revolution fast path from float drift in the
        // `angle == two_pi` comparison.
        let clamped = if (angle - two_pi).abs() < 1e-5 {
            two_pi
        } else {
            angle
        };
        Ok(Self {
            profile,
            segments,
            angle: clamped,
        })
    }

    /// Number of segments (always `>= 3` once constructed via
    /// [`RevolveOp::new`] or [`RevolveOp::partial`]).
    #[must_use]
    pub fn segments(&self) -> u32 {
        self.segments
    }

    /// Sweep angle in radians.
    #[must_use]
    pub fn angle(&self) -> f32 {
        self.angle
    }

    /// Returns `true` if this is a full-revolution operator (no caps emitted,
    /// concave profiles allowed). Uses an epsilon comparison against `2π` to
    /// absorb float drift; constructors clamp inputs within `1e-5` of `2π` to
    /// exactly `2π`, so this check uses a tighter `1e-6` epsilon to match
    /// post-clamp values bit-for-bit while still tolerating any residual
    /// arithmetic noise.
    #[must_use]
    pub fn is_full_revolution(&self) -> bool {
        let two_pi = 2.0 * PI;
        (self.angle - two_pi).abs() < 1e-6
    }
}

impl Operator for RevolveOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Revolve
    }

    fn arity(&self) -> usize {
        0
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"revolve:");
        hasher.update(&self.segments.to_le_bytes());
        hasher.update(&self.angle.to_le_bytes());
        let profile_len = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        hasher.update(&profile_len.to_le_bytes());
        for [x, y] in self.profile.points() {
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

        // Defensive — `RevolveOp::new` already enforces, but `segments` is a
        // pub field and a caller could have mutated it post-construction.
        if self.segments < 3 {
            return Err(OpError::InvalidParameter(format!(
                "revolve segments must be >= 3 (got {})",
                self.segments
            )));
        }

        // Defensive angle re-validation — `angle` is a pub field.
        if !self.angle.is_finite() {
            return Err(OpError::InvalidParameter(format!(
                "revolve angle must be finite (got {})",
                self.angle
            )));
        }
        let two_pi = 2.0 * PI;
        if self.angle <= 0.0 || self.angle > two_pi + 1e-5 {
            return Err(OpError::InvalidParameter(format!(
                "revolve angle must be in (0, 2π] (got {})",
                self.angle
            )));
        }

        // Defensive profile-shape re-validation. `Polygon2D::new` already
        // checked `len >= 3` and finiteness, but `profile` is pub.
        if self.profile.len() < 3 {
            return Err(OpError::InvalidParameter(format!(
                "revolve profile needs >= 3 points (got {})",
                self.profile.len()
            )));
        }
        for (i, [x, y]) in self.profile.points().iter().enumerate() {
            if !x.is_finite() || !y.is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "revolve profile has non-finite coordinate at index {i}"
                )));
            }
        }

        // +X-side restriction.
        for (i, [x, _y]) in self.profile.points().iter().enumerate() {
            if *x < 0.0 {
                return Err(OpError::InvalidParameter(format!(
                    "revolve profile must lie on +X side of Y-axis (all x >= 0); index {i} has x = {x}"
                )));
            }
        }

        // Reject near-zero-area / collinear profiles. Epsilon comparison
        // rather than exact == 0.0 to defend against tiny float-drift in
        // the shoelace sum that would otherwise sneak through.
        let signed_area = self.profile.signed_area();
        if signed_area.abs() < 1e-12_f32 {
            return Err(OpError::InvalidParameter(
                "revolve profile is degenerate (near-zero area)".to_string(),
            ));
        }

        // Convexity gate — only for partial revolution (caps need
        // fan-triangulation). Full revolution allows concave profiles since
        // it emits no caps.
        let full_revolution = self.is_full_revolution();
        if !full_revolution {
            match self.profile.convexity() {
                Some(true) => {}
                Some(false) => {
                    return Err(OpError::InvalidParameter(
                        "partial revolution requires convex profile (got concave)".to_string(),
                    ));
                }
                None => {
                    return Err(OpError::InvalidParameter(
                        "revolve profile is degenerate (all points collinear)".to_string(),
                    ));
                }
            }
        }

        // Winding correction: signed_area > 0 → CCW already; < 0 → reverse.
        let n_points = self.profile.len();
        let ordered: Vec<[f32; 2]> = if signed_area > 0.0 {
            self.profile.points().to_vec()
        } else {
            self.profile.points().iter().rev().copied().collect()
        };

        let segments_usize = self.segments as usize;
        let n_u32 = u32::try_from(n_points).map_err(|_| {
            OpError::InvalidParameter(format!("revolve profile too large: {n_points} points"))
        })?;

        if full_revolution {
            full_path::evaluate_full(self.segments, &ordered, n_u32, segments_usize)
        } else {
            partial_path::evaluate_partial(
                self.segments,
                self.angle,
                &ordered,
                n_u32,
                segments_usize,
            )
        }
    }
}

// ---------------------------------------------------------------------------
// BRepProvider — sub-7.2-γ B-Rep face identity for RevolveOp
// ---------------------------------------------------------------------------

/// Pair the per-tessellation `TopologyFaceId`s with rebuild-stable
/// `BRepFaceId`s seeded from the caller-supplied [`BRepOwnerId`].
///
/// `RevolveOp` exercises a topology axis no prior dispatch has touched: a
/// **categorical mode change** (`Full` vs `Partial` revolution) that alters
/// the *face set itself*, not just the face count.
///
/// * `Full` revolution (`is_full_revolution()` returns `true`, i.e.
///   `angle ≈ 2π`): emits `n` Side faces only; no caps.
///   * `TopologyFaceId(0..n)` → [`RevolveFaceTag::Side`] for `i in 0..n`
///     with `mode = Full`.
/// * `Partial` revolution (`angle < 2π`): emits `n` Side faces + start cap
///   + end cap, `n + 2` faces total, in the canonical emission order from
///   [`Operator::evaluate`] (Side walls FIRST, then start-cap fan, then
///   end-cap fan — see `partial_path::evaluate_partial`).
///   * `TopologyFaceId(0..n)` → [`RevolveFaceTag::Side`] for `i in 0..n`
///     with `mode = Partial`.
///   * `TopologyFaceId(n)` → [`RevolveFaceTag::StartCap`].
///   * `TopologyFaceId(n + 1)` → [`RevolveFaceTag::EndCap`].
///
/// The `Side` tag carries `(side_index, profile_count, segment_count, mode)`
/// — all four are topology in this substrate's identity model. Mode flips
/// (`Full` ↔ `Partial`), segment-count changes (8 → 16), and profile-count
/// changes (square → pentagon) all break Side IDs by construction. Cap tags
/// carry `profile_count` only — segment count and angle do not affect cap
/// geometry, so they are deliberately NOT hashed in. See [`RevolveFaceTag`]
/// for the full stability contract.
///
/// Mode is derived from [`RevolveOp::is_full_revolution`] at this impl site
/// — NOT a free parameter, NOT re-derived locally. Different from
/// [`crate::topology::ExtrudeFaceTag::Side`]'s `profile_count`, which is a
/// free numeric parameter.
impl BRepProvider for RevolveOp {
    fn brep_face_ids(&self, owner: BRepOwnerId) -> Vec<(TopologyFaceId, BRepFaceId)> {
        // Mirrors the `n_u32` cast pattern in `evaluate` (extrude.rs L274
        // precedent): saturate to `u32::MAX` for the unreachable >4G-point
        // case (Tessellation::new would have rejected long before the BRep
        // substrate ran).
        let n = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        let segments = self.segments();
        // is_full_revolution() at L180 of this file — canonical
        // Full-vs-Partial discriminator with 1e-6 epsilon vs 2π.
        let mode = if self.is_full_revolution() {
            RevolveMode::Full
        } else {
            RevolveMode::Partial
        };

        let cap_count: u32 = match mode {
            RevolveMode::Full => 0,
            RevolveMode::Partial => 2,
        };
        let total = u64::from(n).saturating_add(u64::from(cap_count));
        let mut ids: Vec<(TopologyFaceId, BRepFaceId)> = Vec::with_capacity(total as usize);

        // Sides: emitted FIRST per partial_path::evaluate_partial canonical
        // order (sides → start-cap fan → end-cap fan; verified at
        // tests.rs:520-540). One per profile edge, indexed 0..n.
        for i in 0..n {
            ids.push((
                TopologyFaceId(u64::from(i)),
                BRepFaceId::for_revolve_face(
                    owner,
                    RevolveFaceTag::Side {
                        side_index: i,
                        profile_count: n,
                        segment_count: segments,
                        mode,
                    },
                ),
            ));
        }

        // Caps (Partial mode only).
        if matches!(mode, RevolveMode::Partial) {
            ids.push((
                TopologyFaceId(u64::from(n)),
                BRepFaceId::for_revolve_face(owner, RevolveFaceTag::StartCap { profile_count: n }),
            ));
            ids.push((
                TopologyFaceId(u64::from(n) + 1),
                BRepFaceId::for_revolve_face(owner, RevolveFaceTag::EndCap { profile_count: n }),
            ));
        }
        ids
    }
}

// ---------------------------------------------------------------------------
// BRepEdgeProvider — sub-7.2-ζ.γ B-Rep edge identity for RevolveOp
// ---------------------------------------------------------------------------

/// Mint stable B-Rep edge identities for a surface of revolution.
///
/// `RevolveOp` is the only direct provider whose **edge count depends on
/// the mode**:
///
/// * `Full` revolution — exactly `n` edges (one per `Side(i) ∩ Side((i+1) % n)`
///   adjacency, the closed circular path swept by each profile-vertex shared
///   between profile edges `i` and `i + 1`). No caps means no cap-perimeter
///   edges; the surface is a closed sweep.
/// * `Partial` revolution — exactly `3 * n` edges:
///   * `n` axial seams — `Side(i) ∩ Side((i + 1) % n)`, the 1/k-circular arc
///     swept by each shared profile-vertex through `angle` radians.
///   * `n` start-cap-perimeter edges — `StartCap ∩ Side(i)` for each `i`
///     (each profile edge of the start cap is shared with exactly one
///     side face).
///   * `n` end-cap-perimeter edges — `EndCap ∩ Side(i)` for each `i`.
///
/// In partial mode, edges are emitted in that order: all `n` Side-Side
/// seams first (indices `0..n`), then all `n` start-cap edges (indices
/// `n..2n`), then all `n` end-cap edges (indices `2n..3n`). Mode is
/// driven by [`RevolveOp::is_full_revolution`] (the same canonical
/// discriminator the `BRepProvider` impl above uses, with a `1e-6`
/// epsilon vs `2π`); the BRep substrate does not recompute the
/// boundary epsilon locally.
///
/// Every edge uses `local_ordinal = 0`.
///
/// Compositional honesty: edge identity propagates the face substrate's
/// mode break by construction. A `Full`-mode `RevolveOp` and a
/// `Partial`-mode `RevolveOp` with otherwise identical parameters
/// produce disjoint Side face IDs (mode is hashed into the Side tag's
/// BLAKE3 input), so their edge IDs are also disjoint — verified by
/// the `revolve_full_and_partial_edge_ids_are_disjoint` integration
/// smoke.
impl BRepEdgeProvider for RevolveOp {
    fn brep_edge_ids(&self, owner: BRepOwnerId) -> Vec<BRepEdgeId> {
        let face_ids: Vec<BRepFaceId> = self
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        // Face emission order (sub-7.2-γ) — see `impl BRepProvider for
        // RevolveOp` above:
        //   Full mode:    TopologyFaceId(0..n) = Side(0..n-1)
        //   Partial mode: TopologyFaceId(0..n) = Side(0..n-1),
        //                 TopologyFaceId(n)    = StartCap,
        //                 TopologyFaceId(n+1)  = EndCap
        let n = u32::try_from(self.profile.len()).unwrap_or(u32::MAX);
        let is_full = self.is_full_revolution();

        let total: u64 = if is_full {
            u64::from(n)
        } else {
            (u64::from(n)).saturating_mul(3)
        };
        let mut edges: Vec<BRepEdgeId> = Vec::with_capacity(total as usize);

        // Side ∩ Side adjacencies — n edges in BOTH modes. The profile is a
        // closed polygon, so the sequence of side faces wraps modulo n.
        // Each adjacency is a circular arc (full) or 1/k-circular arc
        // (partial), but topologically it's one edge per profile-vertex
        // pair.
        for i in 0..n {
            let next = (i + 1) % n;
            edges.push(BRepEdgeId::for_face_pair(
                face_ids[i as usize],
                face_ids[next as usize],
                0,
            ));
        }

        if !is_full {
            // Partial mode adds 2n cap-perimeter edges. The caps live at
            // TopologyFaceId(n) = StartCap and TopologyFaceId(n + 1) =
            // EndCap; each cap is a fan-triangulated copy of the profile
            // polygon and its boundary is n profile edges, each shared
            // with exactly one Side face.
            let start_cap = face_ids[n as usize];
            let end_cap = face_ids[n as usize + 1];
            for i in 0..n {
                let side = face_ids[i as usize];
                edges.push(BRepEdgeId::for_face_pair(start_cap, side, 0));
            }
            for i in 0..n {
                let side = face_ids[i as usize];
                edges.push(BRepEdgeId::for_face_pair(end_cap, side, 0));
            }
        }
        edges
    }
}
