//! `RoundFilletOp` — real round fillet substrate (chapter sub-α).
//!
//! Failure class: snapshot-recoverable.
//!
//! Per ADR-119 (real round fillet substrate), `RoundFilletOp` is a
//! NEW operator beside chamfer [`crate::operators::FilletOp`], not an
//! in-place evolution. The two operators share NO substrate (no
//! shared spec, no shared trait, no shared resolver arms); chamfer's
//! `FilletOp` + `ChamferSpec` + `FilletUpstream` + sub-ε.α/β resolver
//! arms are byte-identical to their pre-this-dispatch state (per ADR
//! D6).
//!
//! # Sub-α scope (this dispatch)
//!
//! - Substrate: `RoundFilletOp` struct + `RoundFilletSpec` + `pub(crate) trait RoundFilletUpstream` + new `OperatorNode::RoundFillet(_)` resolver arms in BOTH face + edge resolvers
//! - Upstream: `CuboidOp` only (per ADR D7's chapter shape — sub-β
//!   Extrude / sub-γ Revolve cap-side / sub-δ Loft follow if chapter
//!   continues)
//! - Geometry: rolled quarter-cylinder surface with N=8 segments per
//!   filleted edge; face-strip removal via vertex-substitution
//!   (preserves upstream's shared corner positions byte-identical;
//!   ADDS new inset vertices and re-indexes the adjacent face's
//!   triangles to use the insets in place of the filleted-edge
//!   endpoint indices); cylinder cap surfaces nameless
//!   ([`TopologyFaceId::DEGENERATE`]) per ADR D3
//! - Correctness target per user direction: single-edge + non-
//!   corner-sharing multi-edge cases produce visually + topologically
//!   correct output; corner-sharing multi-edge produces "visually
//!   weird but topologically valid" output per ADR D8 — NOT a sub-α
//!   success criterion
//!
//! # NON-GOALS (sub-α scope discipline)
//!
//! - **No multi-edge corner blending** (torus-patch generation at
//!   corners where 2+ filleted edges meet) — ADR D8; sub-ε scope
//! - **No circular-path Revolve edges** — ADR D8; sub-ζ scope (would
//!   require multi-segment `RoundFilletSpec` evolution)
//! - **No perpendicular-face re-tessellation** at filleted-edge
//!   endpoints (the cylinder's quarter-arc end-cap floats in the
//!   "corner gap" between the rolled surface and the perpendicular
//!   face's unchanged original corner geometry) — documented v0
//!   visual imperfection; matches chamfer FilletOp's "visually weird
//!   but topologically valid" framing
//! - **No `impl BRepProvider for RoundFilletOp`** — face identity
//!   flows via the graph-level resolver per ADR D4 (`OperatorNode::RoundFillet(_)`
//!   face-resolver arm recurses to upstream and returns upstream
//!   `BRepFaceId`s unchanged; faces retain identity under face-strip
//!   removal because identity = semantic surface, not mesh shape)
//! - **No `impl BRepEdgeProvider for RoundFilletOp`** — edge identity
//!   flows via the graph-level resolver per ADR D2 (`OperatorNode::RoundFillet(_)`
//!   edge-resolver arm recurses to upstream and returns ALL upstream
//!   edges including filleted ones; curved-edge inheritance via the
//!   shape-agnostic `BRepEdgeId::for_face_pair` derivation)
//! - **No cap-face / corner-patch `BRepFaceId`** — caps + corner
//!   patches are nameless in v0 (ADR D3; pressure-deferred); cylinder
//!   surface triangles emit `TopologyFaceId::DEGENERATE`
//! - **No `Strategy::Winch` / engine-default change** — orthogonal
//!   to this dispatch
//! - **No chamfer `FilletOp` change** — byte-identical per ADR D6
//! - **No new architecture lint, no new ADR, no new doctrine doc**

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topology::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId};

mod cuboid;

// ---------------------------------------------------------------------------
// RoundFilletError
// ---------------------------------------------------------------------------

/// Construction-time errors for [`RoundFilletOp::new`].
///
/// Marked `#[non_exhaustive]` so future variant additions
/// (e.g., circular-path-Revolve support per ADR D8 / sub-ζ) are
/// non-breaking. Variants intentionally mirror
/// [`crate::operators::FilletError`] so callers can apply the same
/// error-handling patterns to both operators.
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RoundFilletError {
    /// `radius` must be finite and strictly positive.
    #[error("round fillet radius must be finite and > 0; got {radius}")]
    InvalidRadius {
        /// The offending radius value.
        radius: f32,
    },

    /// Caller passed an empty edge selection — degenerate operator.
    #[error("round fillet edge list is empty; degenerate operator")]
    EmptyEdgeSelection,

    /// One of the supplied [`BRepEdgeId`]s does not match any edge
    /// emitted by the upstream's [`BRepEdgeProvider`].
    #[error("edge id {edge:?} does not appear in upstream's BRepEdgeProvider output")]
    EdgeNotInUpstream {
        /// The unknown edge id.
        edge: BRepEdgeId,
    },

    /// The supplied edge ID is valid against the upstream's
    /// [`BRepEdgeProvider`], but its geometry is not supported by
    /// `RoundFilletOp`'s v0 rolled-cylinder pattern.
    ///
    /// Reserved for future use (e.g., circular-path Revolve edges
    /// per ADR D8 / sub-ζ). Cuboid upstream in sub-α never produces
    /// this error — every Cuboid edge is a clean 2-endpoint
    /// adjacency.
    #[error("edge id {edge:?} has unsupported geometry: {reason}")]
    UnsupportedEdgeGeometry {
        /// The offending edge id.
        edge: BRepEdgeId,
        /// Static description of why the geometry is not supported.
        reason: &'static str,
    },
}

// ---------------------------------------------------------------------------
// RoundFilletSpec — per-filleted-edge data
// ---------------------------------------------------------------------------

/// Per-filleted-edge data used at evaluation. Stored in the order the
/// caller supplied edges. Computed at construction time so evaluation
/// is upstream-agnostic.
///
/// Distinct from [`crate::operators::ChamferSpec`] (chamfer's spec):
/// round fillet needs two in-plane inward directions (one per
/// adjacent face) for the inset vertices AND to compute the cylinder
/// axis center. Chamfer carries one fused inward direction; round
/// fillet's per-face split is load-bearing for the face-strip removal
/// substitution.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RoundFilletSpec {
    /// Index of edge endpoint 1 in upstream's `positions` array.
    pub(crate) vertex_a: u32,
    /// Index of edge endpoint 2 in upstream's `positions` array.
    pub(crate) vertex_b: u32,
    /// `TopologyFaceId` of adjacent face A (one of the two faces
    /// sharing this edge). Used to locate face A's triangles in the
    /// upstream's `face_labels` for the face-strip-removal
    /// substitution.
    pub(crate) face_a_id: TopologyFaceId,
    /// Same for adjacent face B.
    pub(crate) face_b_id: TopologyFaceId,
    /// In-plane inward direction for face A's inset — perpendicular
    /// to the filleted edge, lying in face A's plane, pointing INTO
    /// face A's interior from the filleted edge. Unit vector.
    pub(crate) face_a_inward: [f32; 3],
    /// Same for face B.
    pub(crate) face_b_inward: [f32; 3],
}

// ---------------------------------------------------------------------------
// RoundFilletUpstream — internal trait abstracting per-upstream resolution
// ---------------------------------------------------------------------------

/// Internal trait that abstracts the per-upstream-operator pieces of
/// `RoundFilletOp` construction. Sibling to [`crate::operators::FilletUpstream`]
/// (chamfer's trait) per ADR D5 — substrate is PARALLEL to chamfer's,
/// not shared.
///
/// Per ADR D6, the existing chamfer `FilletUpstream` trait + its 4
/// per-upstream impls (Cuboid + Extrude + Revolve + Loft) are
/// byte-identical to their pre-this-dispatch state; this new trait
/// adds round-fillet-specific resolution alongside.
///
/// Currently `pub(crate)` only — abstraction earned when the second
/// implementation lands (sub-β Extrude); for sub-α Cuboid is the only
/// implementor. External consumer plug-in is a separate ADR-level
/// decision.
pub(crate) trait RoundFilletUpstream: BRepEdgeProvider {
    /// Resolve a canonical edge index (the position in
    /// `brep_edge_ids` output) to the data needed for round-fillet
    /// evaluation.
    ///
    /// # Errors
    ///
    /// Returns `Err(reason)` when the edge's geometry is not
    /// supported by `RoundFilletOp`'s v0 rolled-cylinder pattern.
    /// The caller wraps this with the edge ID into
    /// [`RoundFilletError::UnsupportedEdgeGeometry`].
    ///
    /// Cuboid implementation always returns `Ok(spec)` — every Cuboid
    /// edge is a clean 2-endpoint adjacency between two perpendicular
    /// axis-aligned faces.
    fn resolve_round_spec(&self, canonical_index: usize) -> Result<RoundFilletSpec, &'static str>;
}

// ---------------------------------------------------------------------------
// RoundFilletOp
// ---------------------------------------------------------------------------

/// Tessellation segments around the quarter-cylinder cross-section.
/// 8 segments produces a visually smooth quarter-arc at typical
/// fillet radii; can be raised by a future LoD knob.
const ROUND_FILLET_SEGMENTS: usize = 8;

/// `RoundFilletOp` — real round fillet along selected upstream edges.
///
/// Constructed via [`RoundFilletOp::new`] (Cuboid upstream in sub-α);
/// validates each edge against the upstream's
/// [`crate::topology::BRepEdgeProvider`] and resolves each
/// [`BRepEdgeId`] back to a [`RoundFilletSpec`] so evaluation can
/// locate the geometry without holding a graph reference. Arity 1 —
/// takes the upstream's tessellation as input.
///
/// Per ADR D1 + D6: distinct from chamfer
/// [`crate::operators::FilletOp`]; both operators coexist in v0,
/// serving distinct use cases (chamfer for fast preview /
/// constant-time-per-edge; round for production geometry with
/// face-strip removal).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoundFilletOp {
    /// Selected edges by stable identity. Mirrors the user-facing
    /// API surface of chamfer's `FilletOp`.
    pub(super) edges: Vec<BRepEdgeId>,
    /// Resolved per-edge round-fillet spec — one per selected edge,
    /// in the same order. Used at evaluation time to locate vertices
    /// and apply the rolled-cylinder geometry. Computed at
    /// construction time.
    pub(super) round_specs: Vec<RoundFilletSpec>,
    /// Fillet radius (cylinder cross-section radius), in world units.
    pub(super) radius: f32,
    /// Owner the substrate-resolved IDs were derived against.
    pub(super) owner: BRepOwnerId,
}

impl RoundFilletOp {
    /// Borrow the validated edge selection.
    #[must_use]
    pub fn edges(&self) -> &[BRepEdgeId] {
        &self.edges
    }

    /// Returns the round-fillet radius.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Returns the owner the edge IDs were validated against.
    #[must_use]
    pub fn owner(&self) -> BRepOwnerId {
        self.owner
    }

    /// Generic constructor over any [`RoundFilletUpstream`].
    ///
    /// Performs the shared validation (radius finiteness, non-empty
    /// edge selection, per-edge upstream lookup) and per-upstream
    /// round-spec resolution.
    ///
    /// # Errors
    ///
    /// * [`RoundFilletError::InvalidRadius`] if `radius` is non-finite
    ///   or `<= 0`.
    /// * [`RoundFilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`RoundFilletError::EdgeNotInUpstream`] if any edge ID does
    ///   not appear in `upstream.brep_edge_ids(owner)`.
    /// * [`RoundFilletError::UnsupportedEdgeGeometry`] if a known edge
    ///   ID has geometry `RoundFilletOp` cannot round in v0 (reserved
    ///   for future circular-path Revolve support; Cuboid never
    ///   produces this).
    pub(super) fn from_upstream<U: RoundFilletUpstream>(
        upstream: &U,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, RoundFilletError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(RoundFilletError::InvalidRadius { radius });
        }
        if edges.is_empty() {
            return Err(RoundFilletError::EmptyEdgeSelection);
        }

        let upstream_edges = upstream.brep_edge_ids(owner);
        let mut round_specs = Vec::with_capacity(edges.len());
        for &edge_id in &edges {
            let canonical_index = upstream_edges
                .iter()
                .position(|id| *id == edge_id)
                .ok_or(RoundFilletError::EdgeNotInUpstream { edge: edge_id })?;
            let spec = upstream
                .resolve_round_spec(canonical_index)
                .map_err(|reason| RoundFilletError::UnsupportedEdgeGeometry {
                    edge: edge_id,
                    reason,
                })?;
            round_specs.push(spec);
        }

        Ok(Self {
            edges,
            round_specs,
            radius,
            owner,
        })
    }
}

impl Operator for RoundFilletOp {
    fn op_kind(&self) -> OpKind {
        OpKind::RoundFillet
    }

    fn arity(&self) -> usize {
        1
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"round_fillet:");
        hasher.update(&self.radius.to_le_bytes());
        hasher.update(self.owner.as_bytes());
        hasher.update(
            &u32::try_from(self.edges.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        for edge in &self.edges {
            hasher.update(edge.as_bytes());
        }
        let hash = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(hash.as_bytes());
        out
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        let upstream = inputs[0];
        let mut positions = upstream.positions.clone();
        let mut indices = upstream.indices.clone();
        // Round fillet preserves labeled-ness — every Cuboid input is
        // labeled (CuboidOp always emits face_labels); the modified
        // face triangles inherit their original TopologyFaceId, and
        // new cylinder triangles get TopologyFaceId::DEGENERATE per
        // ADR D3 (nameless cap surfaces in v0). If upstream is
        // unlabeled (None), we don't fabricate labels; the
        // face-strip substitution can't run without labels (it needs
        // face_id to locate triangles), so unlabeled upstream
        // produces an unlabeled output with cylinder geometry
        // appended but NO face-strip removal — degenerate case for
        // v0 Cuboid (Cuboid is always labeled).
        let mut face_labels = upstream.face_labels.clone();

        for spec in &self.round_specs {
            let vertex_a_usize = spec.vertex_a as usize;
            let vertex_b_usize = spec.vertex_b as usize;
            if vertex_a_usize >= positions.len() || vertex_b_usize >= positions.len() {
                return Err(OpError::InvalidParameter(format!(
                    "round fillet vertex index out of bounds: a={}, b={}, positions.len={}",
                    spec.vertex_a,
                    spec.vertex_b,
                    positions.len()
                )));
            }

            let pos_a = positions[vertex_a_usize];
            let pos_b = positions[vertex_b_usize];

            // Inset vertices: 4 per filleted edge (one per
            // adjacent-face-per-endpoint combination).
            let inset_a1 = vec_add(pos_a, vec_scale(spec.face_a_inward, self.radius));
            let inset_a2 = vec_add(pos_b, vec_scale(spec.face_a_inward, self.radius));
            let inset_b1 = vec_add(pos_a, vec_scale(spec.face_b_inward, self.radius));
            let inset_b2 = vec_add(pos_b, vec_scale(spec.face_b_inward, self.radius));

            let inset_a1_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_a1);
            let inset_a2_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_a2);
            let inset_b1_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_b1);
            let inset_b2_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_b2);

            // Cylinder cross-section vertices.
            //
            // Axis center for each endpoint: at vertex_i_pos +
            // r*(face_a_inward + face_b_inward) — the position
            // equidistant (distance r) from both adjacent face planes.
            // For Cuboid axis-aligned cube + perpendicular faces, this
            // is the "inward bisector point" at the edge corner.
            //
            // Quarter-arc parameterization: theta in [0, π/2]; at
            // theta=0 the cylinder vertex coincides with inset_a (on
            // face A's plane); at theta=π/2 it coincides with inset_b
            // (on face B's plane); intermediate values trace the
            // rolled-surface arc.
            //
            // Radial direction at angle theta: -face_b_inward * cos(θ)
            // - face_a_inward * sin(θ). At θ=0 → -face_b_inward
            // direction → vertex at axis_center - r * face_b_inward,
            // which equals vertex_i_pos + r * face_a_inward = inset_a.
            // ✓
            let two_inward_sum = vec_add(spec.face_a_inward, spec.face_b_inward);
            let axis_center_1 = vec_add(pos_a, vec_scale(two_inward_sum, self.radius));
            let axis_center_2 = vec_add(pos_b, vec_scale(two_inward_sum, self.radius));

            let mut endpoint_1_cylinder_indices = Vec::with_capacity(ROUND_FILLET_SEGMENTS + 1);
            let mut endpoint_2_cylinder_indices = Vec::with_capacity(ROUND_FILLET_SEGMENTS + 1);

            #[allow(
                clippy::cast_precision_loss,
                reason = "ROUND_FILLET_SEGMENTS is 8; precision loss in usize→f32 is well below tessellation tolerance"
            )]
            let segments_f = ROUND_FILLET_SEGMENTS as f32;
            for k in 0..=ROUND_FILLET_SEGMENTS {
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "k bounded by ROUND_FILLET_SEGMENTS; precision loss negligible"
                )]
                let t = k as f32 / segments_f;
                let theta = std::f32::consts::FRAC_PI_2 * t;
                let cos_t = theta.cos();
                let sin_t = theta.sin();
                let radial = [
                    -spec.face_b_inward[0] * cos_t - spec.face_a_inward[0] * sin_t,
                    -spec.face_b_inward[1] * cos_t - spec.face_a_inward[1] * sin_t,
                    -spec.face_b_inward[2] * cos_t - spec.face_a_inward[2] * sin_t,
                ];

                let pos_1 = vec_add(axis_center_1, vec_scale(radial, self.radius));
                let pos_2 = vec_add(axis_center_2, vec_scale(radial, self.radius));

                endpoint_1_cylinder_indices
                    .push(u32::try_from(positions.len()).unwrap_or(u32::MAX));
                positions.push(pos_1);
                endpoint_2_cylinder_indices
                    .push(u32::try_from(positions.len()).unwrap_or(u32::MAX));
                positions.push(pos_2);
            }

            // Face-strip removal: substitute the filleted-edge
            // endpoint vertex indices with the inset indices in face A
            // + face B triangles. Per-vertex substitution is keyed by
            // face_a_id / face_b_id (located via face_labels) and by
            // vertex_a / vertex_b. Other faces' references to
            // vertex_a / vertex_b stay unchanged — the perpendicular
            // faces at filleted-edge endpoints keep their original
            // corner positions (v0 visual imperfection per ADR D8).
            if let Some(labels) = face_labels.as_ref() {
                for (tri_idx, label) in labels.iter().enumerate() {
                    let (replace_a_with, replace_b_with) = if *label == spec.face_a_id {
                        (inset_a1_idx, inset_a2_idx)
                    } else if *label == spec.face_b_id {
                        (inset_b1_idx, inset_b2_idx)
                    } else {
                        continue;
                    };
                    for j in 0..3 {
                        let idx_pos = tri_idx * 3 + j;
                        if indices[idx_pos] == spec.vertex_a {
                            indices[idx_pos] = replace_a_with;
                        } else if indices[idx_pos] == spec.vertex_b {
                            indices[idx_pos] = replace_b_with;
                        }
                    }
                }
            }

            // Append cylinder surface triangles. For each quad between
            // adjacent angular positions (k, k+1) and the two
            // endpoints (1, 2), two triangles form the quad surface.
            // Winding chosen so the outward-facing normal points
            // RADIALLY OUTWARD from the cylinder axis (away from the
            // cube body) — i.e., toward the original edge corner
            // direction. CCW from outside the cylinder.
            for k in 0..ROUND_FILLET_SEGMENTS {
                let a1 = endpoint_1_cylinder_indices[k];
                let a2 = endpoint_1_cylinder_indices[k + 1];
                let b1 = endpoint_2_cylinder_indices[k];
                let b2 = endpoint_2_cylinder_indices[k + 1];

                indices.push(a1);
                indices.push(b1);
                indices.push(b2);
                indices.push(a1);
                indices.push(b2);
                indices.push(a2);

                if let Some(labels) = face_labels.as_mut() {
                    labels.push(TopologyFaceId::DEGENERATE);
                    labels.push(TopologyFaceId::DEGENERATE);
                }
            }
        }

        let result = if let Some(labels) = face_labels {
            Tessellation::with_labels(positions, indices, labels)
        } else {
            Tessellation::new(positions, indices)
        };
        result.map_err(|e| OpError::InvalidParameter(format!("round fillet output invalid: {e}")))
    }

    /// Round fillet preserves labeled output when the input was
    /// labeled — the modified upstream-face triangles inherit their
    /// original `TopologyFaceId`, and new cylinder triangles get
    /// `TopologyFaceId::DEGENERATE`. For unlabeled input (no Cuboid
    /// case in sub-α — Cuboid always labels — but future upstreams
    /// may produce unlabeled output), output is unlabeled.
    fn output_is_labeled(&self, inputs_labeled: &[bool]) -> bool {
        inputs_labeled.first().copied().unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Small vector helpers (kept local to avoid a glam / nalgebra dep
// for this module — chamfer FilletOp uses the same pattern).
// ---------------------------------------------------------------------------

fn vec_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn vec_scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

// ---------------------------------------------------------------------------
// Operator-trait + accessor unit tests (upstream-agnostic).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::CuboidOp;

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
    fn op_kind_is_round_fillet() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert_eq!(op.op_kind(), OpKind::RoundFillet);
    }

    #[test]
    fn arity_is_one() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert_eq!(op.arity(), 1);
    }

    #[test]
    fn structural_hash_changes_with_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let a = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("a");
        let b = RoundFilletOp::new(&cube, owner(), vec![edge], 0.2).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_changes_with_edge_selection() {
        let cube = unit_cube();
        let edges = cube.brep_edge_ids(owner());
        let a = RoundFilletOp::new(&cube, owner(), vec![edges[0]], 0.1).expect("a");
        let b = RoundFilletOp::new(&cube, owner(), vec![edges[0], edges[1]], 0.1).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_is_deterministic() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let a = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("a");
        let b = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("b");
        assert_eq!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_distinct_from_chamfer_fillet_byte_string() {
        // Chamfer FilletOp uses b"fillet:" prefix; RoundFilletOp uses
        // b"round_fillet:". Even with identical edges + radius +
        // owner, the structural_hashes MUST differ — distinct
        // operator types in the BLAKE3 derivation.
        use crate::operators::FilletOp;
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let chamfer = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("chamfer");
        let round = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("round");
        assert_ne!(
            chamfer.structural_hash(),
            round.structural_hash(),
            "chamfer FilletOp and RoundFilletOp must produce distinct structural_hashes \
             even with identical edges / radius / owner — the BLAKE3 domain-separator \
             bytestring (b\"fillet:\" vs b\"round_fillet:\") is load-bearing for cache \
             non-collision."
        );
    }

    #[test]
    fn evaluate_rejects_wrong_arity_zero_inputs() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 0
            }
        ));
    }

    #[test]
    fn output_is_labeled_preserves_input_labeling() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert!(op.output_is_labeled(&[true]));
        assert!(!op.output_is_labeled(&[false]));
    }
}
