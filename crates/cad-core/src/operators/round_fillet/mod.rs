// SPLIT-EXEMPTION: cohesive RoundFilletOp substrate — `RoundFilletError`
// enum + `RoundFilletSpec` struct + `RoundFilletUpstream` trait +
// `RoundFilletOp` struct + `Operator` impl (general-dihedral evaluate
// body) + the unit tests that pin both sub-α's 90°-only invariants
// AND sub-β.γ's general-dihedral 60° / 90° / 120° / radius /
// endpoint / degenerate-rejection invariants. Splitting would force
// the test module to consume `pub(super) round_specs` / `pub(crate)
// RoundFilletSpec` through a public shim, breaking the "the
// operator owns its identity recipe" contract that
// `extrude.rs::SPLIT-EXEMPTION` and `loft.rs::SPLIT-EXEMPTION` cite
// at the same line-cap boundary. Per PLAN.md §1.3 Rule 3 (1043 lines
// vs 1000-line hard cap; growth from sub-β.γ general-dihedral
// formulas + 6 new pinning tests).
//
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
mod extrude;
mod revolve;

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

/// Tessellation segments around the cylinder cross-section arc.
///
/// 8 subdivisions of the arc span `π − φ` (where φ is the interior
/// dihedral angle between the two adjacent face inward directions).
/// For 90° dihedrals (sub-α Cuboid + sub-β Extrude cap-perimeter +
/// sub-γ Revolve cap-side) this is a quarter-arc; for general
/// dihedrals (sub-β.γ onward) the same `N=8` subdivides the actual
/// arc span — finer tessellation for acute dihedrals comes "for free"
/// because the arc spans more radians. Can be raised by a future LoD
/// knob.
const ROUND_FILLET_SEGMENTS: usize = 8;

/// Threshold on `sin²(φ) = 1 − (a · b)²` below which the dihedral is
/// considered degenerate (faces near-coplanar same-side or near-anti-
/// parallel knife-edge). At this threshold `|sin(φ)| < 1e-3`, i.e.
/// φ is within ~0.057° of 0° or 180° — well below any meaningful
/// fillet geometry. Below threshold, the inset / axis_center / radial
/// formulas all involve division by `sin(φ)` and would produce NaN
/// or unbounded magnitudes; we reject at evaluation time with
/// [`OpError::InvalidParameter`] (the same path the existing
/// vertex-index-out-of-bounds + Tessellation-construction-failure
/// cases use — no new error variant required, per ADR-119 D-α scope).
///
/// No current sub-α/β/γ upstream produces a degenerate dihedral
/// (Cuboid axis-aligned faces are 90° exactly; Extrude / Revolve
/// cap-perimeter and cap-side dihedrals are 90° by algebraic
/// construction). The threshold is defense-in-depth for synthetic
/// specs and future upstreams.
const DIHEDRAL_EPSILON_SQ: f32 = 1e-6;

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

            // General-dihedral cylinder math (sub-β.γ; supersedes
            // sub-α/β/γ's 90°-only formulas while preserving them
            // exactly at φ = 90°).
            //
            // For unit inward vectors `a` and `b` with interior
            // dihedral angle φ = arccos(a · b) in the perpendicular-
            // to-edge cross-section:
            //
            //   inset_a     = pos + r · (1 + a·b) / sin(φ) · a
            //                 (= r · cot(φ/2) · a; at φ=90°: r · a)
            //   inset_b     = pos + r · (1 + a·b) / sin(φ) · b
            //   axis_center = pos + r · (a + b) / sin(φ)
            //                 (at φ=90°: r · (a + b))
            //   vertex(θ)   = axis_center + r · (cos(θ+φ)·a − cos(θ)·b)/sin(φ)
            //                 for θ ∈ [0, π − φ]
            //                 (at φ=90°: −sin(θ)·a − cos(θ)·b, i.e.
            //                 sub-α's `-b·cos(θ) - a·sin(θ)`)
            //
            // `dot_ab_raw` is clamped to [-1.0, 1.0] before acos/sqrt
            // to prevent NaN on tiny float overshoot (future upstream
            // impls computing non-unit-length normals or accumulating
            // ULP-level drift). The near-degenerate guard then catches
            // dihedrals within √DIHEDRAL_EPSILON_SQ of 0° or 180°
            // where the formulas divide by sin(φ) → 0. Per ADR-119
            // sub-β.γ green-light: no new error variant — the existing
            // OpError::InvalidParameter path carries the rejection
            // (same shape as the vertex-index-out-of-bounds + Tessellation-
            // construction-failure paths above/below).
            //
            // Face-strip substitution semantics UNCHANGED: insets'
            // INDICES and their pairing with vertex_a/vertex_b /
            // face_a_id/face_b_id are byte-identical to sub-α; only
            // the POSITIONS of the 4 inset vertices change (no longer
            // pos + r·a but pos + r·cot(φ/2)·a). The substitution
            // loop below operates on indices/face_ids, not positions
            // — face-strip identity contract preserved by construction.
            let a = spec.face_a_inward;
            let b = spec.face_b_inward;
            let dot_ab_raw = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            let dot_ab = dot_ab_raw.clamp(-1.0, 1.0);
            let sin_phi_sq = 1.0 - dot_ab * dot_ab;
            if sin_phi_sq < DIHEDRAL_EPSILON_SQ {
                return Err(OpError::InvalidParameter(format!(
                    "round fillet face inward vectors near-degenerate dihedral: \
                     a·b = {dot_ab_raw} (sin²(φ) = {sin_phi_sq} < {DIHEDRAL_EPSILON_SQ}); \
                     faces are near-coplanar same-side (φ→0) or near-anti-parallel \
                     knife-edge (φ→π)"
                )));
            }
            let sin_phi = sin_phi_sq.sqrt();
            let phi = dot_ab.acos();
            let inv_sin_phi = 1.0 / sin_phi;
            let inset_scale = (1.0 + dot_ab) * inv_sin_phi;
            let axis_scale = inv_sin_phi;

            // Inset vertices: 4 per filleted edge (one per
            // adjacent-face-per-endpoint combination). Position
            // formula generalizes sub-α's `pos + r·a` to
            // `pos + r·cot(φ/2)·a`; reduces to sub-α at φ=90°
            // (cot(45°) = 1).
            let inset_a1 = vec_add(pos_a, vec_scale(a, self.radius * inset_scale));
            let inset_a2 = vec_add(pos_b, vec_scale(a, self.radius * inset_scale));
            let inset_b1 = vec_add(pos_a, vec_scale(b, self.radius * inset_scale));
            let inset_b2 = vec_add(pos_b, vec_scale(b, self.radius * inset_scale));

            let inset_a1_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_a1);
            let inset_a2_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_a2);
            let inset_b1_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_b1);
            let inset_b2_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_b2);

            // Cylinder axis_center for each endpoint: the unique
            // point in the perpendicular-to-edge cross-section at
            // distance `r` from BOTH adjacent face planes (=
            // r / sin(φ/2) along the inward bisector from the edge
            // endpoint, equivalently r · (a + b) / sin(φ)).
            let two_inward_sum = vec_add(a, b);
            let axis_center_1 = vec_add(pos_a, vec_scale(two_inward_sum, self.radius * axis_scale));
            let axis_center_2 = vec_add(pos_b, vec_scale(two_inward_sum, self.radius * axis_scale));

            // Arc parameterization: θ sweeps the EXTERIOR dihedral
            // π − φ from inset_a (at θ=0) to inset_b (at θ=π−φ).
            // The radial formula
            //   (cos(θ + φ)·a − cos(θ)·b) / sin(φ)
            // is the orthonormal cylinder-cross-section parameterization
            // in the (u_a, −a) basis where
            //   u_a = (cos(φ)·a − b) / sin(φ)
            // is the unit vector from axis_center toward inset_a.
            // At θ=0: radial = u_a → vertex = inset_a. At
            // θ=π−φ: radial = u_b → vertex = inset_b. Cylinder
            // surface radius preserved (|radial| = 1 for all θ).
            //
            // ROUND_FILLET_SEGMENTS subdivisions of the arc span:
            // for 90° dihedrals this is a quarter-arc (matches sub-α);
            // for 60° dihedrals it's 120° (sweeping more); for 120°
            // dihedrals it's 60° (sweeping less). Subdivision count
            // stays at N=8 regardless of dihedral — substrate
            // simplicity beats per-dihedral-adaptive subdivision in
            // v0 (future LoD knob can adapt).
            let arc_span = std::f32::consts::PI - phi;

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
                let theta = arc_span * t;
                let cos_t = theta.cos();
                let cos_t_plus_phi = (theta + phi).cos();
                let coef_a = cos_t_plus_phi * inv_sin_phi;
                let coef_b = -cos_t * inv_sin_phi;
                let radial = [
                    coef_a * a[0] + coef_b * b[0],
                    coef_a * a[1] + coef_b * b[1],
                    coef_a * a[2] + coef_b * b[2],
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

    // -----------------------------------------------------------------------
    // sub-β.γ — general-dihedral cylinder math
    //
    // Tests pin the new general-dihedral evaluate body at three
    // dihedral angles (60° / 90° / 120°) plus radius/endpoint
    // invariants and the degenerate-dihedral rejection. The 90° case
    // is preserved within tight geometry-equivalence epsilon (Posture
    // A — single code path, no byte-identical fast path) so any
    // future float-drift gets caught here rather than silently
    // shifting downstream geometry.
    //
    // The synthetic upstream isolates the evaluate body's geometry
    // math from any per-upstream `RoundFilletUpstream::resolve_round_spec`
    // logic — current sub-α/β/γ upstreams emit only 90° specs by
    // construction, so a hand-crafted spec is the only way to
    // exercise the general-dihedral code paths until sub-β.γ-extend
    // dispatches lift per-upstream restrictions.
    // -----------------------------------------------------------------------

    /// Build a minimal labeled upstream tessellation suitable for
    /// driving `RoundFilletOp::evaluate` with a synthetic
    /// `RoundFilletSpec`. Vertices 0/1 are the filleted-edge
    /// endpoints; vertices 2/3 are dummy third-points so two
    /// triangles (one per adjacent face) reference vertex_a/vertex_b.
    fn synthetic_upstream_for_general_dihedral_tests() -> Tessellation {
        let positions = vec![
            [0.0, 0.0, 0.0],  // vertex_a
            [0.0, 1.0, 0.0],  // vertex_b (edge along +Y)
            [1.0, 0.5, 0.0],  // dummy for face_a triangle
            [-1.0, 0.5, 0.0], // dummy for face_b triangle
        ];
        let indices = vec![
            0, 1, 2, // triangle labeled face_a (TopologyFaceId(0))
            0, 1, 3, // triangle labeled face_b (TopologyFaceId(1))
        ];
        let labels = vec![TopologyFaceId(0), TopologyFaceId(1)];
        Tessellation::with_labels(positions, indices, labels).expect("synthetic upstream")
    }

    /// Build a synthetic `RoundFilletOp` carrying a single
    /// hand-crafted `RoundFilletSpec`. Bypasses
    /// `RoundFilletUpstream::resolve_round_spec` so tests can
    /// exercise non-90° dihedrals that no current upstream impl
    /// produces.
    fn make_synthetic_op(
        face_a_inward: [f32; 3],
        face_b_inward: [f32; 3],
        radius: f32,
    ) -> RoundFilletOp {
        RoundFilletOp {
            // edges field is unused at evaluate time (validation
            // happens at construction; we're bypassing it here).
            edges: Vec::new(),
            round_specs: vec![RoundFilletSpec {
                vertex_a: 0,
                vertex_b: 1,
                face_a_id: TopologyFaceId(0),
                face_b_id: TopologyFaceId(1),
                face_a_inward,
                face_b_inward,
            }],
            radius,
            owner: BRepOwnerId::from_bytes([0xee; 16]),
        }
    }

    /// 60° dihedral: `cot(30°) = √3 ≈ 1.732` ⇒ inset offset distance
    /// from edge endpoint is `r · √3` along each face's inward
    /// direction. Pins the general-dihedral inset formula
    /// `pos + r · (1 + a·b) / sin(φ) · a` for acute dihedrals where
    /// the inset reaches FURTHER from the edge than the 90° case.
    #[test]
    fn evaluate_60_degree_dihedral_inset_distance_matches_cot_half_phi() {
        let sqrt3_over_2 = 3.0_f32.sqrt() / 2.0;
        let a = [1.0, 0.0, 0.0];
        let b = [0.5, sqrt3_over_2, 0.0]; // 60° from a (a·b = 0.5)
        let r = 1.0_f32;
        let op = make_synthetic_op(a, b, r);
        let upstream = synthetic_upstream_for_general_dihedral_tests();
        let out = op.evaluate(&[&upstream]).expect("evaluate 60°");

        // Inset_a1 at offset upstream.len() = 4. Expected:
        //   pos_a + r · cot(30°) · a = (0,0,0) + 1·√3·(1,0,0) = (√3, 0, 0).
        let expected_scale = 3.0_f32.sqrt();
        let inset_a1 = out.positions[upstream.positions.len()];
        assert!(
            (inset_a1[0] - expected_scale).abs() < 1e-5,
            "60° inset_a1.x: expected {expected_scale}, got {}",
            inset_a1[0]
        );
        assert!(inset_a1[1].abs() < 1e-5);
        assert!(inset_a1[2].abs() < 1e-5);

        // Inset_b1 at offset upstream.len() + 2. Expected:
        //   pos_a + r · cot(30°) · b = √3·(0.5, √3/2, 0) = (√3/2, 3/2, 0).
        let inset_b1 = out.positions[upstream.positions.len() + 2];
        assert!((inset_b1[0] - expected_scale * 0.5).abs() < 1e-5);
        assert!((inset_b1[1] - expected_scale * sqrt3_over_2).abs() < 1e-5);
        assert!(inset_b1[2].abs() < 1e-5);
    }

    /// 90° dihedral (sub-α/β/γ regression): inset_scale = cot(45°) =
    /// 1.0, so `inset_a1 = pos_a + r·a` byte-for-byte equivalent to
    /// the sub-α formula (within 1e-5 — float drift from the new
    /// arc_span computation is negligible at the inset placement
    /// step, which uses only `inset_scale` not `arc_span`). Pins
    /// the regression invariant — any change to the 90° inset
    /// placement breaks this assertion.
    #[test]
    fn evaluate_90_degree_dihedral_geometry_equivalence_with_sub_alpha_formula() {
        let a = [1.0, 0.0, 0.0];
        let b = [0.0, 1.0, 0.0]; // a·b = 0 (exact in f32; axis-aligned)
        let r = 0.3_f32;
        let op = make_synthetic_op(a, b, r);
        let upstream = synthetic_upstream_for_general_dihedral_tests();
        let out = op.evaluate(&[&upstream]).expect("evaluate 90°");

        // Inset_a1: sub-α formula = pos_a + r·a = (r, 0, 0).
        let inset_a1 = out.positions[upstream.positions.len()];
        assert!((inset_a1[0] - r).abs() < 1e-5, "got {inset_a1:?}");
        assert!(inset_a1[1].abs() < 1e-5);
        assert!(inset_a1[2].abs() < 1e-5);

        // Inset_b1: sub-α formula = pos_a + r·b = (0, r, 0).
        let inset_b1 = out.positions[upstream.positions.len() + 2];
        assert!(inset_b1[0].abs() < 1e-5);
        assert!((inset_b1[1] - r).abs() < 1e-5);
        assert!(inset_b1[2].abs() < 1e-5);

        // axis_center_1: sub-α formula = pos_a + r·(a+b) = (r, r, 0).
        // Verified via cylinder-radius invariant: every cylinder
        // endpoint-1 vertex sits at distance r from (r, r, 0).
        let axis_center_1 = [r, r, 0.0];
        let cylinder_start = upstream.positions.len() + 4;
        for k in 0..=ROUND_FILLET_SEGMENTS {
            let pos = out.positions[cylinder_start + 2 * k];
            let dx = pos[0] - axis_center_1[0];
            let dy = pos[1] - axis_center_1[1];
            let dz = pos[2] - axis_center_1[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!(
                (dist - r).abs() < 1e-5,
                "90° cylinder vert k={k} dist {dist} != r {r}"
            );
        }
    }

    /// 120° dihedral: `cot(60°) = 1/√3 ≈ 0.577` ⇒ inset offset is
    /// CLOSER to the edge endpoint than the 90° case. Pins the
    /// obtuse-dihedral half of the general-dihedral inset formula.
    #[test]
    fn evaluate_120_degree_dihedral_inset_distance_matches_cot_half_phi() {
        let sqrt3_over_2 = 3.0_f32.sqrt() / 2.0;
        let a = [1.0, 0.0, 0.0];
        let b = [-0.5, sqrt3_over_2, 0.0]; // 120° from a (a·b = -0.5)
        let r = 1.0_f32;
        let op = make_synthetic_op(a, b, r);
        let upstream = synthetic_upstream_for_general_dihedral_tests();
        let out = op.evaluate(&[&upstream]).expect("evaluate 120°");

        let expected_scale = 1.0 / 3.0_f32.sqrt(); // cot(60°) = 1/√3
        let inset_a1 = out.positions[upstream.positions.len()];
        assert!(
            (inset_a1[0] - expected_scale).abs() < 1e-5,
            "120° inset_a1.x: expected {expected_scale}, got {}",
            inset_a1[0]
        );

        let inset_b1 = out.positions[upstream.positions.len() + 2];
        assert!((inset_b1[0] - expected_scale * -0.5).abs() < 1e-5);
        assert!((inset_b1[1] - expected_scale * sqrt3_over_2).abs() < 1e-5);
    }

    /// Cylinder-radius invariant across all three dihedral angles:
    /// every cylinder vertex sits at distance EXACTLY `r` from its
    /// endpoint's axis_center. Pins the orthonormal cross-section
    /// parameterization — if the radial formula
    /// `(cos(θ+φ)·a − cos(θ)·b) / sin(φ)` ever produces a non-unit-
    /// length vector, this assertion catches it before downstream
    /// consumers see a non-cylindrical "cylinder".
    #[test]
    fn evaluate_cylinder_vertex_radius_invariant_across_dihedrals() {
        for &angle_deg in &[60.0_f32, 90.0_f32, 120.0_f32] {
            let theta = angle_deg.to_radians();
            let a = [1.0, 0.0, 0.0];
            let b = [theta.cos(), theta.sin(), 0.0];

            let r = 0.2_f32;
            let op = make_synthetic_op(a, b, r);
            let upstream = synthetic_upstream_for_general_dihedral_tests();
            let out = op.evaluate(&[&upstream]).expect("evaluate");

            // axis_center_1 = pos_a + r/sin(φ) · (a + b).
            let pos_a = [0.0_f32, 0.0, 0.0];
            let sin_phi = theta.sin();
            let axis_center_1 = [
                pos_a[0] + r / sin_phi * (a[0] + b[0]),
                pos_a[1] + r / sin_phi * (a[1] + b[1]),
                pos_a[2] + r / sin_phi * (a[2] + b[2]),
            ];

            let cylinder_start = upstream.positions.len() + 4;
            for k in 0..=ROUND_FILLET_SEGMENTS {
                let pos = out.positions[cylinder_start + 2 * k];
                let dx = pos[0] - axis_center_1[0];
                let dy = pos[1] - axis_center_1[1];
                let dz = pos[2] - axis_center_1[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                assert!(
                    (dist - r).abs() < 1e-4,
                    "φ={angle_deg}° cylinder vert k={k} dist {dist} != r {r}"
                );
            }
        }
    }

    /// Arc endpoint coincidence: `vertex(θ=0)` must coincide with
    /// `inset_a` and `vertex(θ=arc_span)` must coincide with
    /// `inset_b`, within float epsilon, across multiple dihedrals.
    /// Pins the consistency between the arc parameterization and
    /// the inset placement — if either formula drifts independently,
    /// the cylinder surface would no longer tangent the two cap
    /// faces at the inset points and the rolled surface would
    /// "miss" the geometry.
    #[test]
    fn evaluate_arc_endpoints_coincide_with_inset_vertices() {
        for &angle_deg in &[60.0_f32, 90.0_f32, 120.0_f32] {
            let theta = angle_deg.to_radians();
            let a = [1.0, 0.0, 0.0];
            let b = [theta.cos(), theta.sin(), 0.0];
            let r = 0.5_f32;
            let op = make_synthetic_op(a, b, r);
            let upstream = synthetic_upstream_for_general_dihedral_tests();
            let out = op.evaluate(&[&upstream]).expect("evaluate");

            let inset_a1 = out.positions[upstream.positions.len()];
            let inset_b1 = out.positions[upstream.positions.len() + 2];
            let cylinder_start = upstream.positions.len() + 4;
            let cyl_first = out.positions[cylinder_start];
            let cyl_last = out.positions[cylinder_start + 2 * ROUND_FILLET_SEGMENTS];

            let dist_first_to_a = ((cyl_first[0] - inset_a1[0]).powi(2)
                + (cyl_first[1] - inset_a1[1]).powi(2)
                + (cyl_first[2] - inset_a1[2]).powi(2))
            .sqrt();
            let dist_last_to_b = ((cyl_last[0] - inset_b1[0]).powi(2)
                + (cyl_last[1] - inset_b1[1]).powi(2)
                + (cyl_last[2] - inset_b1[2]).powi(2))
            .sqrt();
            assert!(
                dist_first_to_a < 1e-4,
                "φ={angle_deg}° vertex(θ=0) at {cyl_first:?} should coincide with \
                 inset_a1 at {inset_a1:?} (dist {dist_first_to_a})"
            );
            assert!(
                dist_last_to_b < 1e-4,
                "φ={angle_deg}° vertex(θ=arc_span) at {cyl_last:?} should coincide \
                 with inset_b1 at {inset_b1:?} (dist {dist_last_to_b})"
            );
        }
    }

    /// Degenerate dihedrals (faces coplanar same-side `φ→0°` OR
    /// anti-parallel knife-edge `φ→180°`) reject at evaluate time
    /// with [`OpError::InvalidParameter`]. The `dot_ab.clamp(-1, 1)`
    /// before `acos`/`sqrt` prevents NaN on tiny float overshoot
    /// from non-unit-length inputs; the `sin_phi_sq <
    /// DIHEDRAL_EPSILON_SQ` guard then catches the degenerate case
    /// uniformly for both `dot_ab → +1` and `dot_ab → −1`. No new
    /// `RoundFilletError` variant per ADR-119 D-α scope; existing
    /// `OpError::InvalidParameter` carries the rejection signal.
    #[test]
    fn evaluate_rejects_near_degenerate_dihedral_at_zero_and_pi() {
        let upstream = synthetic_upstream_for_general_dihedral_tests();

        // a · b = 1: faces coplanar same-side (φ=0°). Exact unit
        // vectors, no float ambiguity.
        let op_parallel = make_synthetic_op([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.1);
        match op_parallel.evaluate(&[&upstream]).unwrap_err() {
            OpError::InvalidParameter(msg) => {
                assert!(
                    msg.contains("degenerate dihedral") || msg.contains("near-coplanar"),
                    "parallel-inward case: expected degenerate-dihedral message, got: {msg}"
                );
            }
            other => panic!("parallel-inward: expected InvalidParameter, got {other:?}"),
        }

        // a · b = -1: anti-parallel knife edge (φ=π).
        let op_anti = make_synthetic_op([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 0.1);
        match op_anti.evaluate(&[&upstream]).unwrap_err() {
            OpError::InvalidParameter(msg) => {
                assert!(
                    msg.contains("degenerate dihedral") || msg.contains("knife-edge"),
                    "anti-parallel case: expected degenerate-dihedral message, got: {msg}"
                );
            }
            other => panic!("anti-parallel: expected InvalidParameter, got {other:?}"),
        }

        // a · b = 1 + tiny overshoot (e.g., non-unit-length input):
        // the clamp catches this before acos / sqrt would NaN. Tests
        // the clamp guard itself, not just the geometric degeneracy.
        let op_overshoot = make_synthetic_op([1.000001, 0.0, 0.0], [1.0, 0.0, 0.0], 0.1);
        let err = op_overshoot.evaluate(&[&upstream]).unwrap_err();
        assert!(
            matches!(err, OpError::InvalidParameter(_)),
            "ULP-overshoot input must clamp + reject, got {err:?}"
        );
    }
}
